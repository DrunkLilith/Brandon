//! Local UI search. Derivative FTS cache only; not an agent authorization service.
//! Never open referenced Canvas paths, URLs, binary contents or canonical task DB
//! for writing. The cache is separate from authoritative task storage.
use super::{
    Result, Vault, VaultError, attachments, fingerprint, normalized_relative, parse_document,
    relative_display,
};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params, params_from_iter};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

const CACHE: &str = "search-v1.sqlite3";
const CACHE_FILES: [&str; 4] = [
    CACHE,
    "search-v1.sqlite3-journal",
    "search-v1.sqlite3-wal",
    "search-v1.sqlite3-shm",
];
const INDEX_POLICY_VERSION: &str = "4";
// A valid 20k-card Canvas exceeds 2 MiB even with short text. Keep the read
// bounded without silently demoting the product's supported board sizes.
const MAX_SOURCE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TOTAL_BYTES: u64 = 128 * 1024 * 1024;
const MAX_SOURCES: usize = 50_000;
const MAX_CHUNKS: usize = 1024;

#[derive(Clone, Copy, Default, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum SearchMode {
    #[default]
    Words,
    Literal,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub path: String,
    pub title: String,
    pub snippet: String,
    pub line: usize,
    pub score: f64,
    pub kind: String,
    pub node_id: Option<String>,
    pub revision: String,
    pub content_indexed: bool,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResponse {
    pub hits: Vec<SearchHit>,
    pub indexed_sources: usize,
    pub name_only_sources: usize,
    pub failed_sources: usize,
    pub skipped_sources: usize,
    pub changed_sources: usize,
    pub removed_sources: usize,
    pub elapsed_ms: f64,
    pub scanned_sources: usize,
    pub last_reconciled_unix_ms: Option<u64>,
    pub syncing: bool,
    pub pending_sources: usize,
    pub stale_sources: usize,
    #[serde(skip)]
    pub stale_paths: Vec<String>,
    #[serde(skip)]
    pub bytes_read: u64,
}

#[derive(Clone, Debug)]
pub enum SearchUpdate {
    Full,
    Paths(Vec<String>),
}

pub struct SearchInventory {
    pub paths: Vec<String>,
    pub skipped_sources: usize,
    pub failed_sources: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRebuild {
    pub search: SearchResponse,
    pub backup_path: Option<String>,
}

#[derive(Default, Debug)]
struct Query {
    terms: Vec<String>,
    path: Option<String>,
    tag: Option<String>,
    kind: Option<String>,
}
impl Query {
    fn literal(value: &str) -> Result<Self> {
        if value.chars().count() > 512 {
            return Err(VaultError::InvalidPath(
                "Поисковый запрос ограничен 512 символами",
            ));
        }
        // Preserve the exact text, including quotes and repeated whitespace.
        // Only explicit path:/tag:/type: tokens retain their filter meaning.
        let (mut start, mut quoted, mut parts) = (None, false, Vec::new());
        for (offset, ch) in value.char_indices() {
            if start.is_none() && !ch.is_whitespace() {
                start = Some(offset);
            }
            if ch == '"' {
                quoted = !quoted;
            }
            if ch.is_whitespace()
                && !quoted
                && let Some(start) = start.take()
            {
                parts.push((start, offset));
            }
        }
        if let Some(start) = start {
            parts.push((start, value.len()));
        }
        let (mut query, mut literal, mut previous_end) = (Self::default(), String::new(), None);
        for (start, end) in parts {
            let token = &value[start..end];
            let folded = token.to_lowercase();
            if ["path:", "tag:", "type:"]
                .iter()
                .any(|prefix| folded.starts_with(prefix))
            {
                let filter = Self::parse(token)?;
                if filter.path.is_some() {
                    query.path = filter.path;
                }
                if filter.tag.is_some() {
                    query.tag = filter.tag;
                }
                if filter.kind.is_some() {
                    query.kind = filter.kind;
                }
                previous_end = None;
            } else {
                if let Some(previous) = previous_end {
                    literal.push_str(&value[previous..start]);
                } else if !literal.is_empty() {
                    literal.push(' ');
                }
                literal.push_str(token);
                previous_end = Some(end);
            }
        }
        if !literal.trim().is_empty() {
            query.terms.push(literal.trim().to_lowercase());
        }
        Ok(query)
    }

