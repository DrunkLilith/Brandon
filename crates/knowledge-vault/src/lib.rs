//! Local-first filesystem boundary for portable knowledge vaults.

mod attachments;
pub use attachments::{AttachmentImport, AttachmentInfo, AttachmentKind};
mod search;
pub use search::{
    SearchHit, SearchInventory, SearchMode, SearchRebuild, SearchResponse, SearchUpdate,
};
mod relocation;
pub use relocation::{LinkUpdateReason, LinkUpdateWarning};
#[cfg(test)]
mod relocation_tests;

use chrono::{NaiveDate, NaiveDateTime};
use knowledge_markdown::{DocumentMetadata, TaskMetadata, parse_document, rewrite_task_source};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VaultError {
    #[error("Индексация отменена")]
    SearchCancelled,
    #[error("{context}: {source}")]
    Io {
        context: &'static str,
        #[source]
        source: io::Error,
    },
    #[error("{0}")]
    InvalidPath(&'static str),
    #[error("{0}")]
    InvalidName(&'static str),
    #[error("Объект с таким названием уже существует")]
    AlreadyExists,
    #[error("Локальная база данных недоступна: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("База задач недоступна; восстановление поискового кэша её не исправит: {0}")]
    SearchTasks(Box<VaultError>),
    #[error(
        "Восстановление поиска не завершено. Резервная папка: {backup}; часть прежнего кэша может оставаться на месте: {source}"
    )]
    SearchRecovery {
        backup: String,
        #[source]
        source: Box<VaultError>,
    },
}

pub type Result<T> = std::result::Result<T, VaultError>;

