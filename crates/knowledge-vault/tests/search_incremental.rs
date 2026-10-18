use knowledge_vault::{SearchMode, SearchUpdate, Vault, VaultError};
use std::{
    fs,
    sync::atomic::{AtomicUsize, Ordering},
};
const BUDGET: u64 = 128 * 1024 * 1024;
fn fixture() -> (tempfile::TempDir, Vault) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Note.md"), "# Note\n\noldneedle\n").unwrap();
    let vault = Vault::open(dir.path()).unwrap();
    vault
        .sync_search_batch(SearchUpdate::Full, BUDGET, &|| false)
        .unwrap();
    (dir, vault)
}
fn update(vault: &Vault, paths: &[&str]) {
    vault
        .sync_search_batch(
            SearchUpdate::Paths(paths.iter().map(|path| path.to_string()).collect()),
            BUDGET,
            &|| false,
        )
        .unwrap();
}
#[test]
fn cached_query_does_not_discover_or_scan_unrelated_new_files() {
    let (dir, vault) = fixture();
    fs::write(dir.path().join("New.md"), "newneedle").unwrap();
    let query = vault
        .search_cached("newneedle", SearchMode::Words, 20)
        .unwrap();
    assert!(query.hits.is_empty());
    assert_eq!(query.scanned_sources, 0);
    assert_eq!(query.indexed_sources, 1);
    update(&vault, &["New.md"]);
    assert_eq!(
        vault
            .search_cached("newneedle", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
}
#[test]
fn changed_deleted_and_same_mtime_sources_never_disclose_cached_old_text() {
    let (dir, vault) = fixture();
    let path = dir.path().join("Note.md");
    let modified = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "# Note\n\nnewneedle\n").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let stale = vault
        .search_cached("oldneedle", SearchMode::Words, 20)
        .unwrap();
    assert!(stale.hits.is_empty());
    assert_eq!(stale.stale_paths, vec!["Note.md"]);
    update(&vault, &["Note.md"]);
    assert_eq!(
        vault
            .search_cached("newneedle", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
    fs::remove_file(path).unwrap();
    assert!(
        vault
            .search_cached("newneedle", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
    update(&vault, &["Note.md"]);
    assert_eq!(
        vault
            .search_cached("type:note", SearchMode::Words, 20)
            .unwrap()
            .indexed_sources,
        0
    );
}
#[test]
fn directory_changes_are_scoped_and_do_not_match_a_sibling_prefix() {
    let (dir, vault) = fixture();
    for name in ["Folder", "Folder2"] {
        fs::create_dir(dir.path().join(name)).unwrap();
        fs::write(
            dir.path().join(name).join("A.md"),
            format!("{name} scopedneedle"),
        )
        .unwrap();
    }
    update(&vault, &["Folder", "Folder2"]);
    fs::rename(dir.path().join("Folder"), dir.path().join("Moved")).unwrap();
    update(&vault, &["Folder", "Moved"]);
    let hits = vault
        .search_cached("scopedneedle", SearchMode::Words, 20)
        .unwrap()
        .hits;
    assert_eq!(hits.len(), 2);
    assert!(hits.iter().any(|hit| hit.path == "Folder2/A.md"));
    assert!(hits.iter().any(|hit| hit.path == "Moved/A.md"));
    fs::write(dir.path().join("token.txt"), "privateuniqueneedle").unwrap();
    update(&vault, &["token.txt"]);
    assert!(
        vault
            .search_cached("privateuniqueneedle", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
}
#[test]
fn cancellation_rolls_back_a_write_batch_and_inventory_is_interruptible() {
    let (dir, vault) = fixture();
    fs::write(dir.path().join("Note.md"), "newneedle").unwrap();
    let attempts = AtomicUsize::new(0);
    let result =
        vault.sync_search_batch(SearchUpdate::Paths(vec!["Note.md".into()]), BUDGET, &|| {
            attempts.fetch_add(1, Ordering::Relaxed) >= 2
        });
    assert!(matches!(result, Err(VaultError::SearchCancelled)));
    assert_eq!(
        vault
            .search_cached("oldneedle", SearchMode::Words, 20)
            .unwrap()
            .stale_paths,
        vec!["Note.md"]
    );
    assert!(matches!(
        vault.search_inventory(&|| true),
        Err(VaultError::SearchCancelled)
    ));
    update(&vault, &["Note.md"]);
    assert_eq!(
        vault
            .search_cached("newneedle", SearchMode::Words, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
}
#[test]
fn cancelled_recovery_does_not_quarantine_the_existing_cache() {
    let (dir, vault) = fixture();
    let cache = dir.path().join(".brandon/search-v1.sqlite3");
    let before = fs::read(&cache).unwrap();
    assert!(matches!(
        vault.rebuild_search_cancellable("oldneedle", SearchMode::Words, 20, &|| true),
        Err(VaultError::SearchCancelled)
    ));
    assert_eq!(fs::read(cache).unwrap(), before);
    assert!(
        !fs::read_dir(dir.path().join(".brandon"))
            .unwrap()
            .any(|entry| entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("search-recovery-"))
    );
}
#[test]
fn reconciliation_recovers_changes_with_no_watcher_events_including_restored_mtime() {
    let (dir, vault) = fixture();
    let path = dir.path().join("Note.md");
    let mtime = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, "# Note\n\nnewneedle\n").unwrap();
    fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(fs::FileTimes::new().set_modified(mtime))
        .unwrap();
    fs::write(dir.path().join("New.md"), "missedeventneedle").unwrap();
    assert!(
        vault
            .search_cached("missedeventneedle", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
    let inventory = vault.search_inventory(&|| false).unwrap();
    assert_eq!(inventory.paths.len(), 2);
    for path in inventory.paths {
        update(&vault, &[&path]);
    }
    vault
        .mark_search_reconciled(inventory.skipped_sources, inventory.failed_sources)
        .unwrap();
    for query in ["newneedle", "missedeventneedle"] {
        assert_eq!(
            vault
                .search_cached(query, SearchMode::Words, 20)
                .unwrap()
                .hits
                .len(),
            1
        );
    }
    assert!(
        vault
            .search_cached("oldneedle", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
}
#[test]
fn task_changes_are_detected_without_a_watcher_or_any_vault_inventory() {
    let (dir, vault) = fixture();
    vault.create_task("Task", Some("2026-10-21")).unwrap();
    let stale = vault
        .search_cached("2026-10-21", SearchMode::Literal, 20)
        .unwrap();
    assert!(stale.stale_paths.iter().any(|path| path == "Tasks/Task.md"));
    let canonical = fs::read(dir.path().join(".brandon/tasks.sqlite3")).unwrap();
    update(&vault, &[]);
    assert_eq!(
        vault
            .search_cached("2026-10-21", SearchMode::Literal, 20)
            .unwrap()
            .hits
            .len(),
        1
    );
    assert_eq!(
        fs::read(dir.path().join(".brandon/tasks.sqlite3")).unwrap(),
        canonical
    );
}
#[test]
fn bulk_task_metadata_changes_drain_in_bounded_batches_without_losing_the_remainder() {
    let (_dir, vault) = fixture();
    for id in 0..140 {
        vault
            .create_task(&format!("Bulk {id}"), Some("2026-10-21"))
            .unwrap();
    }
    let first = vault
        .sync_search_batch(SearchUpdate::Paths(vec![]), BUDGET, &|| false)
        .unwrap();
    assert_eq!(first.scanned_sources, 128);
    assert_eq!(first.stale_paths.len(), 12);
    let second = vault
        .sync_search_batch(SearchUpdate::Paths(vec![]), BUDGET, &|| false)
        .unwrap();
    assert_eq!(second.scanned_sources, 12);
    assert!(second.stale_paths.is_empty());
    assert_eq!(
        vault
            .search_cached("type:task", SearchMode::Words, 200)
            .unwrap()
            .hits
            .len(),
        140
    );
}
#[cfg(unix)]
#[test]
fn unreadable_and_symlinked_files_are_filtered_before_cached_snippets_escape() {
    use std::os::unix::fs::{PermissionsExt, symlink};
    let (dir, vault) = fixture();
    let note = dir.path().join("Note.md");
    fs::set_permissions(&note, fs::Permissions::from_mode(0o000)).unwrap();
    // Root bypasses permission bits; the non-root Linux invocation exercises denial.
    if fs::File::open(&note).is_err() {
        assert!(
            vault
                .search_cached("oldneedle", SearchMode::Words, 20)
                .unwrap()
                .hits
                .is_empty()
        );
    }
    fs::set_permissions(&note, fs::Permissions::from_mode(0o600)).unwrap();
    fs::rename(&note, dir.path().join("Actual.md")).unwrap();
    symlink(dir.path().join("Actual.md"), &note).unwrap();
    assert!(
        vault
            .search_cached("oldneedle", SearchMode::Words, 20)
            .unwrap()
            .hits
            .is_empty()
    );
    update(&vault, &["Note.md"]);
    assert_eq!(
        vault
            .search_cached("type:note", SearchMode::Words, 20)
            .unwrap()
            .indexed_sources,
        0
    );
}
