//! Machine-local workspace metadata and durable crash drafts. Never a vault writer.
use crate::{VaultState, current_vault};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{AppHandle, Manager, State};

#[derive(Default)]
pub struct WorkspaceState(pub Mutex<()>);

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Draft {
    pub path: String,
    pub pane: String,
    pub version: String,
    pub content: String,
    pub base_content: String,
    pub fingerprint: String,
    #[serde(default)]
    pub updated: i64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceData {
    session: Option<serde_json::Value>,
    drafts: Vec<Draft>,
}

#[derive(Serialize)]
pub struct DocumentVersion {
    id: i64,
    updated: i64,
    content: String,
}

fn connect(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    db.execute_batch("PRAGMA synchronous=FULL;
        CREATE TABLE IF NOT EXISTS sessions(root TEXT PRIMARY KEY, payload TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS drafts(root TEXT NOT NULL,path TEXT NOT NULL,pane TEXT NOT NULL,version TEXT NOT NULL,payload TEXT NOT NULL,PRIMARY KEY(root,path,pane));
        CREATE TABLE IF NOT EXISTS versions(id INTEGER PRIMARY KEY,root TEXT NOT NULL,path TEXT NOT NULL,updated INTEGER NOT NULL,content TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS version_path ON versions(root,path,id);")
        .map_err(|e| e.to_string())?;
    Ok(db)
}

fn database(app: &AppHandle) -> Result<Connection, String> {
    let directory = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join("workspace-v1.sqlite3");
    if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err("Журнал workspace не может быть символической ссылкой".into());
    }
    connect(&path)
}

fn check_root(state: &State<'_, VaultState>, root: &str) -> Result<(), String> {
    if current_vault(state)?.root().to_string_lossy() != root {
        return Err("Vault изменился; запись workspace отменена".into());
    }
    Ok(())
}

fn safe_path(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 2048
        && !path.starts_with('/')
        && !path.contains(['\\', ':', '\0'])
        && path.split('/').all(|part| !matches!(part, "" | "." | ".."))
        && [".md", ".canvas", ".base"]
            .iter()
            .any(|ext| path.to_lowercase().ends_with(ext))
}

fn validate(draft: &Draft) -> Result<(), String> {
    if !safe_path(&draft.path)
        || !matches!(
            draft.pane.as_str(),
            "main" | "side" | "linked-main" | "linked-side"
        )
        || draft.version.is_empty()
        || draft.version.len() > 80
        || draft.fingerprint.len() > 256
        || draft.content.len() > 8 * 1024 * 1024
        || draft.base_content.len() > 8 * 1024 * 1024
    {
        return Err("Некорректный или слишком большой черновик (лимит 8 MiB)".into());
    }
    Ok(())
}

fn put(db: &mut Connection, root: &str, draft: &Draft) -> Result<(), String> {
    validate(draft)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let (count, bytes): (i64, i64) = tx.query_row("SELECT count(*),coalesce(sum(length(CAST(payload AS BLOB))),0) FROM drafts WHERE root=?1 AND NOT(path=?2 AND pane=?3)",params![root,draft.path,draft.pane],|r| Ok((r.get(0)?,r.get(1)?))).map_err(|e| e.to_string())?;
    let payload = serde_json::to_string(draft).map_err(|e| e.to_string())?;
    if count >= 100 || bytes + payload.len() as i64 > 64 * 1024 * 1024 {
        return Err("Журнал черновиков заполнен; сохраните восстановленные копии".into());
    }
    remember_version(&tx, root, &draft.path, &draft.base_content, draft.updated)?;
    tx.execute("INSERT INTO drafts(root,path,pane,version,payload) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(root,path,pane) DO UPDATE SET version=excluded.version,payload=excluded.payload",params![root,draft.path,draft.pane,draft.version,payload]).map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())
}

fn remember_version(
    tx: &rusqlite::Transaction<'_>,
    root: &str,
    path: &str,
    content: &str,
    updated: i64,
) -> Result<(), String> {
    let previous: Option<String> = tx
        .query_row(
            "SELECT content FROM versions WHERE root=?1 AND path=?2 ORDER BY id DESC LIMIT 1",
            params![root, path],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if previous.as_deref() != Some(content) {
        tx.execute(
            "INSERT INTO versions(root,path,updated,content) VALUES(?1,?2,?3,?4)",
            params![root, path, updated, content],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM versions WHERE root=?1 AND path=?2 AND id NOT IN (SELECT id FROM versions WHERE root=?1 AND path=?2 ORDER BY id DESC LIMIT 20)",params![root,path]).map_err(|e| e.to_string())?;
    }
    loop {
        let size: i64 = tx
            .query_row(
                "SELECT coalesce(sum(length(CAST(content AS BLOB))),0) FROM versions WHERE root=?1",
                [root],
                |r| r.get(0),
            )
            .map_err(|e| e.to_string())?;
        if size <= 128 * 1024 * 1024 {
            break;
        }
        tx.execute(
            "DELETE FROM versions WHERE id=(SELECT min(id) FROM versions WHERE root=?1)",
            [root],
        )
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

// Capture writes preserve the same bounded source history without creating or
// acknowledging any main/side crash draft as an implementation side effect.
pub fn capture_checkpoint(
    app: &AppHandle,
    journal: &State<'_, WorkspaceState>,
    root: &str,
    path: &str,
    content: &str,
) -> Result<(), String> {
    if !safe_path(path) || content.len() > 8 * 1024 * 1024 {
        return Err("Некорректная версия capture".into());
    }
    let _guard = journal.0.lock().map_err(|e| e.to_string())?;
    let mut db = database(app)?;
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let updated = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as i64;
    remember_version(&tx, root, path, content, updated)?;
    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
pub fn load_workspace(
    app: AppHandle,
    state: State<'_, VaultState>,
    journal: State<'_, WorkspaceState>,
    root: String,
) -> Result<WorkspaceData, String> {
    let _guard = journal.0.lock().map_err(|e| e.to_string())?;
    check_root(&state, &root)?;
    let db = database(&app)?;
    let payload: Option<String> = db
        .query_row("SELECT payload FROM sessions WHERE root=?1", [&root], |r| {
            r.get(0)
        })
        .optional()
        .map_err(|e| e.to_string())?;
    let mut query = db
        .prepare("SELECT payload FROM drafts WHERE root=?1")
        .map_err(|e| e.to_string())?;
    let rows = query
        .query_map([&root], |r| r.get::<_, String>(0))
        .map_err(|e| e.to_string())?;
    let mut drafts = Vec::new();
    for row in rows {
        let draft: Draft =
            serde_json::from_str(&row.map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        validate(&draft)?;
        drafts.push(draft);
    }
    Ok(WorkspaceData {
        session: payload
            .map(|p| serde_json::from_str(&p))
            .transpose()
            .map_err(|e| e.to_string())?,
        drafts,
    })
}

#[tauri::command]
pub fn save_workspace(
    app: AppHandle,
    state: State<'_, VaultState>,
    journal: State<'_, WorkspaceState>,
    root: String,
    session: serde_json::Value,
) -> Result<(), String> {
    let _guard = journal.0.lock().map_err(|e| e.to_string())?;
    check_root(&state, &root)?;
    let payload = serde_json::to_string(&session).map_err(|e| e.to_string())?;
    if payload.len() > 512 * 1024 || session.get("version").and_then(|v| v.as_u64()) != Some(1) {
        return Err("Некорректная сессия workspace".into());
    }
    database(&app)?.execute("INSERT INTO sessions(root,payload) VALUES(?1,?2) ON CONFLICT(root) DO UPDATE SET payload=excluded.payload",params![root,payload]).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn put_draft(
    app: AppHandle,
    state: State<'_, VaultState>,
    journal: State<'_, WorkspaceState>,
    root: String,
    mut draft: Draft,
) -> Result<(), String> {
    let _guard = journal.0.lock().map_err(|e| e.to_string())?;
    check_root(&state, &root)?;
    draft.updated = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis() as i64;
    put(&mut database(&app)?, &root, &draft)
}

#[tauri::command]
pub fn remove_draft(
    app: AppHandle,
    state: State<'_, VaultState>,
    journal: State<'_, WorkspaceState>,
    root: String,
    path: String,
    pane: String,
    version: String,
) -> Result<(), String> {
    let _guard = journal.0.lock().map_err(|e| e.to_string())?;
    check_root(&state, &root)?;
    database(&app)?
        .execute(
            "DELETE FROM drafts WHERE root=?1 AND path=?2 AND pane=?3 AND version=?4",
            params![root, path, pane, version],
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn document_versions(
    app: AppHandle,
    state: State<'_, VaultState>,
    journal: State<'_, WorkspaceState>,
    root: String,
    path: String,
) -> Result<Vec<DocumentVersion>, String> {
    let _guard = journal.0.lock().map_err(|e| e.to_string())?;
    check_root(&state, &root)?;
    let db = database(&app)?;
    let mut query = db.prepare("SELECT id,updated,content FROM versions WHERE root=?1 AND path=?2 ORDER BY id DESC LIMIT 20").map_err(|e| e.to_string())?;
    query
        .query_map(params![root, path], |r| {
            Ok(DocumentVersion {
                id: r.get(0)?,
                updated: r.get(1)?,
                content: r.get(2)?,
            })
        })
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())
}

fn promote(
    db: &mut Connection,
    root: &str,
    path: &str,
    pane: &str,
    version: &str,
) -> Result<Draft, String> {
    let destination = match pane {
        "linked-main" => "main",
        "linked-side" => "side",
        _ => return Err("Это не черновик связанного документа".into()),
    };
    let tx = db.transaction().map_err(|e| e.to_string())?;
    let payload: String = tx
        .query_row(
            "SELECT payload FROM drafts WHERE root=?1 AND path=?2 AND pane=?3 AND version=?4",
            params![root, path, pane, version],
            |r| r.get(0),
        )
        .map_err(|_| "Черновик изменился или больше недоступен")?;
    let mut draft: Draft = serde_json::from_str(&payload).map_err(|_| "Черновик повреждён")?;
    validate(&draft)?;
    if draft.path != path || draft.pane != pane || draft.version != version {
        return Err("Содержимое журнала не совпадает с ключом черновика".into());
    }
    let exists: bool = tx
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM drafts WHERE root=?1 AND path=?2 AND pane=?3)",
            params![root, path, destination],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    if exists {
        return Err("В целевой панели уже есть черновик этого файла. Сначала восстановите или явно отбросьте его; ни одна копия не заменена.".into());
    }
    draft.pane = destination.into();
    draft.version = uuid::Uuid::now_v7().to_string();
    let payload = serde_json::to_string(&draft).map_err(|e| e.to_string())?;
    tx.execute(
        "INSERT INTO drafts(root,path,pane,version,payload) VALUES(?1,?2,?3,?4,?5)",
        params![root, path, destination, draft.version, payload],
    )
    .map_err(|e| e.to_string())?;
    tx.execute(
        "DELETE FROM drafts WHERE root=?1 AND path=?2 AND pane=?3 AND version=?4",
        params![root, path, pane, version],
    )
    .map_err(|e| e.to_string())?;
    tx.commit().map_err(|e| e.to_string())?;
    Ok(draft)
}
#[tauri::command]
pub fn promote_linked_draft(
    app: AppHandle,
    state: State<'_, VaultState>,
    journal: State<'_, WorkspaceState>,
    root: String,
    path: String,
    pane: String,
    version: String,
) -> Result<Draft, String> {
    let _guard = journal.0.lock().map_err(|e| e.to_string())?;
    check_root(&state, &root)?;
    promote(&mut database(&app)?, &root, &path, &pane, &version)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn linked_drafts_never_replace_pane_drafts_and_promotion_is_atomic() {
        let mut db = connect(Path::new(":memory:")).unwrap();
        let mut draft = Draft {
            path: "a.md".into(),
            pane: "main".into(),
            version: "normal".into(),
            content: "normal draft".into(),
            base_content: "base".into(),
            fingerprint: "fp".into(),
            updated: 1,
        };
        put(&mut db, "root", &draft).unwrap();
        draft.pane = "linked-main".into();
        draft.version = "linked".into();
        draft.content = "linked draft".into();
        put(&mut db, "root", &draft).unwrap();
        assert!(promote(&mut db, "root", "a.md", "linked-main", "linked").is_err());
        assert_eq!(
            db.query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            2
        );
        db.execute("DELETE FROM drafts WHERE pane='main'", [])
            .unwrap();
        assert!(promote(&mut db, "root", "a.md", "linked-main", "wrong-version").is_err());
        let restored = promote(&mut db, "root", "a.md", "linked-main", "linked").unwrap();
        assert_eq!(restored.pane, "main");
        assert_eq!(restored.content, "linked draft");
        assert_ne!(restored.version, "linked");
        assert_eq!(
            db.query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            db.query_row("SELECT pane FROM drafts", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "main"
        );
    }
    #[test]
    fn capture_history_does_not_acknowledge_or_replace_a_crash_draft() {
        let mut db = connect(Path::new(":memory:")).unwrap();
        let draft = Draft {
            path: "Note.md".into(),
            pane: "side".into(),
            version: "durable".into(),
            content: "UNSAVED SIDE".into(),
            base_content: "base".into(),
            fingerprint: "fp".into(),
            updated: 1,
        };
        put(&mut db, "root", &draft).unwrap();
        let before: String = db
            .query_row("SELECT payload FROM drafts", [], |row| row.get(0))
            .unwrap();
        let tx = db.transaction().unwrap();
        remember_version(&tx, "root", "Note.md", "capture-before", 2).unwrap();
        tx.commit().unwrap();
        assert_eq!(
            db.query_row("SELECT payload FROM drafts", [], |row| row
                .get::<_, String>(0))
                .unwrap(),
            before
        );
        assert_eq!(
            db.query_row(
                "SELECT count(*) FROM versions WHERE content='capture-before'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
    }
    #[test]
    fn draft_transaction_isolated_by_vault_and_conditional_clear() {
        let mut db = connect(Path::new(":memory:")).expect("db");
        let mut draft = Draft {
            path: "Note.md".into(),
            pane: "main".into(),
            version: "v1".into(),
            content: "new".into(),
            base_content: "old".into(),
            fingerprint: "fp".into(),
            updated: 1,
        };
        put(&mut db, "a", &draft).expect("draft");
        draft.version = "v2".into();
        put(&mut db, "a", &draft).expect("new draft");
        db.execute("DELETE FROM drafts WHERE root='a' AND version='v1'", [])
            .expect("clear old");
        assert_eq!(
            db.query_row("SELECT count(*) FROM drafts WHERE root='a'", [], |r| r
                .get::<_, i64>(0))
                .expect("count"),
            1
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM drafts WHERE root='b'", [], |r| r
                .get::<_, i64>(0))
                .expect("count"),
            0
        );
        assert_eq!(
            db.query_row("SELECT count(*) FROM versions", [], |r| r.get::<_, i64>(0))
                .expect("count"),
            1
        );
    }
    #[test]
    fn journal_rejects_unsafe_and_oversized_paths() {
        for path in [
            "../a.md", "/a.md", "C:/a.md", "a\\b.md", "a.txt", "a/./b.md",
        ] {
            assert!(!safe_path(path));
        }
        assert!(safe_path("Папка/Заметка.md"));
    }
    #[test]
    fn four_draft_slots_coexist_and_side_promotion_is_root_scoped() {
        let mut db = connect(Path::new(":memory:")).unwrap();
        for pane in ["main", "side", "linked-main", "linked-side"] {
            let draft = Draft {
                path: "a.md".into(),
                pane: pane.into(),
                version: pane.into(),
                content: pane.into(),
                base_content: "base".into(),
                fingerprint: "fp".into(),
                updated: 1,
            };
            put(&mut db, "root", &draft).unwrap();
        }
        assert_eq!(
            db.query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            4
        );
        assert!(promote(&mut db, "other-root", "a.md", "linked-side", "linked-side").is_err());
        assert!(promote(&mut db, "root", "a.md", "linked-side", "linked-side").is_err());
        db.execute("DELETE FROM drafts WHERE pane='side'", [])
            .unwrap();
        let next = promote(&mut db, "root", "a.md", "linked-side", "linked-side").unwrap();
        assert_eq!(next.pane, "side");
        assert_eq!(next.content, "linked-side");
        assert_eq!(
            db.query_row("SELECT count(*) FROM drafts", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            3
        );
        assert_eq!(
            db.query_row("SELECT payload FROM drafts WHERE pane='main'", [], |r| {
                r.get::<_, String>(0)
            })
            .map(|s| serde_json::from_str::<Draft>(&s).unwrap().content)
            .unwrap(),
            "main"
        );
    }
    #[test]
    fn a_tampered_linked_payload_is_not_promoted_under_another_key() {
        let mut db = connect(Path::new(":memory:")).unwrap();
        let draft = Draft {
            path: "a.md".into(),
            pane: "linked-main".into(),
            version: "v1".into(),
            content: "draft".into(),
            base_content: "base".into(),
            fingerprint: "fp".into(),
            updated: 1,
        };
        put(&mut db, "root", &draft).unwrap();
        let mut foreign = draft.clone();
        foreign.path = "other.md".into();
        db.execute(
            "UPDATE drafts SET payload=?1",
            [serde_json::to_string(&foreign).unwrap()],
        )
        .unwrap();
        assert!(promote(&mut db, "root", "a.md", "linked-main", "v1").is_err());
        assert_eq!(
            db.query_row("SELECT pane FROM drafts", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "linked-main"
        );
    }
    #[test]
    fn version_history_is_bounded() {
        let mut db = connect(Path::new(":memory:")).expect("db");
        for n in 0..30 {
            put(
                &mut db,
                "a",
                &Draft {
                    path: "a.md".into(),
                    pane: "main".into(),
                    version: n.to_string(),
                    content: "draft".into(),
                    base_content: n.to_string(),
                    fingerprint: "fp".into(),
                    updated: 1,
                },
            )
            .expect("put");
        }
        assert_eq!(
            db.query_row("SELECT count(*) FROM versions", [], |r| r.get::<_, i64>(0))
                .expect("count"),
            20
        );
    }
}