#[derive(Clone, Debug)]
pub struct Vault {
    root: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultFile {
    pub path: String,
    pub name: String,
    pub folder: String,
    pub modified_unix_ms: u128,
    pub size_bytes: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VaultEntryKind {
    File,
    Folder,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultEntry {
    pub path: String,
    pub name: String,
    pub parent: String,
    pub kind: VaultEntryKind,
    pub extension: Option<String>,
    pub modified_unix_ms: u128,
    pub size_bytes: u64,
    pub is_note: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct VaultSnapshot {
    pub root: String,
    pub name: String,
    pub files: Vec<VaultFile>,
    pub entries: Vec<VaultEntry>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NoteDocument {
    pub path: String,
    pub content: String,
    pub fingerprint: String,
    pub modified_unix_ms: u128,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum SaveOutcome {
    Saved {
        snapshot: VaultSnapshot,
        document: NoteDocument,
    },
    Conflict {
        disk: NoteDocument,
    },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultMutation {
    pub snapshot: VaultSnapshot,
    pub path: String,
    pub link_warnings: Vec<LinkUpdateWarning>,
}

pub type CreatedNote = VaultMutation;

#[derive(Clone, Debug, Serialize)]
pub struct VaultIndex {
    pub documents: Vec<DocumentMetadata>,
    pub tags: BTreeMap<String, usize>,
    pub tasks: Vec<TaskMetadata>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskIndexMutation {
    pub snapshot: VaultSnapshot,
    pub index: VaultIndex,
}

impl Vault {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let root = path
            .as_ref()
            .canonicalize()
            .map_err(|source| VaultError::Io {
                context: "Папка недоступна",
                source,
            })?;
        if !root.is_dir() {
            return Err(VaultError::InvalidPath("Выбранный путь не является папкой"));
        }
        Ok(Self { root })
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    fn task_connection(&self) -> Result<Connection> {
        let directory = self.root.join(".brandon");
        fs::create_dir_all(&directory).map_err(|source| VaultError::Io {
            context: "Не удалось подготовить локальную базу Brandon",
            source,
        })?;
        let connection = Connection::open(directory.join("tasks.sqlite3"))?;
        connection.execute_batch(
            "PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 3000;
             CREATE TABLE IF NOT EXISTS tasks (
               id TEXT PRIMARY KEY,
               note_path TEXT NOT NULL UNIQUE,
               title TEXT NOT NULL,
               completed INTEGER NOT NULL DEFAULT 0,
               scheduled TEXT,
               due TEXT,
               duration_minutes INTEGER,
               priority TEXT,
               created_unix_ms INTEGER NOT NULL,
               updated_unix_ms INTEGER NOT NULL
             );
             CREATE INDEX IF NOT EXISTS tasks_scheduled_idx ON tasks(scheduled);
             CREATE INDEX IF NOT EXISTS tasks_due_idx ON tasks(due);",
        )?;
        Ok(connection)
    }

    fn sync_managed_task_files(&self, snapshot: &VaultSnapshot) -> Result<()> {
        let connection = self.task_connection()?;
        let known_paths = {
            let mut statement = connection.prepare("SELECT note_path FROM tasks")?;
            statement
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<std::result::Result<HashSet<_>, _>>()?
        };
        for file in snapshot.files.iter().filter(|file| {
            file.path.starts_with("Tasks/") && !file.path.eq_ignore_ascii_case("Tasks/Inbox.md")
        }) {
            if known_paths.contains(&file.path) {
                continue;
            }
            let path = self.secure_existing_note(&file.path)?;
            let source = fs::read_to_string(&path).map_err(|source| VaultError::Io {
                context: "Не удалось прочитать файл задачи",
                source,
            })?;
            let document = parse_document(&file.path, &source, file.modified_unix_ms);
            let legacy = (document.tasks.len() == 1
                && document.tasks[0]
                    .block_id
                    .as_deref()
                    .is_some_and(|id| id.starts_with("task-")))
            .then(|| document.tasks[0].clone());
            let id = legacy
                .as_ref()
                .map_or_else(new_task_id, |task| task.id.clone());
            let now = current_unix_ms();
            connection.execute(
                "INSERT INTO tasks (id, note_path, title, completed, scheduled, due, duration_minutes, priority, created_unix_ms, updated_unix_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                params![
                    id,
                    file.path,
                    document.title,
                    legacy.as_ref().is_some_and(|task| task.completed),
                    legacy.as_ref().and_then(|task| task.scheduled.as_deref()),
                    legacy.as_ref().and_then(|task| task.due.as_deref()),
                    legacy.as_ref().and_then(|task| task.duration_minutes),
                    legacy.as_ref().and_then(|task| task.priority.as_deref()),
                    now,
                ],
            )?;
            if let Some(task) = legacy {
                let cleaned = without_line(&source, task.line);
                if let Err(error) = write_atomic(&path, cleaned.as_bytes()) {
                    let _ = connection.execute("DELETE FROM tasks WHERE id = ?1", [&id]);
                    return Err(error);
                }
            }
        }
        Ok(())
    }

    fn managed_tasks(&self, documents: &[DocumentMetadata]) -> Result<Vec<TaskMetadata>> {
        let documents = documents
            .iter()
            .map(|document| (document.path.as_str(), document))
            .collect::<HashMap<_, _>>();
        let connection = self.task_connection()?;
        let mut statement = connection.prepare(
            "SELECT id, note_path, title, completed, scheduled, due, duration_minutes, priority
             FROM tasks ORDER BY completed, COALESCE(scheduled, due, '9999-12-31'), title",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, bool>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<String>>(5)?,
                row.get::<_, Option<u32>>(6)?,
                row.get::<_, Option<String>>(7)?,
            ))
        })?;
        let mut tasks = Vec::new();
        for row in rows {
            let (id, path, stored_title, completed, scheduled, due, duration_minutes, priority) =
                row?;
            let Some(document) = documents.get(path.as_str()) else {
                continue;
            };
            tasks.push(TaskMetadata {
                id,
                managed: true,
                block_id: None,
                path,
                line: 1,
                title: if document.title.trim().is_empty() {
                    stored_title
                } else {
                    document.title.clone()
                },
                completed,
                scheduled,
                due,
                duration_minutes,
                priority,
                tags: document.tags.clone(),
            });
        }
        Ok(tasks)
    }

    fn update_managed_task(
        &self,
        task_id: &str,
        completed: Option<bool>,
        scheduled: Option<Option<&str>>,
        due: Option<Option<&str>>,
    ) -> Result<bool> {
        let connection = self.task_connection()?;
        let stored = connection
            .query_row(
                "SELECT completed, scheduled, due FROM tasks WHERE id = ?1",
                [task_id],
                |row| {
                    Ok((
                        row.get::<_, bool>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                    ))
                },
            )
            .optional()?;
        let Some((stored_completed, stored_scheduled, stored_due)) = stored else {
            return Ok(false);
        };
        let next_completed = completed.unwrap_or(stored_completed);
        let next_scheduled = scheduled
            .map(|value| value.map(str::to_owned))
            .unwrap_or(stored_scheduled);
        let next_due = due
            .map(|value| value.map(str::to_owned))
            .unwrap_or(stored_due);
        connection.execute(
            "UPDATE tasks SET completed = ?2, scheduled = ?3, due = ?4, updated_unix_ms = ?5 WHERE id = ?1",
            params![task_id, next_completed, next_scheduled, next_due, current_unix_ms()],
        )?;
        Ok(true)
    }

    fn rewrite_managed_task_paths(
        &self,
        old_path: &str,
        new_path: &str,
        directory: bool,
    ) -> Result<()> {
        let mut connection = self.task_connection()?;
        let transaction = connection.transaction()?;
        let rows = {
            let mut statement = transaction.prepare("SELECT id, note_path FROM tasks")?;
            statement
                .query_map([], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };
        let old_prefix = format!("{}/", old_path.trim_end_matches('/'));
        let new_prefix = format!("{}/", new_path.trim_end_matches('/'));
        for (id, path) in rows {
            let next = if path == old_path {
                Some(new_path.to_owned())
            } else if directory {
                path.strip_prefix(&old_prefix)
                    .map(|suffix| format!("{new_prefix}{suffix}"))
            } else {
                None
            };
            if let Some(next) = next {
                transaction.execute(
                    "UPDATE tasks SET note_path = ?2, updated_unix_ms = ?3 WHERE id = ?1",
                    params![id, next, current_unix_ms()],
                )?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    fn remove_managed_task_paths(&self, path: &str, directory: bool) -> Result<()> {
        let connection = self.task_connection()?;
        if directory {
            let prefix = format!("{}/", path.trim_end_matches('/'));
            connection.execute(
                "DELETE FROM tasks WHERE note_path = ?1 OR instr(note_path, ?2) = 1",
                params![path, prefix],
            )?;
        } else {
            connection.execute("DELETE FROM tasks WHERE note_path = ?1", [path])?;
        }
        Ok(())
    }

    /// Resolves an existing vault entry after enforcing the vault boundary.
    pub fn absolute_entry_path(&self, relative_path: &str) -> Result<PathBuf> {
        self.secure_existing_entry(relative_path)
    }

    pub fn snapshot(&self) -> Result<VaultSnapshot> {
        let mut files = Vec::new();
        let mut entries = Vec::new();
        scan_directory(&self.root, &self.root, &mut files, &mut entries)?;
        files.sort_by_cached_key(|file| file.path.to_lowercase());
        entries.sort_by_cached_key(|entry| {
            (
                entry.parent.to_lowercase(),
                entry.kind != VaultEntryKind::Folder,
                entry.name.to_lowercase(),
            )
        });
        let name = self
            .root
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| self.root.to_string_lossy().into_owned());
        Ok(VaultSnapshot {
            root: self.root.to_string_lossy().into_owned(),
            name,
            files,
            entries,
        })
    }

    pub fn read_note(&self, relative_path: &str) -> Result<NoteDocument> {
        let path = self.secure_existing_note(relative_path)?;
        read_document(&self.root, &path)
    }

    /// Reads a portable UTF-8 workspace resource without exposing arbitrary files.
    /// Markdown stays on the note API; this surface is intentionally limited to
    /// the two open Obsidian-compatible structured formats Brandon understands.
    pub fn read_resource(&self, relative_path: &str) -> Result<NoteDocument> {
        let path = self.secure_existing_resource(relative_path)?;
        read_document(&self.root, &path)
    }

    pub fn save_note(
        &self,
        relative_path: &str,
        contents: &str,
        expected_fingerprint: Option<&str>,
    ) -> Result<SaveOutcome> {
        let path = self.secure_existing_note(relative_path)?;
        let disk = read_document(&self.root, &path)?;
        if expected_fingerprint.is_some_and(|expected| expected != disk.fingerprint) {
            return Ok(SaveOutcome::Conflict { disk });
        }
        write_atomic(&path, contents.as_bytes())?;
        Ok(SaveOutcome::Saved {
            snapshot: self.snapshot()?,
            document: read_document(&self.root, &path)?,
        })
    }

    pub fn save_resource(
        &self,
        relative_path: &str,
        contents: &str,
        expected_fingerprint: Option<&str>,
    ) -> Result<SaveOutcome> {
        let path = self.secure_existing_resource(relative_path)?;
        let disk = read_document(&self.root, &path)?;
        if expected_fingerprint.is_some_and(|expected| expected != disk.fingerprint) {
            return Ok(SaveOutcome::Conflict { disk });
        }
        write_atomic(&path, contents.as_bytes())?;
        Ok(SaveOutcome::Saved {
            snapshot: self.snapshot()?,
            document: read_document(&self.root, &path)?,
        })
    }

    pub fn create_note(&self, folder: Option<&str>, title: &str) -> Result<CreatedNote> {
        let name = safe_note_name(title)?;
        let directory = self.secure_existing_directory(folder.unwrap_or(""))?;
        let path = directory.join(&name);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(map_create_error("Не удалось создать заметку"))?;
        let heading = format!("# {}\n\n", clean_title(title));
        file.write_all(heading.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| VaultError::Io {
                context: "Не удалось записать заметку",
                source,
            })?;
        Ok(VaultMutation {
            snapshot: self.snapshot()?,
            path: relative_display(&self.root, &path)?,
            link_warnings: Vec::new(),
        })
    }

    pub fn create_resource(
        &self,
        folder: Option<&str>,
        title: &str,
        extension: &str,
        contents: &str,
    ) -> Result<VaultMutation> {
        // Recovery creates an exact Markdown copy through the same exclusive
        // create-new boundary as Canvas/Bases, never a save over an existing file.
        let extension = if extension
            .trim()
            .trim_start_matches('.')
            .eq_ignore_ascii_case("md")
        {
            "md"
        } else {
            normalized_resource_extension(extension)?
        };
        let title = title
            .trim()
            .strip_suffix(&format!(".{extension}"))
            .unwrap_or(title.trim());
        let name = format!("{}.{}", safe_entry_name(title)?, extension);
        let directory = self.secure_existing_directory(folder.unwrap_or(""))?;
        let path = directory.join(name);
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(map_create_error("Не удалось создать файл"))?;
        file.write_all(contents.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|source| VaultError::Io {
                context: "Не удалось записать файл",
                source,
            })?;
        Ok(VaultMutation {
            snapshot: self.snapshot()?,
            path: relative_display(&self.root, &path)?,
            link_warnings: Vec::new(),
        })
    }

    pub fn create_folder(&self, parent: Option<&str>, name: &str) -> Result<VaultMutation> {
        let parent = self.secure_existing_directory(parent.unwrap_or(""))?;
        let name = safe_entry_name(name)?;
        let path = parent.join(name);
        fs::create_dir(&path).map_err(map_create_error("Не удалось создать папку"))?;
        Ok(VaultMutation {
            snapshot: self.snapshot()?,
            path: relative_display(&self.root, &path)?,
            link_warnings: Vec::new(),
        })
    }

    pub fn rename_entry(
        &self,
        relative_path: &str,
        new_name: &str,
        update_links: bool,
    ) -> Result<VaultMutation> {
        let source = self.secure_existing_entry(relative_path)?;
        let is_directory = source.is_dir();
        let is_note = source.is_file() && is_markdown(&source);
        let resource_extension = source
            .is_file()
            .then(|| source.extension().and_then(|value| value.to_str()))
            .flatten()
            .and_then(|value| normalized_resource_extension(value).ok());
        let new_name = if is_note {
            safe_note_name(new_name)?
        } else if let Some(extension) = resource_extension {
            let title = new_name
                .trim()
                .strip_suffix(&format!(".{extension}"))
                .unwrap_or(new_name.trim());
            format!("{}.{}", safe_entry_name(title)?, extension)
        } else {
            safe_entry_name(new_name)?
        };
        let target = source
            .parent()
            .ok_or(VaultError::InvalidPath("Некорректный путь"))?
            .join(new_name);
        if target.exists() {
            return Err(VaultError::AlreadyExists);
        }
        let old_relative = relative_display(&self.root, &source)?;
        let new_relative = relative_display(&self.root, &target)?;
        let link_plan = update_links
            .then(|| self.prepare_link_updates(&old_relative, &new_relative, is_directory))
            .transpose()?;
        fs::rename(&source, &target).map_err(|source| VaultError::Io {
            context: "Не удалось переименовать объект",
            source,
        })?;
        if let Err(error) =
            self.rewrite_managed_task_paths(&old_relative, &new_relative, is_directory)
        {
            let _ = fs::rename(&target, &source);
            return Err(error);
        }
        let link_warnings = link_plan.map_or_else(Vec::new, |plan| self.apply_link_updates(plan));
        Ok(VaultMutation {
            snapshot: self.snapshot()?,
            path: new_relative,
            link_warnings,
        })
    }

    pub fn move_entry(
        &self,
        relative_path: &str,
        destination_folder: Option<&str>,
        update_links: bool,
    ) -> Result<VaultMutation> {
        let source = self.secure_existing_entry(relative_path)?;
        let destination = self.secure_existing_directory(destination_folder.unwrap_or(""))?;
        if source.is_dir() && destination.starts_with(&source) {
            return Err(VaultError::InvalidPath(
                "Нельзя переместить папку внутрь самой себя",
            ));
        }
        let file_name = source
            .file_name()
            .ok_or(VaultError::InvalidPath("Некорректный путь"))?;
        let target = destination.join(file_name);
        if target.exists() {
            return Err(VaultError::AlreadyExists);
        }
        let old_relative = relative_display(&self.root, &source)?;
        let was_directory = source.is_dir();
        let new_relative = relative_display(&self.root, &target)?;
        let link_plan = update_links
            .then(|| self.prepare_link_updates(&old_relative, &new_relative, was_directory))
            .transpose()?;
        fs::rename(&source, &target).map_err(|source| VaultError::Io {
            context: "Не удалось переместить объект",
            source,
        })?;
        if let Err(error) =
            self.rewrite_managed_task_paths(&old_relative, &new_relative, was_directory)
        {
            let _ = fs::rename(&target, &source);
            return Err(error);
        }
        let link_warnings = link_plan.map_or_else(Vec::new, |plan| self.apply_link_updates(plan));
        Ok(VaultMutation {
            snapshot: self.snapshot()?,
            path: new_relative,
            link_warnings,
        })
    }

    pub fn trash_entry(&self, relative_path: &str) -> Result<VaultSnapshot> {
        let source = self.secure_existing_entry(relative_path)?;
        let old_relative = relative_display(&self.root, &source)?;
        let was_directory = source.is_dir();
        let trash = self.root.join(".trash");
        fs::create_dir_all(&trash).map_err(|source| VaultError::Io {
            context: "Не удалось подготовить корзину",
            source,
        })?;
        let name = source
            .file_name()
            .ok_or(VaultError::InvalidPath("Некорректный путь"))?
            .to_string_lossy();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis());
        let target = trash.join(format!("{stamp}-{name}"));
        fs::rename(&source, &target).map_err(|source| VaultError::Io {
            context: "Не удалось переместить объект в корзину",
            source,
        })?;
        if let Err(error) = self.remove_managed_task_paths(&old_relative, was_directory) {
            let _ = fs::rename(&target, &source);
            return Err(error);
        }
        self.snapshot()
    }

    pub fn index(&self) -> Result<VaultIndex> {
        let snapshot = self.snapshot()?;
        self.sync_managed_task_files(&snapshot)?;
        let snapshot = self.snapshot()?;
        let mut documents = Vec::with_capacity(snapshot.files.len());
        for file in &snapshot.files {
            let path = self.secure_existing_note(&file.path)?;
            let source = fs::read_to_string(path).map_err(|source| VaultError::Io {
                context: "Не удалось прочитать заметку для индекса",
                source,
            })?;
            documents.push(parse_document(&file.path, &source, file.modified_unix_ms));
        }
        resolve_links(&mut documents);
        let mut tags = BTreeMap::new();
        for document in &documents {
            for tag in &document.tags {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        let mut tasks = self.managed_tasks(&documents)?;
        tasks.extend(
            documents
                .iter()
                .flat_map(|document| document.tasks.iter().cloned())
                .collect::<Vec<_>>(),
        );
        Ok(VaultIndex {
            documents,
            tags,
            tasks,
        })
    }

    pub fn create_task(&self, title: &str, scheduled: Option<&str>) -> Result<TaskIndexMutation> {
        let title = title
            .chars()
            .map(|character| {
                if matches!(character, '\r' | '\n') {
                    ' '
                } else {
                    character
                }
            })
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if title.is_empty() {
            return Err(VaultError::InvalidName("Введите название задачи"));
        }
        let scheduled = normalize_task_datetime(scheduled)?;
        let tasks_directory = self.root.join("Tasks");
        fs::create_dir_all(&tasks_directory).map_err(|source| VaultError::Io {
            context: "Не удалось подготовить папку задач",
            source,
        })?;
        let safe_title = safe_entry_name(&title)?;
        let mut suffix = 1_u32;
        let (path, mut file) = loop {
            let name = if suffix == 1 {
                format!("{safe_title}.md")
            } else {
                format!("{safe_title} ({suffix}).md")
            };
            let path = tasks_directory.join(name);
            match fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => break (path, file),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => suffix += 1,
                Err(source) => {
                    return Err(VaultError::Io {
                        context: "Не удалось создать файл задачи",
                        source,
                    });
                }
            }
        };
        let stable_id = new_task_id();
        let source = format!(
            "# {title}\n\n## Описание\n\nОпишите задачу здесь. Можно добавлять ссылки, вложения, таблицы и код.\n"
        );
        if let Err(source) = file
            .write_all(source.as_bytes())
            .and_then(|()| file.sync_all())
        {
            drop(file);
            let _ = fs::remove_file(&path);
            return Err(VaultError::Io {
                context: "Не удалось записать файл задачи",
                source,
            });
        }
        drop(file);
        debug_assert!(path.starts_with(&tasks_directory));
        let relative_path = relative_display(&self.root, &path)?;
        let now = current_unix_ms();
        if let Err(error) = self.task_connection()?.execute(
            "INSERT INTO tasks (id, note_path, title, completed, scheduled, due, duration_minutes, priority, created_unix_ms, updated_unix_ms)
             VALUES (?1, ?2, ?3, 0, ?4, NULL, NULL, NULL, ?5, ?5)",
            params![stable_id, relative_path, title, scheduled, now],
        ) {
            let _ = fs::remove_file(&path);
            return Err(error.into());
        }
        Ok(TaskIndexMutation {
            snapshot: self.snapshot()?,
            index: self.index()?,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update_task(
        &self,
        task_id: &str,
        relative_path: &str,
        line: usize,
        block_id: Option<&str>,
        completed: Option<bool>,
        scheduled: Option<&str>,
        clear_scheduled: bool,
        due: Option<&str>,
        clear_due: bool,
    ) -> Result<TaskIndexMutation> {
        let scheduled = normalize_task_datetime(scheduled)?;
        let schedule_patch = if clear_scheduled {
            Some(None)
        } else {
            scheduled.map(Some)
        };
        let due = normalize_task_datetime(due)?;
        let due_patch = if clear_due { Some(None) } else { due.map(Some) };
        if self.update_managed_task(task_id, completed, schedule_patch, due_patch)? {
            return Ok(TaskIndexMutation {
                snapshot: self.snapshot()?,
                index: self.index()?,
            });
        }
        let path = self.secure_existing_note(relative_path)?;
        let source = fs::read_to_string(&path).map_err(|source| VaultError::Io {
            context: "Не удалось прочитать задачу",
            source,
        })?;
        let stable_id = block_id.map(str::to_owned).unwrap_or_else(new_task_id);
        let rewritten = rewrite_task_source(
            &source,
            line,
            block_id,
            completed,
            schedule_patch,
            due_patch,
            &stable_id,
        )
        .ok_or(VaultError::InvalidPath(
            "Задача изменилась на диске. Обновите календарь и повторите",
        ))?;
        write_atomic(&path, rewritten.as_bytes())?;
        Ok(TaskIndexMutation {
            snapshot: self.snapshot()?,
            index: self.index()?,
        })
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<SearchHit>> {
        // Retained for Markdown-only callers; desktop IPC uses search_local.
        search::markdown_search(self, query, limit)
    }

    fn secure_existing_note(&self, relative_path: &str) -> Result<PathBuf> {
        let canonical = self.secure_existing_entry(relative_path)?;
        if !canonical.is_file() || !is_markdown(&canonical) {
            return Err(VaultError::InvalidPath(
                "Можно открывать только Markdown-файлы",
            ));
        }
        Ok(canonical)
    }

    fn secure_existing_resource(&self, relative_path: &str) -> Result<PathBuf> {
        let canonical = self.secure_existing_entry(relative_path)?;
        if !canonical.is_file() || !is_portable_resource(&canonical) {
            return Err(VaultError::InvalidPath(
                "Можно открывать только файлы Canvas и Bases",
            ));
        }
        Ok(canonical)
    }

    fn secure_existing_entry(&self, relative_path: &str) -> Result<PathBuf> {
        let relative = normalized_relative(relative_path, false)?;
        let candidate = self.root.join(relative);
        let canonical = candidate.canonicalize().map_err(|source| VaultError::Io {
            context: "Объект vault недоступен",
            source,
        })?;
        if canonical == self.root || !canonical.starts_with(&self.root) {
            return Err(VaultError::InvalidPath(
                "Путь выходит за пределы vault или указывает на его корень",
            ));
        }
        Ok(canonical)
    }

    fn secure_existing_directory(&self, relative_path: &str) -> Result<PathBuf> {
        let candidate = if relative_path.trim().is_empty() {
            self.root.clone()
        } else {
            self.root.join(normalized_relative(relative_path, false)?)
        };
        let canonical = candidate.canonicalize().map_err(|source| VaultError::Io {
            context: "Папка vault недоступна",
            source,
        })?;
        if !canonical.starts_with(&self.root) || !canonical.is_dir() {
            return Err(VaultError::InvalidPath("Некорректная папка vault"));
        }
        Ok(canonical)
    }
}

fn resolve_links(documents: &mut [DocumentMetadata]) {
    let mut aliases: HashMap<String, Vec<String>> = HashMap::new();
    for document in documents.iter() {
        let path = without_markdown_extension(&document.path);
        let stem = Path::new(&path)
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        for alias in std::iter::once(path)
            .chain(std::iter::once(stem))
            .chain(std::iter::once(document.title.clone()))
            .chain(document.aliases.iter().cloned())
        {
            aliases
                .entry(alias.to_lowercase())
                .or_default()
                .push(document.path.clone());
        }
    }
    for paths in aliases.values_mut() {
        paths.sort_by_key(|path| (path.matches('/').count(), path.len(), path.clone()));
        paths.dedup();
    }
    for document in documents {
        for link in &mut document.links {
            let normalized = without_markdown_extension(&link.target).to_lowercase();
            link.resolved_path = aliases
                .get(&normalized)
                .and_then(|paths| paths.first())
                .cloned();
        }
    }
}

fn normalized_relative(path: &str, allow_empty: bool) -> Result<PathBuf> {
    // Vault paths are portable: recognize both separators and drive prefixes
    // even on Unix, where std::path treats Windows paths as ordinary names.
    let portable = path.replace('\\', "/");
    let candidate = Path::new(&portable);
    let has_drive_prefix =
        portable.as_bytes().get(1) == Some(&b':') && portable.as_bytes()[0].is_ascii_alphabetic();
    if (!allow_empty && path.trim().is_empty()) || has_drive_prefix || candidate.is_absolute() {
        return Err(VaultError::InvalidPath("Некорректный путь vault"));
    }
    if candidate.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(VaultError::InvalidPath("Путь выходит за пределы vault"));
    }
    Ok(candidate.to_owned())
}

fn relative_display(root: &Path, path: &Path) -> Result<String> {
    path.strip_prefix(root)
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .map_err(|_| VaultError::InvalidPath("Не удалось определить путь объекта"))
}

fn scan_directory(
    root: &Path,
    directory: &Path,
    files: &mut Vec<VaultFile>,
    entries: &mut Vec<VaultEntry>,
) -> Result<()> {
    let directory_entries = fs::read_dir(directory).map_err(|source| VaultError::Io {
        context: "Не удалось прочитать папку vault",
        source,
    })?;
    for entry in directory_entries {
        let entry = entry.map_err(|source| VaultError::Io {
            context: "Не удалось прочитать содержимое vault",
            source,
        })?;
        let file_type = entry.file_type().map_err(|source| VaultError::Io {
            context: "Не удалось определить тип объекта",
            source,
        })?;
        if file_type.is_symlink() {
            continue;
        }
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target") {
            continue;
        }
        let relative = relative_display(root, &path)?;
        let parent = Path::new(&relative)
            .parent()
            .filter(|value| !value.as_os_str().is_empty())
            .map_or_else(String::new, |value| {
                value.to_string_lossy().replace('\\', "/")
            });
        let metadata = entry.metadata().map_err(|source| VaultError::Io {
            context: "Не удалось прочитать свойства объекта",
            source,
        })?;
        let modified_unix_ms = modified_unix_ms(&metadata);
        if file_type.is_dir() {
            entries.push(VaultEntry {
                path: relative,
                name,
                parent,
                kind: VaultEntryKind::Folder,
                extension: None,
                modified_unix_ms,
                size_bytes: 0,
                is_note: false,
            });
            scan_directory(root, &path, files, entries)?;
        } else if file_type.is_file() {
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .map(str::to_lowercase);
            let is_note = is_markdown(&path);
            entries.push(VaultEntry {
                path: relative.clone(),
                name: name.clone(),
                parent: parent.clone(),
                kind: VaultEntryKind::File,
                extension,
                modified_unix_ms,
                size_bytes: metadata.len(),
                is_note,
            });
            if is_note {
                files.push(VaultFile {
                    path: relative,
                    name,
                    folder: parent,
                    modified_unix_ms,
                    size_bytes: metadata.len(),
                });
            }
        }
    }
    Ok(())
}

fn modified_unix_ms(metadata: &fs::Metadata) -> u128 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |duration| duration.as_millis())
}

fn current_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_millis()).unwrap_or(i64::MAX)
        })
}

fn without_line(source: &str, line_number: usize) -> String {
    source
        .split_inclusive('\n')
        .enumerate()
        .filter_map(|(index, line)| (index + 1 != line_number).then_some(line))
        .collect()
}

fn read_document(root: &Path, path: &Path) -> Result<NoteDocument> {
    let bytes = fs::read(path).map_err(|source| VaultError::Io {
        context: "Не удалось прочитать заметку",
        source,
    })?;
    let content = String::from_utf8(bytes.clone())
        .map_err(|_| VaultError::InvalidPath("Markdown-файл должен быть в UTF-8"))?;
    let metadata = fs::metadata(path).map_err(|source| VaultError::Io {
        context: "Не удалось прочитать свойства заметки",
        source,
    })?;
    Ok(NoteDocument {
        path: relative_display(root, path)?,
        content,
        fingerprint: fingerprint(&bytes),
        modified_unix_ms: modified_unix_ms(&metadata),
    })
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SAVE_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or(VaultError::InvalidPath("Нет папки для сохранения"))?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let temporary = parent.join(format!(
        ".brandon-save-{}-{nonce}-{}.tmp",
        std::process::id(),
        SAVE_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let write_result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|source| VaultError::Io {
                context: "Не удалось подготовить сохранение",
                source,
            })?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|source| VaultError::Io {
                context: "Не удалось записать заметку",
                source,
            })?;
        drop(file);
        // rename replaces the existing file on supported Windows/Unix targets.
        // Never unlink the original first: a failed replacement must keep it.
        fs::rename(&temporary, path).map_err(|source| VaultError::Io {
            context: "Не удалось завершить сохранение",
            source,
        })
    })();
    if write_result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    write_result
}

fn fingerprint(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn is_markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("md"))
}

