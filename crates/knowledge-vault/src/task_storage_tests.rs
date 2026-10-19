use super::{ManagedTaskPatch, Vault, task_storage};
use rusqlite::Connection;
use std::fs;

#[test]
fn task_status_dates_fields_and_revision_are_canonical_and_independent() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
    while vault.metadata_status().unwrap().stats.pending_sources > 0 {
        vault.metadata_step(128).unwrap();
    }
    let created = vault
        .create_task_record("Не забыть", Some("2026-10-19"))
        .unwrap();
    let before = fs::read(root.path().join(&created.record.note_path)).unwrap();
    let change = vault
        .patch_managed_task(
            &created.record.id,
            &created.revision,
            &ManagedTaskPatch {
                status: Some("doing".into()),
                project: Some("Учёба".into()),
                priority: Some("high".into()),
                tags: Some(vec!["x".into(), "x".into()]),
                duration_minutes: Some(45),
                due: Some("2026-10-25".into()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(change.record.id, created.record.id);
    assert_eq!(change.record.status, "doing");
    assert!(!change.record.completed);
    assert_eq!(change.record.scheduled.as_deref(), Some("2026-10-19"));
    assert_eq!(change.record.due.as_deref(), Some("2026-10-25"));
    assert_eq!(change.record.project.as_deref(), Some("Учёба"));
    assert_eq!(change.record.tags, ["x"]);
    assert_eq!(
        fs::read(root.path().join(&created.record.note_path)).unwrap(),
        before
    );
    assert!(
        vault
            .patch_managed_task(
                &created.record.id,
                &created.revision,
                &ManagedTaskPatch {
                    completed: Some(true),
                    ..Default::default()
                }
            )
            .is_err()
    );
    let done = vault
        .patch_managed_task(
            &created.record.id,
            &change.revision,
            &ManagedTaskPatch {
                completed: Some(true),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(done.record.status, "done");
    let unscheduled = vault
        .patch_managed_task(
            &created.record.id,
            &done.revision,
            &ManagedTaskPatch {
                clear_scheduled: true,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(unscheduled.record.status, "done");
    assert_eq!(unscheduled.record.due.as_deref(), Some("2026-10-25"));
    assert_eq!(unscheduled.record.scheduled, None);
    let metadata = vault.index().unwrap();
    assert_eq!(
        metadata
            .tasks
            .iter()
            .find(|task| task.id == created.record.id)
            .unwrap()
            .project
            .as_deref(),
        Some("Учёба")
    );
}

#[test]
fn legacy_schema_migration_preserves_identity_and_persists_a_pre_migration_backup() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join(".brandon")).unwrap();
    let conn = Connection::open(root.path().join(".brandon/tasks.sqlite3")).unwrap();
    conn.execute_batch("CREATE TABLE tasks(id TEXT PRIMARY KEY,note_path TEXT NOT NULL UNIQUE,title TEXT NOT NULL,completed INTEGER NOT NULL DEFAULT 0,scheduled TEXT,due TEXT,duration_minutes INTEGER,priority TEXT,created_unix_ms INTEGER NOT NULL,updated_unix_ms INTEGER NOT NULL);INSERT INTO tasks VALUES('stable','Tasks/A.md','A',1,'2026-10-19','2026-10-25',45,'high',1,2);").unwrap();
    drop(conn);
    let vault = Vault::open(root.path()).unwrap();
    let conn = vault.task_connection().unwrap();
    let rows = task_storage::records(&conn).unwrap();
    assert_eq!(rows[0].id, "stable");
    assert_eq!(rows[0].status, "done");
    assert_eq!(rows[0].due.as_deref(), Some("2026-10-25"));
    let backups = fs::read_dir(root.path().join(".brandon/task-migrations"))
        .unwrap()
        .collect::<Vec<_>>();
    assert_eq!(backups.len(), 1);
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(backups[0].as_ref().unwrap().path()).unwrap()).unwrap();
    assert_eq!(value["tasks"][0]["id"], "stable");
    assert_eq!(value["fromVersion"], 0);
    drop(conn);
    vault.task_connection().unwrap();
    assert_eq!(
        fs::read_dir(root.path().join(".brandon/task-migrations"))
            .unwrap()
            .count(),
        1
    );
}

#[test]
fn future_and_foreign_task_schemas_are_not_rewritten() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let conn = vault.task_connection().unwrap();
    conn.execute_batch("PRAGMA user_version=99;").unwrap();
    drop(conn);
    let before = fs::read(root.path().join(".brandon/tasks.sqlite3")).unwrap();
    assert!(vault.task_connection().is_err());
    assert_eq!(
        fs::read(root.path().join(".brandon/tasks.sqlite3")).unwrap(),
        before
    );
    let conn = Connection::open(root.path().join(".brandon/tasks.sqlite3")).unwrap();
    conn.execute_batch("PRAGMA user_version=1;CREATE TABLE other(secret TEXT);")
        .unwrap();
    drop(conn);
    let before = fs::read(root.path().join(".brandon/tasks.sqlite3")).unwrap();
    assert!(vault.task_connection().is_err());
    assert_eq!(
        fs::read(root.path().join(".brandon/tasks.sqlite3")).unwrap(),
        before
    );
}

#[test]
fn task_views_preserve_unknown_fields_and_refuse_stale_or_future_writes() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let initial = vault.load_task_views().unwrap();
    let value = serde_json::json!({"format":"brandon.task-views","version":1,"future":{"keep":true},"views":[{"id":"stable","name":"Учёба","mode":"list","extra":[1,2]}]});
    let saved = vault.save_task_views(&initial.fingerprint, &value).unwrap();
    assert_eq!(vault.load_task_views().unwrap().value, value);
    assert!(vault.save_task_views(&initial.fingerprint, &value).is_err());
    let mut future = value.clone();
    future["version"] = serde_json::json!(99);
    assert!(vault.save_task_views(&saved.fingerprint, &future).is_err());
    assert_eq!(vault.load_task_views().unwrap().value, value);
    fs::write(
        root.path().join(".brandon/task-views.v1.json"),
        future.to_string(),
    )
    .unwrap();
    assert!(vault.load_task_views().is_err());
    assert_eq!(
        fs::read_to_string(root.path().join(".brandon/task-views.v1.json")).unwrap(),
        future.to_string()
    );
}

#[test]
fn invalid_task_fields_never_commit() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let created = vault.create_task_record("Valid", None).unwrap();
    for patch in [
        ManagedTaskPatch {
            status: Some("arbitrary".into()),
            ..Default::default()
        },
        ManagedTaskPatch {
            due: Some("2026-02-30".into()),
            ..Default::default()
        },
        ManagedTaskPatch {
            duration_minutes: Some(0),
            ..Default::default()
        },
        ManagedTaskPatch {
            tags: Some(vec!["two words".into()]),
            ..Default::default()
        },
        ManagedTaskPatch {
            status: Some("doing".into()),
            completed: Some(true),
            ..Default::default()
        },
    ] {
        assert!(
            vault
                .patch_managed_task(&created.record.id, &created.revision, &patch)
                .is_err()
        );
    }
    assert_eq!(
        task_storage::current_record(&vault, &created.record.id)
            .unwrap()
            .revision(),
        created.revision
    );
}

