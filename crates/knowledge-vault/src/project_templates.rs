//! Own local project templates: inert sources, CAS catalogue, preview/create-only bundle.
use super::*;
use crate::project_template_render::{public_path, render};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json, value::RawValue};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Read,
    path::Path,
    sync::Mutex,
};
const MAX_SOURCE: usize = 8 * 1024 * 1024;
const MAX_REGISTRY: usize = 32 * 1024 * 1024;
const REGISTRY: &str = "project-templates.v1.json";
const MANIFEST: &str = "project.brandon.json";
static LOCK: Mutex<()> = Mutex::new(());
#[cfg(test)]
#[path = "project_templates_tests.rs"]
mod tests;
pub(super) fn invalid(message: impl Into<String>) -> VaultError {
    VaultError::UnsupportedProperty(message.into())
}
fn io_error(source: io::Error) -> VaultError {
    VaultError::Io {
        context: "Операция с шаблоном не завершена",
        source,
    }
}
#[derive(Serialize, Clone)]
pub(super) struct TemplateFile {
    pub path: String,
    pub content: String,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Box<RawValue>>,
}
#[derive(Serialize, Clone)]
pub(super) struct Template {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub entrypoint: String,
    pub files: Vec<TemplateFile>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Box<RawValue>>,
}
#[derive(Serialize)]
struct Entry {
    id: String,
    source: String,
    #[serde(flatten)]
    extra: BTreeMap<String, Box<RawValue>>,
}
#[derive(Serialize)]
struct Registry {
    format: String,
    version: u32,
    entries: Vec<Entry>,
    #[serde(flatten)]
    extra: BTreeMap<String, Box<RawValue>>,
}
type RawFields = BTreeMap<String, Box<RawValue>>;
fn raw_fields<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<RawFields, D::Error> {
    struct Fields;
    impl<'de> serde::de::Visitor<'de> for Fields {
        type Value = RawFields;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("JSON object without duplicate keys")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut fields = BTreeMap::new();
            while let Some((key, value)) = map.next_entry::<String, Box<RawValue>>()? {
                if fields.len() >= 4096 || fields.insert(key, value).is_some() {
                    return Err(serde::de::Error::custom(
                        "Повторяющееся поле или слишком большой объект шаблона",
                    ));
                }
            }
            Ok(fields)
        }
    }
    deserializer.deserialize_map(Fields)
}
fn field<T: serde::de::DeserializeOwned, E: serde::de::Error>(
    fields: &mut RawFields,
    key: &str,
) -> std::result::Result<T, E> {
    let value = fields
        .remove(key)
        .ok_or_else(|| E::custom(format!("Отсутствует поле {key}")))?;
    serde_json::from_str(value.get()).map_err(E::custom)
}
impl<'de> Deserialize<'de> for TemplateFile {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let mut fields = raw_fields(d)?;
        Ok(Self {
            path: field(&mut fields, "path")?,
            content: field(&mut fields, "content")?,
            extra: fields,
        })
    }
}
impl<'de> Deserialize<'de> for Template {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let mut f = raw_fields(d)?;
        Ok(Self {
            format: field(&mut f, "format")?,
            version: field(&mut f, "version")?,
            id: field(&mut f, "id")?,
            name: field(&mut f, "name")?,
            category: field(&mut f, "category")?,
            description: field(&mut f, "description")?,
            entrypoint: field(&mut f, "entrypoint")?,
            files: field(&mut f, "files")?,
            extra: f,
        })
    }
}
impl<'de> Deserialize<'de> for Entry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let mut f = raw_fields(d)?;
        Ok(Self {
            id: field(&mut f, "id")?,
            source: field(&mut f, "source")?,
            extra: f,
        })
    }
}
impl<'de> Deserialize<'de> for Registry {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        let mut f = raw_fields(d)?;
        Ok(Self {
            format: field(&mut f, "format")?,
            version: field(&mut f, "version")?,
            entries: field(&mut f, "entries")?,
            extra: f,
        })
    }
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            format: "brandon.project-template-catalog".into(),
            version: 1,
            entries: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TemplateChoice {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub entrypoint: String,
    pub files: Vec<String>,
    pub fingerprint: String,
    pub builtin: bool,
    pub error: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCatalog {
    pub fingerprint: Option<String>,
    pub choices: Vec<TemplateChoice>,
    pub error: Option<String>,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TemplateInput {
    pub id: String,
    pub fingerprint: String,
    pub name: String,
    pub parent: String,
    pub date: String,
    pub request_id: String,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TemplateOutput {
    pub path: String,
    pub content: String,
    pub fingerprint: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplatePreview {
    pub input: TemplateInput,
    pub path: String,
    pub entrypoint: String,
    pub project_key: String,
    pub files: Vec<TemplateOutput>,
    pub warnings: Vec<String>,
    pub plan_hash: String,
}
#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct TemplateCreated {
    pub path: String,
    pub entrypoint: String,
    pub project_key: String,
    pub created: bool,
    pub files: usize,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateRemoved {
    pub catalog: TemplateCatalog,
    pub archive_path: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateFileEdit {
    pub path: String,
    pub content: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateEdit {
    pub name: String,
    pub category: String,
    pub description: String,
    pub entrypoint: String,
    pub files: Vec<TemplateFileEdit>,
}
pub fn edit_project_template(source: &str, edit: TemplateEdit) -> Result<String> {
    if edit.files.len() > 32
        || edit.files.iter().any(|f| f.content.len() > 1024 * 1024)
        || edit.files.iter().map(|f| f.content.len()).sum::<usize>() > 4 * 1024 * 1024
        || edit.name.len() > 512
        || edit.category.len() > 128
        || edit.description.len() > 4096
    {
        return Err(invalid("Правка превышает бюджет шаблона"));
    }
    let mut template = parse(source)?;
    template.name = edit.name;
    template.category = edit.category;
    template.description = edit.description;
    template.entrypoint = edit.entrypoint;
    let previous: BTreeMap<_, _> = template
        .files
        .into_iter()
        .map(|f| (f.path.clone(), f))
        .collect();
    template.files = edit
        .files
        .into_iter()
        .map(|f| TemplateFile {
            extra: previous
                .get(&f.path)
                .map(|old| old.extra.clone())
                .unwrap_or_default(),
            path: f.path,
            content: f.content,
        })
        .collect();
    let result = serde_json::to_string_pretty(&template)
        .map_err(|_| invalid("Не удалось подготовить шаблон"))?;
    parse(&result)?;
    Ok(result)
}
fn id_valid(id: &str) -> bool {
    !id.is_empty() && id.len() <= 100 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
}
fn parse(source: &str) -> Result<Template> {
    if source.len() > MAX_SOURCE {
        return Err(invalid("Шаблон превышает 8 МиБ JSON"));
    }
    let template: Template = serde_json::from_str(source)
        .map_err(|_| invalid("Повреждённый или неизвестный формат шаблона"))?;
    if template.format != "brandon.project-template" || template.version != 1 {
        return Err(invalid("Неизвестная версия шаблона; исходник не изменён"));
    }
    if !id_valid(&template.id)
        || template.name.trim().is_empty()
        || template.name.len() > 512
        || template.category.trim().is_empty()
        || template.category.len() > 128
        || template.description.len() > 4096
        || template
            .name
            .chars()
            .chain(template.category.chars())
            .any(char::is_control)
    {
        return Err(invalid("Некорректные метаданные шаблона"));
    }
    if template.files.is_empty() || template.files.len() > 32 {
        return Err(invalid("Шаблон должен содержать от 1 до 32 файлов"));
    }
    let mut paths = BTreeSet::new();
    let mut total = 0;
    for f in &template.files {
        public_path(&f.path)?;
        let lower = f.path.to_lowercase();
        if ![".md", ".canvas", ".base"]
            .iter()
            .any(|ext| lower.ends_with(ext))
            || f.path
                .split('/')
                .next()
                .is_some_and(|p| p.eq_ignore_ascii_case(MANIFEST))
        {
            return Err(invalid(
                "Только публичные .md/.canvas/.base, без замены manifest",
            ));
        }
        if f.content.len() > 1024 * 1024 {
            return Err(invalid("Источник одного файла шаблона превышает 1 МиБ"));
        }
        total += f.content.len();
        if !paths.insert(lower) {
            return Err(invalid(
                "Повторяющиеся пути (в том числе без учёта регистра)",
            ));
        }
    }
    if total > 4 * 1024 * 1024 {
        return Err(invalid("Исходники шаблона превышают 4 МиБ"));
    }
    for path in &paths {
        let mut prefix = String::new();
        for part in path.split('/').take(path.split('/').count() - 1) {
            if !prefix.is_empty() {
                prefix.push('/')
            }
            prefix.push_str(part);
            if paths.contains(&prefix) {
                return Err(invalid("Путь файла конфликтует с папкой шаблона"));
            }
        }
    }
    if !template.files.iter().any(|f| f.path == template.entrypoint) {
        return Err(invalid("Начальный файл отсутствует в составе"));
    }
    let input = TemplateInput {
        id: template.id.clone(),
        fingerprint: String::new(),
        name: "Preview".into(),
        parent: String::new(),
        date: "2026-01-01".into(),
        request_id: "inspect".into(),
    };
    render(&template, &input, "Preview")?;
    Ok(template)
}
fn choice(source: &str, builtin: bool, fallback: &str) -> TemplateChoice {
    match parse(source) {
        Ok(t) => TemplateChoice {
            id: t.id,
            name: t.name,
            category: t.category,
            description: t.description,
            entrypoint: t.entrypoint,
            files: t.files.into_iter().map(|f| f.path).collect(),
            fingerprint: fingerprint(source.as_bytes()),
            builtin,
            error: None,
        },
        Err(e) => TemplateChoice {
            id: fallback.into(),
            name: fallback.into(),
            category: "Недоступные".into(),
            description: String::new(),
            entrypoint: String::new(),
            files: Vec::new(),
            fingerprint: fingerprint(source.as_bytes()),
            builtin,
            error: Some(e.to_string()),
        },
    }
}
pub fn inspect_project_template(source: &str) -> TemplateChoice {
    choice(source, false, "import")
}
fn read_limited(path: &Path, max: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io_error)?
        .take(max as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > max {
        return Err(invalid("Файл превышает установленный лимит"));
    }
    Ok(bytes)
}
fn existing_chain(path: &Path) -> Result<PathBuf> {
    if !path.is_absolute() {
        return Err(invalid("Нужен абсолютный путь"));
    }
    let mut current = PathBuf::new();
    for part in path.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            return Err(invalid("Неоднозначный путь"));
        }
        current.push(part);
        if matches!(part, Component::Normal(_))
            && attachments::is_link(&fs::symlink_metadata(&current).map_err(io_error)?)
        {
            return Err(invalid("Ссылки/junction в пути шаблона не разрешены"));
        }
    }
    path.canonicalize().map_err(io_error)
}
pub fn read_project_template(path: &str) -> Result<String> {
    let path = existing_chain(Path::new(path))?;
    if !path.is_file() || !path.to_string_lossy().to_lowercase().ends_with(".json") {
        return Err(invalid("Выберите JSON-файл шаблона"));
    }
    String::from_utf8(read_limited(&path, MAX_SOURCE)?)
        .map_err(|_| invalid("Шаблон должен быть UTF-8"))
}
impl Vault {
    fn template_registry(&self) -> Result<(Registry, String)> {
        let directory = self.root.join(".brandon");
        match fs::symlink_metadata(&directory) {
            Ok(meta) => {
                if !meta.is_dir() || attachments::is_link(&meta) {
                    return Err(invalid(
                        "Каталог шаблонов заменён ссылкой или другим объектом",
                    ));
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok((Registry::default(), fingerprint(b"")));
            }
            Err(e) => return Err(io_error(e)),
        }
        let path = directory.join(REGISTRY);
        task_storage::checked_file(&path)?;
        if !path.exists() {
            return Ok((Registry::default(), fingerprint(b"")));
        }
        let bytes = read_limited(&path, MAX_REGISTRY)?;
        let registry: Registry = serde_json::from_slice(&bytes)
            .map_err(|_| invalid("Каталог шаблонов повреждён; замена пустым запрещена"))?;
        if registry.format != "brandon.project-template-catalog"
            || registry.version != 1
            || registry.entries.len() > 64
        {
            return Err(invalid(
                "Неизвестная версия или превышен лимит каталога шаблонов",
            ));
        }
        let mut ids = BTreeSet::new();
        for entry in &registry.entries {
            if !id_valid(&entry.id)
                || entry.id.to_lowercase().starts_with("builtin-")
                || !ids.insert(entry.id.to_lowercase())
                || entry.source.len() > MAX_SOURCE
            {
                return Err(invalid(
                    "Некорректная запись каталога; данные оставлены без изменений",
                ));
            }
        }
        Ok((registry, fingerprint(&bytes)))
    }
    pub fn project_template_catalog(&self) -> TemplateCatalog {
        let mut choices: Vec<_> = crate::project_template_starters::sources()
            .into_iter()
            .map(|s| choice(&s, true, "builtin"))
            .collect();
        match self.template_registry() {
            Ok((registry, fp)) => {
                for entry in registry.entries {
                    let mut c = choice(&entry.source, false, &entry.id);
                    if c.id != entry.id {
                        c.error = Some("ID источника не совпадает с ключом каталога".into());
                        c.id = entry.id
                    }
                    choices.push(c)
                }
                TemplateCatalog {
                    fingerprint: Some(fp),
                    choices,
                    error: None,
                }
            }
            Err(e) => TemplateCatalog {
                fingerprint: None,
                choices,
                error: Some(e.to_string()),
            },
        }
    }
    fn template_source(&self, id: &str, expected: &str) -> Result<String> {
        let builtin = crate::project_template_starters::sources()
            .into_iter()
            .find(|source| serde_json::from_str::<Template>(source).is_ok_and(|t| t.id == id));
        let source = if let Some(source) = builtin {
            source
        } else {
            self.template_registry()?
                .0
                .entries
                .into_iter()
                .find(|e| e.id == id)
                .ok_or_else(|| invalid("Шаблон больше не найден"))?
                .source
        };
        if fingerprint(source.as_bytes()) != expected {
            return Err(invalid("Шаблон изменился; откройте актуальную версию"));
        }
        Ok(source)
    }
    pub fn project_template_source(&self, id: &str, expected: &str, copy: bool) -> Result<String> {
        let source = self.template_source(id, expected)?;
        if !copy {
            return Ok(source);
        }
        let mut template = parse(&source)?;
        template.id = format!("local-{}", uuid::Uuid::now_v7());
        template.name = format!("{} — своя копия", template.name);
        serde_json::to_string_pretty(&template)
            .map_err(|_| invalid("Не удалось подготовить копию шаблона"))
    }
    pub fn import_project_template(
        &self,
        source: &str,
        expected_catalog: &str,
        replace_fingerprint: Option<&str>,
    ) -> Result<TemplateCatalog> {
        let template = parse(source)?;
        if template.id.to_lowercase().starts_with("builtin-") {
            return Err(invalid(
                "Встроенный шаблон неизменяем; сначала сделайте свою копию",
            ));
        }
        let _guard = LOCK.lock().map_err(|_| invalid("Каталог недоступен"))?;
        let (mut registry, fp) = self.template_registry()?;
        if fp != expected_catalog {
            return Err(invalid("Каталог изменился; импорт не записан"));
        }
        if let Some(entry) = registry
            .entries
            .iter_mut()
            .find(|e| e.id.eq_ignore_ascii_case(&template.id))
        {
            if entry.source == source {
                return Ok(self.project_template_catalog());
            }
            if replace_fingerprint != Some(fingerprint(entry.source.as_bytes()).as_str())
                || entry.id != template.id
            {
                return Err(invalid(
                    "Этот ID уже существует; замена требует явного согласия и fingerprint прежней версии",
                ));
            }
            entry.source = source.into();
        } else {
            if replace_fingerprint.is_some() || registry.entries.len() >= 64 {
                return Err(invalid(
                    "Версия для замены отсутствует или каталог заполнен",
                ));
            }
            registry.entries.push(Entry {
                id: template.id,
                source: source.into(),
                extra: BTreeMap::new(),
            })
        }
        let bytes = serde_json::to_vec_pretty(&registry)
            .map_err(|_| invalid("Не удалось записать каталог"))?;
        if bytes.len() > MAX_REGISTRY {
            return Err(invalid("Каталог превышает 32 МиБ"));
        }
        let directory = task_storage::task_directory(self)?;
        task_storage::checked_file(&directory.join(REGISTRY))?;
        if self.template_registry()?.1 != expected_catalog {
            return Err(invalid("Каталог изменился перед записью"));
        }
        write_atomic(&directory.join(REGISTRY), &bytes)?;
        Ok(self.project_template_catalog())
    }
    pub fn remove_project_template(
        &self,
        id: &str,
        expected: &str,
        expected_catalog: &str,
    ) -> Result<TemplateRemoved> {
        let _guard = LOCK.lock().map_err(|_| invalid("Каталог недоступен"))?;
        let (mut registry, fp) = self.template_registry()?;
        if fp != expected_catalog {
            return Err(invalid("Каталог изменился; удаление не выполнено"));
        }
        let at = registry
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or_else(|| invalid("Локальный шаблон не найден"))?;
        if fingerprint(registry.entries[at].source.as_bytes()) != expected {
            return Err(invalid("Шаблон изменился; удаление не выполнено"));
        }
        let directory = task_storage::task_directory(self)?.join("template-trash");
        task_storage::checked_directory(&directory)?;
        let mut count = 0;
        let mut size = 0u64;
        for entry in fs::read_dir(&directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let meta = fs::symlink_metadata(entry.path()).map_err(io_error)?;
            if !meta.is_file() || attachments::is_link(&meta) {
                return Err(invalid("Архив содержит ссылку или неподдерживаемый объект"));
            }
            count += 1;
            size += meta.len()
        }
        if count >= 64 || size + registry.entries[at].source.len() as u64 > 64 * 1024 * 1024 {
            return Err(invalid(
                "Архив шаблонов заполнен; сначала сохраните нужные копии отдельно",
            ));
        }
        let archive = directory.join(format!("{id}-{}.json", uuid::Uuid::now_v7()));
        let stage = tempfile::Builder::new()
            .prefix(".template-archive-stage-")
            .tempdir_in(&directory)
            .map_err(io_error)?;
        let stage_file = stage.path().join("template.json");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stage_file)
            .map_err(io_error)?;
        file.write_all(registry.entries[at].source.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(io_error)?;
        drop(file);
        renamore::rename_exclusive(&stage_file, &archive).map_err(io_error)?;
        drop(stage);
        registry.entries.remove(at);
        if self.template_registry()?.1 != expected_catalog {
            return Err(invalid(
                "Каталог изменился; сохранён архив, но исходная запись не удалена",
            ));
        }
        write_atomic(
            &self.root.join(".brandon").join(REGISTRY),
            &serde_json::to_vec_pretty(&registry)
                .map_err(|_| invalid("Не удалось обновить каталог"))?,
        )?;
        Ok(TemplateRemoved {
            catalog: self.project_template_catalog(),
            archive_path: archive.to_string_lossy().into(),
        })
    }
    fn project_parent(&self, parent: &str) -> Result<PathBuf> {
        if !parent.is_empty() {
            public_path(parent)?
        }
        let mut path = self.root.clone();
        for part in parent.split('/').filter(|p| !p.is_empty()) {
            path.push(part);
            let meta = fs::symlink_metadata(&path).map_err(io_error)?;
            if !meta.is_dir() || attachments::is_link(&meta) {
                return Err(invalid(
                    "Родитель проекта должен быть обычной публичной папкой vault",
                ));
            }
        }
        let canonical = path.canonicalize().map_err(io_error)?;
        if !canonical.starts_with(&self.root) {
            return Err(invalid("Папка вышла за границы vault"));
        }
        let relative = relative_display(&self.root, &canonical)?;
        if !relative.is_empty() {
            public_path(&relative)?
        }
        Ok(canonical)
    }
    pub fn preview_project_template(&self, input: &TemplateInput) -> Result<TemplatePreview> {
        if !id_valid(&input.id)
            || input.fingerprint.len() != 64
            || !input.fingerprint.bytes().all(|b| b.is_ascii_hexdigit())
            || input.parent.len() > 512
            || public_path(&input.name).is_err()
            || input.name.contains('/')
            || !id_valid(&input.request_id)
            || input.name.len() > 256
            || input.name.trim() != input.name
            || safe_entry_name(&input.name)? != input.name
            || input.date.len() != 10
            || input.date.starts_with("0000")
            || NaiveDate::parse_from_str(&input.date, "%Y-%m-%d")
                .map(|d| d.format("%Y-%m-%d").to_string())
                .ok()
                .as_deref()
                != Some(input.date.as_str())
        {
            return Err(invalid("Проверьте имя проекта, дату и ID запроса"));
        }
        let parent = self.project_parent(&input.parent)?;
        let parent = relative_display(&self.root, &parent)?;
        let path = if parent.is_empty() {
            input.name.clone()
        } else {
            format!("{parent}/{}", input.name)
        };
        if path.len() > 512 {
            return Err(invalid("Ключ проекта превышает 512 байт"));
        }
        let template = parse(&self.template_source(&input.id, &input.fingerprint)?)?;
        let (files, mut warnings) = render(&template, input, &path)?;
        if !template.extra.is_empty() || template.files.iter().any(|f| !f.extra.is_empty()) {
            warnings
                .push("Неизвестные поля v1 сохранены в шаблоне; при создании не исполняются".into())
        }
        warnings.push("Canvas/Bases расширения Brandon могут не поддерживаться другими редакторами. Задачи не копируются; виды читают общую базу по ключу проекта.".into());
        let hashes: Vec<_> = files.iter().map(|f| (&f.path, &f.fingerprint)).collect();
        let hash_input =
            json!({"scope":self.identity.as_str(),"input":input,"path":path,"files":hashes});
        let plan_hash = fingerprint(
            serde_json::to_string(&hash_input)
                .expect("plan JSON")
                .as_bytes(),
        );
        let preview = TemplatePreview {
            input: input.clone(),
            entrypoint: format!("{path}/{}", template.entrypoint),
            project_key: path.clone(),
            path,
            files,
            warnings,
            plan_hash,
        };
        self.project_existing(&preview)?;
        Ok(preview)
    }
    fn project_existing(&self, preview: &TemplatePreview) -> Result<bool> {
        let path = self.root.join(&preview.path);
        let meta = match fs::symlink_metadata(&path) {
            Ok(meta) => meta,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(io_error(e)),
        };
        if !meta.is_dir() || attachments::is_link(&meta) {
            return Err(VaultError::AlreadyExists);
        }
        let manifest = path.join(MANIFEST);
        task_storage::checked_file(&manifest)?;
        let value: Value = serde_json::from_slice(
            &read_limited(&manifest, 65536).map_err(|_| VaultError::AlreadyExists)?,
        )
        .map_err(|_| VaultError::AlreadyExists)?;
        if value.get("format") == Some(&json!("brandon.project"))
            && value.get("version") == Some(&json!(1))
            && value.get("requestId") == Some(&json!(preview.input.request_id))
            && value.get("planHash") == Some(&json!(preview.plan_hash))
        {
            Ok(true)
        } else {
            Err(VaultError::AlreadyExists)
        }
    }
    pub fn apply_project_template(
        &self,
        input: &TemplateInput,
        expected_plan: &str,
    ) -> Result<TemplateCreated> {
        let _guard = LOCK
            .lock()
            .map_err(|_| invalid("Проект временно недоступен"))?;
        let preview = self.preview_project_template(input)?;
        if preview.plan_hash != expected_plan {
            return Err(invalid(
                "План, источник или сессия vault изменились. Подготовьте предпросмотр заново.",
            ));
        }
        let created = TemplateCreated {
            path: preview.path.clone(),
            entrypoint: preview.entrypoint.clone(),
            project_key: preview.project_key.clone(),
            created: !self.project_existing(&preview)?,
            files: preview.files.len(),
        };
        if !created.created {
            return Ok(created);
        }
        let parent = self.project_parent(&input.parent)?;
        let stage = tempfile::Builder::new()
            .prefix(".brandon-project-stage-")
            .tempdir_in(&parent)
            .map_err(io_error)?;
        for f in &preview.files {
            let path = stage.path().join(&f.path);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(io_error)?
            }
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&path)
                .map_err(io_error)?;
            file.write_all(f.content.as_bytes())
                .and_then(|()| file.sync_all())
                .map_err(io_error)?;
        }
        let hashes: Vec<_> = preview
            .files
            .iter()
            .map(|f| json!({"path":f.path,"fingerprint":f.fingerprint}))
            .collect();
        let manifest = json!({"format":"brandon.project","version":1,"templateId":input.id,"templateFingerprint":input.fingerprint,"requestId":input.request_id,"planHash":preview.plan_hash,"projectKey":preview.project_key,"entrypoint":preview.entrypoint.strip_prefix(&(preview.path.clone()+"/")),"files":hashes,"compatibility":{"taskState":"canonical task DB, not copied here","projectKey":"stable label; folder rename does not implicitly rename canonical projects","canvas":"JSON Canvas fallback with optional Brandon v1 extensions","base":"task views are an explicit Brandon extension","network":"no network/process actions during template creation"}});
        let bytes = serde_json::to_vec_pretty(&manifest)
            .map_err(|_| invalid("Не удалось подготовить manifest"))?;
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(stage.path().join(MANIFEST))
            .map_err(io_error)?;
        file.write_all(&bytes)
            .and_then(|()| file.sync_all())
            .map_err(io_error)?;
        drop(file);
        if self.project_parent(&input.parent)? != parent
            || self.template_source(&input.id, &input.fingerprint).is_err()
        {
            return Err(invalid("Источник или папка изменились перед созданием"));
        }
        renamore::rename_exclusive(stage.path(), self.root.join(&preview.path))
            .map_err(io_error)?;
        let _ = stage.keep();
        self.metadata_touch(&[&preview.path]);
        Ok(created)
    }
    pub fn export_project_template(
        &self,
        id: &str,
        expected: &str,
        folder: &str,
    ) -> Result<String> {
        let source = self.template_source(id, expected)?;
        let parent = existing_chain(Path::new(folder))?;
        if !parent.is_dir() || parent.starts_with(&self.root) {
            return Err(invalid("Для экспорта выберите папку вне исходного vault"));
        }
        let stage = tempfile::Builder::new()
            .prefix(".brandon-template-export-")
            .tempdir_in(&parent)
            .map_err(io_error)?;
        let path = stage.path().join("template.json");
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(io_error)?;
        file.write_all(source.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(io_error)?;
        drop(file);
        let destination = parent.join(format!("Brandon-{id}-{}.json", uuid::Uuid::now_v7()));
        if existing_chain(Path::new(folder))? != parent {
            return Err(invalid("Папка экспорта изменилась"));
        }
        renamore::rename_exclusive(&path, &destination).map_err(io_error)?;
        Ok(destination.to_string_lossy().into())
    }
}
