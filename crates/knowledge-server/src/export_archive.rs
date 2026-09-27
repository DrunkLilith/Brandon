//! Bounded deterministic export archive layout and stream encryption.
//!
//! Original names remain data inside the encrypted archive. Every tar path is
//! derived exclusively from UUIDs, so untrusted titles and filenames can never
//! affect archive paths.
use std::collections::HashSet;

use bytes::{Bytes, BytesMut};
use knowledge_crypto::{
    CryptoError, KeyVersion, PurposeKey,
    stream::{AttachmentStream, STREAM_CHUNK_SIZE},
};
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;
use uuid::Uuid;

const TAR_BLOCK: u64 = 512;
const TAR_END: u64 = 1024;

#[derive(Clone, Copy)]
pub struct ArchiveLimits {
    pub max_pages: usize,
    pub max_attachments: usize,
    pub max_page_metadata_bytes: usize,
    pub max_manifest_bytes: usize,
    pub max_archive_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_pages: 1_000,
            max_attachments: 1_000,
            max_page_metadata_bytes: 64 * 1024,
            max_manifest_bytes: 4 * 1024 * 1024,
            max_archive_bytes: 1024 * 1024 * 1024,
        }
    }
}

#[derive(Clone)]
pub struct ArchiveInput {
    pub space_id: Uuid,
    pub space_name: String,
    pub pages: Vec<PageInput>,
    pub attachments: Vec<AttachmentInput>,
}

#[derive(Clone)]
pub struct PageInput {
    pub page_id: Uuid,
    pub stable_key: Uuid,
    pub parent_page_id: Option<Uuid>,
    pub revision_id: Uuid,
    pub revision: i64,
    pub title: String,
    pub properties: Value,
    pub markdown_size: u64,
}