#[test]
fn missing_or_truncated_authoritative_task_db_does_not_create_a_replacement() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let task = vault.create_task_record("Keep my identity", None).unwrap();
    let marker = root.path().join(".brandon/task-store.v1.json");
    assert!(marker.exists());
    let database = root.path().join(".brandon/tasks.sqlite3");
    let moved = root.path().join(".brandon/original-task-data.sqlite3");
    fs::rename(&database, &moved).unwrap();
    assert!(vault.task_connection().is_err());
    assert!(!database.exists());
    assert!(
        vault
            .create_task_record("Must not create a note", None)
            .is_err()
    );
    assert!(!root.path().join("Tasks/Must not create a note.md").exists());
    fs::rename(&moved, &database).unwrap();
    assert_eq!(
        task_storage::current_record(&vault, &task.record.id)
            .unwrap()
            .id,
        task.record.id
    );
    fs::write(&database, []).unwrap();
    assert!(vault.task_connection().is_err());
    assert_eq!(fs::metadata(&database).unwrap().len(), 0);
}

#[test]
fn a_plain_note_or_checkbox_in_tasks_folder_is_not_silently_adopted() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("Tasks")).unwrap();
    let source = "# My own note\n\n- [ ] Not an imported task ^custom\n";
    fs::write(root.path().join("Tasks/Plain.md"), source).unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let index = vault.index().unwrap();
    assert_eq!(index.tasks.len(), 1);
    assert!(!index.tasks[0].managed);
    assert!(
        task_storage::records(&vault.task_connection().unwrap())
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        fs::read_to_string(root.path().join("Tasks/Plain.md")).unwrap(),
        source
    );
}

#[test]
fn inline_checkbox_block_ids_are_scoped_to_their_note_not_global_task_ids() {
    let root = tempfile::tempdir().unwrap();
    let vault = Vault::open(root.path()).unwrap();
    fs::write(root.path().join("A.md"), "- [ ] A ^same\n").unwrap();
    fs::write(root.path().join("B.md"), "- [ ] B ^same\n").unwrap();
    let index = vault.index().unwrap();
    assert_eq!(index.tasks.len(), 2);
    assert_ne!(index.tasks[0].id, index.tasks[1].id);
    assert!(
        index
            .tasks
            .iter()
            .all(|task| !task.managed && task.block_id.as_deref() == Some("same"))
    );
}

#[cfg(windows)]
#[test]
fn private_task_directory_and_task_folder_junctions_are_denied() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let vault = Vault::open(root.path()).unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(root.path().join(".brandon"))
        .arg(outside.path())
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(vault.task_connection().is_err());
    assert_eq!(fs::read_dir(outside.path()).unwrap().count(), 0);
    fs::remove_dir(root.path().join(".brandon")).unwrap();
    vault.task_connection().unwrap();
    let status = std::process::Command::new("cmd")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(root.path().join("Tasks"))
        .arg(outside.path())
        .output()
        .unwrap();
    assert!(status.status.success());
    assert!(vault.create_task_record("Escaped", None).is_err());
    assert!(!outside.path().join("Escaped.md").exists());
}
