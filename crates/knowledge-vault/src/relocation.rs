//! Optimistic, bounded backlink updates. File moves are not multi-file transactions:
//! changed/unreadable resources stay intact and are reported after a successful move.
use super::{
    Result, Vault, attachments, fingerprint, normalized_relative, without_markdown_extension,
    write_atomic,
};
use knowledge_markdown::rewrite_wiki_link_targets;
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::{
    fs,
    io::Read,
    path::{Component, Path},
};

const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const MAX_SOURCES: usize = 50_000;
const MAX_CANVAS_NODES: usize = 20_480;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum LinkUpdateReason {
    Changed,
    Unavailable,
    InvalidCanvas,
    Limit,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkUpdateWarning {
    pub path: String,
    pub reason: LinkUpdateReason,
}

pub(super) struct LinkUpdatePlan {
    updates: Vec<LinkUpdate>,
    warnings: Vec<LinkUpdateWarning>,
}

struct LinkUpdate {
    path: String,
    expected: String,
    content: String,
}

fn relocated_path(path: &str, old: &str, new: &str, directory: bool) -> Option<String> {
    if path == old {
        return Some(new.to_owned());
    }
    directory
        .then(|| {
            path.strip_prefix(old)?
                .strip_prefix('/')
                .map(|suffix| format!("{new}/{suffix}"))
        })
        .flatten()
}

// Borrow raw JSON values and patch only the file string's span. Untouched bytes,
// including unknown fields, precision, whitespace and future node kinds survive.
// Duplicate known keys are rejected by derive rather than ambiguously rewritten.
#[derive(Deserialize)]
struct Canvas<'a> {
    #[serde(borrow)]
    nodes: Vec<&'a RawValue>,
}

#[derive(Deserialize)]
struct CanvasNode<'a> {
    #[serde(rename = "type")]
    kind: Option<String>,
    #[serde(borrow)]
    file: Option<&'a RawValue>,
}

fn rewrite_canvas(
    source: &str,
    old: &str,
    new: &str,
    directory: bool,
) -> std::result::Result<String, LinkUpdateReason> {
    let canvas: Canvas<'_> =
        serde_json::from_str(source).map_err(|_| LinkUpdateReason::InvalidCanvas)?;
    if canvas.nodes.len() > MAX_CANVAS_NODES {
        return Err(LinkUpdateReason::Limit);
    }
    let mut replacements = Vec::new();
    for raw in canvas.nodes {
        let node: CanvasNode<'_> =
            serde_json::from_str(raw.get()).map_err(|_| LinkUpdateReason::InvalidCanvas)?;
        if node.kind.as_deref() != Some("file") {
            continue;
        }
        let raw_file = node.file.ok_or(LinkUpdateReason::InvalidCanvas)?;
        let file: String =
            serde_json::from_str(raw_file.get()).map_err(|_| LinkUpdateReason::InvalidCanvas)?;
        if file.contains(':') {
            continue;
        } // URI schemes / Windows alternate streams are not vault references.
        let Ok(path) = normalized_relative(&file, false) else {
            continue;
        };
        if private_path(&path) {
            continue;
        }
        let portable = path
            .components()
            .filter(|part| matches!(part, Component::Normal(_)))
            .collect::<std::path::PathBuf>()
            .to_string_lossy()
            .replace('\\', "/");
        if let Some(next) = relocated_path(&portable, old, new, directory) {
            let start = raw_file.get().as_ptr() as usize - source.as_ptr() as usize;
            let value =
                serde_json::to_string(&next).map_err(|_| LinkUpdateReason::InvalidCanvas)?;
            replacements.push((start, raw_file.get().len(), value));
        }
    }
    let mut rewritten = source.to_owned();
    for (start, length, value) in replacements.into_iter().rev() {
        rewritten.replace_range(start..start + length, &value);
    }
    Ok(rewritten)
}

fn read_bounded(path: &Path) -> std::result::Result<Vec<u8>, LinkUpdateReason> {
    let file = fs::File::open(path).map_err(|_| LinkUpdateReason::Unavailable)?;
    let size = file
        .metadata()
        .map_err(|_| LinkUpdateReason::Unavailable)?
        .len();
    if size > MAX_SOURCE_BYTES {
        return Err(LinkUpdateReason::Limit);
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| LinkUpdateReason::Unavailable)?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err(LinkUpdateReason::Limit);
    }
    Ok(bytes)
}