#[derive(Clone)]
pub struct AttachmentInput {
    pub attachment_id: Uuid,
    pub page_id: Uuid,
    pub original_name: String,
    pub media_type: String,
    pub size: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceId {
    PageMarkdown(Uuid),
    Attachment(Uuid),
}

pub enum TarPiece {
    Bytes(Bytes),
    Source { id: SourceId, size: u64 },
}

enum Payload {
    Page {
        id: Uuid,
        frontmatter: Bytes,
        markdown_size: u64,
    },
    Attachment {
        id: Uuid,
        size: u64,
    },
    Static(Bytes),
}

struct Entry {
    path: String,
    size: u64,
    payload: Payload,
}

pub struct ArchivePlan {
    entries: Vec<Entry>,
    archive_size: u64,
}

#[derive(Debug, Error)]
#[error("export archive input is invalid or exceeds a configured limit")]
pub struct ArchiveError;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Frontmatter<'a> {
    page_id: Uuid,
    stable_key: Uuid,
    parent_page_id: Option<Uuid>,
    revision_id: Uuid,
    revision: i64,
    title: &'a str,
    properties: &'a Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest<'a> {
    format: &'static str,
    space: ManifestSpace<'a>,
    pages: Vec<ManifestPage<'a>>,
    attachments: Vec<ManifestAttachment<'a>>,
}

#[derive(Serialize)]
struct ManifestSpace<'a> {
    id: Uuid,
    name: &'a str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestPage<'a> {
    page_id: Uuid,
    stable_key: Uuid,
    parent_page_id: Option<Uuid>,
    revision_id: Uuid,
    revision: i64,
    title: &'a str,
    path: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestAttachment<'a> {
    attachment_id: Uuid,
    page_id: Uuid,
    original_name: &'a str,
    media_type: &'a str,
    size: u64,
    path: String,
}

impl ArchivePlan {
    pub fn new(mut input: ArchiveInput, limits: ArchiveLimits) -> Result<Self, ArchiveError> {
        if input.pages.len() > limits.max_pages
            || input.attachments.len() > limits.max_attachments
            || input.space_name.len() > 1024
        {
            return Err(ArchiveError);
        }
        input.pages.sort_by_key(|page| page.page_id);
        input
            .attachments
            .sort_by_key(|attachment| attachment.attachment_id);
        let page_ids: HashSet<_> = input.pages.iter().map(|page| page.page_id).collect();
        let stable_keys: HashSet<_> = input.pages.iter().map(|page| page.stable_key).collect();
        let revision_ids: HashSet<_> = input.pages.iter().map(|page| page.revision_id).collect();
        let attachment_ids: HashSet<_> = input
            .attachments
            .iter()
            .map(|item| item.attachment_id)
            .collect();
        if page_ids.len() != input.pages.len()
            || stable_keys.len() != input.pages.len()
            || revision_ids.len() != input.pages.len()
            || attachment_ids.len() != input.attachments.len()
            || input.pages.iter().any(|page| {
                page.revision <= 0
                    || page.title.chars().count() > 500
                    || !page.properties.is_object()
                    || page
                        .parent_page_id
                        .is_some_and(|parent| !page_ids.contains(&parent))
            })
            || input.attachments.iter().any(|attachment| {
                !page_ids.contains(&attachment.page_id)
                    || attachment.original_name.chars().count() > 255
                    || attachment.media_type.len() > 255
            })
        {
            return Err(ArchiveError);
        }

        let root = format!("space-{}", input.space_id);
        let page_manifest: Vec<_> = input
            .pages
            .iter()
            .map(|page| ManifestPage {
                page_id: page.page_id,
                stable_key: page.stable_key,
                parent_page_id: page.parent_page_id,
                revision_id: page.revision_id,
                revision: page.revision,
                title: &page.title,
                path: format!("pages/{}.md", page.page_id),
            })
            .collect();
        let attachment_manifest: Vec<_> = input
            .attachments
            .iter()
            .map(|attachment| ManifestAttachment {
                attachment_id: attachment.attachment_id,
                page_id: attachment.page_id,
                original_name: &attachment.original_name,
                media_type: &attachment.media_type,
                size: attachment.size,
                path: format!("attachments/{}", attachment.attachment_id),
            })
            .collect();
        let mut manifest = serde_json::to_vec_pretty(&Manifest {
            format: "knowledge-platform-export/v1",
            space: ManifestSpace {
                id: input.space_id,
                name: &input.space_name,
            },
            pages: page_manifest,
            attachments: attachment_manifest,
        })
        .map_err(|_| ArchiveError)?;
        manifest.push(b'\n');
        if manifest.len() > limits.max_manifest_bytes {
            return Err(ArchiveError);
        }

        let mut entries = Vec::with_capacity(input.pages.len() + input.attachments.len() + 1);
        for page in &input.pages {
            let yaml = serde_json::to_vec(&Frontmatter {
                page_id: page.page_id,
                stable_key: page.stable_key,
                parent_page_id: page.parent_page_id,
                revision_id: page.revision_id,
                revision: page.revision,
                title: &page.title,
                properties: &page.properties,
            })
            .map_err(|_| ArchiveError)?;
            if yaml.len() > limits.max_page_metadata_bytes {
                return Err(ArchiveError);
            }
            let mut prefix = Vec::with_capacity(yaml.len() + 9);
            prefix.extend_from_slice(b"---\n");
            prefix.extend_from_slice(&yaml);
            prefix.extend_from_slice(b"\n---\n");
            let size = u64::try_from(prefix.len())
                .map_err(|_| ArchiveError)?
                .checked_add(page.markdown_size)
                .ok_or(ArchiveError)?;
            entries.push(Entry {
                path: format!("{root}/pages/{}.md", page.page_id),
                size,
                payload: Payload::Page {
                    id: page.page_id,
                    frontmatter: Bytes::from(prefix),
                    markdown_size: page.markdown_size,
                },
            });
        }
        for attachment in &input.attachments {
            entries.push(Entry {
                path: format!("{root}/attachments/{}", attachment.attachment_id),
                size: attachment.size,
                payload: Payload::Attachment {
                    id: attachment.attachment_id,
                    size: attachment.size,
                },
            });
        }
        entries.push(Entry {
            path: format!("{root}/manifest.json"),
            size: manifest.len() as u64,
            payload: Payload::Static(Bytes::from(manifest)),
        });
        let archive_size = entries
            .iter()
            .try_fold(TAR_END, |total, entry| {
                total
                    .checked_add(TAR_BLOCK)?
                    .checked_add(padded_size(entry.size))
            })
            .ok_or(ArchiveError)?;
        if archive_size > limits.max_archive_bytes {
            return Err(ArchiveError);
        }
        Ok(Self {
            entries,
            archive_size,
        })
    }

    #[must_use]
    pub const fn archive_size(&self) -> u64 {
        self.archive_size
    }

