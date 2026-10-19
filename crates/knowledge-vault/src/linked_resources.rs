//! Bounded public text references for board Document / working-format peeks.
use super::*;
use std::io::Read;
pub const LINKED_TEXT_LIMIT: usize = 2 * 1024 * 1024;
impl Vault {
    pub fn read_linked_resource(&self, relative: &str) -> Result<NoteDocument> {
        if ![".md", ".base"]
            .iter()
            .any(|ext| relative.to_lowercase().ends_with(ext))
        {
            return Err(VaultError::InvalidPath(
                "Связанное текстовое представление требует .md или .base",
            ));
        }
        let path = self.public_attachment_path(relative)?;
        let mut file = fs::File::open(&path).map_err(|source| VaultError::Io {
            context: "Связанный источник недоступен",
            source,
        })?;
        let mut bytes = Vec::new();
        std::io::Read::by_ref(&mut file)
            .take(LINKED_TEXT_LIMIT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| VaultError::Io {
                context: "Не удалось прочитать связанный источник",
                source,
            })?;
        if bytes.len() > LINKED_TEXT_LIMIT {
            return Err(VaultError::InvalidPath(
                "Для встроенного просмотра — не более 2 МиБ; откройте исходник отдельно",
            ));
        }
        let digest = fingerprint(&bytes);
        let content = String::from_utf8(bytes)
            .map_err(|_| VaultError::InvalidPath("Связанный источник должен быть UTF-8"))?;
        let meta = file.metadata().map_err(|source| VaultError::Io {
            context: "Связанный источник недоступен",
            source,
        })?;
        Ok(NoteDocument {
            path: relative_display(&self.root, &path)?,
            content,
            fingerprint: digest,
            modified_unix_ms: modified_unix_ms(&meta),
        })
    }
    pub fn save_linked_resource(
        &self,
        relative: &str,
        expected: &str,
        contents: &str,
    ) -> Result<SaveOutcome> {
        if contents.len() > LINKED_TEXT_LIMIT
            || expected.len() != 64
            || !expected.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(VaultError::InvalidPath(
                "Правка превышает 2 МиБ или не содержит fingerprint просмотренного источника",
            ));
        }
        let disk = self.read_linked_resource(relative)?;
        if disk.fingerprint != expected {
            return Ok(SaveOutcome::Conflict { disk });
        }
        if relative.to_lowercase().ends_with(".md") {
            self.save_note(relative, contents, Some(expected))
        } else {
            self.save_resource(relative, contents, Some(expected))
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linked_edit_uses_original_bytes_and_conflicts_instead_of_overwriting() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        for name in ["Document.md", "Tasks.base"] {
            let original = "foreign: {keep: 73}\r\n# Связанный источник\r\n";
            fs::write(directory.path().join(name), original).unwrap();
            let document = vault.read_linked_resource(name).unwrap();
            assert_eq!(document.content, original);
            fs::write(directory.path().join(name), "external change").unwrap();
            assert!(matches!(
                vault
                    .save_linked_resource(name, &document.fingerprint, "draft")
                    .unwrap(),
                SaveOutcome::Conflict { .. }
            ));
            assert_eq!(
                fs::read_to_string(directory.path().join(name)).unwrap(),
                "external change"
            );
            let fresh = vault.read_linked_resource(name).unwrap();
            assert!(matches!(
                vault
                    .save_linked_resource(name, &fresh.fingerprint, original)
                    .unwrap(),
                SaveOutcome::Saved { .. }
            ));
            assert_eq!(
                fs::read_to_string(directory.path().join(name)).unwrap(),
                original
            );
        }
    }
    #[test]
    fn linked_sources_are_bounded_public_utf8_md_or_base_and_reads_create_no_state() {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        fs::write(directory.path().join("Note.md"), "note").unwrap();
        vault.read_linked_resource("Note.md").unwrap();
        assert!(!directory.path().join(".brandon").exists());
        assert!(vault.read_linked_resource("../Note.md").is_err());
        assert!(vault.read_linked_resource("Note.png").is_err());
        assert!(vault.save_linked_resource("Note.md", "", "bad").is_err());
        fs::create_dir(directory.path().join(".hidden")).unwrap();
        fs::write(directory.path().join(".hidden/Note.md"), "secret").unwrap();
        assert!(vault.read_linked_resource(".hidden/Note.md").is_err());
        fs::write(
            directory.path().join("Big.md"),
            vec![b'x'; LINKED_TEXT_LIMIT + 1],
        )
        .unwrap();
        assert!(vault.read_linked_resource("Big.md").is_err());
        fs::write(directory.path().join("Bad.md"), [0xff, 0xfe]).unwrap();
        assert!(vault.read_linked_resource("Bad.md").is_err());
        let doc = vault.read_linked_resource("Note.md").unwrap();
        assert!(
            vault
                .save_linked_resource(
                    "Note.md",
                    &doc.fingerprint,
                    &"x".repeat(LINKED_TEXT_LIMIT + 1)
                )
                .is_err()
        );
        assert_eq!(
            fs::read_to_string(directory.path().join("Note.md")).unwrap(),
            "note"
        );
    }
}
