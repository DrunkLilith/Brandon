//! Portable, staged directory backups. Never replaces a working vault in place.
use super::{
    Result, Vault, VaultEntryKind, VaultError, attachments, current_unix_ms, fingerprint,
    task_storage::{self, TaskRecord, checked_file, invalid, portable_path},
};
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_TOTAL: u64 = 4 * 1024 * 1024 * 1024;
const MAX_FILES: usize = 50000;
const MAX_MANIFEST: u64 = 32 * 1024 * 1024;
const MAX_MTIME: u64 = 253402300799999;
const PRIVATE_FILES: [&str; 1] = ["task-views.v1.json"];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FileRecord {
    path: String,
    size_bytes: u64,
    sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    modified_unix_ms: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: String,
    version: u32,
    task_schema_version: i64,
    vault_name: String,
    created_unix_ms: i64,
    directories: Vec<String>,
    files: Vec<FileRecord>,
    private_files: Vec<FileRecord>,
    database: FileRecord,
    tasks_json: FileRecord,
    tasks_csv: FileRecord,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TaskExport {
    format: String,
    version: u32,
    tasks: Vec<TaskRecord>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBackupSummary {
    pub path: String,
    pub task_count: usize,
    pub file_count: usize,
    pub bytes: u64,
    pub created_unix_ms: i64,
    pub vault_name: String,
}

fn io_error(source: std::io::Error) -> VaultError {
    VaultError::Io {
        context: "Backup задач не завершён",
        source,
    }
}
fn safe_file(root: &Path, relative: &Path) -> Result<PathBuf> {
    let mut target = root.to_path_buf();
    for component in relative.components() {
        target.push(component);
        let meta = fs::symlink_metadata(&target).map_err(io_error)?;
        if attachments::is_link(&meta) {
            return Err(invalid("Backup не следует symlink/junction"));
        }
    }
    let canonical = target.canonicalize().map_err(io_error)?;
    if !canonical.starts_with(root) || !canonical.is_file() {
        return Err(invalid(
            "Backup содержит недоступный или выходящий за корень файл",
        ));
    }
    Ok(canonical)
}
fn hash_file(path: &Path, limit: u64) -> Result<(u64, String)> {
    checked_file(path)?;
    let mut input = fs::File::open(path).map_err(io_error)?;
    let mut size = 0;
    let mut digest = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let length = input.read(&mut buffer).map_err(io_error)?;
        if length == 0 {
            break;
        }
        size += length as u64;
        if size > limit {
            return Err(invalid("Файл backup превышает допустимый размер"));
        }
        digest.update(&buffer[..length]);
    }
    Ok((size, hex::encode(digest.finalize())))
}
fn copy_checked(source: &Path, target: &Path, expected: Option<&FileRecord>) -> Result<FileRecord> {
    checked_file(source)?;
    let before = fs::metadata(source).map_err(io_error)?;
    if !before.is_file() || before.len() > MAX_FILE {
        return Err(invalid("Один файл backup превышает 512 МиБ"));
    }
    fs::create_dir_all(
        target
            .parent()
            .ok_or_else(|| invalid("Некорректный staging path"))?,
    )
    .map_err(io_error)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)
        .map_err(io_error)?;
    let mut input = fs::File::open(source).map_err(io_error)?;
    let mut digest = Sha256::new();
    let mut size = 0;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let length = input.read(&mut buffer).map_err(io_error)?;
        if length == 0 {
            break;
        }
        size += length as u64;
        if size > MAX_FILE {
            return Err(invalid("Файл вырос во время backup"));
        }
        digest.update(&buffer[..length]);
        output.write_all(&buffer[..length]).map_err(io_error)?;
    }
    output.sync_all().map_err(io_error)?;
    let after = fs::metadata(source).map_err(io_error)?;
    let sha256 = hex::encode(digest.finalize());
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || size != after.len()
        || expected.is_some_and(|record| record.size_bytes != size || record.sha256 != sha256)
    {
        return Err(invalid(
            "Источник изменился или hash backup не совпал. Ничего не опубликовано",
        ));
    }
    let modified_unix_ms = expected
        .and_then(|record| record.modified_unix_ms)
        .or_else(|| {
            before
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_millis()
                .try_into()
                .ok()
        });
    if let Some(value) = modified_unix_ms {
        if value > MAX_MTIME {
            return Err(invalid(
                "Дата ресурса за пределами поддерживаемого диапазона backup",
            ));
        }
        output
            .set_times(
                fs::FileTimes::new()
                    .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_millis(value)),
            )
            .map_err(io_error)?;
        output.sync_all().map_err(io_error)?;
    }
    Ok(FileRecord {
        path: String::new(),
        size_bytes: size,
        sha256,
        modified_unix_ms,
    })
}
fn write_file(path: &Path, bytes: &[u8]) -> Result<FileRecord> {
    if bytes.len() as u64 > MAX_FILE {
        return Err(invalid("Экспорт превышает лимит размера"));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(io_error)?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(io_error)?;
    Ok(FileRecord {
        path: String::new(),
        size_bytes: bytes.len() as u64,
        sha256: fingerprint(bytes),
        modified_unix_ms: None,
    })
}
fn read_checked(root: &Path, record: &FileRecord, limit: u64) -> Result<Vec<u8>> {
    if record.size_bytes > limit {
        return Err(invalid("Слишком большой manifest/export"));
    }
    let path = safe_file(root, Path::new(&record.path))?;
    let (length, hash) = hash_file(&path, limit)?;
    if length != record.size_bytes || hash != record.sha256 {
        return Err(invalid("Hash или размер backup не совпадает"));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io_error)?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 != length || fingerprint(&bytes) != record.sha256 {
        return Err(invalid("Backup изменился во время чтения"));
    }
    Ok(bytes)
}
fn csv_cell(value: &str) -> String {
    let trimmed = value.trim_start();
    let prefix = if trimmed.starts_with(['=', '+', '-', '@', '\t', '\r']) {
        "'"
    } else {
        ""
    };
    format!("\"{prefix}{}\"", value.replace('"', "\"\""))
}
fn tasks_csv(tasks: &[TaskRecord]) -> String {
    let mut output = String::from(
        "TaskId,Title,Status,Project,Priority,Scheduled,Due,DurationMinutes,Tags,Description\r\n",
    );
    for task in tasks {
        let values = [
            task.id.clone(),
            task.title.clone(),
            task.status.clone(),
            task.project.clone().unwrap_or_default(),
            task.priority.clone().unwrap_or_default(),
            task.scheduled.clone().unwrap_or_default(),
            task.due.clone().unwrap_or_default(),
            task.duration_minutes
                .map(|n| n.to_string())
                .unwrap_or_default(),
            task.tags.join(";"),
            format!("vault/{}", task.note_path),
        ];
        output.push_str(
            &values
                .iter()
                .map(|s| csv_cell(s))
                .collect::<Vec<_>>()
                .join(","),
        );
        output.push_str("\r\n");
    }
    output
}
fn selected_parent(path: &Path, excluded: &[&Path]) -> Result<PathBuf> {
    let parent = path.canonicalize().map_err(io_error)?;
    if !parent.is_dir() || excluded.iter().any(|root| parent.starts_with(root)) {
        return Err(invalid("Выберите папку вне исходного vault и backup"));
    }
    Ok(parent)
}
fn publish(stage: tempfile::TempDir, parent: &Path, prefix: &str) -> Result<PathBuf> {
    let target = parent.join(format!(
        "{prefix}-{}-{}",
        current_unix_ms(),
        super::new_task_id()
    ));
    if target.exists() {
        return Err(invalid("Целевая папка уже существует"));
    }
    fs::rename(stage.path(), &target).map_err(io_error)?;
    let _ = stage.keep();
    Ok(target)
}
fn summary(manifest: &Manifest, path: &Path, tasks: usize) -> TaskBackupSummary {
    TaskBackupSummary {
        path: path.to_string_lossy().into_owned(),
        task_count: tasks,
        file_count: manifest.files.len(),
        bytes: manifest
            .files
            .iter()
            .chain(&manifest.private_files)
            .chain([
                &manifest.database,
                &manifest.tasks_json,
                &manifest.tasks_csv,
            ])
            .map(|file| file.size_bytes)
            .sum(),
        created_unix_ms: manifest.created_unix_ms,
        vault_name: manifest.vault_name.clone(),
    }
}

impl Vault {
    /// Consistent SQLite snapshot includes committed WAL frames; resources are
    /// copied and rehashed before publishing. An external edit aborts publication.
    pub fn backup_tasks(&self, parent: impl AsRef<Path>) -> Result<TaskBackupSummary> {
        self.backup_impl(parent, || Ok(()))
    }
    fn backup_impl(
        &self,
        parent: impl AsRef<Path>,
        after_copy: impl FnOnce() -> Result<()>,
    ) -> Result<TaskBackupSummary> {
        let parent = selected_parent(parent.as_ref(), &[&self.root])?;
        let stage = tempfile::Builder::new()
            .prefix(".brandon-backup-stage-")
            .tempdir_in(&parent)
            .map_err(io_error)?;
        let vault_stage = stage.path().join("vault");
        fs::create_dir(&vault_stage).map_err(io_error)?;
        let private = vault_stage.join(".brandon");
        fs::create_dir(&private).map_err(io_error)?;
        let raw = Vault::open(&self.root)?;
        let inventory = raw.snapshot()?;
        if inventory.entries.len() > MAX_FILES + 10000 {
            return Err(invalid("Backup ограничен 50000 файлами и 10000 папками"));
        }
        let connection = self.task_connection()?;
        let db_path = private.join("tasks.sqlite3");
        connection.execute("VACUUM INTO ?1", [db_path.to_string_lossy().as_ref()])?;
        let snapshot = Connection::open_with_flags(&db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        snapshot.execute_batch("PRAGMA trusted_schema=OFF;PRAGMA query_only=ON;")?;
        let check: String = snapshot.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if check != "ok" {
            return Err(invalid("Task DB не прошла проверку целостности"));
        }
        let tasks = task_storage::records(&snapshot)?;
        drop(snapshot);
        drop(connection);
        if tasks.len() > MAX_FILES {
            return Err(invalid("Слишком много задач для backup"));
        }
        let mut files = Vec::new();
        let mut directories = Vec::new();
        let mut bytes = 0_u64;
        for entry in &inventory.entries {
            let relative = portable_path(&entry.path)?;
            if entry.kind == VaultEntryKind::Folder {
                if directories.len() >= 10000 {
                    return Err(invalid("Backup ограничен 10000 папками"));
                }
                directories.push(entry.path.clone());
                fs::create_dir_all(vault_stage.join(relative)).map_err(io_error)?;
                continue;
            }
            if files.len() >= MAX_FILES {
                return Err(invalid("Backup ограничен 50000 файлами"));
            }
            let source = safe_file(&self.root, &relative)?;
            let mut record = copy_checked(&source, &vault_stage.join(relative), None)?;
            record.path = entry.path.clone();
            bytes += record.size_bytes;
            if bytes > MAX_TOTAL {
                return Err(invalid("Backup превышает лимит 4 ГиБ"));
            }
            files.push(record);
        }
        let known: HashSet<_> = files.iter().map(|file| file.path.as_str()).collect();
        if tasks
            .iter()
            .any(|task| !known.contains(task.note_path.as_str()))
        {
            return Err(invalid(
                "Описание задачи отсутствует; неполный backup не опубликован",
            ));
        }
        let mut private_files = Vec::new();
        let source_private = task_storage::task_directory(self)?;
        for name in PRIVATE_FILES {
            let path = source_private.join(name);
            checked_file(&path)?;
            if path.exists() {
                if fs::metadata(&path).map_err(io_error)?.len() > 1024 * 1024 {
                    return Err(invalid("Конфигурация task views превышает 1 МиБ"));
                }
                let mut record = copy_checked(&path, &private.join(name), None)?;
                record.path = name.into();
                private_files.push(record)
            }
        }
        after_copy()?;
        // Recheck all file bytes, not only mtime/size (same-mtime edits count).
        for file in &files {
            let path = safe_file(&self.root, &portable_path(&file.path)?)?;
            let (size, hash) = hash_file(&path, MAX_FILE)?;
            if size != file.size_bytes || hash != file.sha256 {
                return Err(invalid(
                    "Ресурс изменился во время backup; повторите операцию",
                ));
            }
        }
        let fresh = raw.snapshot()?;
        let initial_paths: HashSet<_> = inventory
            .entries
            .iter()
            .map(|entry| (&entry.path, entry.kind))
            .collect();
        let fresh_paths: HashSet<_> = fresh
            .entries
            .iter()
            .map(|entry| (&entry.path, entry.kind))
            .collect();
        if initial_paths != fresh_paths {
            return Err(invalid(
                "Состав vault изменился во время backup; повторите операцию",
            ));
        }
        let export = serde_json::to_vec_pretty(&TaskExport {
            format: "brandon.tasks-export".into(),
            version: 1,
            tasks: tasks.clone(),
        })
        .map_err(|error| invalid(&error.to_string()))?;
        let csv_source = tasks_csv(&tasks);
        if export.len() as u64 > MAX_MANIFEST || csv_source.len() as u64 > MAX_MANIFEST {
            return Err(invalid("JSON/CSV export превышает 32 МиБ"));
        }
        let mut json = write_file(&stage.path().join("tasks.json"), &export)?;
        json.path = "tasks.json".into();
        let mut csv = write_file(&stage.path().join("tasks.csv"), csv_source.as_bytes())?;
        csv.path = "tasks.csv".into();
        let (size, hash) = hash_file(&db_path, MAX_FILE)?;
        let database = FileRecord {
            path: "vault/.brandon/tasks.sqlite3".into(),
            size_bytes: size,
            sha256: hash,
            modified_unix_ms: None,
        };
        let manifest = Manifest {
            format: "brandon.vault-task-backup".into(),
            version: 1,
            task_schema_version: task_storage::SCHEMA_VERSION,
            vault_name: inventory.name,
            created_unix_ms: current_unix_ms(),
            directories,
            files,
            private_files,
            database,
            tasks_json: json,
            tasks_csv: csv,
        };
        let encoded =
            serde_json::to_vec_pretty(&manifest).map_err(|error| invalid(&error.to_string()))?;
        if encoded.len() as u64 > MAX_MANIFEST {
            return Err(invalid("Manifest превышает 32 МиБ"));
        }
        write_file(&stage.path().join("manifest.json"), &encoded)?;
        // A produced archive must satisfy the exact reader contract before it
        // becomes visible as a completed backup (including cross-platform paths).
        validate_backup(stage.path())?;
        let target = publish(stage, &parent, "Brandon-backup")?;
        Ok(summary(&manifest, &target, tasks.len()))
    }

    pub fn inspect_task_backup(path: impl AsRef<Path>) -> Result<TaskBackupSummary> {
        let (root, manifest, tasks) = validate_backup(path.as_ref())?;
        Ok(summary(&manifest, &root, tasks.len()))
    }

    pub fn restore_task_backup(
        &self,
        backup: impl AsRef<Path>,
        parent: impl AsRef<Path>,
    ) -> Result<TaskBackupSummary> {
        self.restore_impl(backup, parent, |_| Ok(()))
    }
    fn restore_impl(
        &self,
        backup: impl AsRef<Path>,
        parent: impl AsRef<Path>,
        before_publish: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<TaskBackupSummary> {
        let (backup, manifest, tasks) = validate_backup(backup.as_ref())?;
        let parent = selected_parent(parent.as_ref(), &[&backup, &self.root])?;
        let stage = tempfile::Builder::new()
            .prefix(".brandon-restore-stage-")
            .tempdir_in(&parent)
            .map_err(io_error)?;
        for directory in &manifest.directories {
            fs::create_dir_all(stage.path().join(portable_path(directory)?)).map_err(io_error)?;
        }
        for file in &manifest.files {
            let relative = portable_path(&file.path)?;
            let source = safe_file(&backup.join("vault"), &relative)?;
            copy_checked(&source, &stage.path().join(relative), Some(file))?;
        }
        let restored = Vault::open(stage.path())?;
        let mut connection = restored.task_connection()?;
        let transaction = connection.transaction()?;
        for task in &tasks {
            task_storage::insert_record(&transaction, task)?;
        }
        transaction.commit()?;
        let integrity: String =
            connection.query_row("PRAGMA integrity_check", [], |row| row.get(0))?;
        if integrity != "ok" {
            return Err(invalid("Восстановленная DB не прошла integrity_check"));
        }
        drop(connection);
        let private = task_storage::task_directory(&restored)?;
        for file in &manifest.private_files {
            copy_checked(
                &safe_file(&backup.join("vault/.brandon"), Path::new(&file.path))?,
                &private.join(&file.path),
                Some(file),
            )?;
        }
        copy_checked(
            &safe_file(&backup, Path::new(&manifest.database.path))?,
            &private.join("restore-source.sqlite3"),
            Some(&manifest.database),
        )?;
        // Validate ordinary resources and stable TaskIds without importing tasks
        // from arbitrary Markdown/Tasks folders during the restore operation.
        let loaded = task_storage::records(&restored.task_connection()?)?;
        if loaded.len() != tasks.len() {
            return Err(invalid("Число восстановленных задач не совпало"));
        }
        before_publish(stage.path())?;
        let target = publish(stage, &parent, "Brandon-restored")?;
        Ok(summary(&manifest, &target, tasks.len()))
    }
}

fn validate_backup(path: &Path) -> Result<(PathBuf, Manifest, Vec<TaskRecord>)> {
    let root = path.canonicalize().map_err(io_error)?;
    if !root.is_dir() {
        return Err(invalid("Выберите папку backup"));
    }
    let manifest_path = safe_file(&root, Path::new("manifest.json"))?;
    if fs::metadata(&manifest_path).map_err(io_error)?.len() > MAX_MANIFEST {
        return Err(invalid("Manifest превышает 32 МиБ"));
    }
    let mut source = Vec::new();
    fs::File::open(&manifest_path)
        .map_err(io_error)?
        .take(MAX_MANIFEST + 1)
        .read_to_end(&mut source)
        .map_err(io_error)?;
    if source.len() as u64 > MAX_MANIFEST {
        return Err(invalid("Manifest слишком велик"));
    }
    let manifest: Manifest = serde_json::from_slice(&source)
        .map_err(|error| invalid(&format!("Некорректный manifest: {error}")))?;
    if manifest.format != "brandon.vault-task-backup"
        || manifest.version != 1
        || manifest.task_schema_version != task_storage::SCHEMA_VERSION
        || manifest.files.len() > MAX_FILES
        || manifest.directories.len() > 10000
        || manifest.created_unix_ms < 0
    {
        return Err(invalid("Неподдерживаемый или слишком большой backup"));
    }
    if manifest.database.path != "vault/.brandon/tasks.sqlite3"
        || manifest.tasks_json.path != "tasks.json"
        || manifest.tasks_csv.path != "tasks.csv"
        || manifest
            .private_files
            .iter()
            .any(|file| !PRIVATE_FILES.contains(&file.path.as_str()))
    {
        return Err(invalid("Некорректные служебные пути backup"));
    }
    let mut names = HashSet::new();
    let mut total = 0;
    for directory in &manifest.directories {
        portable_path(directory)?;
        if !names.insert(directory.to_lowercase()) {
            return Err(invalid("Повторный или неоднозначный путь в backup"));
        }
    }
    for file in &manifest.files {
        let relative = portable_path(&file.path)?;
        if !names.insert(file.path.to_lowercase())
            || file.size_bytes > MAX_FILE
            || file.sha256.len() != 64
            || file.modified_unix_ms.is_some_and(|value| value > MAX_MTIME)
        {
            return Err(invalid("Некорректный или повторный ресурс backup"));
        }
        total += file.size_bytes;
        if total > MAX_TOTAL {
            return Err(invalid("Backup превышает 4 ГиБ"));
        }
        let path = safe_file(&root.join("vault"), &relative)?;
        let (size, hash) = hash_file(&path, MAX_FILE)?;
        if size != file.size_bytes || hash != file.sha256 {
            return Err(invalid("Hash ресурса backup не совпал"));
        }
    }
    for file in &manifest.private_files {
        if file.modified_unix_ms.is_some_and(|value| value > MAX_MTIME) {
            return Err(invalid("Некорректная дата ресурса backup"));
        }
        let path = safe_file(&root.join("vault/.brandon"), Path::new(&file.path))?;
        let (size, hash) = hash_file(&path, 1024 * 1024)?;
        if size != file.size_bytes || hash != file.sha256 {
            return Err(invalid("Hash конфигурации backup не совпал"));
        }
    }
    let db = safe_file(&root, Path::new(&manifest.database.path))?;
    let (size, hash) = hash_file(&db, MAX_FILE)?;
    if size != manifest.database.size_bytes || hash != manifest.database.sha256 {
        return Err(invalid("SQLite snapshot повреждён"));
    }
    let export = read_checked(&root, &manifest.tasks_json, MAX_MANIFEST)?;
    let export: TaskExport = serde_json::from_slice(&export)
        .map_err(|error| invalid(&format!("Некорректный JSON export: {error}")))?;
    if export.format != "brandon.tasks-export"
        || export.version != 1
        || export.tasks.len() > MAX_FILES
    {
        return Err(invalid("Неподдерживаемый tasks export"));
    }
    read_checked(&root, &manifest.tasks_csv, MAX_MANIFEST)?;
    let mut ids = HashSet::new();
    let mut paths = HashSet::new();
    let files: HashSet<_> = manifest
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    for task in &export.tasks {
        task.validate()?;
        if !ids.insert(&task.id)
            || !paths.insert(task.note_path.to_lowercase())
            || !files.contains(task.note_path.as_str())
        {
            return Err(invalid(
                "Повторная задача или отсутствующее Markdown-описание",
            ));
        }
    }
    // Never open an untrusted archive DB for writing or run its schema. The
    // known typed export is the restore input; the snapshot remains evidence.
    Ok((root, manifest, export.tasks))
}

#[cfg(test)]
mod tests {
    use super::super::ManagedTaskPatch;
    use super::*;
    fn source() -> (tempfile::TempDir, Vault) {
        let directory = tempfile::tempdir().unwrap();
        let vault = Vault::open(directory.path()).unwrap();
        (directory, vault)
    }
    fn manifest(path: &str) -> Manifest {
        serde_json::from_slice(&fs::read(Path::new(path).join("manifest.json")).unwrap()).unwrap()
    }
    fn write_manifest(path: &str, value: &Manifest) {
        fs::write(
            Path::new(path).join("manifest.json"),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap()
    }
    #[test]
    fn consistent_wal_backup_and_clean_folder_restore_keep_task_ids_fields_notes_assets_and_views()
    {
        let (source, vault) = source();
        let output = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let created = vault
            .create_task_record("Test task", Some("2026-10-19"))
            .unwrap();
        let patched = vault
            .patch_managed_task(
                &created.record.id,
                &created.revision,
                &ManagedTaskPatch {
                    status: Some("doing".into()),
                    project: Some("Учёба".into()),
                    priority: Some("urgent".into()),
                    tags: Some(vec!["one".into()]),
                    duration_minutes: Some(45),
                    due: Some("2026-10-25".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let note = source.path().join(&patched.record.note_path);
        let contents = "---\r\n# retained\r\ncustom: {unknown: true}\r\n---\r\n# Task\r\n\r\n![[Assets/blob.bin]]\r\n";
        fs::write(&note, contents).unwrap();
        let note_mtime = std::time::UNIX_EPOCH + std::time::Duration::from_millis(1700000000123);
        fs::OpenOptions::new()
            .write(true)
            .open(&note)
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(note_mtime))
            .unwrap();
        fs::create_dir(source.path().join("Assets")).unwrap();
        fs::create_dir(source.path().join("Empty folder")).unwrap();
        let binary = vec![0, 255, 42, 0, 128, 127];
        fs::write(source.path().join("Assets/blob.bin"), &binary).unwrap();
        fs::write(
            source.path().join("View.base"),
            "unknown: {keep: true}\nviews: [{type: table}]\n",
        )
        .unwrap();
        let views = serde_json::json!({"format":"brandon.task-views","version":1,"future":{"x":1},"views":[{"id":"v","name":"Study","mode":"list","filter":{"project":"Учёба"}}]});
        vault.save_task_views("", &views).unwrap();
        let connection = vault.task_connection().unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=WAL;PRAGMA wal_autocheckpoint=0;")
            .unwrap();
        connection
            .execute(
                "UPDATE tasks SET project='WAL committed' WHERE id=?1",
                [&created.record.id],
            )
            .unwrap();
        assert!(source.path().join(".brandon/tasks.sqlite3-wal").exists());
        let backed = vault.backup_tasks(output.path()).unwrap();
        assert_eq!(backed.task_count, 1);
        let snapshot = Connection::open_with_flags(
            Path::new(&backed.path).join("vault/.brandon/tasks.sqlite3"),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            snapshot
                .query_row("SELECT project FROM tasks", [], |row| row
                    .get::<_, String>(0))
                .unwrap(),
            "WAL committed"
        );
        drop(snapshot);
        assert_eq!(
            Vault::inspect_task_backup(&backed.path).unwrap().file_count,
            backed.file_count
        );
        let restored = vault
            .restore_task_backup(&backed.path, destination.path())
            .unwrap();
        let restored_root = Path::new(&restored.path);
        let restored_vault = Vault::open(restored_root).unwrap();
        let records = task_storage::records(&restored_vault.task_connection().unwrap()).unwrap();
        assert_eq!(records[0].id, created.record.id);
        assert_eq!(records[0].status, "doing");
        assert_eq!(records[0].project.as_deref(), Some("WAL committed"));
        assert_eq!(records[0].scheduled.as_deref(), Some("2026-10-19"));
        assert_eq!(records[0].due.as_deref(), Some("2026-10-25"));
        assert_eq!(records[0].tags, ["one"]);
        assert_eq!(
            fs::read_to_string(restored_root.join(&created.record.note_path)).unwrap(),
            contents
        );
        assert_eq!(
            fs::metadata(restored_root.join(&created.record.note_path))
                .unwrap()
                .modified()
                .unwrap(),
            note_mtime
        );
        assert_eq!(
            fs::read(restored_root.join("Assets/blob.bin")).unwrap(),
            binary
        );
        assert!(restored_root.join("Empty folder").is_dir());
        assert_eq!(restored_vault.load_task_views().unwrap().value, views);
        assert!(
            restored_root
                .join(".brandon/restore-source.sqlite3")
                .exists()
        );
        assert!(!restored_root.join(".brandon/search-v1.sqlite3").exists());
        assert!(!restored_root.join(".brandon/tasks.sqlite3-wal").exists());
        assert_eq!(fs::read_to_string(note).unwrap(), contents);
        drop(connection);
    }
    #[test]
    fn tampering_unsupported_versions_and_traversal_fail_before_publication() {
        let (source, vault) = source();
        vault.create_task_record("Task", None).unwrap();
        let output = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::write(destination.path().join("user-file"), "KEEP").unwrap();
        let backup = vault.backup_tasks(output.path()).unwrap();
        let original = manifest(&backup.path);
        for mutate in [0, 1, 2, 3] {
            let mut bad = original.clone();
            match mutate {
                0 => bad.version = 99,
                1 => bad.files[0].path = "../escape.md".into(),
                2 => bad.files.push(bad.files[0].clone()),
                _ => bad.files[0].modified_unix_ms = Some(MAX_MTIME + 1),
            };
            write_manifest(&backup.path, &bad);
            assert!(Vault::inspect_task_backup(&backup.path).is_err());
            assert!(
                vault
                    .restore_task_backup(&backup.path, destination.path())
                    .is_err()
            );
            assert_eq!(
                fs::read_to_string(destination.path().join("user-file")).unwrap(),
                "KEEP"
            );
            assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 1);
        }
        write_manifest(&backup.path, &original);
        let file = &original.files[0];
        fs::write(
            Path::new(&backup.path).join("vault").join(&file.path),
            "TAMPERED",
        )
        .unwrap();
        assert!(Vault::inspect_task_backup(&backup.path).is_err());
        assert!(
            vault
                .restore_task_backup(&backup.path, destination.path())
                .is_err()
        );
        assert!(source.path().exists());
    }
    #[test]
    fn duplicate_task_ids_even_with_rehashed_export_cannot_be_restored() {
        let (_source, vault) = source();
        vault.create_task_record("Task", None).unwrap();
        let output = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let backup = vault.backup_tasks(output.path()).unwrap();
        let mut manifest = manifest(&backup.path);
        let path = Path::new(&backup.path).join("tasks.json");
        let mut export: TaskExport = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        export.tasks.push(export.tasks[0].clone());
        let bytes = serde_json::to_vec(&export).unwrap();
        fs::write(path, &bytes).unwrap();
        manifest.tasks_json.size_bytes = bytes.len() as u64;
        manifest.tasks_json.sha256 = fingerprint(&bytes);
        write_manifest(&backup.path, &manifest);
        assert!(
            vault
                .restore_task_backup(&backup.path, destination.path())
                .is_err()
        );
        assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
    }
    #[test]
    fn interrupted_staging_and_changed_sources_never_publish_partial_folders() {
        let (source, vault) = source();
        vault.create_task_record("Task", None).unwrap();
        let output = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        let backup = vault.backup_tasks(output.path()).unwrap();
        assert!(
            vault
                .restore_impl(&backup.path, destination.path(), |stage| {
                    assert!(stage.join(".brandon/tasks.sqlite3").exists());
                    Err(invalid("simulated interruption before publish"))
                })
                .is_err()
        );
        assert_eq!(fs::read_dir(destination.path()).unwrap().count(), 0);
        let file = source.path().join("Extra.md");
        fs::write(&file, "# Before\n").unwrap();
        let outputs_before = fs::read_dir(output.path()).unwrap().count();
        assert!(
            vault
                .backup_impl(output.path(), || {
                    fs::write(&file, "# Change\n").unwrap();
                    Ok(())
                })
                .is_err()
        );
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), outputs_before);
    }
    #[test]
    fn bad_task_db_is_not_silently_recreated_or_exported() {
        let (source, vault) = source();
        let connection = vault.task_connection().unwrap();
        drop(connection);
        let database = source.path().join(".brandon/tasks.sqlite3");
        fs::write(&database, b"not sqlite").unwrap();
        let output = tempfile::tempdir().unwrap();
        assert!(vault.backup_tasks(output.path()).is_err());
        assert_eq!(fs::read(&database).unwrap(), b"not sqlite");
        assert_eq!(fs::read_dir(output.path()).unwrap().count(), 0);
    }
    #[test]
    fn export_csv_neutralizes_spreadsheet_formulas_without_mutating_json_values() {
        assert_eq!(csv_cell("=HYPERLINK(\"x\")"), "\"'=HYPERLINK(\"\"x\"\")\"");
        assert_eq!(csv_cell("normal, text"), "\"normal, text\"");
    }
}