    pub fn pieces(&self) -> std::vec::IntoIter<TarPiece> {
        let mut pieces = Vec::with_capacity(self.entries.len() * 4 + 1);
        for entry in &self.entries {
            pieces.push(TarPiece::Bytes(Bytes::copy_from_slice(&tar_header(
                &entry.path,
                entry.size,
            ))));
            match &entry.payload {
                Payload::Page {
                    id,
                    frontmatter,
                    markdown_size,
                } => {
                    pieces.push(TarPiece::Bytes(frontmatter.clone()));
                    pieces.push(TarPiece::Source {
                        id: SourceId::PageMarkdown(*id),
                        size: *markdown_size,
                    });
                }
                Payload::Attachment { id, size } => pieces.push(TarPiece::Source {
                    id: SourceId::Attachment(*id),
                    size: *size,
                }),
                Payload::Static(bytes) => pieces.push(TarPiece::Bytes(bytes.clone())),
            }
            let padding = padded_size(entry.size) - entry.size;
            if padding > 0 {
                pieces.push(TarPiece::Bytes(Bytes::from(vec![0; padding as usize])));
            }
        }
        pieces.push(TarPiece::Bytes(Bytes::from_static(&[0; TAR_END as usize])));
        pieces.into_iter()
    }
}

const fn padded_size(size: u64) -> u64 {
    size.div_ceil(TAR_BLOCK) * TAR_BLOCK
}

fn tar_header(path: &str, size: u64) -> [u8; 512] {
    let mut header = [0_u8; 512];
    header[..path.len()].copy_from_slice(path.as_bytes());
    write_octal(&mut header[100..108], 0o644);
    write_octal(&mut header[108..116], 0);
    write_octal(&mut header[116..124], 0);
    write_octal(&mut header[124..136], size);
    write_octal(&mut header[136..148], 0);
    header[148..156].fill(b' ');
    header[156] = b'0';
    header[257..263].copy_from_slice(b"ustar\0");
    header[263..265].copy_from_slice(b"00");
    let checksum: u64 = header.iter().map(|byte| u64::from(*byte)).sum();
    let value = format!("{checksum:06o}\0 ");
    header[148..156].copy_from_slice(value.as_bytes());
    header
}

fn write_octal(field: &mut [u8], value: u64) {
    let digit_count = field.len() - 1;
    let digits = format!("{value:0digit_count$o}");
    field[..digit_count].copy_from_slice(digits.as_bytes());
    field[digit_count] = 0;
}

pub struct ArchiveEncryptor {
    stream: AttachmentStream,
    pending: BytesMut,
    declared_size: u64,
    consumed: u64,
    next_frame: u64,
    finished: bool,
}

impl ArchiveEncryptor {
    #[must_use]
    pub fn new(key: PurposeKey, workspace: Uuid, export: Uuid, declared_size: u64) -> Self {
        Self::with_version(key, KeyVersion(1), workspace, export, declared_size)
    }
    pub fn with_version(
        key: PurposeKey,
        version: KeyVersion,
        workspace: Uuid,
        export: Uuid,
        declared_size: u64,
    ) -> Self {
        Self {
            stream: AttachmentStream::new_export(
                key,
                version,
                workspace.as_bytes(),
                export.as_bytes(),
                declared_size,
            ),
            pending: BytesMut::with_capacity(STREAM_CHUNK_SIZE),
            declared_size,
            consumed: 0,
            next_frame: 0,
            finished: false,
        }
    }
    #[must_use]
    pub fn header(&self) -> &[u8] {
        self.stream.header()
    }
    #[must_use]
    pub fn ciphertext_size(&self) -> u64 {
        self.stream.ciphertext_size()
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<Vec<Vec<u8>>, CryptoError> {
        if self.finished
            || bytes.len() > STREAM_CHUNK_SIZE
            || self
                .consumed
                .checked_add(bytes.len() as u64)
                .is_none_or(|size| size > self.declared_size)
        {
            return Err(CryptoError::InvalidEnvelope);
        }
        self.pending.extend_from_slice(bytes);
        self.consumed += bytes.len() as u64;
        let mut frames = Vec::new();
        while self.pending.len() >= STREAM_CHUNK_SIZE
            || (self.consumed == self.declared_size && !self.pending.is_empty())
        {
            let length = self.pending.len().min(STREAM_CHUNK_SIZE);
            let chunk = self.pending.split_to(length);
            frames.push(self.stream.encrypt_chunk(self.next_frame, &chunk)?);
            self.next_frame += 1;
        }
        Ok(frames)
    }

    pub fn finish(&mut self) -> Result<Vec<u8>, CryptoError> {
        if self.finished || self.consumed != self.declared_size || !self.pending.is_empty() {
            return Err(CryptoError::InvalidEnvelope);
        }
        self.finished = true;
        self.stream.encrypt_footer()
    }
}