    fn parse(value: &str) -> Result<Self> {
        if value.chars().count() > 512 {
            return Err(VaultError::InvalidPath(
                "Поисковый запрос ограничен 512 символами",
            ));
        }
        let mut parts = Vec::new();
        let (mut token, mut quoted) = (String::new(), false);
        for ch in value.chars() {
            if ch == '"' {
                quoted = !quoted;
            } else if ch.is_whitespace() && !quoted {
                if !token.is_empty() {
                    parts.push(std::mem::take(&mut token));
                }
            } else {
                token.push(ch);
            }
        }
        if quoted {
            return Err(VaultError::InvalidPath("Закройте кавычки поисковой фразы"));
        }
        if !token.is_empty() {
            parts.push(token);
        }
        if parts.len() > 24 {
            return Err(VaultError::InvalidPath(
                "Слишком много слов в поисковом запросе",
            ));
        }
        let mut query = Self::default();
        for token in parts {
            let token = token.to_lowercase();
            if let Some(path) = token.strip_prefix("path:") {
                query.path = Some(path.replace('\\', "/"));
            } else if let Some(tag) = token.strip_prefix("tag:") {
                query.tag = Some(tag.trim_start_matches('#').to_owned());
            } else if let Some(kind) = token.strip_prefix("type:") {
                query.kind = Some(
                    match kind {
                        "md" | "note" => "note",
                        "canvas" => "canvas",
                        "task" => "task",
                        "file" => "file",
                        _ => {
                            return Err(VaultError::InvalidPath(
                                "type: поддерживает note, canvas, task и file",
                            ));
                        }
                    }
                    .to_owned(),
                );
            } else {
                query.terms.push(token);
            }
        }
        Ok(query)
    }
    fn empty(&self) -> bool {
        self.terms.is_empty() && self.path.is_none() && self.tag.is_none() && self.kind.is_none()
    }
    fn fts(&self) -> String {
        self.terms
            .iter()
            .map(|term| {
                let prefix = term.ends_with('*') && term.len() > 1;
                let term = if prefix {
                    &term[..term.len() - 1]
                } else {
                    term
                };
                format!(
                    "\"{}\"{}",
                    term.replace('"', "\"\""),
                    if prefix { "*" } else { "" }
                )
            })
            .collect::<Vec<_>>()
            .join(" AND ")
    }
}

struct Chunk {
    title: String,
    body: String,
    tags: String,
    line: usize,
    node: Option<String>,
}
struct Source {
    path: String,
    absolute: PathBuf,
}

// Compatibility helper for callers expecting a Markdown-only Vec<SearchHit>.
// The desktop uses search_local and its explicit coverage report instead.
pub(super) fn markdown_search(vault: &Vault, value: &str, limit: usize) -> Result<Vec<SearchHit>> {
    let mut query = Query::default();
    for token in value.split_whitespace() {
        let token = token.trim_matches('"').to_lowercase();
        if let Some(path) = token.strip_prefix("path:") {
            query.path = Some(path.to_owned());
        } else if let Some(tag) = token.strip_prefix("tag:") {
            query.tag = Some(tag.trim_start_matches('#').to_owned());
        } else if !token.is_empty() {
            query.terms.push(token);
        }
    }
    if query.empty() {
        return Ok(Vec::new());
    }
    let mut hits = Vec::new();
    for document in vault.index()?.documents {
        if query
            .path
            .as_ref()
            .is_some_and(|path| !document.path.to_lowercase().contains(path))
            || query.tag.as_ref().is_some_and(|tag| {
                !document
                    .tags
                    .iter()
                    .any(|value| value.to_lowercase().contains(tag))
            })
        {
            continue;
        }
        let source = vault.read_note(&document.path)?;
        let lower = source.content.to_lowercase();
        let path = document.path.to_lowercase();
        let title = document.title.to_lowercase();
        if !query
            .terms
            .iter()
            .all(|term| lower.contains(term) || path.contains(term) || title.contains(term))
        {
            continue;
        }
        // Locate in original lines rather than indexing UTF-8 with a lowercased
        // byte offset: some Unicode case conversions change byte length.
        let matched = source.content.lines().enumerate().find(|(_, line)| {
            query
                .terms
                .iter()
                .any(|term| line.to_lowercase().contains(term))
        });
        let (line, snippet) = matched.map_or(
            (1, source.content.lines().next().unwrap_or("")),
            |(line, text)| (line + 1, text),
        );
        let title_hits = query
            .terms
            .iter()
            .filter(|term| title.contains(term.as_str()))
            .count();
        let path_hits = query
            .terms
            .iter()
            .filter(|term| path.contains(term.as_str()))
            .count();
        hits.push(SearchHit {
            path: document.path,
            title: document.title,
            snippet: snippet.trim().chars().take(180).collect(),
            line,
            score: (1 + title_hits * 10 + path_hits * 4) as f64,
            kind: "note".into(),
            node_id: None,
            revision: source.fingerprint,
            content_indexed: true,
        });
    }
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.path.to_lowercase().cmp(&b.path.to_lowercase()))
    });
    hits.truncate(limit.clamp(1, 200));
    Ok(hits)
}

