//! Declarative QuickAdd Capture adapter, not a plugin/script runtime.
use super::{Result, Vault, VaultError, attachments, fingerprint, task_storage, write_atomic};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use serde_json::{Value, value::RawValue};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
};
#[cfg(test)]
#[path = "captures_tests.rs"]
mod captures_tests;

const REGISTRY: &str = "captures.v1.json";
const MAX_JSON: usize = 1024 * 1024;
const MAX_NOTE: usize = 1024 * 1024;
static CAPTURE_LOCK: Mutex<()> = Mutex::new(());
fn invalid(message: impl Into<String>) -> VaultError {
    VaultError::UnsupportedProperty(message.into())
}
fn io_error(source: std::io::Error) -> VaultError {
    VaultError::Io {
        context: "Capture не завершён",
        source,
    }
}

#[derive(Serialize, Deserialize)]
struct Registry {
    format: String,
    version: u32,
    choices: Vec<Entry>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Entry {
    id: String,
    source_version: Option<String>,
    raw: Box<RawValue>,
    #[serde(flatten)]
    extra: BTreeMap<String, Value>,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureChoice {
    pub id: String,
    pub name: String,
    pub command: bool,
    pub target: String,
    pub bottom: bool,
    pub create: bool,
    pub open_file: bool,
    pub fields: Vec<String>,
    pub error: Option<String>,
    #[serde(skip)]
    template: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureLibrary {
    pub fingerprint: String,
    pub choices: Vec<CaptureChoice>,
}
#[derive(Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureInput {
    pub choice_id: String,
    pub values: BTreeMap<String, String>,
    pub date_time: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapturePreview {
    pub input: CaptureInput,
    pub path: String,
    pub before: String,
    pub after: String,
    pub created: bool,
    pub bottom: bool,
    pub open_file: bool,
    pub plan_hash: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureApplied {
    pub created: bool,
    pub path: String,
    pub fingerprint: String,
    pub open_file: bool,
}

fn inert(value: &Value) -> bool {
    value.is_null()
        || value == false
        || value == ""
        || value.as_array().is_some_and(Vec::is_empty)
        || value
            .as_object()
            .is_some_and(|o| o.is_empty() || o.get("enabled") == Some(&Value::Bool(false)))
}
fn boolean(raw: &Value, name: &str) -> Result<bool> {
    match raw.get(name) {
        None => Ok(false),
        Some(Value::Bool(v)) => Ok(*v),
        _ => Err(invalid(format!("{name}: ожидается boolean"))),
    }
}
fn text<'a>(raw: &'a Value, name: &str) -> Result<&'a str> {
    raw.get(name)
        .and_then(Value::as_str)
        .ok_or_else(|| invalid(format!("{name}: ожидается строка")))
}
fn date_format(format: &str, time: NaiveDateTime) -> Result<String> {
    if format.is_empty() || format.len() > 64 {
        return Err(invalid("Слишком длинный DATE format"));
    }
    let mut rest = format;
    let mut result = String::new();
    while !rest.is_empty() {
        let token = [
            ("YYYY", "%Y"),
            ("MM", "%m"),
            ("DD", "%d"),
            ("HH", "%H"),
            ("mm", "%M"),
            ("ss", "%S"),
        ]
        .into_iter()
        .find(|(token, _)| rest.starts_with(token));
        if let Some((token, format)) = token {
            result.push_str(&time.format(format).to_string());
            rest = &rest[token.len()..];
        } else {
            let ch = rest.chars().next().unwrap();
            if !"-/: ._".contains(ch) {
                return Err(invalid(
                    "DATE: поддерживаются YYYY MM DD HH mm ss и простые разделители",
                ));
            }
            result.push(ch);
            rest = &rest[ch.len_utf8()..];
        }
    }
    Ok(result)
}
fn expand(
    template: &str,
    values: &BTreeMap<String, String>,
    time: NaiveDateTime,
    path: bool,
    fields: &mut BTreeSet<String>,
) -> Result<String> {
    if template.len() > 8192 || template.contains("<%") {
        return Err(invalid(
            "Слишком длинный формат или неподдерживаемый Templater",
        ));
    }
    let mut rest = template;
    let mut result = String::new();
    while let Some(start) = rest.find("{{") {
        result.push_str(&rest[..start]);
        let tail = &rest[start + 2..];
        let end = tail
            .find("}}")
            .ok_or_else(|| invalid("Незакрытая переменная capture"))?;
        let token = &tail[..end];
        let (kind, arg) = token
            .split_once(':')
            .map_or((token, None), |(k, v)| (k, Some(v)));
        if kind.eq_ignore_ascii_case("VALUE") {
            let key = arg.unwrap_or("");
            if key.len() > 128
                || key
                    .chars()
                    .any(|ch| !ch.is_alphanumeric() && !" _-".contains(ch))
            {
                return Err(invalid("Некорректное имя VALUE"));
            }
            fields.insert(key.into());
            if fields.len() > 16 {
                return Err(invalid("Не более 16 полей VALUE"));
            }
            let value = values.get(key).map_or("sample", String::as_str);
            if path && (value.chars().any(char::is_control) || value.contains(['/', '\\'])) {
                return Err(invalid(
                    "VALUE в пути не может содержать разделители или управляющие символы",
                ));
            }
            result.push_str(value);
        } else if kind.eq_ignore_ascii_case("DATE") {
            result.push_str(&date_format(arg.unwrap_or("YYYY-MM-DD"), time)?);
        } else {
            return Err(invalid(format!("Переменная не поддерживается: {kind}")));
        }
        rest = &tail[end + 2..];
    }
    if rest.contains("}}") {
        return Err(invalid("Лишняя скобка capture format"));
    }
    result.push_str(rest);
    Ok(result)
}
fn target_path(value: &str) -> Result<String> {
    let mut path = value.to_owned();
    if !path.to_lowercase().ends_with(".md") {
        if path
            .rsplit('/')
            .next()
            .is_none_or(|part| part.contains('.'))
        {
            return Err(invalid("Capture target должен быть Markdown .md"));
        }
        path.push_str(".md");
    }
    task_storage::portable_path(&path)?;
    if path.split('/').count() > 16 {
        return Err(invalid("Путь глубже 16 папок"));
    }
    for part in path.split('/') {
        let stem = part.split('.').next().unwrap_or("").to_ascii_uppercase();
        if part.is_empty()
            || part.eq_ignore_ascii_case("node_modules")
            || part.eq_ignore_ascii_case("target")
            || part == "."
            || part.ends_with([' ', '.'])
            || part
                .chars()
                .any(|ch| ch.is_control() || "<>:\"|?*".contains(ch))
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err(invalid("Непереносимое имя capture target"));
        }
    }
    Ok(path)
}
fn compile(raw: &Value) -> Result<CaptureChoice> {
    if text(raw, "type")? != "Capture" {
        return Err(invalid(
            "Поддерживается только declarative Capture, не Macro/Template/Script",
        ));
    }
    let id = text(raw, "id")?;
    let name = text(raw, "name")?;
    if id.is_empty()
        || id.len() > 128
        || name.trim().is_empty()
        || name.len() > 512
        || id.chars().chain(name.chars()).any(char::is_control)
    {
        return Err(invalid("Некорректные id/name capture"));
    }
    let known = [
        "id",
        "name",
        "type",
        "command",
        "icon",
        "captureTo",
        "captureToActiveFile",
        "captureToCanvasNodeId",
        "activeFileWritePosition",
        "createFileIfItDoesntExist",
        "format",
        "prepend",
        "appendLink",
        "copyLinkToClipboard",
        "task",
        "eachLine",
        "insertAfter",
        "insertBefore",
        "newLineCapture",
        "openFile",
        "fileOpening",
        "templater",
        "useSelectionAsCaptureValue",
        "dateOrigin",
        "pickDayCommand",
        "onePageInput",
        "propertyCapture",
    ];
    for (key, value) in raw
        .as_object()
        .ok_or_else(|| invalid("Capture должен быть объектом"))?
    {
        if !known.contains(&key.as_str()) && !inert(value) {
            return Err(invalid(format!("Неизвестная активная опция: {key}")));
        }
    }
    for name in [
        "captureToActiveFile",
        "copyLinkToClipboard",
        "eachLine",
        "useSelectionAsCaptureValue",
        "pickDayCommand",
    ] {
        if boolean(raw, name)? {
            return Err(invalid(format!("Опция пока не поддерживается: {name}")));
        }
    }
    for name in [
        "captureToCanvasNodeId",
        "insertAfter",
        "insertBefore",
        "newLineCapture",
        "appendLink",
    ] {
        if raw.get(name).is_some_and(|value| !inert(value)) {
            return Err(invalid(format!("Опция пока не поддерживается: {name}")));
        }
    }
    for name in ["propertyCapture", "dateOrigin"] {
        if raw.get(name).is_some_and(|value| !value.is_null()) {
            return Err(invalid(format!("Опция пока не поддерживается: {name}")));
        }
    }
    if let Some(value) = raw.get("templater")
        && (value.get("afterCapture").is_some_and(|v| v != "none")
            || value
                .as_object()
                .is_none_or(|o| o.keys().any(|k| k != "afterCapture")))
    {
        return Err(invalid("Templater не поддерживается"));
    }
    let create = raw.get("createFileIfItDoesntExist");
    if let Some(value) = create
        && (!value.is_object()
            || value
                .as_object()
                .unwrap()
                .keys()
                .any(|k| !["enabled", "createWithTemplate", "template"].contains(&k.as_str()))
            || boolean(value, "createWithTemplate")?)
    {
        return Err(invalid(
            "Создание с шаблоном или неизвестными опциями не поддерживается",
        ));
    }
    let enabled = create.map_or(Ok(false), |v| boolean(v, "enabled"))?;
    let format = raw.get("format");
    let mut template = "{{VALUE}}".to_owned();
    if let Some(value) = format {
        if !value.is_object()
            || value
                .as_object()
                .unwrap()
                .keys()
                .any(|k| !["enabled", "format"].contains(&k.as_str()))
        {
            return Err(invalid("Неизвестные опции format"));
        }
        if boolean(value, "enabled")? {
            template = text(value, "format")?.into();
        }
    }
    if boolean(raw, "task")? {
        template = format!("- [ ] {template}\n");
    }
    let open_file = boolean(raw, "openFile")?;
    if open_file
        && let Some(value) = raw.get("fileOpening")
        && (!value.is_object()
            || value
                .as_object()
                .unwrap()
                .keys()
                .any(|k| !["location", "direction", "mode", "focus"].contains(&k.as_str()))
            || value.get("location").is_some_and(|v| v != "tab")
            || value.get("mode").is_some_and(|v| v != "default")
            || value.get("focus").is_some_and(|v| v != true))
    {
        return Err(invalid(
            "openFile: поддерживается только обычная вкладка с фокусом",
        ));
    }
    let target = text(raw, "captureTo")?.to_owned();
    let mut fields = BTreeSet::new();
    let sample = NaiveDateTime::parse_from_str("2026-10-19T12:00:00", "%Y-%m-%dT%H:%M:%S").unwrap();
    target_path(&expand(
        &target,
        &BTreeMap::new(),
        sample,
        true,
        &mut fields,
    )?)?;
    expand(&template, &BTreeMap::new(), sample, false, &mut fields)?;
    Ok(CaptureChoice {
        id: id.into(),
        name: name.into(),
        command: boolean(raw, "command")?,
        target,
        bottom: boolean(raw, "prepend")?,
        create: enabled,
        open_file,
        fields: fields.into_iter().collect(),
        error: None,
        template,
    })
}
fn candidate(raw: &RawValue) -> CaptureChoice {
    let value: Value = serde_json::from_str(raw.get()).unwrap_or(Value::Null);
    match compile(&value) {
        Ok(choice) => choice,
        Err(error) => CaptureChoice {
            id: value.get("id").and_then(Value::as_str).unwrap_or("").into(),
            name: value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("Без названия")
                .into(),
            command: false,
            target: String::new(),
            bottom: false,
            create: false,
            open_file: false,
            fields: Vec::new(),
            error: Some(error.to_string()),
            template: String::new(),
        },
    }
}
fn import_entries(source: &str) -> Result<(Vec<Box<RawValue>>, Option<String>)> {
    if source.len() > MAX_JSON {
        return Err(invalid("Импорт больше 1 МиБ"));
    }
    let root: BTreeMap<String, Box<RawValue>> = serde_json::from_str(source)
        .map_err(|_| invalid("Ожидается JSON настройки QuickAdd с choices"))?;
    let values: Vec<Box<RawValue>> = serde_json::from_str(
        root.get("choices")
            .ok_or_else(|| invalid("В настройке нет choices"))?
            .get(),
    )
    .map_err(|_| invalid("choices должен быть массивом"))?;
    if values.len() > 100 {
        return Err(invalid("Не более 100 choices в одном импорте"));
    }
    let version = root
        .get("version")
        .and_then(|raw| serde_json::from_str::<String>(raw.get()).ok());
    let mut ids = HashSet::new();
    for raw in &values {
        let value: Value =
            serde_json::from_str(raw.get()).map_err(|_| invalid("Некорректный choice"))?;
        if let Some(id) = value.get("id").and_then(Value::as_str)
            && !ids.insert(id.to_owned())
        {
            return Err(invalid("Повторный id в импорте"));
        }
    }
    Ok((values, version))
}
fn load(vault: &Vault) -> Result<(Registry, String)> {
    let path = task_storage::task_directory(vault)?.join(REGISTRY);
    task_storage::checked_file(&path)?;
    if !path.exists() {
        return Ok((
            Registry {
                format: "brandon.captures".into(),
                version: 1,
                choices: Vec::new(),
                extra: BTreeMap::new(),
            },
            String::new(),
        ));
    }
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io_error)?
        .take(MAX_JSON as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_JSON {
        return Err(invalid("Реестр captures больше 1 МиБ"));
    }
    let registry: Registry = serde_json::from_slice(&bytes)
        .map_err(|_| invalid("Реестр captures повреждён; файл не изменён"))?;
    if registry.format != "brandon.captures"
        || registry.version != 1
        || registry.choices.len() > 100
    {
        return Err(invalid("Неизвестная версия captures; файл не перезаписан"));
    }
    let mut ids = HashSet::new();
    for entry in &registry.choices {
        let raw = candidate(&entry.raw);
        if raw.id != entry.id || entry.id.is_empty() || !ids.insert(&entry.id) {
            return Err(invalid("Повторный/некорректный id registry"));
        }
    }
    Ok((registry, fingerprint(&bytes)))
}
pub fn inspect_capture_import(source: &str) -> Result<Vec<CaptureChoice>> {
    Ok(import_entries(source)?
        .0
        .iter()
        .map(|raw| candidate(raw))
        .collect())
}
pub fn read_capture_import(path: impl AsRef<Path>) -> Result<String> {
    let path = path.as_ref();
    if path
        .extension()
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("json"))
    {
        return Err(invalid("Выберите JSON файл настройки"));
    }
    task_storage::checked_file(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(io_error)?
        .take(MAX_JSON as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_JSON {
        return Err(invalid("Импорт больше 1 МиБ"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("JSON должен быть UTF-8"))
}
fn library(registry: &Registry, fingerprint: String) -> CaptureLibrary {
    CaptureLibrary {
        fingerprint,
        choices: registry
            .choices
            .iter()
            .map(|entry| candidate(&entry.raw))
            .collect(),
    }
}
impl Vault {
    pub fn load_captures(&self) -> Result<CaptureLibrary> {
        let (registry, hash) = load(self)?;
        Ok(library(&registry, hash))
    }
    pub fn import_capture(
        &self,
        source: &str,
        choice_id: &str,
        expected: &str,
    ) -> Result<CaptureLibrary> {
        let _guard = CAPTURE_LOCK
            .lock()
            .map_err(|_| invalid("Capture lock недоступен"))?;
        let (mut registry, hash) = load(self)?;
        if hash != expected {
            return Err(invalid(
                "Реестр изменился; обновите captures перед импортом",
            ));
        }
        if registry.choices.iter().any(|entry| entry.id == choice_id) {
            return Err(invalid(
                "Choice с этим id уже импортирован; исходная настройка не заменяется",
            ));
        }
        let (entries, version) = import_entries(source)?;
        let raw = entries
            .into_iter()
            .find(|raw| candidate(raw).id == choice_id)
            .ok_or_else(|| invalid("Choice не найден"))?;
        let value: Value =
            serde_json::from_str(raw.get()).map_err(|_| invalid("Некорректный choice"))?;
        compile(&value)?;
        registry.choices.push(Entry {
            id: choice_id.into(),
            source_version: version,
            raw,
            extra: BTreeMap::new(),
        });
        let bytes = serde_json::to_vec_pretty(&registry)
            .map_err(|_| invalid("Не удалось сохранить captures"))?;
        if bytes.len() > MAX_JSON || registry.choices.len() > 100 {
            return Err(invalid("Слишком большой реестр captures"));
        }
        write_atomic(&task_storage::task_directory(self)?.join(REGISTRY), &bytes)?;
        Ok(library(&registry, fingerprint(&bytes)))
    }
    pub fn preview_capture(&self, input: &CaptureInput) -> Result<CapturePreview> {
        self.capture_plan(input).map(|(preview, _)| preview)
    }
    fn capture_plan(&self, input: &CaptureInput) -> Result<(CapturePreview, String)> {
        let (registry, registry_hash) = load(self)?;
        let entry = registry
            .choices
            .iter()
            .find(|entry| entry.id == input.choice_id)
            .ok_or_else(|| invalid("Capture не найден"))?;
        let choice = compile(
            &serde_json::from_str(entry.raw.get()).map_err(|_| invalid("Некорректный capture"))?,
        )?;
        if input.values.len() != choice.fields.len()
            || choice
                .fields
                .iter()
                .any(|key| !input.values.contains_key(key))
            || input.values.values().map(String::len).sum::<usize>() > 65536
            || input.values.values().any(|value| value.contains('\0'))
        {
            return Err(invalid(
                "Заполните ожидаемые VALUE поля, суммарно до 64 КиБ",
            ));
        }
        let time = NaiveDateTime::parse_from_str(&input.date_time, "%Y-%m-%dT%H:%M:%S")
            .map_err(|_| invalid("Укажите локальные дату и время с секундами"))?;
        let mut path = target_path(&expand(
            &choice.target,
            &input.values,
            time,
            true,
            &mut BTreeSet::new(),
        )?)?;
        let absolute = self.capture_target(&path, false)?;
        let created = !absolute.exists();
        if !created {
            path =
                super::relative_display(&self.root, &absolute.canonicalize().map_err(io_error)?)?;
        }
        if created && !choice.create {
            return Err(invalid("Файл отсутствует; create-if-missing выключен"));
        }
        let before = if created {
            String::new()
        } else {
            let mut bytes = Vec::new();
            fs::File::open(&absolute)
                .map_err(io_error)?
                .take(MAX_NOTE as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(io_error)?;
            if bytes.len() > MAX_NOTE {
                return Err(invalid("Capture target больше 1 МиБ; используйте редактор"));
            }
            String::from_utf8(bytes).map_err(|_| invalid("Capture target не UTF-8"))?
        };
        let eol = if before.contains("\r\n") {
            "\r\n"
        } else {
            "\n"
        };
        let inserted = expand(
            &choice.template,
            &input.values,
            time,
            false,
            &mut BTreeSet::new(),
        )?
        .replace("\r\n", "\n")
        .replace('\n', eol);
        if inserted.trim().is_empty() {
            return Err(invalid("Пустой capture не записывается"));
        }
        let after = if choice.bottom {
            format!(
                "{before}{}{inserted}",
                if !before.is_empty() && !before.ends_with('\n') {
                    eol
                } else {
                    ""
                }
            )
        } else {
            let offset = body_start(&before)?;
            format!(
                "{}{}{inserted}{}{}",
                &before[..offset],
                if offset
                    > if before.starts_with('\u{feff}') {
                        '\u{feff}'.len_utf8()
                    } else {
                        0
                    }
                    && !before[..offset].ends_with('\n')
                {
                    eol
                } else {
                    ""
                },
                if offset < before.len() && !inserted.ends_with('\n') {
                    eol
                } else {
                    ""
                },
                &before[offset..]
            )
        };
        if after.len() > MAX_NOTE {
            return Err(invalid("Результат capture больше 1 МиБ"));
        }
        let plan_hash = fingerprint(
            serde_json::to_string(&(
                input,
                &path,
                &before,
                &after,
                &registry_hash,
                created,
                choice.open_file,
            ))
            .unwrap()
            .as_bytes(),
        );
        Ok((
            CapturePreview {
                input: input.clone(),
                path,
                before,
                after,
                created,
                bottom: choice.bottom,
                open_file: choice.open_file,
                plan_hash,
            },
            registry_hash,
        ))
    }
    fn capture_target(&self, path: &str, create_parents: bool) -> Result<PathBuf> {
        let mut target = self.root.clone();
        let parts: Vec<_> = path.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            target.push(part);
            match fs::symlink_metadata(&target) {
                Ok(meta) => {
                    if attachments::is_link(&meta)
                        || (i + 1 < parts.len() && !meta.is_dir())
                        || (i + 1 == parts.len() && !meta.is_file())
                    {
                        return Err(invalid(
                            "Capture target заменён ссылкой/junction или другим типом",
                        ));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    if create_parents && i + 1 < parts.len() {
                        fs::create_dir(&target).map_err(io_error)?;
                    }
                }
                Err(error) => return Err(io_error(error)),
            }
        }
        Ok(target)
    }
    pub fn apply_capture(
        &self,
        input: &CaptureInput,
        expected_plan: &str,
    ) -> Result<CaptureApplied> {
        let _guard = CAPTURE_LOCK
            .lock()
            .map_err(|_| invalid("Capture lock недоступен"))?;
        let (plan, _) = self.capture_plan(input)?;
        if plan.plan_hash != expected_plan {
            return Err(invalid(
                "Файл или настройка изменились после предпросмотра. Ввод сохранён; получите новый preview",
            ));
        }
        let mut touches = vec![plan.path.clone()];
        let mut parent = Path::new(&plan.path).parent();
        while let Some(value) = parent {
            if value.as_os_str().is_empty() {
                break;
            }
            if !self.root.join(value).exists() {
                touches.push(value.to_string_lossy().replace('\\', "/"));
            }
            parent = value.parent();
        }
        let path = self.capture_target(&plan.path, true)?;
        if plan.created {
            let mut stage = tempfile::NamedTempFile::new_in(
                path.parent()
                    .ok_or_else(|| invalid("Нет родителя target"))?,
            )
            .map_err(io_error)?;
            stage
                .write_all(plan.after.as_bytes())
                .and_then(|()| stage.as_file().sync_all())
                .map_err(io_error)?;
            stage
                .persist_noclobber(&path)
                .map_err(|error| io_error(error.error))?;
        } else {
            // Revalidate after path preparation, immediately before atomic replace.
            let disk = self.read_note(&plan.path)?;
            if disk.content != plan.before {
                return Err(invalid("Файл изменился; capture не записан"));
            }
            write_atomic(&path, plan.after.as_bytes())?;
        }
        self.metadata_touch(&touches.iter().map(String::as_str).collect::<Vec<_>>());
        Ok(CaptureApplied {
            created: plan.created,
            path: plan.path,
            fingerprint: fingerprint(plan.after.as_bytes()),
            open_file: plan.open_file,
        })
    }
}
fn body_start(source: &str) -> Result<usize> {
    let bom = if source.starts_with('\u{feff}') {
        '\u{feff}'.len_utf8()
    } else {
        0
    };
    let body = &source[bom..];
    if !body.starts_with("---\n") && !body.starts_with("---\r\n") {
        return Ok(bom);
    }
    let mut offset = bom;
    for (i, line) in body.split_inclusive('\n').enumerate() {
        offset += line.len();
        if i > 0 && matches!(line.trim_end_matches(['\r', '\n']), "---" | "...") {
            return Ok(offset);
        }
    }
    Err(invalid(
        "Frontmatter не закрыт; позиция body-start неоднозначна",
    ))
}