fn is_portable_resource(path: &Path) -> bool {
    path.extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "canvas" | "base"))
}

fn normalized_resource_extension(extension: &str) -> Result<&'static str> {
    match extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase()
        .as_str()
    {
        "canvas" => Ok("canvas"),
        "base" => Ok("base"),
        _ => Err(VaultError::InvalidName(
            "Поддерживаются только расширения .canvas и .base",
        )),
    }
}

fn without_markdown_extension(value: &str) -> String {
    value
        .strip_suffix(".md")
        .or_else(|| value.strip_suffix(".MD"))
        .unwrap_or(value)
        .replace('\\', "/")
}

fn clean_title(title: &str) -> &str {
    title.trim().trim_end_matches(".md").trim()
}

fn safe_note_name(title: &str) -> Result<String> {
    let title = clean_title(title);
    safe_entry_name(title).map(|name| format!("{name}.md"))
}

fn safe_entry_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.contains('/') || name.contains('\\') {
        return Err(VaultError::InvalidName("Название не должно содержать путь"));
    }
    let mut sanitized = String::with_capacity(name.len());
    for character in name.chars() {
        if character.is_control() || "<>:\"|?*".contains(character) {
            sanitized.push('-');
        } else {
            sanitized.push(character);
        }
    }
    let sanitized = sanitized.trim_matches([' ', '.']).trim();
    if sanitized.is_empty() {
        return Err(VaultError::InvalidName("Введите название"));
    }
    let stem = Path::new(sanitized)
        .file_stem()
        .map(|value| value.to_string_lossy())
        .unwrap_or_default();
    let reserved = [
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    if reserved
        .iter()
        .any(|value| stem.eq_ignore_ascii_case(value))
    {
        return Err(VaultError::InvalidName(
            "Это название зарезервировано Windows",
        ));
    }
    Ok(sanitized.to_owned())
}

