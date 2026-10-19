//! Shared, disposable in-memory metadata. Originals and task state stay on disk.
//! Cold inventory is separate from bounded parsing; normal edits never inventory
//! the whole vault. A reverse *candidate* index also tracks unresolved links.
use super::*;
use knowledge_markdown::{DocumentLink, LinkKind};
use std::{
    collections::{BTreeSet, VecDeque},
    io::Read,
    sync::atomic::{AtomicU64, Ordering},
    time::Instant,
};

const SOURCE_LIMIT: u64 = 16 * 1024 * 1024;
const BATCH_BYTES: u64 = 32 * 1024 * 1024;
const HISTORY_LIMIT: usize = 2048;
static GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataStats {
    pub parsed_sources: u64,
    pub read_sources: u64,
    pub bytes_read: u64,
    pub scanned_entries: u64,
    pub resolved_sources: u64,
    pub full_scans: u64,
    pub pending_sources: usize,
    pub failed_sources: usize,
    pub skipped_sources: usize,
    pub documents: usize,
    pub last_batch_ms: f64,
    pub last_reconciled_unix_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MetadataDelta {
    pub root: String,
    pub generation: u64,
    pub revision: u64,
    pub reset: bool,
    pub entries: Vec<VaultEntry>,
    pub removed_entries: Vec<String>,
    pub documents: Vec<DocumentMetadata>,
    pub removed_documents: Vec<String>,
    pub tags: BTreeMap<String, usize>,
    pub tasks: Vec<TaskMetadata>,
    pub task_paths: Vec<String>,
    pub stats: MetadataStats,
    pub error: Option<String>,
}

#[derive(Debug, Default)]
struct Changes {
    revision: u64,
    entries: BTreeSet<String>,
    documents: BTreeSet<String>,
    tasks: BTreeSet<String>,
    tags: BTreeSet<String>,
    sources: BTreeSet<String>,
}
impl Changes {
    fn extend(&mut self, other: &Self) {
        self.entries.extend(other.entries.iter().cloned());
        self.documents.extend(other.documents.iter().cloned());
        self.tasks.extend(other.tasks.iter().cloned());
        self.tags.extend(other.tags.iter().cloned());
        self.sources.extend(other.sources.iter().cloned());
    }
    fn is_empty(&self) -> bool {
        self.entries.is_empty()
            && self.documents.is_empty()
            && self.tasks.is_empty()
            && self.tags.is_empty()
    }
}

#[derive(Debug)]
pub(super) struct MetadataCache {
    generation: u64,
    revision: u64,
    name: String,
    entries: BTreeMap<String, VaultEntry>,
    documents: BTreeMap<String, DocumentMetadata>,
    fingerprints: HashMap<String, String>,
    tags: BTreeMap<String, usize>,
    tasks: BTreeMap<String, Vec<TaskMetadata>>,
    // e: exact normalized path, a: globally unique alias/title/stem.
    targets: HashMap<String, BTreeSet<String>>,
    dependants: HashMap<String, BTreeSet<String>>,
    pending: BTreeSet<String>,
    urgent: BTreeSet<String>,
    history: VecDeque<Changes>,
    stats: MetadataStats,
    failures: BTreeSet<String>,
    skipped: BTreeSet<String>,
    reconcile: bool,
    task_refresh: bool,
    error: Option<String>,
}

fn public_path(path: &str) -> bool {
    !path.is_empty()
        && !path.contains(['\\', ':', '\0'])
        && !path.starts_with('/')
        && path.split('/').all(|part| {
            !part.is_empty()
                && part != ".."
                && !attachments::private_name(part)
                && !matches!(part, "node_modules" | "target")
        })
}

fn target_keys(document: &DocumentMetadata) -> BTreeSet<String> {
    let path = without_markdown_extension(&document.path);
    let stem = Path::new(&path)
        .file_name()
        .map(|v| v.to_string_lossy().into_owned())
        .unwrap_or_default();
    std::iter::once(format!("e:{}", path.to_lowercase()))
        .chain(
            std::iter::once(path)
                .chain(std::iter::once(stem))
                .chain(std::iter::once(document.title.clone()))
                .chain(document.aliases.iter().cloned())
                .map(|alias| format!("a:{}", alias.to_lowercase())),
        )
        .collect()
}

fn link_keys(path: &str, link: &DocumentLink) -> Vec<String> {
    if link.target.is_empty() {
        return vec![format!(
            "e:{}",
            without_markdown_extension(path).to_lowercase()
        )];
    }
    let folder = path.rsplit_once('/').map_or("", |(folder, _)| folder);
    let relative = normalized_link_path(&format!("{folder}/{}", link.target));
    let root = normalized_link_path(&link.target);
    let candidates = if link.target.starts_with('/') {
        vec![root]
    } else if link.kind == LinkKind::Markdown
        || link.target.starts_with("./")
        || link.target.starts_with("../")
    {
        vec![relative]
    } else {
        vec![relative, root]
    };
    let mut keys: Vec<_> = candidates
        .into_iter()
        .flatten()
        .map(|candidate| {
            format!(
                "e:{}",
                without_markdown_extension(&candidate).to_lowercase()
            )
        })
        .collect();
    if link.kind != LinkKind::Markdown && !link.target.contains('/') {
        keys.push(format!(
            "a:{}",
            without_markdown_extension(&link.target).to_lowercase()
        ));
    }
    keys
}

fn remove_key(map: &mut HashMap<String, BTreeSet<String>>, key: &str, path: &str) {
    if let Some(paths) = map.get_mut(key) {
        paths.remove(path);
        if paths.is_empty() {
            map.remove(key);
        }
    }
}

impl MetadataCache {
    fn new(snapshot: VaultSnapshot) -> Self {
        let entries: BTreeMap<_, _> = snapshot
            .entries
            .into_iter()
            .filter(|entry| public_path(&entry.path))
            .map(|entry| (entry.path.clone(), entry))
            .collect();
        let pending = entries
            .values()
            .filter(|entry| entry.is_note)
            .map(|entry| entry.path.clone())
            .collect();
        Self {
            generation: GENERATION.fetch_add(1, Ordering::Relaxed),
            revision: 1,
            name: snapshot.name,
            stats: MetadataStats {
                full_scans: 1,
                scanned_entries: entries.len() as u64,
                ..MetadataStats::default()
            },
            entries,
            pending,
            urgent: BTreeSet::new(),
            documents: BTreeMap::new(),
            fingerprints: HashMap::new(),
            tags: BTreeMap::new(),
            tasks: BTreeMap::new(),
            targets: HashMap::new(),
            dependants: HashMap::new(),
            history: VecDeque::new(),
            failures: BTreeSet::new(),
            skipped: BTreeSet::new(),
            reconcile: false,
            task_refresh: true,
            error: None,
        }
    }

    pub(super) fn snapshot(&self, vault: &Vault, partial: bool) -> VaultSnapshot {
        let mut entries: Vec<_> = if partial {
            vec![]
        } else {
            self.entries.values().cloned().collect()
        };
        entries.sort_by_cached_key(|entry| {
            (
                entry.parent.to_lowercase(),
                entry.kind != VaultEntryKind::Folder,
                entry.name.to_lowercase(),
            )
        });
        let mut files: Vec<_> = entries
            .iter()
            .filter(|entry| entry.is_note)
            .map(|entry| VaultFile {
                path: entry.path.clone(),
                name: entry.name.clone(),
                folder: entry.parent.clone(),
                modified_unix_ms: entry.modified_unix_ms,
                size_bytes: entry.size_bytes,
            })
            .collect();
        files.sort_by_cached_key(|file| file.path.to_lowercase());
        VaultSnapshot {
            root: vault.root.to_string_lossy().into_owned(),
            name: self.name.clone(),
            entries,
            files,
            partial,
            metadata_revision: Some(self.revision),
            metadata_generation: Some(self.generation),
        }
    }

    pub(super) fn index(&self) -> VaultIndex {
        VaultIndex {
            documents: self.documents.values().cloned().collect(),
            tags: self.tags.clone(),
            tasks: self.tasks.values().flatten().cloned().collect(),
        }
    }

    fn commit(&mut self, mut changes: Changes) {
        if changes.is_empty() {
            return;
        }
        self.revision += 1;
        changes.revision = self.revision;
        self.history.push_back(changes);
        if self.history.len() > HISTORY_LIMIT {
            self.history.pop_front();
        }
    }

    fn unlink_document(
        &mut self,
        path: &str,
        changed_keys: &mut BTreeSet<String>,
        changes: &mut Changes,
    ) {
        if let Some(document) = self.documents.remove(path) {
            for key in target_keys(&document) {
                remove_key(&mut self.targets, &key, path);
                changed_keys.insert(key);
            }
            for link in &document.links {
                for key in link_keys(path, link) {
                    remove_key(&mut self.dependants, &key, path);
                }
            }
            for tag in &document.tags {
                if let Some(count) = self.tags.get_mut(tag) {
                    *count -= 1;
                    if *count == 0 {
                        self.tags.remove(tag);
                    }
                }
                changes.tags.insert(tag.clone());
            }
            if let Some(tasks) = self.tasks.remove(path) {
                self.task_refresh |= tasks.iter().any(|task| task.managed);
                changes.tasks.insert(path.to_owned());
            }
            changes.documents.insert(path.to_owned());
        }
        self.fingerprints.remove(path);
    }

    fn put_document(
        &mut self,
        document: DocumentMetadata,
        hash: String,
        changed_keys: &mut BTreeSet<String>,
        changes: &mut Changes,
    ) {
        let path = document.path.clone();
        if self.fingerprints.contains_key(&path) {
            changes.sources.insert(path.clone());
        }
        self.task_refresh |= self
            .tasks
            .get(&path)
            .is_some_and(|tasks| tasks.iter().any(|task| task.managed));
        let old_keys = self
            .documents
            .get(&path)
            .map(target_keys)
            .unwrap_or_default();
        let new_keys = target_keys(&document);
        self.unlink_document(&path, &mut BTreeSet::new(), changes);
        changed_keys.extend(old_keys.symmetric_difference(&new_keys).cloned());
        for key in new_keys {
            self.targets
                .entry(key.clone())
                .or_default()
                .insert(path.clone());
        }
        for link in &document.links {
            for key in link_keys(&path, link) {
                self.dependants.entry(key).or_default().insert(path.clone());
            }
        }
        for tag in &document.tags {
            *self.tags.entry(tag.clone()).or_default() += 1;
            changes.tags.insert(tag.clone());
        }
        if !document.tasks.is_empty() {
            self.tasks.insert(path.clone(), document.tasks.clone());
            changes.tasks.insert(path.clone());
        }
        changes.documents.insert(path.clone());
        self.fingerprints.insert(path.clone(), hash);
        self.documents.insert(path, document);
    }

    fn resolve_changed(&mut self, changed_keys: &BTreeSet<String>, changes: &mut Changes) {
        let mut affected = changes.documents.clone();
        for key in changed_keys {
            if let Some(paths) = self.dependants.get(key) {
                affected.extend(paths.iter().cloned());
            }
        }
        for path in affected {
            if let Some(document) = self.documents.get_mut(&path) {
                let mut changed = false;
                for link in &mut document.links {
                    let target = link_keys(&path, link).iter().find_map(|key| {
                        self.targets
                            .get(key)
                            .filter(|paths| paths.len() == 1)
                            .and_then(|paths| paths.first().cloned())
                    });
                    if link.resolved_path != target {
                        link.resolved_path = target;
                        changed = true;
                    }
                }
                self.stats.resolved_sources += 1;
                if changed {
                    changes.documents.insert(path);
                }
            }
        }
    }

    fn remove_prefix(&mut self, prefix: &str, changes: &mut Changes, keys: &mut BTreeSet<String>) {
        let paths: Vec<_> = self
            .entries
            .range(prefix.to_owned()..)
            .take_while(|(path, _)| path.starts_with(prefix))
            .filter(|(path, _)| path.as_str() == prefix || path.starts_with(&format!("{prefix}/")))
            .map(|(path, _)| path.clone())
            .collect();
        for path in paths {
            self.entries.remove(&path);
            self.pending.remove(&path);
            self.urgent.remove(&path);
            self.failures.remove(&path);
            self.skipped.remove(&path);
            changes.entries.insert(path.clone());
            self.unlink_document(&path, keys, changes);
        }
    }

    fn safe_entry(&self, vault: &Vault, path: &str) -> Option<VaultEntry> {
        if !public_path(path) {
            return None;
        }
        let mut absolute = vault.root.clone();
        for part in path.split('/') {
            absolute.push(part);
            if attachments::is_link(&fs::symlink_metadata(&absolute).ok()?) {
                return None;
            }
        }
        let metadata = fs::metadata(&absolute).ok()?;
        if !metadata.is_file() && !metadata.is_dir() {
            return None;
        }
        // Revalidate the canonical boundary even when the file existed at inventory time.
        if !absolute.canonicalize().ok()?.starts_with(&vault.root) {
            return None;
        }
        let name = path.rsplit('/').next()?.to_owned();
        Some(VaultEntry {
            path: path.to_owned(),
            name,
            parent: path
                .rsplit_once('/')
                .map_or("", |(parent, _)| parent)
                .to_owned(),
            kind: if metadata.is_dir() {
                VaultEntryKind::Folder
            } else {
                VaultEntryKind::File
            },
            extension: if metadata.is_file() {
                absolute
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_lowercase)
            } else {
                None
            },
            modified_unix_ms: modified_unix_ms(&metadata),
            size_bytes: if metadata.is_file() {
                metadata.len()
            } else {
                0
            },
            is_note: metadata.is_file() && is_markdown(&absolute),
        })
    }

    fn touch(
        &mut self,
        vault: &Vault,
        path: &str,
        changes: &mut Changes,
        keys: &mut BTreeSet<String>,
    ) {
        if path.starts_with(".brandon/tasks.sqlite3") {
            self.task_refresh = true;
            return;
        }
        if !public_path(path) {
            return;
        }
        self.stats.scanned_entries += 1;
        let Some(entry) = self.safe_entry(vault, path) else {
            self.remove_prefix(path, changes, keys);
            return;
        };
        let changed = self.entries.get(path).is_none_or(|old| {
            old.kind != entry.kind
                || old.modified_unix_ms != entry.modified_unix_ms
                || old.size_bytes != entry.size_bytes
        });
        if self
            .entries
            .get(path)
            .is_some_and(|old| old.kind != entry.kind)
        {
            self.remove_prefix(path, changes, keys);
        }
        if !entry.is_note {
            self.unlink_document(path, keys, changes);
        }
        if changed || entry.kind == VaultEntryKind::File {
            self.entries.insert(path.to_owned(), entry.clone());
            changes.entries.insert(path.to_owned());
        }
        if entry.kind == VaultEntryKind::File {
            if entry.is_note {
                self.pending.insert(path.to_owned());
                self.urgent.insert(path.to_owned());
            }
        } else {
            // Directory create/rename is a subtree operation, not a vault scan.
            let mut files = Vec::new();
            let mut entries = Vec::new();
            match scan_directory(
                &vault.root,
                &vault.root.join(path),
                &mut files,
                &mut entries,
            ) {
                Ok(()) => {
                    let found: BTreeSet<_> =
                        entries.iter().map(|entry| entry.path.clone()).collect();
                    let missing: Vec<_> = self
                        .entries
                        .keys()
                        .filter(|old| old.starts_with(&format!("{path}/")) && !found.contains(*old))
                        .cloned()
                        .collect();
                    for old in missing {
                        self.remove_prefix(&old, changes, keys);
                    }
                    for child in entries {
                        if let Some(safe) = self.safe_entry(vault, &child.path) {
                            self.stats.scanned_entries += 1;
                            if safe.is_note {
                                self.pending.insert(safe.path.clone());
                            }
                            changes.entries.insert(safe.path.clone());
                            self.entries.insert(safe.path.clone(), safe);
                        }
                    }
                }
                Err(error) => {
                    self.error = Some(error.to_string());
                    self.remove_prefix(path, changes, keys);
                }
            }
        }
        if let Some((parent, _)) = path.rsplit_once('/')
            && !self.entries.contains_key(parent)
        {
            self.touch(vault, parent, changes, keys);
        }
    }

    fn refresh_tasks(&mut self, vault: &Vault, changes: &mut Changes) -> Result<()> {
        if !self.task_refresh {
            return Ok(());
        }
        let mut next: BTreeMap<String, Vec<TaskMetadata>> = self
            .documents
            .values()
            .filter(|doc| !doc.tasks.is_empty())
            .map(|doc| (doc.path.clone(), doc.tasks.clone()))
            .collect();
        for task in vault.managed_tasks_refs(self.documents.values())? {
            next.entry(task.path.clone()).or_default().push(task);
        }
        let paths: BTreeSet<_> = self.tasks.keys().chain(next.keys()).cloned().collect();
        for path in paths {
            if self.tasks.get(&path) != next.get(&path) {
                changes.tasks.insert(path);
            }
        }
        self.tasks = next;
        self.task_refresh = false;
        Ok(())
    }

    fn process(&mut self, vault: &Vault, limit: usize) -> Result<bool> {
        let started = Instant::now();
        let before = self.revision;
        let mut changes = Changes::default();
        let mut keys = BTreeSet::new();
        if self.reconcile {
            let raw = Vault {
                root: vault.root.clone(),
                metadata: None,
            }
            .snapshot()?;
            self.reconcile = false;
            self.stats.full_scans += 1;
            self.stats.scanned_entries += raw.entries.len() as u64;
            let found: BTreeMap<_, _> = raw
                .entries
                .into_iter()
                .filter(|entry| public_path(&entry.path))
                .map(|entry| (entry.path.clone(), entry))
                .collect();
            let removed: Vec<_> = self
                .entries
                .keys()
                .filter(|path| !found.contains_key(*path))
                .cloned()
                .collect();
            for path in removed {
                self.remove_prefix(&path, &mut changes, &mut keys);
            }
            for (path, entry) in found {
                // Lost events can include same-size writes with restored mtime.
                // A full reconciliation validates hashes, but unchanged bytes
                // never go through the Markdown parser or back over IPC.
                if entry.is_note {
                    self.pending.insert(path.clone());
                }
                if entry
                    .extension
                    .as_deref()
                    .is_some_and(|extension| matches!(extension, "canvas" | "base"))
                {
                    changes.entries.insert(path.clone());
                }
                if self.entries.get(&path).is_none_or(|old| {
                    old.kind != entry.kind
                        || old.size_bytes != entry.size_bytes
                        || old.modified_unix_ms != entry.modified_unix_ms
                }) {
                    if entry.is_note {
                        self.pending.insert(path.clone());
                    }
                    changes.entries.insert(path.clone());
                    self.entries.insert(path, entry);
                }
            }
            self.task_refresh = true;
            self.stats.last_reconciled_unix_ms = Some(current_unix_ms());
        }
        let paths: Vec<_> = self
            .urgent
            .iter()
            .chain(
                self.pending
                    .iter()
                    .filter(|path| !self.urgent.contains(*path)),
            )
            .take(limit.clamp(1, 256))
            .cloned()
            .collect();
        let mut bytes = 0;
        for path in paths {
            if bytes >= BATCH_BYTES {
                break;
            }
            self.pending.remove(&path);
            self.urgent.remove(&path);
            // Remove stale metadata on revocation, symlink replacement or read failure.
            let Some(mut entry) = self.safe_entry(vault, &path).filter(|entry| entry.is_note)
            else {
                self.remove_prefix(&path, &mut changes, &mut keys);
                continue;
            };
            if entry.size_bytes > SOURCE_LIMIT {
                self.skipped.insert(path.clone());
                self.unlink_document(&path, &mut keys, &mut changes);
                continue;
            }
            let read = (|| -> Result<Vec<u8>> {
                let absolute = vault.public_attachment_path(&path)?;
                let file = fs::File::open(absolute).map_err(|source| VaultError::Io {
                    context: "Metadata source недоступен",
                    source,
                })?;
                let mut data = Vec::new();
                file.take(SOURCE_LIMIT + 1)
                    .read_to_end(&mut data)
                    .map_err(|source| VaultError::Io {
                        context: "Metadata source не читается",
                        source,
                    })?;
                if data.len() as u64 > SOURCE_LIMIT {
                    return Err(VaultError::InvalidPath("Metadata source превышает лимит"));
                }
                Ok(data)
            })();
            match read {
                Ok(mut data) => {
                    bytes += data.len() as u64;
                    self.stats.read_sources += 1;
                    self.stats.bytes_read += data.len() as u64;
                    if path.starts_with("Tasks/")
                        && !path.eq_ignore_ascii_case("Tasks/Inbox.md")
                        && let Ok(source) = std::str::from_utf8(&data)
                        && let Some(cleaned) = vault.migrate_legacy_task_source(
                            &path,
                            source,
                            entry.modified_unix_ms,
                        )?
                    {
                        data = cleaned.into_bytes();
                        self.task_refresh = true;
                        if let Some(updated) = self.safe_entry(vault, &path) {
                            entry = updated;
                            self.entries.insert(path.clone(), entry.clone());
                            changes.entries.insert(path.clone());
                        }
                    }
                    let hash = fingerprint(&data);
                    self.failures.remove(&path);
                    self.skipped.remove(&path);
                    if self.fingerprints.get(&path) != Some(&hash) {
                        match std::str::from_utf8(&data) {
                            Ok(source) => {
                                let document =
                                    parse_document(&path, source, entry.modified_unix_ms);
                                self.stats.parsed_sources += 1;
                                self.put_document(document, hash, &mut keys, &mut changes);
                            }
                            Err(_) => {
                                self.failures.insert(path.clone());
                                self.unlink_document(&path, &mut keys, &mut changes);
                            }
                        }
                    } else if let Some(document) = self.documents.get_mut(&path)
                        && document.modified_unix_ms != entry.modified_unix_ms
                    {
                        document.modified_unix_ms = entry.modified_unix_ms;
                        changes.documents.insert(path.clone());
                    }
                }
                Err(_) => {
                    self.failures.insert(path.clone());
                    self.unlink_document(&path, &mut keys, &mut changes);
                }
            }
        }
        self.resolve_changed(&keys, &mut changes);
        match self.refresh_tasks(vault, &mut changes) {
            Ok(()) => self.error = None,
            Err(error) => self.error = Some(error.to_string()),
        }
        self.commit(changes);
        self.stats.last_batch_ms = started.elapsed().as_secs_f64() * 1000.;
        Ok(before != self.revision)
    }

    fn delta(&self, vault: &Vault, generation: Option<u64>, after: Option<u64>) -> MetadataDelta {
        let reset = generation != Some(self.generation)
            || after.is_none_or(|after| {
                after > self.revision
                    || self
                        .history
                        .front()
                        .is_some_and(|first| after + 1 < first.revision)
            });
        let mut changes = Changes::default();
        if reset {
            changes.entries.extend(self.entries.keys().cloned());
            changes.documents.extend(self.documents.keys().cloned());
            changes.tasks.extend(self.tasks.keys().cloned());
            changes.tags.extend(self.tags.keys().cloned());
        } else {
            for change in self
                .history
                .iter()
                .filter(|change| change.revision > after.unwrap_or(0))
            {
                changes.extend(change);
            }
        }
        let mut stats = self.stats.clone();
        stats.pending_sources = self.pending.len() + usize::from(self.reconcile);
        stats.failed_sources = self.failures.len();
        stats.skipped_sources = self.skipped.len();
        stats.documents = self.documents.len();
        MetadataDelta {
            root: vault.root.to_string_lossy().into_owned(),
            generation: self.generation,
            revision: self.revision,
            reset,
            entries: changes
                .entries
                .iter()
                .filter_map(|path| self.entries.get(path).cloned())
                .collect(),
            removed_entries: changes
                .entries
                .into_iter()
                .filter(|path| !self.entries.contains_key(path))
                .collect(),
            documents: changes
                .documents
                .iter()
                .filter_map(|path| self.documents.get(path).cloned())
                .collect(),
            removed_documents: changes
                .documents
                .into_iter()
                .filter(|path| !self.documents.contains_key(path))
                .collect(),
            tags: changes
                .tags
                .into_iter()
                .map(|tag| {
                    let count = self.tags.get(&tag).copied().unwrap_or(0);
                    (tag, count)
                })
                .collect(),
            tasks: changes
                .tasks
                .iter()
                .filter_map(|path| self.tasks.get(path))
                .flatten()
                .cloned()
                .collect(),
            task_paths: changes.tasks.into_iter().collect(),
            stats,
            error: self.error.clone(),
        }
    }
}

