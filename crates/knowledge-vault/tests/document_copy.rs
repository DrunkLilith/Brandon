use knowledge_vault::{Vault, VaultError};
use std::fs;

#[test]
fn creates_exact_markdown_recovery_copy_without_clobbering_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    let draft = "# Черновик\r\n\r\nТекст, `код`, [[ссылка]] и незавершённый ввод";
    let copy = vault
        .create_resource(None, "Восстановленная копия", "md", draft)
        .unwrap();
    assert_eq!(copy.path, "Восстановленная копия.md");
    assert_eq!(vault.read_note(&copy.path).unwrap().content, draft);
    assert!(matches!(
        vault.create_resource(None, "Восстановленная копия", "md", "must not replace"),
        Err(VaultError::AlreadyExists)
    ));
    assert_eq!(
        fs::read_to_string(directory.path().join(&copy.path)).unwrap(),
        draft
    );
    assert!(
        vault
            .create_resource(Some("../"), "Unsafe", "md", draft)
            .is_err()
    );
    assert!(vault.create_resource(None, "Unsafe", "exe", draft).is_err());
}

#[test]
fn recovery_copy_keeps_canvas_bytes_and_relative_references_without_reformatting() {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    let draft = "{\n\"nodes\":[{\"type\":\"file\",\"file\":\"Photo.png\"}],\"edges\":[],\"future\":123456789012345678901234567890\n}";
    let copy = vault
        .create_resource(None, "Recovered board", "canvas", draft)
        .unwrap();
    assert_eq!(vault.read_resource(&copy.path).unwrap().content, draft);
    assert!(matches!(
        vault.create_resource(None, "Recovered board", "canvas", "nope"),
        Err(VaultError::AlreadyExists)
    ));
    assert_eq!(vault.read_resource(&copy.path).unwrap().content, draft);
}
