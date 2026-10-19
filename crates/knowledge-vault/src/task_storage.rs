//! Authoritative task state, versioned independently of derivative metadata/search.
use super::{
    Result, Vault, VaultError, attachments, current_unix_ms, fingerprint, normalize_task_datetime,
    normalized_relative, write_atomic,
};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
};
static SCHEMA_LOCK: Mutex<()> = Mutex::new(());
const STORE_MARKER: &[u8] = b"{\"format\":\"brandon.task-store\",\"version\":1}\n";

pub(crate) const SCHEMA_VERSION: i64 = 1;
const LEGACY_COLUMNS: [&str; 10] = [
    "id",
    "note_path",
    "title",
    "completed",
    "scheduled",
    "due",
    "duration_minutes",
    "priority",
    "created_unix_ms",
    "updated_unix_ms",
];
const COLUMNS: [&str; 13] = [
    "id",
    "note_path",
    "title",
    "completed",
    "scheduled",
    "due",
    "duration_minutes",
    "priority",
    "created_unix_ms",
    "updated_unix_ms",
    "status",
    "project",
    "tags_json",
];
pub(crate) const SELECT_RECORDS: &str = "SELECT id,note_path,title,completed,scheduled,due,duration_minutes,priority,created_unix_ms,updated_unix_ms,status,project,tags_json FROM tasks";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskRecord {
    pub id: String,
    pub note_path: String,
    pub title: String,
    pub completed: bool,
    pub scheduled: Option<String>,
    pub due: Option<String>,
    pub duration_minutes: Option<u32>,
    pub priority: Option<String>,
    pub created_unix_ms: i64,
    pub updated_unix_ms: i64,
    pub status: String,
    pub project: Option<String>,
    pub tags: Vec<String>,
}
impl TaskRecord {
    pub fn revision(&self) -> String {
        fingerprint(
            serde_json::to_string(self)
                .expect("serializable task record")
                .as_bytes(),
        )
    }
    pub(crate) fn validate(&self) -> Result<()> {
        if self.id.is_empty()
            || self.id.len() > 256
            || self.id.chars().any(char::is_control)
            || self.title.len() > 4096
            || self.created_unix_ms < 0
            || self.updated_unix_ms < 0
        {
            return Err(invalid("Некорректная запись задачи"));
        }
        portable_path(&self.note_path)?;
        if !self.note_path.to_lowercase().ends_with(".md")
            || !matches!(self.status.as_str(), "todo" | "doing" | "done")
            || self.completed != (self.status == "done")
        {
            return Err(invalid("Некорректный путь или статус задачи"));
        }
        normalize_task_datetime(self.scheduled.as_deref())?;
        normalize_task_datetime(self.due.as_deref())?;
        validate_fields(
            self.project.as_deref(),
            self.priority.as_deref(),
            self.duration_minutes,
            Some(&self.tags),
        )
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManagedTaskPatch {
    pub status: Option<String>,
    pub completed: Option<bool>,
    pub project: Option<String>,
    #[serde(default)]
    pub clear_project: bool,
    pub priority: Option<String>,
    #[serde(default)]
    pub clear_priority: bool,
    pub tags: Option<Vec<String>>,
    pub duration_minutes: Option<u32>,
    #[serde(default)]
    pub clear_duration: bool,
    pub scheduled: Option<String>,
    #[serde(default)]
    pub clear_scheduled: bool,
    pub due: Option<String>,
    #[serde(default)]
    pub clear_due: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskChange {
    pub record: TaskRecord,
    pub revision: String,
    pub metadata_warning: Option<String>,
}

pub(crate) fn task_change(vault: &Vault, record: TaskRecord) -> TaskChange {
    let metadata_warning = vault
        .metadata_step(128)
        .err()
        .map(|error| error.to_string());
    TaskChange {
        revision: record.revision(),
        record,
        metadata_warning,
    }
}
pub(crate) fn current_record(vault: &Vault, id: &str) -> Result<TaskRecord> {
    record_by_id(&vault.task_connection()?, id)?
        .ok_or_else(|| invalid("Задача не найдена после записи"))
}

pub(crate) fn invalid(message: &str) -> VaultError {
    VaultError::TaskStorage(message.into())
}
fn io_error(error: std::io::Error) -> VaultError {
    VaultError::Io {
        context: "Хранилище задач недоступно",
        source: error,
    }
}
pub(crate) fn portable_path(path: &str) -> Result<PathBuf> {
    let normalized = normalized_relative(path, false)?;
    if path.len() > 2048
        || path.contains('\\')
        || normalized
            .components()
            .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
    {
        return Err(invalid("Backup содержит скрытый или непереносимый путь"));
    }
    Ok(normalized)
}
pub(crate) fn checked_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) => {
            if !meta.is_dir() || attachments::is_link(&meta) {
                return Err(invalid("Каталог задач заменён ссылкой или другим объектом"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if let Err(error) = fs::create_dir(path) {
                if error.kind() != std::io::ErrorKind::AlreadyExists {
                    return Err(io_error(error));
                }
                // Metadata bootstrap and a capture-library read may create the
                // private directory concurrently. Accept the winner only after
                // the same no-link/type validation, never blindly follow it.
                let meta = fs::symlink_metadata(path).map_err(io_error)?;
                if !meta.is_dir() || attachments::is_link(&meta) {
                    return Err(invalid("Каталог задач заменён ссылкой или другим объектом"));
                }
            }
        }
        Err(error) => return Err(io_error(error)),
    }
    Ok(())
}
pub(crate) fn checked_file(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() || attachments::is_link(&meta) => {
            Err(invalid("Файл задач заменён ссылкой или другим объектом"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(error)),
    }
}
pub(crate) fn task_directory(vault: &Vault) -> Result<PathBuf> {
    let directory = vault.root.join(".brandon");
    checked_directory(&directory)?;
    Ok(directory)
}
fn columns(connection: &Connection) -> Result<Vec<String>> {
    let mut query = connection.prepare("PRAGMA table_info(tasks)")?;
    Ok(query
        .query_map([], |row| row.get(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?)
}
fn reject_extra_schema(connection: &Connection) -> Result<()> {
    let unsafe_count:i64=connection.query_row("SELECT count(*) FROM sqlite_schema WHERE type IN ('trigger','view') OR (type='table' AND name NOT IN ('tasks','sqlite_sequence','sqlite_stat1','sqlite_stat4'))",[],|row|row.get(0))?;
    if unsafe_count != 0 {
        return Err(invalid(
            "Неизвестные таблицы/триггеры в task DB; исходная база не изменена",
        ));
    }
    Ok(())
}
pub(crate) fn connection(vault: &Vault) -> Result<Connection> {
    let _schema_guard = SCHEMA_LOCK
        .lock()
        .map_err(|_| invalid("Не удалось заблокировать схему task DB"))?;
    let directory = task_directory(vault)?;
    let marker = directory.join("task-store.v1.json");
    checked_file(&marker)?;
    let marker_exists = marker.exists();
    if marker_exists {
        let mut bytes = Vec::new();
        fs::File::open(&marker)
            .map_err(io_error)?
            .take(1025)
            .read_to_end(&mut bytes)
            .map_err(io_error)?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("Повреждён marker task DB; исходные файлы не изменены"))?;
        if bytes.len() > 1024
            || value.get("format").and_then(Value::as_str) != Some("brandon.task-store")
            || value.get("version").and_then(Value::as_u64) != Some(1)
        {
            return Err(invalid(
                "Неизвестный marker task DB; требуется совместимая версия",
            ));
        }
    }
    let db_path = directory.join("tasks.sqlite3");
    match fs::symlink_metadata(&db_path) {
        Ok(meta) if meta.len() < 100 => {
            return Err(invalid(
                "Каноническая task DB пуста или повреждена. Восстановите backup; автоматическая подмена запрещена",
            ));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && marker_exists => {
            return Err(invalid(
                "Каноническая task DB потеряна. Восстановите backup; новый TaskId автоматически не создаётся",
            ));
        }
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => return Err(io_error(error)),
        _ => {}
    }
    for file in [
        "tasks.sqlite3",
        "tasks.sqlite3-wal",
        "tasks.sqlite3-shm",
        "tasks.sqlite3-journal",
    ] {
        checked_file(&directory.join(file))?;
    }
    let mut connection = Connection::open(directory.join("tasks.sqlite3"))?;
    connection.execute_batch("PRAGMA foreign_keys=ON;PRAGMA busy_timeout=5000;PRAGMA synchronous=FULL;PRAGMA trusted_schema=OFF;")?;
    let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let version: i64 = transaction.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(invalid(
            "Task DB создана более новой версией. Ничего не перезаписано",
        ));
    }
    reject_extra_schema(&transaction)?;
    let existing = columns(&transaction)?;
    if version == 0 && existing.is_empty() {
        transaction.execute_batch("CREATE TABLE tasks(id TEXT PRIMARY KEY,note_path TEXT NOT NULL UNIQUE,title TEXT NOT NULL,completed INTEGER NOT NULL DEFAULT 0,scheduled TEXT,due TEXT,duration_minutes INTEGER,priority TEXT,created_unix_ms INTEGER NOT NULL,updated_unix_ms INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'todo' CHECK(status IN ('todo','doing','done')),project TEXT,tags_json TEXT NOT NULL DEFAULT '[]');PRAGMA user_version=1;")?;
    } else if version == 0 && existing == LEGACY_COLUMNS {
        let records = legacy_records(&transaction)?;
        let migrations = directory.join("task-migrations");
        checked_directory(&migrations)?;
        let backup = migrations.join(format!(
            "v0-{}-{}.json",
            current_unix_ms(),
            super::new_task_id()
        ));
        let content=serde_json::to_vec_pretty(&serde_json::json!({"format":"brandon.task-migration","version":1,"fromVersion":0,"tasks":records})).map_err(|e|invalid(&e.to_string()))?;
        write_atomic(&backup, &content)?;
        transaction.execute_batch("ALTER TABLE tasks ADD COLUMN status TEXT NOT NULL DEFAULT 'todo' CHECK(status IN ('todo','doing','done'));ALTER TABLE tasks ADD COLUMN project TEXT;ALTER TABLE tasks ADD COLUMN tags_json TEXT NOT NULL DEFAULT '[]';UPDATE tasks SET status=CASE WHEN completed THEN 'done' ELSE 'todo' END;PRAGMA user_version=1;")?;
    } else if version != SCHEMA_VERSION || existing != COLUMNS {
        return Err(invalid(
            "Неизвестная схема task DB; автоматическая перезапись запрещена",
        ));
    }
    transaction.execute_batch("CREATE INDEX IF NOT EXISTS tasks_scheduled_idx ON tasks(scheduled);CREATE INDEX IF NOT EXISTS tasks_due_idx ON tasks(due);")?;
    transaction.commit()?;
    if !marker_exists {
        write_atomic(&marker, STORE_MARKER)?;
    }
    Ok(connection)
}
fn legacy_records(connection: &Connection) -> Result<Vec<TaskRecord>> {
    let mut query=connection.prepare("SELECT id,note_path,title,completed,scheduled,due,duration_minutes,priority,created_unix_ms,updated_unix_ms FROM tasks ORDER BY id")?;
    let records = query
        .query_map([], |row| {
            let completed: bool = row.get(3)?;
            Ok(TaskRecord {
                id: row.get(0)?,
                note_path: row.get(1)?,
                title: row.get(2)?,
                completed,
                scheduled: row.get(4)?,
                due: row.get(5)?,
                duration_minutes: row.get(6)?,
                priority: row.get(7)?,
                created_unix_ms: row.get(8)?,
                updated_unix_ms: row.get(9)?,
                status: if completed { "done" } else { "todo" }.into(),
                project: None,
                tags: Vec::new(),
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(records)
}
fn read_record(row: &rusqlite::Row<'_>) -> rusqlite::Result<TaskRecord> {
    let tags: String = row.get(12)?;
    Ok(TaskRecord {
        id: row.get(0)?,
        note_path: row.get(1)?,
        title: row.get(2)?,
        completed: row.get(3)?,
        scheduled: row.get(4)?,
        due: row.get(5)?,
        duration_minutes: row.get(6)?,
        priority: row.get(7)?,
        created_unix_ms: row.get(8)?,
        updated_unix_ms: row.get(9)?,
        status: row.get(10)?,
        project: row.get(11)?,
        tags: serde_json::from_str(&tags).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                12,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
    })
}
pub(crate) fn records(connection: &Connection) -> Result<Vec<TaskRecord>> {
    let mut query = connection.prepare(&format!("{SELECT_RECORDS} ORDER BY id LIMIT 50001"))?;
    let records = query
        .query_map([], read_record)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if records.len() > 50000 {
        return Err(invalid("Task DB превышает лимит 50000 задач"));
    }
    for record in &records {
        record.validate()?;
    }
    Ok(records)
}
fn record_by_id(connection: &Connection, id: &str) -> Result<Option<TaskRecord>> {
    Ok(connection
        .query_row(&format!("{SELECT_RECORDS} WHERE id=?1"), [id], read_record)
        .optional()?)
}
pub(crate) fn insert_record(connection: &Connection, record: &TaskRecord) -> Result<()> {
    record.validate()?;
    connection.execute("INSERT INTO tasks(id,note_path,title,completed,scheduled,due,duration_minutes,priority,created_unix_ms,updated_unix_ms,status,project,tags_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",params![record.id,record.note_path,record.title,record.completed,record.scheduled,record.due,record.duration_minutes,record.priority,record.created_unix_ms,record.updated_unix_ms,record.status,record.project,serde_json::to_string(&record.tags).expect("tags")])?;
    Ok(())
}
fn validate_fields(
    project: Option<&str>,
    priority: Option<&str>,
    duration: Option<u32>,
    tags: Option<&Vec<String>>,
) -> Result<()> {
    if project.is_some_and(|value| value.len() > 512 || value.chars().any(char::is_control))
        || priority.is_some_and(|value| !matches!(value, "urgent" | "high" | "low" | "lowest"))
        || duration.is_some_and(|value| value == 0 || value > 525600)
        || tags.is_some_and(|values| {
            values.len() > 256
                || values.iter().any(|tag| {
                    tag.is_empty()
                        || tag.len() > 256
                        || tag.starts_with('#')
                        || tag.chars().any(char::is_whitespace)
                })
        })
    {
        return Err(invalid(
            "Некорректный проект, приоритет, длительность или теги",
        ));
    }
    Ok(())
}

impl Vault {
    /// Snapshot only requested canonical rows. Never creates or migrates a task store.
    pub fn task_records_snapshot(&self, ids: &[String]) -> Result<HashMap<String, TaskRecord>> {
        if ids.len() > 2000
            || ids.iter().any(|id| {
                id.is_empty()
                    || id.len() > 128
                    || !id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            })
        {
            return Err(invalid("Некорректный список TaskId"));
        }
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let directory = self.root.join(".brandon");
        let meta = match fs::symlink_metadata(&directory) {
            Ok(meta) => meta,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(HashMap::new()),
            Err(error) => return Err(io_error(error)),
        };
        if !meta.is_dir() || attachments::is_link(&meta) {
            return Err(invalid("Папка задач недоступна для чтения"));
        }
        for file in [
            "tasks.sqlite3",
            "tasks.sqlite3-wal",
            "tasks.sqlite3-shm",
            "tasks.sqlite3-journal",
            "task-store.v1.json",
        ] {
            checked_file(&directory.join(file))?;
        }
        let path = directory.join("tasks.sqlite3");
        let marker = directory.join("task-store.v1.json");
        if marker.exists() {
            let mut bytes = Vec::new();
            fs::File::open(&marker)
                .map_err(io_error)?
                .take(1025)
                .read_to_end(&mut bytes)
                .map_err(io_error)?;
            let value: Value =
                serde_json::from_slice(&bytes).map_err(|_| invalid("Marker task DB повреждён"))?;
            if bytes.len() > 1024
                || value.get("format").and_then(Value::as_str) != Some("brandon.task-store")
                || value.get("version").and_then(Value::as_u64) != Some(1)
            {
                return Err(invalid(
                    "Неизвестный marker task DB; чтение не выполняет миграции",
                ));
            }
        }
        if !path.exists() {
            if directory.join("task-store.v1.json").exists() {
                return Err(invalid("Каноническая task DB потеряна; требуется backup"));
            }
            return Ok(HashMap::new());
        }
        let mut connection = Connection::open_with_flags(
            path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        connection.execute_batch(
            "PRAGMA trusted_schema=OFF;PRAGMA query_only=ON;PRAGMA busy_timeout=2500;",
        )?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Deferred)?;
        let version: i64 = transaction.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version != SCHEMA_VERSION || columns(&transaction)? != COLUMNS {
            return Err(invalid(
                "Схема task DB требует совместимой версии; экспорт не выполняет миграции",
            ));
        }
        reject_extra_schema(&transaction)?;
        let mut records = HashMap::new();
        for id in ids {
            if let Some(record) = record_by_id(&transaction, id)? {
                record.validate()?;
                records.insert(id.clone(), record);
            }
        }
        transaction.commit()?;
        Ok(records)
    }
    pub fn patch_managed_task(
        &self,
        task_id: &str,
        expected_revision: &str,
        patch: &ManagedTaskPatch,
    ) -> Result<TaskChange> {
        validate_fields(
            patch.project.as_deref(),
            patch.priority.as_deref(),
            patch.duration_minutes,
            patch.tags.as_ref(),
        )?;
        normalize_task_datetime(patch.scheduled.as_deref())?;
        normalize_task_datetime(patch.due.as_deref())?;
        if patch
            .status
            .as_deref()
            .is_some_and(|status| !matches!(status, "todo" | "doing" | "done"))
            || patch
                .status
                .as_ref()
                .zip(patch.completed)
                .is_some_and(|(status, completed)| (status == "done") != completed)
        {
            return Err(invalid("Некорректный или противоречивый статус"));
        }
        let mut connection = self.task_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let mut current = record_by_id(&transaction, task_id)?
            .ok_or_else(|| invalid("Управляемая задача не найдена"))?;
        if current.revision() != expected_revision {
            return Err(invalid(
                "Задача изменилась. Обновите сведения и повторите правку",
            ));
        }
        if let Some(status) = &patch.status {
            current.status = status.clone();
            current.completed = status == "done";
        } else if let Some(completed) = patch.completed {
            current.completed = completed;
            if completed {
                current.status = "done".into()
            } else if current.status == "done" {
                current.status = "todo".into()
            }
        }
        if patch.clear_project {
            current.project = None
        } else if let Some(value) = &patch.project {
            current.project = (!value.trim().is_empty()).then(|| value.trim().to_owned())
        }
        if patch.clear_priority {
            current.priority = None
        } else if let Some(value) = &patch.priority {
            current.priority = Some(value.clone())
        }
        if patch.clear_duration {
            current.duration_minutes = None
        } else if let Some(value) = patch.duration_minutes {
            current.duration_minutes = Some(value)
        }
        if let Some(tags) = &patch.tags {
            current.tags = tags.clone();
            current.tags.sort();
            current.tags.dedup()
        }
        if patch.clear_scheduled {
            current.scheduled = None
        } else if let Some(value) = &patch.scheduled {
            current.scheduled = Some(value.clone())
        }
        if patch.clear_due {
            current.due = None
        } else if let Some(value) = &patch.due {
            current.due = Some(value.clone())
        }
        current.updated_unix_ms = current_unix_ms();
        current.validate()?;
        transaction.execute("UPDATE tasks SET completed=?2,scheduled=?3,due=?4,duration_minutes=?5,priority=?6,updated_unix_ms=?7,status=?8,project=?9,tags_json=?10 WHERE id=?1",params![current.id,current.completed,current.scheduled,current.due,current.duration_minutes,current.priority,current.updated_unix_ms,current.status,current.project,serde_json::to_string(&current.tags).expect("tags")])?;
        transaction.commit()?;
        self.metadata_touch(&[".brandon/tasks.sqlite3"]);
        Ok(task_change(self, current))
    }

    pub fn load_task_views(&self) -> Result<TaskViewsDocument> {
        let path = task_directory(self)?.join("task-views.v1.json");
        checked_file(&path)?;
        let file = match fs::File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(TaskViewsDocument {
                    value: serde_json::json!({"format":"brandon.task-views","version":1,"views":[]}),
                    fingerprint: String::new(),
                });
            }
            Err(error) => return Err(io_error(error)),
        };
        let mut source = Vec::new();
        file.take(1024 * 1024 + 1)
            .read_to_end(&mut source)
            .map_err(io_error)?;
        if source.len() > 1024 * 1024 {
            return Err(invalid("Реестр представлений превышает 1 МиБ"));
        }
        let value = serde_json::from_slice(&source)
            .map_err(|error| invalid(&format!("Некорректный реестр представлений: {error}")))?;
        validate_views(&value)?;
        Ok(TaskViewsDocument {
            value,
            fingerprint: fingerprint(&source),
        })
    }
    pub fn save_task_views(&self, expected: &str, value: &Value) -> Result<TaskViewsDocument> {
        validate_views(value)?;
        let mut connection = self.task_connection()?;
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let current = self.load_task_views()?;
        if current.fingerprint != expected {
            return Err(invalid("Представления изменились извне; запись отменена"));
        }
        let source =
            serde_json::to_vec_pretty(value).map_err(|error| invalid(&error.to_string()))?;
        write_atomic(&task_directory(self)?.join("task-views.v1.json"), &source)?;
        transaction.commit()?;
        Ok(TaskViewsDocument {
            value: value.clone(),
            fingerprint: fingerprint(&source),
        })
    }
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskViewsDocument {
    pub value: Value,
    pub fingerprint: String,
}
pub(crate) fn validate_views(value: &Value) -> Result<()> {
    if value.get("format").and_then(Value::as_str) != Some("brandon.task-views")
        || value.get("version").and_then(Value::as_u64) != Some(1)
    {
        return Err(invalid(
            "Неподдерживаемая версия task views. Исходный файл не перезаписан",
        ));
    }
    let views = value
        .get("views")
        .and_then(Value::as_array)
        .ok_or_else(|| invalid("views должен быть списком"))?;
    if views.len() > 100 || value.to_string().len() > 1024 * 1024 {
        return Err(invalid("Слишком большой реестр task views"));
    }
    let mut ids = HashSet::new();
    for view in views {
        let id = view
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("У представления нет id"))?;
        let name = view
            .get("name")
            .and_then(Value::as_str)
            .ok_or_else(|| invalid("У представления нет name"))?;
        if id.is_empty()
            || id.len() > 128
            || !ids.insert(id)
            || name.trim().is_empty()
            || name.len() > 512
            || !view.is_object()
        {
            return Err(invalid("Некорректное или повторное представление"));
        }
    }
    Ok(())
}
