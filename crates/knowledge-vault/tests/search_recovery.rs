use knowledge_vault::{SearchMode, Vault};
use std::fs;

fn fixture() -> (tempfile::TempDir, Vault) {
    let directory = tempfile::tempdir().unwrap();
    let vault = Vault::open(directory.path()).unwrap();
    fs::write(
        directory.path().join("Note.md"),
        "# Recovery\n\nrecoverneedle\n",
    )
    .unwrap();
    vault.create_task("Synthetic task", None).unwrap();
    (directory, vault)
}

#[test]
fn explicit_rebuild_preserves_corrupt_cache_sources_and_canonical_tasks() {
    let (directory, vault) = fixture();
    let private = directory.path().join(".brandon");
    let task_bytes = fs::read(private.join("tasks.sqlite3")).unwrap();
    let note_bytes = fs::read(directory.path().join("Note.md")).unwrap();
    let corrupt = b"SYNTHETIC_CORRUPT_DERIVATIVE_INDEX";
    fs::write(private.join("search-v1.sqlite3"), corrupt).unwrap();
    assert!(
        vault
            .search_local("recoverneedle", SearchMode::Words, 20)
            .is_err()
    );
    // SQLite itself may clean an invalid journal on a normal open. These files
    // are installed after the failed query to check the explicit rebuild only.
    fs::write(private.join("search-v1.sqlite3-wal"), b"synthetic wal").unwrap();
    fs::write(private.join("search-v1.sqlite3-shm"), b"synthetic shm").unwrap();
    fs::write(
        private.join("search-v1.sqlite3-journal"),
        b"synthetic journal",
    )
    .unwrap();
    fs::write(private.join("unrelated.txt"), b"untouched").unwrap();
    let rebuilt = vault
        .rebuild_search("recoverneedle", SearchMode::Words, 20)
        .unwrap();
    assert_eq!(rebuilt.search.hits.len(), 1);
    let backup = directory.path().join(rebuilt.backup_path.unwrap());
    assert_eq!(fs::read(backup.join("search-v1.sqlite3")).unwrap(), corrupt);
    for suffix in ["wal", "shm", "journal"] {
        assert!(backup.join(format!("search-v1.sqlite3-{suffix}")).is_file());
    }
    assert_eq!(fs::read(private.join("tasks.sqlite3")).unwrap(), task_bytes);
    assert_eq!(
        fs::read(directory.path().join("Note.md")).unwrap(),
        note_bytes
    );
    assert_eq!(
        fs::read(private.join("unrelated.txt")).unwrap(),
        b"untouched"
    );
    assert_eq!(
        vault
            .search_local("recoverneedle", SearchMode::Words, 20)
            .unwrap()
            .changed_sources,
        0
    );
    let again = vault
        .rebuild_search("recoverneedle", SearchMode::Literal, 20)
        .unwrap();
    assert_ne!(
        Some(backup),
        again.backup_path.map(|path| directory.path().join(path))
    );
}

#[test]
fn canonical_task_failure_is_not_repaired_or_quarantined_as_search_cache() {
    let (directory, vault) = fixture();
    let private = directory.path().join(".brandon");
    fs::write(private.join("tasks.sqlite3"), b"SYNTHETIC_BROKEN_TASK_DB").unwrap();
    fs::write(private.join("search-v1.sqlite3"), b"preserve cache").unwrap();
    let error = vault
        .rebuild_search("recoverneedle", SearchMode::Words, 20)
        .unwrap_err()
        .to_string();
    assert!(error.contains("База задач"), "{error}");
    assert_eq!(
        fs::read(private.join("search-v1.sqlite3")).unwrap(),
        b"preserve cache"
    );
    assert_eq!(
        fs::read(private.join("tasks.sqlite3")).unwrap(),
        b"SYNTHETIC_BROKEN_TASK_DB"
    );
    assert!(!fs::read_dir(private).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("search-recovery-")
    }));
}

#[test]
fn invalid_query_and_non_file_cache_fail_without_moving_anything() {
    let (directory, vault) = fixture();
    let cache = directory.path().join(".brandon/search-v1.sqlite3");
    fs::write(&cache, b"corrupt cache").unwrap();
    assert!(vault.rebuild_search("", SearchMode::Words, 20).is_err());
    assert!(
        vault
            .rebuild_search("\"unfinished", SearchMode::Words, 20)
            .is_err()
    );
    assert_eq!(fs::read(&cache).unwrap(), b"corrupt cache");
    fs::remove_file(&cache).unwrap();
    fs::create_dir(&cache).unwrap();
    assert!(
        vault
            .rebuild_search("recoverneedle", SearchMode::Words, 20)
            .is_err()
    );
    assert!(cache.is_dir());
}

#[cfg(windows)]
#[test]
fn locked_sidecar_reports_partial_backup_without_losing_any_old_bytes() {
    use std::os::windows::fs::OpenOptionsExt;
    let (directory, vault) = fixture();
    let private = directory.path().join(".brandon");
    fs::write(private.join("search-v1.sqlite3"), b"preserve database").unwrap();
    let wal = private.join("search-v1.sqlite3-wal");
    fs::write(&wal, b"preserve locked wal").unwrap();
    let locked = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&wal)
        .unwrap();
    let error = vault
        .rebuild_search("recoverneedle", SearchMode::Words, 20)
        .unwrap_err();
    let knowledge_vault::VaultError::SearchRecovery { backup, .. } = error else {
        panic!("Expected actionable recovery error, got {error}");
    };
    assert_eq!(
        fs::read(directory.path().join(backup).join("search-v1.sqlite3")).unwrap(),
        b"preserve database"
    );
    drop(locked);
    assert_eq!(fs::read(wal).unwrap(), b"preserve locked wal");
    assert!(!private.join("search-v1.sqlite3").exists());
}

#[cfg(unix)]
#[test]
fn linked_cache_directory_and_companions_are_never_followed_or_moved() {
    use std::os::unix::fs::symlink;
    let (directory, vault) = fixture();
    let outside = tempfile::tempdir().unwrap();
    let private = directory.path().join(".brandon");
    let cache = private.join("search-v1.sqlite3");
    fs::write(&cache, b"cache stays").unwrap();
    for name in [
        "search-v1.sqlite3-wal",
        "search-v1.sqlite3-shm",
        "search-v1.sqlite3-journal",
        "tasks.sqlite3-wal",
    ] {
        symlink(outside.path().join("absent"), private.join(name)).unwrap();
        assert!(
            vault
                .rebuild_search("recoverneedle", SearchMode::Words, 20)
                .is_err()
        );
        assert_eq!(fs::read(&cache).unwrap(), b"cache stays");
        fs::remove_file(private.join(name)).unwrap();
    }
    fs::rename(&private, directory.path().join("original-private")).unwrap();
    symlink(outside.path(), &private).unwrap();
    assert!(
        vault
            .rebuild_search("recoverneedle", SearchMode::Words, 20)
            .is_err()
    );
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
}