impl Vault {
    /// Reuse the common public catalogue as hints, never as read permission.
    /// Search still validates each candidate and each returned hit on disk.
    pub(super) fn metadata_inventory(&self, limit: usize) -> Option<Result<SearchInventory>> {
        let cache = self.metadata.as_ref()?.lock().ok()?;
        if cache.reconcile || cache.error.is_some() {
            return None;
        }
        let paths: Vec<_> = cache
            .entries
            .values()
            .filter(|entry| {
                entry.kind == VaultEntryKind::File && entry.path.matches('/').count() <= 64
            })
            .map(|entry| entry.path.clone())
            .collect();
        if paths.len() > limit {
            return Some(Err(VaultError::InvalidPath(
                "Первый локальный индекс ограничен 50 000 объектами; сузьте vault",
            )));
        }
        Some(Ok(SearchInventory {
            paths,
            skipped_sources: cache
                .entries
                .values()
                .filter(|entry| {
                    entry.kind == VaultEntryKind::File && entry.path.matches('/').count() > 64
                })
                .count(),
            failed_sources: 0,
        }))
    }
    /// Revision cursor for downstream workers. Does not clone document bodies.
    pub fn metadata_invalidations(&self, after: u64) -> Result<(u64, bool, Vec<String>)> {
        let cache = self
            .metadata
            .as_ref()
            .ok_or(VaultError::InvalidPath("Metadata cache не активирован"))?
            .lock()
            .map_err(|_| VaultError::InvalidPath("Metadata cache недоступен"))?;
        let reset = after > cache.revision
            || cache
                .history
                .front()
                .is_some_and(|first| after.saturating_add(1) < first.revision);
        let mut paths = BTreeSet::new();
        for change in cache
            .history
            .iter()
            .filter(|change| change.revision > after)
        {
            paths.extend(change.entries.iter().cloned());
            paths.extend(change.sources.iter().cloned());
        }
        Ok((cache.revision, reset, paths.into_iter().collect()))
    }
    pub fn snapshot_identity(&self) -> (u64, u64) {
        self.metadata
            .as_ref()
            .and_then(|cache| {
                cache
                    .lock()
                    .ok()
                    .map(|cache| (cache.generation, cache.revision))
            })
            .unwrap_or((0, 0))
    }
    pub fn metadata_kind(&self, path: &str) -> Option<VaultEntryKind> {
        self.metadata.as_ref().and_then(|cache| {
            cache
                .lock()
                .ok()
                .and_then(|cache| cache.entries.get(path).map(|entry| entry.kind))
        })
    }
    /// Explicit incremental lifetime: clones share one cache. Non-watcher library
    /// callers retain the original stateless snapshot/index APIs by default.
    pub fn incremental(mut self) -> Result<Self> {
        if self.metadata.is_none() {
            let snapshot = self.snapshot()?;
            self.metadata = Some(Arc::new(Mutex::new(MetadataCache::new(snapshot))));
        }
        Ok(self)
    }
    pub(super) fn save_snapshot(&self) -> Result<VaultSnapshot> {
        if let Some(cache) = &self.metadata {
            return Ok(cache
                .lock()
                .map_err(|_| VaultError::InvalidPath("Metadata cache недоступен"))?
                .snapshot(self, true));
        }
        self.snapshot()
    }
    pub fn metadata_queue(&self, paths: &[String], reconcile: bool) {
        if let Some(cache) = &self.metadata
            && let Ok(mut cache) = cache.lock()
        {
            cache.reconcile |= reconcile;
            let mut changes = Changes::default();
            let mut keys = BTreeSet::new();
            for path in paths {
                cache.touch(self, path, &mut changes, &mut keys);
            }
            cache.resolve_changed(&keys, &mut changes);
            cache.commit(changes);
        }
    }
    pub(super) fn metadata_touch(&self, paths: &[&str]) {
        self.metadata_queue(
            &paths
                .iter()
                .map(|path| (*path).to_owned())
                .collect::<Vec<_>>(),
            false,
        );
    }
    pub fn metadata_step(&self, limit: usize) -> Result<bool> {
        if let Some(cache) = &self.metadata {
            let mut cache = cache
                .lock()
                .map_err(|_| VaultError::InvalidPath("Metadata cache недоступен"))?;
            let result = cache.process(self, limit);
            if let Err(error) = &result {
                cache.error = Some(error.to_string());
            }
            return result;
        }
        Ok(false)
    }
    pub fn metadata_delta(
        &self,
        generation: Option<u64>,
        after: Option<u64>,
    ) -> Result<MetadataDelta> {
        let cache = self.metadata.as_ref().ok_or(VaultError::InvalidPath(
            "Incremental metadata не активированы",
        ))?;
        Ok(cache
            .lock()
            .map_err(|_| VaultError::InvalidPath("Metadata cache недоступен"))?
            .delta(self, generation, after))
    }
    /// Constant-size observation: never clone documents to ask whether warming
    /// has finished, even when the client has not yet received a revision.
    pub fn metadata_status(&self) -> Result<MetadataDelta> {
        let cache = self.metadata.as_ref().ok_or(VaultError::InvalidPath(
            "Incremental metadata не активированы",
        ))?;
        let cache = cache
            .lock()
            .map_err(|_| VaultError::InvalidPath("Metadata cache недоступен"))?;
        Ok(cache.delta(self, Some(cache.generation), Some(cache.revision)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamp_only_changes_match_stateless_metadata_without_reparsing() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "A.md", "# A\n\n[[B]] #tag\n");
        write(root.path(), "B.md", "# B\n");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let before = vault.metadata_status().unwrap();
        let path = root.path().join("A.md");
        let modified =
            fs::metadata(&path).unwrap().modified().unwrap() + std::time::Duration::from_secs(2);
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        vault.metadata_queue(&["A.md".into()], false);
        drain(&vault);
        let cached = vault.index().unwrap();
        let direct = Vault::open(root.path()).unwrap().index().unwrap();
        assert_eq!(cached.documents, direct.documents);
        assert_eq!(cached.tags, direct.tags);
        assert_eq!(cached.tasks, direct.tasks);
        assert_eq!(
            vault.metadata_status().unwrap().stats.parsed_sources,
            before.stats.parsed_sources
        );
    }
    #[test]
    fn queued_directory_revalidates_descendants_even_when_its_mtime_is_unchanged() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "Folder/A.md", "# A\n");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let before = vault.metadata_status().unwrap();
        vault.metadata_queue(&["Folder".into()], false);
        drain(&vault);
        let after = vault.metadata_status().unwrap();
        assert_eq!(after.stats.read_sources, before.stats.read_sources + 1);
        assert_eq!(after.stats.parsed_sources, before.stats.parsed_sources);
    }
    #[test]
    fn shared_search_catalogue_is_only_a_hint_and_targeted_sync_revalidates_disk() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "A.md", "# A\n\nonly-in-A\n");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        vault.prepare_search_cache().unwrap();
        vault
            .sync_search_batch(
                SearchUpdate::Paths(vec!["A.md".into()]),
                1024 * 1024,
                &|| false,
            )
            .unwrap();
        fs::remove_file(root.path().join("A.md")).unwrap();
        write(root.path(), "B.md", "# B\n");
        let inventory = vault.search_inventory(&|| false).unwrap();
        assert!(inventory.paths.contains(&"A.md".into()));
        assert!(!inventory.paths.contains(&"B.md".into()));
        vault
            .sync_search_batch(SearchUpdate::Paths(inventory.paths), 1024 * 1024, &|| false)
            .unwrap();
        assert!(
            vault
                .search_cached("only-in-A", SearchMode::Words, 10)
                .unwrap()
                .hits
                .is_empty()
        );
        let before = vault.metadata_status().unwrap();
        vault.metadata_queue(&[], true);
        drain(&vault);
        let (_, reset, paths) = vault.metadata_invalidations(before.revision).unwrap();
        assert!(!reset);
        assert!(paths.contains(&"A.md".into()));
        assert!(paths.contains(&"B.md".into()));
        assert!(
            vault
                .search_inventory(&|| false)
                .unwrap()
                .paths
                .contains(&"B.md".into())
        );
    }
    #[test]
    fn body_only_source_invalidations_do_not_forward_unchanged_incoming_documents() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "A.md", "# A\n\nold\n");
        write(root.path(), "Ref.md", "# Ref\n\n[[A]]\n");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let before = vault.metadata_status().unwrap();
        write(root.path(), "A.md", "# A\n\nnew\n");
        vault.metadata_queue(&[], true);
        drain(&vault);
        let (_, reset, paths) = vault.metadata_invalidations(before.revision).unwrap();
        assert!(!reset);
        assert_eq!(paths, vec!["A.md"]);
    }
    #[test]
    fn reconciliation_validates_same_size_sources_even_with_restored_modified_time() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "A.md", "# Old\n");
        write(root.path(), "Keep.md", "# Keep\n");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let before = vault.metadata_status().unwrap();
        let path = root.path().join("A.md");
        let modified = fs::metadata(&path).unwrap().modified().unwrap();
        fs::write(&path, "# New\n").unwrap();
        fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(modified))
            .unwrap();
        vault.metadata_queue(&[], true);
        drain(&vault);
        let delta = vault
            .metadata_delta(Some(before.generation), Some(before.revision))
            .unwrap();
        assert_eq!(source(&delta, "A.md").title, "New");
        assert_eq!(delta.stats.parsed_sources - before.stats.parsed_sources, 1);
        assert_eq!(delta.stats.read_sources - before.stats.read_sources, 2);
    }
    #[test]
    fn replacing_a_cached_directory_with_a_link_revokes_all_its_cached_sources() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        write(root.path(), "Folder/A.md", "# Cached\n");
        write(outside.path(), "Private.md", "# Not in vault\n");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        fs::rename(
            root.path().join("Folder"),
            root.path().join(".retained-original"),
        )
        .unwrap();
        #[cfg(windows)]
        assert!(
            std::process::Command::new("cmd")
                .args(["/d", "/c", "mklink", "/J"])
                .arg(root.path().join("Folder"))
                .arg(outside.path())
                .output()
                .unwrap()
                .status
                .success()
        );
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.path(), root.path().join("Folder")).unwrap();
        vault.metadata_queue(&["Folder".into()], false);
        drain(&vault);
        let full = vault.metadata_delta(None, None).unwrap();
        assert!(full.documents.is_empty());
        assert!(
            full.entries
                .iter()
                .all(|entry| !entry.path.starts_with("Folder"))
        );
        assert!(vault.read_note("Folder/Private.md").is_err());
        #[cfg(windows)]
        fs::remove_dir(root.path().join("Folder")).unwrap();
        #[cfg(unix)]
        fs::remove_file(root.path().join("Folder")).unwrap();
    }
    #[test]
    fn stable_target_body_edit_does_not_revisit_all_incoming_links() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "Target.md", "# Target\n\nBefore\n");
        for n in 0..300 {
            write(
                root.path(),
                &format!("Ref{n:03}.md"),
                "# Ref\n\n[[Target]]\n",
            );
        }
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let before = vault.metadata_status().unwrap();
        let note = vault.read_note("Target.md").unwrap();
        vault
            .save_note(&note.path, "# Target\n\nAfter\n", Some(&note.fingerprint))
            .unwrap();
        drain(&vault);
        let delta = vault
            .metadata_delta(Some(before.generation), Some(before.revision))
            .unwrap();
        assert_eq!(delta.documents.len(), 1);
        assert_eq!(
            delta.stats.resolved_sources - before.stats.resolved_sources,
            1
        );
        assert_eq!(delta.stats.parsed_sources - before.stats.parsed_sources, 1);
        let status = vault.metadata_status().unwrap();
        assert!(status.documents.is_empty());
        assert!(status.entries.is_empty());
    }
    fn write(root: &Path, path: &str, text: &str) {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn drain(vault: &Vault) {
        for _ in 0..2000 {
            vault.metadata_step(128).unwrap();
            if vault
                .metadata_delta(None, None)
                .unwrap()
                .stats
                .pending_sources
                == 0
            {
                return;
            }
        }
        panic!("Metadata queue did not settle");
    }
    fn source<'a>(delta: &'a MetadataDelta, path: &str) -> &'a DocumentMetadata {
        delta.documents.iter().find(|doc| doc.path == path).unwrap()
    }
    #[test]
    fn one_edit_parses_one_source_and_no_full_inventory() {
        let root = tempfile::tempdir().unwrap();
        for i in 0..1000 {
            write(
                root.path(),
                &format!("N{i:04}.md"),
                &format!("# Note {i}\n\n#shared\n"),
            );
        }
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let before = vault.metadata_delta(None, None).unwrap();
        let note = vault.read_note("N0500.md").unwrap();
        let saved = vault
            .save_note(
                &note.path,
                "# Changed\n\n#newtag\n",
                Some(&note.fingerprint),
            )
            .unwrap();
        assert!(matches!(
            saved,
            SaveOutcome::Saved {
                snapshot: VaultSnapshot { partial: true, .. },
                ..
            }
        ));
        drain(&vault);
        let delta = vault
            .metadata_delta(Some(before.generation), Some(before.revision))
            .unwrap();
        assert!(!delta.reset);
        assert_eq!(delta.documents.len(), 1);
        assert_eq!(delta.stats.parsed_sources - before.stats.parsed_sources, 1);
        assert_eq!(delta.stats.full_scans, before.stats.full_scans);
        assert_eq!(
            delta.stats.scanned_entries - before.stats.scanned_entries,
            1
        );
        assert_eq!(delta.tags.get("shared"), Some(&999));
        assert_eq!(delta.tags.get("newtag"), Some(&1));
        assert_eq!(delta.documents[0].title, "Changed");
        vault.metadata_queue(&["N0500.md".into()], false);
        drain(&vault);
        assert_eq!(
            vault
                .metadata_delta(None, None)
                .unwrap()
                .stats
                .parsed_sources,
            delta.stats.parsed_sources
        );
    }
    #[test]
    fn unresolved_and_ambiguous_candidates_follow_target_changes_without_reparse() {
        let root = tempfile::tempdir().unwrap();
        write(
            root.path(),
            "Source.md",
            "# Source\n\n[[Shared]] [[./A#Part]]\n",
        );
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let before = vault.metadata_delta(None, None).unwrap();
        assert_eq!(source(&before, "Source.md").links[0].resolved_path, None);
        vault
            .create_resource(None, "A", "md", "# Shared\n\n## Part\n")
            .unwrap();
        drain(&vault);
        let delta = vault
            .metadata_delta(Some(before.generation), Some(before.revision))
            .unwrap();
        assert_eq!(
            source(&delta, "Source.md").links[0]
                .resolved_path
                .as_deref(),
            Some("A.md")
        );
        assert_eq!(delta.stats.parsed_sources - before.stats.parsed_sources, 1);
        vault
            .create_resource(None, "B", "md", "# Shared\n")
            .unwrap();
        drain(&vault);
        assert_eq!(
            source(&vault.metadata_delta(None, None).unwrap(), "Source.md").links[0].resolved_path,
            None
        );
        vault.trash_entry("B.md").unwrap();
        drain(&vault);
        let full = vault.metadata_delta(None, None).unwrap();
        let document = source(&full, "Source.md");
        assert_eq!(document.links[0].resolved_path.as_deref(), Some("A.md"));
        assert_eq!(document.links[1].resolved_path.as_deref(), Some("A.md"));
    }
    #[test]
    fn subtree_move_delete_and_private_replacement_remove_old_metadata() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "Old/A.md", "# A\n#old\n");
        write(root.path(), "Old/B.md", "# B\n[[A]]\n");
        write(root.path(), "Oldish/C.md", "# C\n");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        vault.rename_entry("Old", "New", false).unwrap();
        drain(&vault);
        let full = vault.metadata_delta(None, None).unwrap();
        assert!(
            full.documents
                .iter()
                .all(|doc| !doc.path.starts_with("Old/"))
        );
        assert_eq!(full.documents.len(), 3);
        assert_eq!(
            source(&full, "New/B.md").links[0].resolved_path.as_deref(),
            Some("New/A.md")
        );
        fs::rename(root.path().join("New"), root.path().join(".hidden")).unwrap();
        vault.metadata_queue(&["New".into(), ".hidden".into()], false);
        drain(&vault);
        let full = vault.metadata_delta(None, None).unwrap();
        assert_eq!(full.documents.len(), 1);
        assert_eq!(full.documents[0].path, "Oldish/C.md");
        assert!(!full.tags.contains_key("old"));
    }
    #[test]
    fn overflow_reconciliation_discovers_changes_but_does_not_parse_unchanged_sources() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "Keep.md", "# Keep\n");
        write(root.path(), "Gone.md", "# Gone\n");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let before = vault.metadata_delta(None, None).unwrap();
        fs::remove_file(root.path().join("Gone.md")).unwrap();
        write(root.path(), "New.md", "# New\n");
        vault.metadata_queue(&[], true);
        drain(&vault);
        let delta = vault
            .metadata_delta(Some(before.generation), Some(before.revision))
            .unwrap();
        assert_eq!(delta.stats.full_scans, before.stats.full_scans + 1);
        assert_eq!(delta.stats.parsed_sources, before.stats.parsed_sources + 1);
        assert!(delta.removed_documents.contains(&"Gone.md".into()));
        assert!(delta.documents.iter().any(|doc| doc.path == "New.md"));
    }
    #[test]
    fn independent_lifetimes_reject_old_generation_even_for_same_root() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "A.md", "# A\n");
        let first = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&first);
        let a = first.metadata_delta(None, None).unwrap();
        let second = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&second);
        let b = second
            .metadata_delta(Some(a.generation), Some(a.revision))
            .unwrap();
        assert!(b.reset);
        assert_ne!(a.generation, b.generation);
        assert!(
            !first
                .metadata_delta(Some(a.generation), Some(a.revision))
                .unwrap()
                .reset
        );
    }
    #[test]
    fn denied_or_oversized_source_never_leaves_a_stale_document() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "A.md", "# A\n#secret\n");
        write(root.path(), "token.txt", "credential");
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        fs::remove_file(root.path().join("A.md")).unwrap();
        fs::create_dir(root.path().join("A.md")).unwrap();
        vault.metadata_queue(
            &["A.md".into(), "../outside.md".into(), "token.txt".into()],
            false,
        );
        drain(&vault);
        let full = vault.metadata_delta(None, None).unwrap();
        assert!(full.documents.is_empty());
        assert!(!full.tags.contains_key("secret"));
        assert!(!full.entries.iter().any(|entry| entry.path == "token.txt"));
        fs::remove_dir(root.path().join("A.md")).unwrap();
        let file = fs::File::create(root.path().join("A.md")).unwrap();
        file.set_len(SOURCE_LIMIT + 1).unwrap();
        vault.metadata_queue(&["A.md".into()], false);
        drain(&vault);
        let full = vault.metadata_delta(None, None).unwrap();
        assert_eq!(full.stats.skipped_sources, 1);
        assert!(full.documents.is_empty());
    }
    #[test]
    fn operational_task_updates_share_the_document_revision_stream() {
        let root = tempfile::tempdir().unwrap();
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        drain(&vault);
        let created = vault.create_task("Managed", Some("2026-10-19")).unwrap();
        let task = created
            .index
            .tasks
            .iter()
            .find(|task| task.managed)
            .unwrap();
        let before = vault.metadata_delta(None, None).unwrap();
        vault
            .update_task(
                &task.id,
                &task.path,
                1,
                None,
                Some(true),
                None,
                false,
                None,
                false,
            )
            .unwrap();
        let delta = vault
            .metadata_delta(Some(before.generation), Some(before.revision))
            .unwrap();
        assert!(
            delta
                .tasks
                .iter()
                .any(|item| item.id == task.id && item.completed)
        );
        assert_eq!(delta.stats.parsed_sources, before.stats.parsed_sources);
    }
}