fn private_path(path: &Path) -> bool {
    path.components().any(|part| {
        matches!(part, Component::Normal(name) if attachments::private_name(&name.to_string_lossy()))
    })
}

impl Vault {
    pub(super) fn prepare_link_updates(
        &self,
        old: &str,
        new: &str,
        directory: bool,
    ) -> Result<LinkUpdatePlan> {
        let snapshot = self.snapshot()?;
        let mappings: Vec<_> = snapshot
            .files
            .iter()
            .filter_map(|file| {
                let next = relocated_path(&file.path, old, new, directory)?;
                let old = without_markdown_extension(&file.path);
                let stem = Path::new(&old).file_name()?.to_string_lossy().into_owned();
                Some((vec![old, stem], without_markdown_extension(&next)))
            })
            .collect();
        let mut plan = LinkUpdatePlan {
            updates: Vec::new(),
            warnings: Vec::new(),
        };
        let mut total_bytes = 0;
        let mut planned_bytes = 0;
        for (index, entry) in snapshot
            .entries
            .iter()
            .filter(|entry| {
                (!mappings.is_empty() && entry.is_note)
                    || entry.extension.as_deref() == Some("canvas")
            })
            .enumerate()
        {
            // Private/symlink/junction resources never participate in automated rewrites.
            if private_path(Path::new(&entry.path)) {
                continue;
            }
            let after_path = relocated_path(&entry.path, old, new, directory)
                .unwrap_or_else(|| entry.path.clone());
            let attempt = (|| {
                if index >= MAX_SOURCES
                    || entry.size_bytes > MAX_SOURCE_BYTES
                    || total_bytes + entry.size_bytes > MAX_TOTAL_BYTES
                {
                    return Err(LinkUpdateReason::Limit);
                }
                let path = self
                    .public_attachment_path(&entry.path)
                    .map_err(|_| LinkUpdateReason::Unavailable)?;
                let bytes = read_bounded(&path)?;
                total_bytes += bytes.len() as u64;
                if total_bytes > MAX_TOTAL_BYTES {
                    return Err(LinkUpdateReason::Limit);
                }
                let expected = fingerprint(&bytes);
                let source = String::from_utf8(bytes).map_err(|_| LinkUpdateReason::Unavailable)?;
                let content = if entry.is_note {
                    mappings
                        .iter()
                        .fold(source.clone(), |content, (targets, next)| {
                            rewrite_wiki_link_targets(&content, targets, next)
                        })
                } else {
                    rewrite_canvas(&source, old, new, directory)?
                };
                if content.len() as u64 > MAX_SOURCE_BYTES {
                    return Err(LinkUpdateReason::Limit);
                }
                if content != source {
                    if planned_bytes + content.len() as u64 > MAX_TOTAL_BYTES {
                        return Err(LinkUpdateReason::Limit);
                    }
                    planned_bytes += content.len() as u64;
                    plan.updates.push(LinkUpdate {
                        path: after_path.clone(),
                        expected,
                        content,
                    });
                }
                Ok(())
            })();
            if let Err(reason) = attempt {
                plan.warnings.push(LinkUpdateWarning {
                    path: after_path,
                    reason,
                });
            }
        }
        Ok(plan)
    }

    pub(super) fn apply_link_updates(&self, mut plan: LinkUpdatePlan) -> Vec<LinkUpdateWarning> {
        for update in plan.updates {
            let attempt = (|| {
                let path = self
                    .public_attachment_path(&update.path)
                    .map_err(|_| LinkUpdateReason::Unavailable)?;
                // Recheck at write time, also for resources inside the moved folder.
                // Same optimistic fingerprint model as save_resource; no filesystem lock
                // or all-or-nothing cross-file transaction is claimed.
                if fingerprint(&read_bounded(&path)?) != update.expected {
                    return Err(LinkUpdateReason::Changed);
                }
                write_atomic(&path, update.content.as_bytes())
                    .map_err(|_| LinkUpdateReason::Unavailable)?;
                self.metadata_touch(&[&update.path]);
                Ok(())
            })();
            if let Err(reason) = attempt {
                plan.warnings.push(LinkUpdateWarning {
                    path: update.path,
                    reason,
                });
            }
        }
        plan.warnings
    }
}