fn normalize_task_datetime(value: Option<&str>) -> Result<Option<&str>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let valid = NaiveDate::parse_from_str(value, "%Y-%m-%d").is_ok()
        || NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M").is_ok()
        || NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M").is_ok();
    if !valid {
        return Err(VaultError::InvalidName(
            "Дата задачи должна быть в формате ГГГГ-ММ-ДД или ГГГГ-ММ-ДД ЧЧ:ММ",
        ));
    }
    Ok(Some(value))
}

fn new_task_id() -> String {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("task-{unique:x}")
}

fn map_create_error(context: &'static str) -> impl FnOnce(io::Error) -> VaultError {
    move |source| {
        if source.kind() == io::ErrorKind::AlreadyExists {
            VaultError::AlreadyExists
        } else {
            VaultError::Io { context, source }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SaveOutcome, Vault, normalized_relative, safe_note_name};
    use std::{fs, path::PathBuf};

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::Builder::new()
                .prefix("brandon-vault-test-")
                .tempdir()
                .expect("fixture")
                .keep();
            Self(root)
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn atomic_save_keeps_unrelated_temp_files_and_cleans_failed_replacements() {
        let fixture = Fixture::new();
        let note = fixture.0.join("same.md");
        let old_temporary = fixture.0.join("same.md.brandon-tmp");
        fs::write(&note, "original").expect("original");
        fs::write(&old_temporary, "do not overwrite").expect("unrelated temp");
        super::write_atomic(&note, b"replacement").expect("atomic replace");
        assert_eq!(fs::read_to_string(&note).expect("replaced"), "replacement");
        assert_eq!(
            fs::read_to_string(&old_temporary).expect("untouched"),
            "do not overwrite"
        );
        let directory = fixture.0.join("cannot-replace-directory");
        fs::create_dir(&directory).expect("directory");
        fs::write(directory.join("keep.txt"), "keep").expect("content");
        assert!(super::write_atomic(&directory, b"must fail").is_err());
        assert_eq!(
            fs::read_to_string(directory.join("keep.txt")).expect("kept"),
            "keep"
        );
        assert!(!fs::read_dir(&fixture.0).expect("entries").any(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .starts_with(".brandon-save-")
        }));
    }

    #[test]
    fn atomic_saves_to_same_stem_different_formats_do_not_share_temporary_files() {
        let fixture = Fixture::new();
        std::thread::scope(|scope| {
            for (name, contents) in [("same.md", "markdown"), ("same.canvas", "canvas")] {
                let target = fixture.0.join(name);
                scope.spawn(move || {
                    for _ in 0..50 {
                        super::write_atomic(&target, contents.as_bytes())
                            .expect("independent save");
                    }
                    assert_eq!(fs::read_to_string(target).expect("saved"), contents);
                });
            }
        });
    }

    #[test]
    fn rejects_paths_outside_vault() {
        for path in [
            "../secret.md",
            "notes/../../secret.md",
            r"..\secret.md",
            r"notes\..\secret.md",
            r"notes/..\secret.md",
            "/etc/secret.md",
            r"\secret.md",
            "C:/secret.md",
            r"C:\secret.md",
            "c:secret.md",
            "D:",
            r"\\server\share\secret.md",
            "//server/share/secret.md",
            r"\\?\C:\secret.md",
            r"\\.\C:\secret.md",
        ] {
            for allow_empty in [false, true] {
                assert!(
                    normalized_relative(path, allow_empty).is_err(),
                    "accepted unsafe path {path:?} with allow_empty={allow_empty}"
                );
            }
        }
    }

    #[test]
    fn normalizes_portable_relative_paths_on_every_platform() {
        for path in ["notes/good.md", r"notes\good.md"] {
            assert_eq!(
                normalized_relative(path, false).expect("portable relative path"),
                PathBuf::from("notes").join("good.md")
            );
        }
        assert_eq!(
            normalized_relative(r"Заметки\Project notes/good.md", false).expect("mixed separators"),
            PathBuf::from("Заметки")
                .join("Project notes")
                .join("good.md")
        );
        assert!(normalized_relative("", false).is_err());
        assert!(normalized_relative("  ", false).is_err());
        assert_eq!(
            normalized_relative("", true).expect("allowed empty path"),
            PathBuf::new()
        );
    }

    #[test]
    fn vault_reads_portable_paths_and_rejects_foreign_roots_before_io() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("notes")).expect("notes");
        fs::write(fixture.0.join("notes/good.md"), "# Portable\n").expect("note");
        let vault = Vault::open(&fixture.0).expect("vault");
        assert_eq!(
            vault
                .read_note(r"notes\good.md")
                .expect("read portable path")
                .content,
            "# Portable\n"
        );
        for path in [
            r"C:\secret.md",
            r"..\secret.md",
            r"\\server\share\secret.md",
        ] {
            assert!(matches!(
                vault.read_note(path),
                Err(super::VaultError::InvalidPath(_))
            ));
            assert!(matches!(
                vault.attachment_info(path),
                Err(super::VaultError::InvalidPath(_))
            ));
        }
    }

    #[test]
    fn sanitizes_note_names() {
        assert_eq!(
            safe_note_name(" Product: plan ").expect("name"),
            "Product- plan.md"
        );
        assert!(safe_note_name("...").is_err());
        assert!(safe_note_name("CON").is_err());
    }

    #[test]
    fn scans_tree_and_round_trips_markdown() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("folder")).expect("folder");
        fs::write(fixture.0.join("folder/note.md"), "# Before\n").expect("note");
        fs::write(fixture.0.join("ignored.txt"), "attachment").expect("attachment");

        let vault = Vault::open(&fixture.0).expect("vault");
        let first = vault.snapshot().expect("snapshot");
        assert_eq!(first.files.len(), 1);
        assert_eq!(first.entries.len(), 3);
        assert_eq!(first.files[0].path, "folder/note.md");
        let before = vault.read_note("folder/note.md").expect("read");
        assert_eq!(before.content, "# Before\n");

        let saved = vault
            .save_note("folder/note.md", "# After\n", Some(&before.fingerprint))
            .expect("save");
        assert!(matches!(saved, SaveOutcome::Saved { .. }));
        assert_eq!(
            fs::read_to_string(fixture.0.join("folder/note.md")).expect("contents"),
            "# After\n"
        );

        let created = vault
            .create_note(Some("folder"), "New note")
            .expect("create");
        assert_eq!(created.path, "folder/New note.md");
        assert_eq!(
            fs::read_to_string(fixture.0.join("folder/New note.md")).expect("created"),
            "# New note\n\n"
        );
    }

    #[test]
    fn detects_external_edit_conflicts() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("note.md"), "before").expect("note");
        let vault = Vault::open(&fixture.0).expect("vault");
        let opened = vault.read_note("note.md").expect("read");
        fs::write(fixture.0.join("note.md"), "outside").expect("outside");
        let outcome = vault
            .save_note("note.md", "mine", Some(&opened.fingerprint))
            .expect("save");
        match outcome {
            SaveOutcome::Conflict { disk } => assert_eq!(disk.content, "outside"),
            SaveOutcome::Saved { .. } => panic!("expected conflict"),
        }
    }

    #[test]
    fn round_trips_json_canvas_and_base_resources() {
        let fixture = Fixture::new();
        let vault = Vault::open(&fixture.0).expect("vault");
        let canvas = vault
            .create_resource(None, "Research map", "canvas", r#"{"nodes":[],"edges":[]}"#)
            .expect("canvas");
        let base = vault
            .create_resource(
                None,
                "Reading list.base",
                ".base",
                "views:\n  - type: table\n",
            )
            .expect("base");
        assert_eq!(canvas.path, "Research map.canvas");
        assert_eq!(base.path, "Reading list.base");

        let opened = vault.read_resource(&canvas.path).expect("read canvas");
        let saved = vault
            .save_resource(
                &canvas.path,
                r#"{"nodes":[{"id":"a","type":"text","text":"Idea","x":0,"y":0,"width":200,"height":100}],"edges":[]}"#,
                Some(&opened.fingerprint),
            )
            .expect("save canvas");
        assert!(matches!(saved, SaveOutcome::Saved { .. }));
        assert!(vault.read_note(&canvas.path).is_err());
        let renamed = vault
            .rename_entry(&canvas.path, "Research map 2", false)
            .expect("rename canvas");
        assert_eq!(renamed.path, "Research map 2.canvas");
        assert!(vault.read_resource(&renamed.path).is_ok());
        assert!(
            vault
                .create_resource(None, "unsafe", "exe", "nope")
                .is_err()
        );
    }

    #[test]
    fn renames_notes_updates_links_and_uses_recoverable_trash() {
        let fixture = Fixture::new();
        fs::write(fixture.0.join("Alpha.md"), "# Alpha\n").expect("alpha");
        fs::write(
            fixture.0.join("Links.md"),
            "See [[Alpha]] and [[Alpha#Part|alias]].",
        )
        .expect("links");
        let vault = Vault::open(&fixture.0).expect("vault");
        let renamed = vault
            .rename_entry("Alpha.md", "Beta", true)
            .expect("rename");
        assert_eq!(renamed.path, "Beta.md");
        assert_eq!(
            fs::read_to_string(fixture.0.join("Links.md")).expect("links"),
            "See [[Beta]] and [[Beta#Part|alias]]."
        );
        vault.trash_entry("Beta.md").expect("trash");
        assert!(!fixture.0.join("Beta.md").exists());
        assert_eq!(
            fs::read_dir(fixture.0.join(".trash"))
                .expect("trash dir")
                .count(),
            1
        );
    }

    #[test]
    fn indexes_links_tags_and_searches_contents() {
        let fixture = Fixture::new();
        fs::create_dir(fixture.0.join("People")).expect("folder");
        fs::write(
            fixture.0.join("People/Ada.md"),
            "---\naliases: [Countess]\ntags: [people]\n---\n# Ada Lovelace\nAnalytical engine",
        )
        .expect("ada");
        fs::write(fixture.0.join("Index.md"), "Meet [[Countess]] #navigation").expect("index");
        let vault = Vault::open(&fixture.0).expect("vault");
        let index = vault.index().expect("index");
        let source = index
            .documents
            .iter()
            .find(|document| document.path == "Index.md")
            .expect("index document");
        assert_eq!(
            source.links[0].resolved_path.as_deref(),
            Some("People/Ada.md")
        );
        assert_eq!(index.tags.get("people"), Some(&1));
        let hits = vault.search("engine tag:people", 20).expect("search");
        assert_eq!(hits[0].path, "People/Ada.md");
    }

    #[test]
    fn creates_task_notes_and_updates_local_task_database() {
        let fixture = Fixture::new();
        let vault = Vault::open(&fixture.0).expect("vault");

        let created = vault
            .create_task("Подготовить демо", Some("2026-10-20 09:30"))
            .expect("create task");
        assert_eq!(created.index.tasks.len(), 1);
        let task = created.index.tasks[0].clone();
        assert!(task.managed);
        assert_eq!(task.path, "Tasks/Подготовить демо.md");
        assert_eq!(task.scheduled.as_deref(), Some("2026-10-20 09:30"));
        assert_eq!(task.block_id, None);
        let stable_id = task.id.clone();

        let updated = vault
            .update_task(
                &task.id,
                &task.path,
                task.line,
                task.block_id.as_deref(),
                Some(true),
                Some("2026-10-21"),
                false,
                Some("2026-10-21"),
                false,
            )
            .expect("update task");
        assert!(updated.index.tasks[0].completed);
        assert_eq!(
            updated.index.tasks[0].scheduled.as_deref(),
            Some("2026-10-21")
        );
        assert_eq!(updated.index.tasks[0].due.as_deref(), Some("2026-10-21"));
        assert_eq!(updated.index.tasks[0].id, stable_id);
        let source = fs::read_to_string(fixture.0.join(&task.path)).expect("source");
        assert!(!source.contains("task-"));
        assert!(!source.contains('⏳'));
        assert!(!source.contains('📅'));
        assert!(source.contains("## Описание"));

        let duplicate = vault
            .create_task("Подготовить демо", None)
            .expect("create duplicate task");
        assert!(
            duplicate
                .index
                .tasks
                .iter()
                .any(|task| task.path == "Tasks/Подготовить демо (2).md")
        );
    }

    #[test]
    fn migrates_legacy_task_notes_into_hidden_database() {
        let fixture = Fixture::new();
        fs::create_dir_all(fixture.0.join("Tasks")).expect("tasks directory");
        fs::write(
            fixture.0.join("Tasks/Legacy.md"),
            "# Legacy\n\n- [ ] Legacy ⏳ 2026-10-24 📅 2026-10-25 ^task-legacy\n\n## Описание\n\nТекст.\n",
        )
        .expect("legacy task");
        let vault = Vault::open(&fixture.0).expect("vault");

        let index = vault.index().expect("index");
        assert_eq!(index.tasks.len(), 1);
        assert!(index.tasks[0].managed);
        assert_eq!(index.tasks[0].id, "task-legacy");
        assert_eq!(index.tasks[0].scheduled.as_deref(), Some("2026-10-24"));
        assert_eq!(index.tasks[0].due.as_deref(), Some("2026-10-25"));

        let source = fs::read_to_string(fixture.0.join("Tasks/Legacy.md")).expect("source");
        assert!(!source.contains("task-legacy"));
        assert!(!source.contains('⏳'));
        assert!(!source.contains('📅'));
        assert!(source.contains("## Описание"));
        assert!(
            vault
                .snapshot()
                .expect("snapshot")
                .entries
                .iter()
                .all(|entry| !entry.path.starts_with(".brandon"))
        );
    }
}