fn scan(
    root: &Path,
    start: &Path,
    report: &mut SearchResponse,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Source>> {
    let mut result = Vec::new();
    let mut pending = vec![(start.to_path_buf(), 0)];
    while let Some((directory, depth)) = pending.pop() {
        if cancel() {
            return Err(VaultError::SearchCancelled);
        }
        if depth > 64 {
            report.skipped_sources += 1;
            continue;
        }
        if !fs::symlink_metadata(&directory)
            .is_ok_and(|metadata| metadata.is_dir() && !attachments::is_link(&metadata))
            || !directory
                .canonicalize()
                .is_ok_and(|path| path.starts_with(root))
        {
            report.failed_sources += 1;
            continue;
        }
        let entries = match fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(_) => {
                report.failed_sources += 1;
                continue;
            }
        };
        for entry in entries {
            if cancel() {
                return Err(VaultError::SearchCancelled);
            }
            let Ok(entry) = entry else {
                report.failed_sources += 1;
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if attachments::private_name(&name)
                || matches!(name.as_str(), "node_modules" | "target")
            {
                continue;
            }
            let path = entry.path();
            let Ok(metadata) = fs::symlink_metadata(&path) else {
                report.failed_sources += 1;
                continue;
            };
            if attachments::is_link(&metadata) {
                continue;
            }
            if metadata.is_dir() {
                pending.push((path, depth + 1));
            } else if metadata.is_file() {
                result.push(Source {
                    path: relative_display(root, &path)?,
                    absolute: path,
                });
            }
            if result.len() + pending.len() > MAX_SOURCES {
                return Err(VaultError::InvalidPath(
                    "Первый локальный индекс ограничен 50 000 объектами; сузьте vault",
                ));
            }
        }
    }
    result.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

fn cache_directory(vault: &Vault) -> Result<PathBuf> {
    let directory = vault.root().join(".brandon");
    // A read-only boundary check must not keep issuing CreateDirectory against
    // an existing directory: Windows may emit parent metadata notifications.
    if !directory.try_exists().map_err(|source| VaultError::Io {
        context: "Не удалось проверить папку поискового кэша",
        source,
    })? {
        match fs::create_dir(&directory) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => (),
            Err(source) => {
                return Err(VaultError::Io {
                    context: "Не удалось создать папку поискового кэша",
                    source,
                });
            }
        }
    }
    let metadata = fs::symlink_metadata(&directory).map_err(|source| VaultError::Io {
        context: "Поисковый кэш недоступен",
        source,
    })?;
    if !metadata.is_dir() || attachments::is_link(&metadata) {
        return Err(VaultError::InvalidPath(
            "Папка локального кэша не должна быть ссылкой",
        ));
    }
    for name in [
        CACHE,
        "search-v1.sqlite3-journal",
        "search-v1.sqlite3-wal",
        "search-v1.sqlite3-shm",
        "tasks.sqlite3",
        "tasks.sqlite3-journal",
        "tasks.sqlite3-wal",
        "tasks.sqlite3-shm",
    ] {
        match fs::symlink_metadata(directory.join(name)) {
            Ok(metadata) if attachments::is_link(&metadata) || !metadata.is_file() => {
                return Err(VaultError::InvalidPath(
                    "Локальная база не должна быть ссылкой",
                ));
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(source) => {
                return Err(VaultError::Io {
                    context: "Не удалось проверить локальную базу",
                    source,
                });
            }
        }
    }
    Ok(directory)
}

fn task_fields(directory: &Path) -> Result<HashMap<String, String>> {
    let path = directory.join("tasks.sqlite3");
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    connection.busy_timeout(std::time::Duration::from_secs(2))?;
    let mut statement =
        connection.prepare("SELECT note_path, title, completed, scheduled, due FROM tasks")?;
    let rows = statement.query_map([], |row| {
        let title: String = row.get(1)?;
        let completed: bool = row.get(2)?;
        let scheduled: Option<String> = row.get(3)?;
        let due: Option<String> = row.get(4)?;
        Ok((
            row.get(0)?,
            format!(
                "{title}\n{}\nЗапланировано: {}\nДедлайн: {}",
                if completed {
                    "Выполнено"
                } else {
                    "В работе"
                },
                scheduled.unwrap_or_default(),
                due.unwrap_or_default()
            ),
        ))
    })?;
    rows.collect::<std::result::Result<_, _>>()
        .map_err(Into::into)
}

fn schema(connection: &Connection) -> Result<()> {
    connection.execute_batch("PRAGMA busy_timeout=3000;
        CREATE TABLE IF NOT EXISTS sources(path TEXT PRIMARY KEY, stamp TEXT NOT NULL, revision TEXT NOT NULL, status TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS sources_status ON sources(status);
        CREATE TABLE IF NOT EXISTS search_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS chunks(id INTEGER PRIMARY KEY, path TEXT NOT NULL, path_folded TEXT NOT NULL, kind TEXT NOT NULL, node_id TEXT, line INTEGER NOT NULL, revision TEXT NOT NULL, title TEXT NOT NULL, body TEXT NOT NULL, tags TEXT NOT NULL, literal TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS chunks_path ON chunks(path);
        CREATE INDEX IF NOT EXISTS chunks_kind ON chunks(kind);
        CREATE VIRTUAL TABLE IF NOT EXISTS search_fts USING fts5(title, path, body, tags, content='chunks', content_rowid='id', tokenize='unicode61 remove_diacritics 2');
        CREATE TRIGGER IF NOT EXISTS chunks_insert AFTER INSERT ON chunks BEGIN INSERT INTO search_fts(rowid,title,path,body,tags) VALUES(new.id,new.title,new.path,new.body,new.tags); END;
        CREATE TRIGGER IF NOT EXISTS chunks_delete AFTER DELETE ON chunks BEGIN INSERT INTO search_fts(search_fts,rowid,title,path,body,tags) VALUES('delete',old.id,old.title,old.path,old.body,old.tags); END;
        CREATE TRIGGER IF NOT EXISTS chunks_update AFTER UPDATE ON chunks BEGIN
          INSERT INTO search_fts(search_fts,rowid,title,path,body,tags) VALUES('delete',old.id,old.title,old.path,old.body,old.tags);
          INSERT INTO search_fts(rowid,title,path,body,tags) VALUES(new.id,new.title,new.path,new.body,new.tags); END;")?;
    Ok(())
}

fn markdown_chunks(path: &str, source: &str) -> Option<Vec<Chunk>> {
    let document = parse_document(path, source, 0);
    let tags = format!("|{}|", document.tags.join("|").to_lowercase());
    let (mut result, mut body, mut start) = (Vec::new(), String::new(), 1);
    let mut heading = document.title.clone();
    for (offset, line) in source.lines().enumerate() {
        let new_heading = line.trim_start().starts_with('#');
        if !body.is_empty() && (new_heading || body.len() + line.len() > 6000) {
            result.push(Chunk {
                title: heading.clone(),
                body: std::mem::take(&mut body),
                tags: tags.clone(),
                line: start,
                node: None,
            });
            if result.len() >= MAX_CHUNKS {
                // Do not label a silently truncated note as content-indexed.
                return None;
            }
        }
        if new_heading {
            let section = line.trim().trim_start_matches('#').trim();
            heading = if section == document.title {
                document.title.clone()
            } else {
                format!("{} · {section}", document.title)
            };
        }
        if body.is_empty() {
            start = offset + 1;
        }
        body.push_str(line);
        body.push('\n');
    }
    if !body.is_empty() && result.len() < MAX_CHUNKS {
        result.push(Chunk {
            title: heading,
            body,
            tags,
            line: start,
            node: None,
        });
    }
    if result.is_empty() {
        result.push(Chunk {
            title: document.title,
            body: String::new(),
            tags: String::new(),
            line: 1,
            node: None,
        });
    }
    Some(result)
}

fn canvas_chunks(source: &str) -> Option<Vec<Chunk>> {
    let document: serde_json::Value = serde_json::from_str(source).ok()?;
    let nodes = document.get("nodes")?.as_array()?;
    if nodes.len() > MAX_CHUNKS * 20 {
        return None;
    }
    let mut result = Vec::new();
    for node in nodes {
        let id = node.get("id")?.as_str()?;
        let kind = node.get("type")?.as_str()?;
        // Do not read reference targets or use unknown object fields as content.
        let fields: &[&str] = match kind {
            "text" => &["text"],
            "group" => &["label"],
            "file" => &["file"],
            "link" => &["url"],
            _ => continue,
        };
        let body = fields
            .iter()
            .filter_map(|key| node.get(key)?.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let title = body
            .lines()
            .next()
            .unwrap_or("Карточка")
            .chars()
            .take(100)
            .collect();
        result.push(Chunk {
            title,
            body,
            tags: String::new(),
            line: 1,
            node: Some(id.to_owned()),
        });
    }
    Some(result)
}

fn meta(connection: &Connection, key: &str) -> Result<Option<String>> {
    Ok(connection
        .query_row("SELECT value FROM search_meta WHERE key=?1", [key], |row| {
            row.get(0)
        })
        .optional()?)
}

fn set_meta(connection: &Connection, key: &str, value: impl ToString) -> Result<()> {
    connection.execute("INSERT INTO search_meta(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", params![key,value.to_string()])?;
    Ok(())
}

fn task_fingerprint(tasks: &HashMap<String, String>) -> String {
    let mut entries: Vec<_> = tasks.iter().collect();
    entries.sort_by_key(|(path, _)| *path);
    let mut text = String::new();
    for (path, fields) in entries {
        text.push_str(&format!("{}:{path}{}:{fields}", path.len(), fields.len()));
    }
    fingerprint(text.as_bytes())
}

fn stamp(metadata: &fs::Metadata, task: Option<&String>) -> String {
    format!(
        "{INDEX_POLICY_VERSION}:{}:{}:{}",
        metadata.len(),
        metadata
            .modified()
            .ok()
            .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |value| value.as_nanos()),
        task.map(|value| fingerprint(value.as_bytes()))
            .unwrap_or_default()
    )
}

fn task_changes(connection: &Connection, tasks: &HashMap<String, String>) -> Result<Vec<String>> {
    if meta(connection, "tasks")?.as_deref() == Some(&task_fingerprint(tasks)) {
        return Ok(Vec::new());
    }
    let mut statement=connection.prepare("SELECT DISTINCT s.path,s.stamp FROM sources s JOIN chunks c ON c.path=s.path WHERE c.kind='task'")?;
    let previous: HashMap<String, String> = statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
        .collect::<std::result::Result<_, _>>()?;
    let mut changed: HashSet<String> = previous
        .keys()
        .filter(|path| !tasks.contains_key(*path))
        .cloned()
        .collect();
    for (path, fields) in tasks {
        if !previous
            .get(path)
            .is_some_and(|stamp| stamp.ends_with(&format!(":{}", fingerprint(fields.as_bytes()))))
        {
            changed.insert(path.clone());
        }
    }
    Ok(changed.into_iter().collect())
}

fn public_relative(path: &str) -> Result<PathBuf> {
    let relative = normalized_relative(path, false)?;
    if relative.components().any(|part| {
        let value = part.as_os_str().to_string_lossy();
        attachments::private_name(&value) || matches!(value.as_ref(), "node_modules" | "target")
    }) {
        return Err(VaultError::InvalidPath("Служебный путь не индексируется"));
    }
    Ok(relative)
}

fn targeted_sources(
    vault: &Vault,
    paths: &[String],
    report: &mut SearchResponse,
    cancel: &dyn Fn() -> bool,
) -> Result<Vec<Source>> {
    let mut found = HashMap::new();
    for path in paths {
        if cancel() {
            return Err(VaultError::SearchCancelled);
        }
        let Ok(relative) = public_relative(path) else {
            continue;
        };
        let mut absolute = vault.root().to_path_buf();
        let mut safe = true;
        for part in relative.components() {
            absolute.push(part.as_os_str());
            if !fs::symlink_metadata(&absolute).is_ok_and(|value| !attachments::is_link(&value)) {
                safe = false;
                break;
            }
        }
        if !safe {
            continue;
        }
        let Ok(metadata) = fs::symlink_metadata(&absolute) else {
            continue;
        };
        if metadata.is_file() {
            found.insert(
                path.clone(),
                Source {
                    path: path.clone(),
                    absolute,
                },
            );
        } else if metadata.is_dir() {
            for source in scan(vault.root(), &absolute, report, cancel)? {
                found.insert(source.path.clone(), source);
            }
        }
        if found.len() > MAX_SOURCES {
            return Err(VaultError::InvalidPath(
                "Первый локальный индекс ограничен 50 000 объектами; сузьте vault",
            ));
        }
    }
    let mut sources: Vec<_> = found.into_values().collect();
    sources.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(sources)
}

fn coverage(connection: &Connection, report: &mut SearchResponse) -> Result<()> {
    let mut statement =
        connection.prepare("SELECT status,COUNT(*) FROM sources GROUP BY status")?;
    for row in statement.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
    })? {
        let (status, count) = row?;
        match status.as_str() {
            "indexed" => report.indexed_sources = count,
            "names" => report.name_only_sources = count,
            _ => report.failed_sources += count,
        }
    }
    report.last_reconciled_unix_ms =
        meta(connection, "reconciled")?.and_then(|value| value.parse().ok());
    Ok(())
}

fn verify_hit(
    vault: &Vault,
    hit: &SearchHit,
    stored_stamp: &str,
    tasks: &HashMap<String, String>,
    bytes_read: &mut u64,
) -> Result<bool> {
    if public_relative(&hit.path).is_err() {
        return Ok(false);
    }
    let Ok(path) = vault.public_attachment_path(&hit.path) else {
        return Ok(false);
    };
    let Ok(mut file) = fs::File::open(&path) else {
        return Ok(false);
    };
    let Ok(metadata) = file.metadata() else {
        return Ok(false);
    };
    if !metadata.is_file()
        || attachments::is_link(&metadata)
        || stamp(&metadata, tasks.get(&hit.path)) != stored_stamp
    {
        return Ok(false);
    }
    if hit.content_indexed {
        if metadata.len() > MAX_SOURCE_BYTES {
            return Ok(false);
        }
        if bytes_read.saturating_add(metadata.len()) > MAX_TOTAL_BYTES {
            return Err(VaultError::InvalidPath(
                "Слишком много данных для безопасной проверки результатов; сузьте поиск",
            ));
        }
        let mut bytes = Vec::new();
        if file
            .by_ref()
            .take(MAX_SOURCE_BYTES + 1)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return Ok(false);
        }
        *bytes_read += bytes.len() as u64;
        if bytes.len() as u64 > MAX_SOURCE_BYTES || fingerprint(&bytes) != hit.revision {
            return Ok(false);
        }
    }
    Ok(vault
        .public_attachment_path(&hit.path)
        .is_ok_and(|current| current == path)
        && fs::metadata(&path)
            .is_ok_and(|current| stamp(&current, tasks.get(&hit.path)) == stored_stamp))
}

impl Vault {
    pub fn prepare_search_cache(&self) -> Result<()> {
        schema(&Connection::open(cache_directory(self)?.join(CACHE))?)
    }

    /// Inventory is a separate reconciliation operation, never a cached query.
    /// Missing cached paths go first, so replacing a 50k vault does not briefly
    /// exceed the index's source cap while inserting its new names.
    pub fn search_inventory(&self, cancel: &dyn Fn() -> bool) -> Result<SearchInventory> {
        let mut report = SearchResponse::default();
        if cancel() {
            return Err(VaultError::SearchCancelled);
        }
        let mut paths = if let Some(inventory) = self.metadata_inventory(MAX_SOURCES) {
            let inventory = inventory?;
            report.skipped_sources = inventory.skipped_sources;
            report.failed_sources = inventory.failed_sources;
            inventory.paths
        } else {
            scan(self.root(), self.root(), &mut report, cancel)?
                .into_iter()
                .map(|source| source.path)
                .collect::<Vec<_>>()
        };
        let actual: HashSet<_> = paths.iter().cloned().collect();
        let connection = Connection::open_with_flags(
            cache_directory(self)?.join(CACHE),
            OpenFlags::SQLITE_OPEN_READ_ONLY,
        )?;
        let mut statement = connection.prepare("SELECT path FROM sources")?;
        let mut removed = Vec::new();
        for path in statement.query_map([], |row| row.get::<_, String>(0))? {
            let path = path?;
            if !actual.contains(&path) {
                removed.push(path);
            }
        }
        removed.append(&mut paths);
        Ok(SearchInventory {
            paths: removed,
            skipped_sources: report.skipped_sources,
            failed_sources: report.failed_sources,
        })
    }

    pub fn mark_search_reconciled(&self, skipped: usize, failed: usize) -> Result<()> {
        let connection = Connection::open(cache_directory(self)?.join(CACHE))?;
        set_meta(
            &connection,
            "reconciled",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis(),
        )?;
        set_meta(&connection, "skipped", skipped)?;
        set_meta(&connection, "scan_failed", failed)
    }

    /// Explicit UI operation, serialized with search by the desktop worker.
    /// Preserve all old derivative files in an exclusively created directory;
    /// never remove them, touch task storage, or auto-repair on a normal query.
    pub fn rebuild_search(
        &self,
        query: &str,
        mode: SearchMode,
        limit: usize,
    ) -> Result<SearchRebuild> {
        self.rebuild_search_cancellable(query, mode, limit, &|| false)
    }

    pub fn rebuild_search_cancellable(
        &self,
        query: &str,
        mode: SearchMode,
        limit: usize,
        cancel: &dyn Fn() -> bool,
    ) -> Result<SearchRebuild> {
        let started = Instant::now();
        if cancel() {
            return Err(VaultError::SearchCancelled);
        }
        let parsed = if mode == SearchMode::Literal {
            Query::literal(query)?
        } else {
            Query::parse(query)?
        };
        if parsed.empty() {
            return Err(VaultError::InvalidPath(
                "Введите запрос или фильтр перед восстановлением",
            ));
        }
        let directory = cache_directory(self)?;
        // A corrupt canonical DB is not a reason to quarantine the search DB.
        task_fields(&directory).map_err(|error| VaultError::SearchTasks(Box::new(error)))?;
        let has_cache = CACHE_FILES.iter().any(|name| directory.join(name).exists());
        let backup = if has_cache {
            Some(
                tempfile::Builder::new()
                    .prefix("search-recovery-")
                    .tempdir_in(&directory)
                    .map_err(|source| VaultError::Io {
                        context: "Не удалось сохранить предыдущий индекс",
                        source,
                    })?
                    .keep(),
            )
        } else {
            None
        };
        let backup_path = backup
            .as_ref()
            .map(|path| relative_display(self.root(), path))
            .transpose()?;
        let rebuild = || -> Result<SearchResponse> {
            if let Some(backup) = &backup {
                for name in CACHE_FILES {
                    if cancel() {
                        return Err(VaultError::SearchCancelled);
                    }
                    // Recheck all private DB paths immediately before each move.
                    cache_directory(self)?;
                    let source = directory.join(name);
                    if source.exists() {
                        fs::rename(source, backup.join(name)).map_err(|source| VaultError::Io {
                            context: "Не удалось переместить прежний поисковый кэш",
                            source,
                        })?;
                    }
                }
            }
            let synced = self.sync_search_batch(SearchUpdate::Full, MAX_TOTAL_BYTES, cancel)?;
            if cancel() {
                return Err(VaultError::SearchCancelled);
            }
            let mut result = self.search_cached(query, mode, limit)?;
            result.changed_sources = synced.changed_sources;
            result.removed_sources = synced.removed_sources;
            result.scanned_sources = synced.scanned_sources;
            result.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            Ok(result)
        };
        match rebuild() {
            Ok(search) => Ok(SearchRebuild {
                search,
                backup_path,
            }),
            Err(source) => match backup_path {
                Some(backup) => Err(VaultError::SearchRecovery {
                    backup,
                    source: Box::new(source),
                }),
                None => Err(source),
            },
        }
    }

    pub fn search_local(
        &self,
        query: &str,
        mode: SearchMode,
        limit: usize,
    ) -> Result<SearchResponse> {
        let started = Instant::now();
        let parsed = if mode == SearchMode::Literal {
            Query::literal(query)?
        } else {
            Query::parse(query)?
        };
        if parsed.empty() {
            return Ok(SearchResponse::default());
        }
        let synced = self.sync_search_batch(SearchUpdate::Full, MAX_TOTAL_BYTES, &|| false)?;
        let mut result = self.search_cached(query, mode, limit)?;
        result.changed_sources = synced.changed_sources;
        result.removed_sources = synced.removed_sources;
        result.scanned_sources = synced.scanned_sources;
        result.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(result)
    }

    /// Atomic bounded write batch. Desktop schedules full inventories separately
    /// and feeds small path batches; regular queries never call a full scan.
    pub fn sync_search_batch(
        &self,
        update: SearchUpdate,
        read_budget: u64,
        cancel: &dyn Fn() -> bool,
    ) -> Result<SearchResponse> {
        let started = Instant::now();
        let mut report = SearchResponse::default();
        let directory = cache_directory(self)?;
        let mut connection = Connection::open(directory.join(CACHE))?;
        schema(&connection)?;
        let tasks =
            task_fields(&directory).map_err(|error| VaultError::SearchTasks(Box::new(error)))?;
        if tasks.len() > MAX_SOURCES {
            return Err(VaultError::InvalidPath(
                "База задач превышает лимит 50 000 объектов",
            ));
        }
        let full = matches!(update, SearchUpdate::Full);
        let mut paths = match update {
            SearchUpdate::Full => Vec::new(),
            SearchUpdate::Paths(paths) => paths,
        };
        let task_deltas = task_changes(&connection, &tasks)?;
        if !full {
            paths.extend(task_deltas.iter().take(128).cloned());
        }
        for path in &mut paths {
            *path = relative_display(Path::new(""), &normalized_relative(path, false)?)?;
        }
        paths.sort();
        paths.dedup();
        if paths.len() > MAX_SOURCES * 2 {
            return Err(VaultError::InvalidPath("Слишком много изменений индекса"));
        }
        let sources = if full {
            scan(self.root(), self.root(), &mut report, cancel)?
        } else {
            targeted_sources(self, &paths, &mut report, cancel)?
        };
        report.scanned_sources = sources.len();
        let existing: HashMap<String, (String, String, String)> = if full {
            let mut statement =
                connection.prepare("SELECT path,stamp,status,revision FROM sources")?;
            statement
                .query_map([], |row| {
                    Ok((row.get(0)?, (row.get(1)?, row.get(2)?, row.get(3)?)))
                })?
                .collect::<std::result::Result<_, _>>()?
        } else {
            let mut existing = HashMap::new();
            let mut statement=connection.prepare("SELECT path,stamp,status,revision FROM sources WHERE path=?1 OR (path>=?2 AND path<?3)")?;
            for path in &paths {
                for row in statement.query_map(
                    params![path, format!("{path}/"), format!("{path}0")],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            (row.get(1)?, row.get(2)?, row.get(3)?),
                        ))
                    },
                )? {
                    let (path, values) = row?;
                    existing.insert(path, values);
                }
            }
            existing
        };
        let transaction = connection.transaction()?;
        let mut seen = HashSet::new();
        let mut bytes_read = 0_u64;
        for file in sources {
            if cancel() {
                return Err(VaultError::SearchCancelled);
            }
            let task = tasks.get(&file.path);
            seen.insert(file.path.clone());
            // Recheck the canonical public boundary at actual read time.
            let Ok(path) = self.public_attachment_path(&file.path) else {
                seen.remove(&file.path);
                report.skipped_sources += 1;
                continue;
            };
            if path != file.absolute.canonicalize().unwrap_or_default() {
                seen.remove(&file.path);
                continue;
            }
            let metadata = fs::metadata(&path).map_err(|source| VaultError::Io {
                context: "Источник индекса недоступен",
                source,
            })?;
            let stamp = stamp(&metadata, task);
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            let supported = matches!(extension.as_str(), "md" | "markdown" | "canvas");
            let mut kind = if extension == "canvas" {
                "canvas"
            } else if supported {
                "note"
            } else {
                "file"
            };
            if task.is_some() {
                kind = "task";
            }
            let (mut chunks, mut status, mut revision) = (Vec::new(), "names", String::new());
            if supported
                && metadata.len() <= MAX_SOURCE_BYTES
                && bytes_read + metadata.len() <= read_budget.min(MAX_TOTAL_BYTES)
            {
                let mut bytes = Vec::new();
                let read = fs::File::open(&path)
                    .and_then(|file| file.take(MAX_SOURCE_BYTES + 1).read_to_end(&mut bytes));
                bytes_read += bytes.len() as u64;
                if read.is_ok() && bytes.len() as u64 <= MAX_SOURCE_BYTES {
                    revision = fingerprint(&bytes);
                    // Reconciliation still reads and hashes the bounded source.
                    // Identical bytes/task stamp need no repeated Markdown parse.
                    if existing
                        .get(&file.path)
                        .is_some_and(|(old_stamp, _, old_revision)| {
                            old_stamp == &stamp && old_revision == &revision
                        })
                    {
                        continue;
                    }
                    if let Ok(source) = String::from_utf8(bytes) {
                        if extension == "canvas" {
                            if let Some(cards) = canvas_chunks(&source) {
                                chunks = cards;
                                status = "indexed";
                            } else {
                                status = "failed";
                            }
                        } else {
                            if let Some(sections) = markdown_chunks(&file.path, &source) {
                                chunks = sections;
                                status = "indexed";
                            }
                        }
                    } else {
                        status = "failed";
                    }
                } else {
                    status = "failed";
                }
            }
            if let Some(fields) = task {
                chunks.push(Chunk {
                    title: path
                        .file_stem()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    body: fields.clone(),
                    tags: String::new(),
                    // Operational fields belong to SQLite, not fictional lines
                    // in the Markdown file. Zero marks a source-title anchor.
                    line: 0,
                    node: None,
                });
            }
            if chunks.is_empty() {
                chunks.push(Chunk {
                    title: path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                    body: String::new(),
                    tags: String::new(),
                    line: 1,
                    node: None,
                });
            }
            if cancel() {
                return Err(VaultError::SearchCancelled);
            }
            if existing
                .get(&file.path)
                .is_some_and(|(old_stamp, old_status, old_revision)| {
                    old_stamp == &stamp && old_status == status && old_revision == &revision
                })
            {
                continue;
            }
            transaction.execute("DELETE FROM chunks WHERE path=?1", [&file.path])?;
            for chunk in chunks {
                let literal = format!(
                    "{}\n{}\n{}\n{}",
                    file.path, chunk.title, chunk.body, chunk.tags
                )
                .to_lowercase();
                transaction.execute("INSERT INTO chunks(path,path_folded,kind,node_id,line,revision,title,body,tags,literal) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)", params![file.path, file.path.to_lowercase(), kind, chunk.node, chunk.line as i64, revision, chunk.title, chunk.body, chunk.tags, literal])?;
            }
            transaction.execute("INSERT INTO sources(path,stamp,revision,status) VALUES(?1,?2,?3,?4) ON CONFLICT(path) DO UPDATE SET stamp=excluded.stamp,revision=excluded.revision,status=excluded.status", params![file.path, stamp, revision, status])?;
            report.changed_sources += 1;
        }
        for path in existing.keys().filter(|path| !seen.contains(*path)) {
            transaction.execute("DELETE FROM chunks WHERE path=?1", [path])?;
            transaction.execute("DELETE FROM sources WHERE path=?1", [path])?;
            report.removed_sources += 1;
        }
        let count: i64 =
            transaction.query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))?;
        if count > MAX_SOURCES as i64 {
            return Err(VaultError::InvalidPath(
                "Первый локальный индекс ограничен 50 000 объектами; сузьте vault",
            ));
        }
        if full
            || task_deltas.iter().all(|task| {
                paths
                    .iter()
                    .any(|path| task == path || task.starts_with(&format!("{path}/")))
            })
        {
            set_meta(&transaction, "tasks", task_fingerprint(&tasks))?;
        }
        if cancel() {
            return Err(VaultError::SearchCancelled);
        }
        transaction.commit()?;
        report.bytes_read = bytes_read;
        if full {
            self.mark_search_reconciled(report.skipped_sources, report.failed_sources)?;
        }
        coverage(&connection, &mut report)?;
        report.stale_paths = task_changes(&connection, &tasks)?;
        report.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(report)
    }

    /// Read-only query: no inventory, reconciliation or canonical task writes.
    pub fn search_cached(
        &self,
        query: &str,
        mode: SearchMode,
        limit: usize,
    ) -> Result<SearchResponse> {
        let started = Instant::now();
        let query = if mode == SearchMode::Literal {
            Query::literal(query)?
        } else {
            Query::parse(query)?
        };
        if query.empty() {
            return Ok(SearchResponse::default());
        }
        let directory = cache_directory(self)?;
        let connection =
            Connection::open_with_flags(directory.join(CACHE), OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        connection.busy_timeout(std::time::Duration::from_secs(2))?;
        let tasks =
            task_fields(&directory).map_err(|error| VaultError::SearchTasks(Box::new(error)))?;
        let mut report = SearchResponse::default();
        coverage(&connection, &mut report)?;
        report.skipped_sources = meta(&connection, "skipped")?
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        report.failed_sources += meta(&connection, "scan_failed")?
            .and_then(|value| value.parse::<usize>().ok())
            .unwrap_or(0);
        report.stale_paths = task_changes(&connection, &tasks)?;
        let words = mode == SearchMode::Words && !query.terms.is_empty();
        let mut sql = if words {
            "SELECT c.id AS chunk_id,c.path,c.kind,c.line,bm25(search_fts,8.0,4.0,1.0,1.0) AS rank FROM search_fts JOIN chunks c ON c.id=search_fts.rowid JOIN sources s ON s.path=c.path WHERE search_fts MATCH ?".to_owned()
        } else {
            "SELECT c.id AS chunk_id,c.path,c.kind,c.line,0.0 AS rank FROM chunks c JOIN sources s ON s.path=c.path WHERE 1=1".to_owned()
        };
        let mut values = Vec::<String>::new();
        if words {
            values.push(query.fts());
        } else if !query.terms.is_empty() {
            // Literal mode is one contiguous substring, not token conjunction.
            // Quoted input also preserves repeated whitespace inside the phrase.
            sql.push_str(" AND instr(c.literal,?)>0");
            values.push(query.terms.join(" "));
        }
        if let Some(path) = &query.path {
            sql.push_str(" AND instr(c.path_folded,?)>0");
            values.push(path.clone());
        }
        if let Some(tag) = &query.tag {
            sql.push_str(" AND instr(c.tags,?)>0");
            values.push(format!("|{tag}"));
        }
        if let Some(kind) = &query.kind {
            sql.push_str(" AND c.kind=?");
            values.push(kind.clone());
        }
        if query.terms.is_empty() {
            // Browsing a type/path/tag lists resources, not duplicate chunks.
            sql.push_str(" GROUP BY c.path");
        }
        // A task may have hundreds of matching description chunks, but it is
        // one result. Collapse it BEFORE the candidate limit; otherwise those
        // chunks consume the budget and silently hide other matching tasks.
        // Materialize only IDs/rank/anchor metadata, not potentially large body
        // strings. FTS5's bm25 must be evaluated in its matching-row context,
        // before the window function. Preserve distinct note/Canvas chunks.
        sql = format!(
            "WITH matched AS MATERIALIZED ({sql}), ranked AS (
                SELECT *,row_number() OVER (
                    PARTITION BY CASE WHEN kind='task' THEN path END,
                                 CASE WHEN kind!='task' THEN chunk_id END
                    ORDER BY rank,line,chunk_id) AS occurrence FROM matched
            ), winners AS (
                SELECT chunk_id,rank FROM ranked WHERE occurrence=1
                ORDER BY rank,path,line,chunk_id LIMIT ?
            ) SELECT c.path,c.title,c.body,c.line,c.kind,c.node_id,c.revision,s.status,w.rank,s.stamp
              FROM winners w JOIN chunks c ON c.id=w.chunk_id
              JOIN sources s ON s.path=c.path ORDER BY w.rank,c.path,c.line,c.id"
        );
        values.push((limit.clamp(1, 200) * 3).to_string());
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params_from_iter(values.iter()), |row| {
            let body: String = row.get(2)?;
            let stored_line = row.get::<_, i64>(3)?;
            let base = stored_line.max(1) as usize;
            let matched = if mode == SearchMode::Literal && !query.terms.is_empty() {
                let folded = body.to_lowercase();
                folded.find(&query.terms[0]).map(|offset| {
                    let line = folded[..offset]
                        .bytes()
                        .filter(|byte| *byte == b'\n')
                        .count();
                    (line, body.lines().nth(line).unwrap_or(body.as_str()))
                })
            } else {
                body.lines().enumerate().find(|(_, line)| {
                    query
                        .terms
                        .iter()
                        .any(|term| line.to_lowercase().contains(term.trim_end_matches('*')))
                })
            };
            let (line, snippet) = matched.map_or((base, body.as_str()), |(offset, line)| {
                (if stored_line == 0 { 1 } else { base + offset }, line)
            });
            Ok((
                SearchHit {
                    path: row.get(0)?,
                    title: row.get(1)?,
                    snippet: snippet.trim().chars().take(240).collect(),
                    line,
                    kind: row.get(4)?,
                    node_id: row.get(5)?,
                    revision: row.get(6)?,
                    content_indexed: row.get::<_, String>(7)? == "indexed",
                    score: -row.get::<_, f64>(8)?,
                },
                row.get::<_, String>(9)?,
            ))
        })?;
        let mut anchors = HashSet::new();
        let mut verified = HashMap::new();
        let mut verification_bytes = 0_u64;
        for row in rows {
            let (mut hit, stored_stamp) = row?;
            if query.terms.is_empty() && hit.kind == "canvas" {
                hit.title = Path::new(&hit.path)
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned();
                hit.node_id = None;
                hit.snippet = "Открыть холст целиком".into();
            }
            let anchor_line = if hit.kind == "task" { 0 } else { hit.line };
            if !anchors.insert((hit.path.clone(), hit.node_id.clone(), anchor_line)) {
                continue;
            }
            let valid = if let Some(valid) = verified.get(&hit.path) {
                *valid
            } else {
                let valid = verify_hit(self, &hit, &stored_stamp, &tasks, &mut verification_bytes)?;
                verified.insert(hit.path.clone(), valid);
                if !valid {
                    report.stale_paths.push(hit.path.clone());
                    report.stale_sources += 1;
                }
                valid
            };
            if !valid {
                continue;
            }
            report.hits.push(hit);
            if report.hits.len() == limit.clamp(1, 200) {
                break;
            }
        }
        report.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(report)
    }
}
