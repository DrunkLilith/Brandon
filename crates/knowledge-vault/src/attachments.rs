use super::{
    Result, Vault, VaultError, modified_unix_ms, normalized_relative, relative_display,
    safe_entry_name,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{self, Cursor, Read, Write},
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

const MAX_IMPORT_BYTES: u64 = 20 * 1024 * 1024 * 1024;
const MAX_PREVIEW_BYTES: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentKind {
    Image,
    Audio,
    Video,
    Pdf,
    File,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentInfo {
    pub path: String,
    pub name: String,
    pub kind: AttachmentKind,
    pub size_bytes: u64,
    pub modified_unix_ms: u128,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentImport {
    pub attachment: AttachmentInfo,
    pub copied: bool,
}

fn io_error(context: &'static str) -> impl FnOnce(io::Error) -> VaultError {
    move |source| VaultError::Io { context, source }
}

pub(crate) fn private_name(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    value.starts_with('.')
        || matches!(
            value.as_str(),
            "token.txt" | "id_rsa" | "id_ed25519" | "credentials.json"
        )
        || value.ends_with(".pem")
        || value.ends_with(".key")
        || value.ends_with(".pfx")
        || value.ends_with(".p12")
}

pub(crate) fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

fn sniff(bytes: &[u8]) -> AttachmentKind {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF8")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
    {
        return AttachmentKind::Image;
    }
    if bytes.starts_with(b"%PDF-") {
        return AttachmentKind::Pdf;
    }
    if bytes.get(4..8) == Some(b"ftyp") || bytes.starts_with(b"\x1a\x45\xdf\xa3") {
        return AttachmentKind::Video;
    }
    if bytes.starts_with(b"ID3")
        || bytes.starts_with(b"fLaC")
        || bytes.starts_with(b"OggS")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WAVE"))
    {
        return AttachmentKind::Audio;
    }
    AttachmentKind::File
}

fn cancelled(flag: &AtomicBool) -> Result<()> {
    if flag.load(Ordering::Relaxed) {
        Err(VaultError::InvalidPath("Импорт отменён"))
    } else {
        Ok(())
    }
}

fn hash_file(path: &Path, cancel: &AtomicBool) -> Result<String> {
    let mut file = fs::File::open(path).map_err(io_error("Не удалось проверить вложение"))?;
    let mut buffer = [0_u8; 128 * 1024];
    let mut hash = Sha256::new();
    loop {
        cancelled(cancel)?;
        let count = file
            .read(&mut buffer)
            .map_err(io_error("Не удалось прочитать вложение"))?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(hex::encode(hash.finalize()))
}

impl Vault {
    pub(crate) fn public_attachment_path(&self, relative: &str) -> Result<PathBuf> {
        let relative = normalized_relative(relative, false)?;
        let mut current = self.root.clone();
        for component in relative.components() {
            let Component::Normal(part) = component else {
                return Err(VaultError::InvalidPath("Некорректный путь вложения"));
            };
            if private_name(&part.to_string_lossy()) {
                return Err(VaultError::InvalidPath(
                    "Служебные файлы и ключи недоступны как вложения",
                ));
            }
            current.push(part);
            if is_link(&fs::symlink_metadata(&current).map_err(io_error("Вложение недоступно"))?)
            {
                return Err(VaultError::InvalidPath(
                    "Ссылки и junction не открываются как вложения",
                ));
            }
        }
        let canonical = self.secure_existing_entry(&relative.to_string_lossy())?;
        if !canonical.is_file() {
            return Err(VaultError::InvalidPath("Ожидается обычный файл"));
        }
        Ok(canonical)
    }

    pub fn attachment_info(&self, relative: &str) -> Result<AttachmentInfo> {
        let path = self.public_attachment_path(relative)?;
        let mut file = fs::File::open(&path).map_err(io_error("Не удалось открыть вложение"))?;
        let metadata = file
            .metadata()
            .map_err(io_error("Не удалось прочитать свойства вложения"))?;
        let mut header = [0_u8; 32];
        let count = file
            .read(&mut header)
            .map_err(io_error("Не удалось прочитать тип вложения"))?;
        Ok(AttachmentInfo {
            path: relative_display(&self.root, &path)?,
            name: path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
            kind: sniff(&header[..count]),
            size_bytes: metadata.len(),
            modified_unix_ms: modified_unix_ms(&metadata),
        })
    }

    /// Explicit user import only. Stream to a hidden staging file, then publish
    /// without clobbering an existing name. Never deserialize bytes through JSON.
    pub fn import_attachment(
        &self,
        source: &Path,
        cancel: &AtomicBool,
        mut progress: impl FnMut(u64, u64),
    ) -> Result<AttachmentImport> {
        cancelled(cancel)?;
        let source_metadata =
            fs::symlink_metadata(source).map_err(io_error("Исходный файл недоступен"))?;
        if is_link(&source_metadata) || !source_metadata.is_file() {
            return Err(VaultError::InvalidPath(
                "Выберите обычный файл, не папку или ссылку",
            ));
        }
        let source = source
            .canonicalize()
            .map_err(io_error("Исходный файл недоступен"))?;
        let name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(VaultError::InvalidName("Некорректное имя вложения"))?;
        if source.components().any(|component| {
            matches!(component, Component::Normal(part) if private_name(&part.to_string_lossy()))
        }) {
            return Err(VaultError::InvalidPath(
                "Служебные файлы и ключи не импортируются автоматически",
            ));
        }
        if source.starts_with(&self.root) {
            return Ok(AttachmentImport {
                attachment: self.attachment_info(&relative_display(&self.root, &source)?)?,
                copied: false,
            });
        }
        if source_metadata.len() > MAX_IMPORT_BYTES {
            return Err(VaultError::InvalidPath(
                "Первый импорт ограничен файлами до 20 ГиБ",
            ));
        }
        let name = safe_entry_name(name)?;
        let directory = self.root.join("Attachments");
        match fs::create_dir(&directory) {
            Ok(()) => (),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
            Err(source) => {
                return Err(VaultError::Io {
                    context: "Не удалось создать папку вложений",
                    source,
                });
            }
        }
        if is_link(
            &fs::symlink_metadata(&directory).map_err(io_error("Папка вложений недоступна"))?,
        ) {
            return Err(VaultError::InvalidPath(
                "Папка Attachments не должна быть ссылкой или junction",
            ));
        }
        let directory = self.secure_existing_directory("Attachments")?;
        let mut source_file =
            fs::File::open(source).map_err(io_error("Не удалось открыть исходный файл"))?;
        let before = source_file
            .metadata()
            .map_err(io_error("Не удалось проверить исходный файл"))?;
        let mut staging = tempfile::Builder::new()
            .prefix(".brandon-import-")
            .tempfile_in(&directory)
            .map_err(io_error("Не удалось подготовить импорт"))?;
        let mut buffer = [0_u8; 128 * 1024];
        let mut hash = Sha256::new();
        let mut copied = 0_u64;
        progress(0, before.len());
        loop {
            cancelled(cancel)?;
            let count = source_file
                .read(&mut buffer)
                .map_err(io_error("Не удалось прочитать исходный файл"))?;
            if count == 0 {
                break;
            }
            copied += count as u64;
            if copied > MAX_IMPORT_BYTES {
                return Err(VaultError::InvalidPath("Вложение превышает лимит импорта"));
            }
            staging
                .write_all(&buffer[..count])
                .map_err(io_error("Не удалось скопировать вложение"))?;
            hash.update(&buffer[..count]);
            progress(copied, before.len());
        }
        let after = source_file
            .metadata()
            .map_err(io_error("Не удалось проверить исходный файл"))?;
        if copied != before.len()
            || after.len() != before.len()
            || after.modified().ok() != before.modified().ok()
        {
            return Err(VaultError::InvalidPath(
                "Исходный файл изменился во время импорта; повторите попытку",
            ));
        }
        staging
            .as_file()
            .sync_all()
            .map_err(io_error("Не удалось сохранить вложение"))?;
        let digest = hex::encode(hash.finalize());
        let original = Path::new(&name);
        for suffix in 0..10_000 {
            cancelled(cancel)?;
            let name = if suffix == 0 {
                name.clone()
            } else {
                let stem = original.file_stem().unwrap_or_default().to_string_lossy();
                match original
                    .extension()
                    .and_then(|extension| extension.to_str())
                {
                    Some(extension) => format!("{stem} ({}).{extension}", suffix + 1),
                    None => format!("{stem} ({})", suffix + 1),
                }
            };
            let target = directory.join(&name);
            if let Ok(metadata) = fs::symlink_metadata(&target) {
                if !is_link(&metadata)
                    && metadata.is_file()
                    && metadata.len() == copied
                    && hash_file(&target, cancel)? == digest
                {
                    return Ok(AttachmentImport {
                        attachment: self
                            .attachment_info(&relative_display(&self.root, &target)?)?,
                        copied: false,
                    });
                }
                continue;
            }
            match staging.persist_noclobber(&target) {
                Ok(_) => {
                    return Ok(AttachmentImport {
                        attachment: self
                            .attachment_info(&relative_display(&self.root, &target)?)?,
                        copied: true,
                    });
                }
                Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                    staging = error.file;
                }
                Err(error) => {
                    return Err(VaultError::Io {
                        context: "Не удалось завершить импорт без перезаписи",
                        source: error.error,
                    });
                }
            }
        }
        Err(VaultError::InvalidName(
            "Слишком много файлов с одинаковым именем",
        ))
    }

    /// Normalized PNG, never SVG/HTML or original unbounded media bytes.
    pub fn attachment_thumbnail(&self, relative: &str) -> Result<Vec<u8>> {
        let path = self.public_attachment_path(relative)?;
        let metadata = fs::metadata(&path).map_err(io_error("Вложение недоступно"))?;
        if metadata.len() > MAX_PREVIEW_BYTES {
            return Err(VaultError::InvalidPath(
                "Файл слишком большой для быстрого превью; оригинал сохранён",
            ));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)
            .map_err(io_error("Не удалось открыть изображение"))?
            .take(MAX_PREVIEW_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(io_error("Не удалось прочитать изображение"))?;
        if bytes.len() as u64 > MAX_PREVIEW_BYTES {
            return Err(VaultError::InvalidPath(
                "Изображение выросло и превышает лимит превью",
            ));
        }
        if sniff(&bytes) != AttachmentKind::Image {
            return Err(VaultError::InvalidPath(
                "Это не поддерживаемое растровое изображение",
            ));
        }
        let mut reader = image::ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(io_error("Не удалось определить изображение"))?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(8192);
        limits.max_image_height = Some(8192);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let image = reader.decode().map_err(|_| {
            VaultError::InvalidPath("Изображение повреждено или превышает лимиты превью")
        })?;
        let mut result = Cursor::new(Vec::new());
        image
            .thumbnail(640, 400)
            .write_to(&mut result, image::ImageFormat::Png)
            .map_err(|_| VaultError::InvalidPath("Не удалось подготовить превью"))?;
        Ok(result.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn setup() -> (tempfile::TempDir, tempfile::TempDir, Vault) {
        let source = tempfile::Builder::new()
            .prefix("brandon-attachment-source-")
            .tempdir()
            .unwrap();
        let destination = tempfile::Builder::new()
            .prefix("brandon-attachment-vault-")
            .tempdir()
            .unwrap();
        let vault = Vault::open(destination.path()).unwrap();
        (source, destination, vault)
    }
    #[test]
    fn imports_reuses_and_handles_name_collisions_without_overwriting() {
        let (source, _destination, vault) = setup();
        let path = source.path().join("Документ.pdf");
        fs::write(&path, b"%PDF-test").unwrap();
        let flag = AtomicBool::new(false);
        let first = vault.import_attachment(&path, &flag, |_, _| {}).unwrap();
        assert!(first.copied);
        assert_eq!(first.attachment.kind, AttachmentKind::Pdf);
        assert!(
            !vault
                .import_attachment(&path, &flag, |_, _| {})
                .unwrap()
                .copied
        );
        fs::write(&path, b"different").unwrap();
        let second = vault.import_attachment(&path, &flag, |_, _| {}).unwrap();
        assert!(second.attachment.path.ends_with(" (2).pdf"));
        assert_eq!(
            fs::read(vault.absolute_entry_path(&first.attachment.path).unwrap()).unwrap(),
            b"%PDF-test"
        );
    }
    #[test]
    fn cancellation_cleans_staging_and_never_publishes_partial_data() {
        let (source, destination, vault) = setup();
        let path = source.path().join("large.bin");
        fs::write(&path, vec![1_u8; 500_000]).unwrap();
        let flag = AtomicBool::new(false);
        assert!(
            vault
                .import_attachment(&path, &flag, |count, _| {
                    if count > 0 {
                        flag.store(true, Ordering::Relaxed);
                    }
                })
                .is_err()
        );
        assert_eq!(
            fs::read_dir(destination.path().join("Attachments"))
                .unwrap()
                .count(),
            0
        );
    }
    #[test]
    fn in_vault_objects_stay_references_and_private_paths_are_denied() {
        let (_source, destination, vault) = setup();
        fs::write(destination.path().join("existing.txt"), "hello").unwrap();
        let result = vault
            .import_attachment(
                &destination.path().join("existing.txt"),
                &AtomicBool::new(false),
                |_, _| {},
            )
            .unwrap();
        assert!(!result.copied);
        assert_eq!(result.attachment.path, "existing.txt");
        fs::write(
            destination.path().join("token.txt"),
            "synthetic-not-a-secret",
        )
        .unwrap();
        assert!(vault.attachment_info("token.txt").is_err());
        assert!(vault.attachment_info("../escape.txt").is_err());
        let private_directory = _source.path().join(".private");
        fs::create_dir(&private_directory).unwrap();
        let private_file = private_directory.join("ordinary.txt");
        fs::write(&private_file, "synthetic private fixture").unwrap();
        assert!(
            vault
                .import_attachment(&private_file, &AtomicBool::new(false), |_, _| {})
                .is_err()
        );
    }
    #[test]
    fn thumbnails_are_bounded_png_and_active_formats_are_not_rendered() {
        let (_source, destination, vault) = setup();
        let image = image::DynamicImage::new_rgb8(1000, 800);
        image.save(destination.path().join("image.png")).unwrap();
        let thumbnail = vault.attachment_thumbnail("image.png").unwrap();
        let decoded = image::load_from_memory(&thumbnail).unwrap();
        assert!(decoded.width() <= 640 && decoded.height() <= 400);
        fs::write(
            destination.path().join("unsafe.svg"),
            b"<svg><script/></svg>",
        )
        .unwrap();
        assert_eq!(
            vault.attachment_info("unsafe.svg").unwrap().kind,
            AttachmentKind::File
        );
        assert!(vault.attachment_thumbnail("unsafe.svg").is_err());
        fs::write(
            destination.path().join("broken.png"),
            b"\x89PNG\r\n\x1a\nbroken",
        )
        .unwrap();
        assert!(vault.attachment_thumbnail("broken.png").is_err());
    }
}
