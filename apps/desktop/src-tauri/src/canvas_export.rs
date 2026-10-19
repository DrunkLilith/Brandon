//! Scoped prepare/preview/create-only publish protocol for derived Canvas PNGs.
use crate::{
    VaultState,
    canvas_export_render::{self, ExportScene, RasterPixels, Rendered},
    canvas_render_scene::Rect,
    current_vault,
};
use knowledge_vault::Vault;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::Write,
    path::{Component, Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{Manager, State, WebviewWindow, ipc::Response};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrepareRequest {
    pub root: String,
    pub request_id: String,
    pub source_path: String,
    pub source: String,
    pub frame_id: String,
    pub scene: ExportScene,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prepared {
    pub request_id: String,
    pub report: Value,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Saved {
    pub directory: String,
    pub png_path: String,
    pub report_path: String,
}
struct PreparedJob {
    vault: Vault,
    request_id: String,
    source_path: String,
    source_fingerprint: String,
    request_fingerprint: String,
    asset_revisions: BTreeMap<String, Option<String>>,
    task_revisions: BTreeMap<String, Option<String>>,
    at: Instant,
    rendered: Rendered,
    report: Value,
}
enum Phase {
    Preparing {
        vault: Vault,
        id: String,
        cancel: Arc<AtomicBool>,
        at: Instant,
    },
    Ready(Arc<PreparedJob>),
    Writing {
        job: Arc<PreparedJob>,
        cancel: Arc<AtomicBool>,
    },
    Done {
        vault: Vault,
        id: String,
        at: Instant,
        result: Saved,
    },
}
#[derive(Default)]
pub struct CanvasExportState {
    inner: Mutex<Option<Phase>>,
    early_cancel: Mutex<HashMap<String, (Vault, Instant)>>,
}
impl CanvasExportState {
    pub fn scope_changed(&self) {
        if let Ok(mut slot) = self.inner.lock() {
            match slot.as_ref() {
                Some(Phase::Preparing { cancel, .. }) | Some(Phase::Writing { cancel, .. }) => {
                    cancel.store(true, Ordering::Relaxed)
                }
                _ => *slot = None,
            }
        }
        if let Ok(mut early) = self.early_cancel.lock() {
            early.clear();
        }
    }
}
const TTL: Duration = Duration::from_secs(180);
#[cfg(test)]
#[path = "canvas_export_tests.rs"]
mod tests;
fn cancelled(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        Err("Экспорт отменён".into())
    } else {
        Ok(())
    }
}
fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn stale() -> String {
    "Источник или vault изменился. Подготовьте экспорт заново.".into()
}
fn validate_id(id: &str) -> Result<(), String> {
    if id.is_empty()
        || id.len() > 100
        || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        Err("Некорректный запрос экспорта".into())
    } else {
        Ok(())
    }
}
fn owner(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Экспорт недоступен в этом окне".into())
    }
}
fn expire(slot: &mut Option<Phase>) {
    let expired = match slot.as_ref() {
        Some(Phase::Ready(j)) => j.at.elapsed() > TTL,
        Some(Phase::Done { at, .. }) => at.elapsed() > TTL,
        Some(Phase::Preparing { cancel, at, .. }) => {
            if at.elapsed() > TTL {
                cancel.store(true, Ordering::Relaxed);
            }
            false
        }
        _ => false,
    };
    if expired {
        *slot = None
    }
}
fn node_bounds(value: &Value) -> Result<Rect, String> {
    let num = |key: &str| {
        value
            .get(key)
            .and_then(Value::as_f64)
            .filter(|v| v.is_finite() && v.abs() <= 10_000_000.)
            .map(|v| v as f32)
            .ok_or_else(|| "Некорректная геометрия исходного Canvas".to_owned())
    };
    let b = Rect {
        x: num("x")?,
        y: num("y")?,
        width: num("width")?,
        height: num("height")?,
    };
    if b.width <= 0. || b.height <= 0. {
        return Err("Некорректный размер исходного Canvas".into());
    }
    Ok(b)
}
fn known_namespace(value: Option<&Value>) -> Result<(), String> {
    if let Some(v) = value
        && (!v.is_object() || v.get("version").and_then(Value::as_u64) != Some(1))
    {
        return Err("Неизвестная версия Canvas Brandon; экспорт не изменил файл".into());
    }
    Ok(())
}
fn task_label(record: Option<&knowledge_vault::TaskRecord>, path: &str) -> String {
    let Some(t) = record else {
        return format!("Задача недоступна\n{path}");
    };
    if !t.note_path.eq_ignore_ascii_case(path) {
        return format!("Путь описания изменился\n{path}");
    }
    let status = match t.status.as_str() {
        "done" => "Готово",
        "doing" => "В работе",
        _ => "К выполнению",
    };
    format!(
        "{} {}\n{}{}\n{}{}",
        if t.completed { "☑" } else { "☐" },
        t.title,
        status,
        t.project
            .as_ref()
            .map_or(String::new(), |p| format!(" · {p}")),
        t.scheduled
            .as_ref()
            .map_or(String::new(), |s| format!("План: {s}  ")),
        t.due
            .as_ref()
            .map_or(String::new(), |s| format!("Срок: {s}"))
    )
    .trim_end()
    .into()
}
fn source_scene(request: &PrepareRequest) -> Result<(String, BTreeMap<String, Value>), String> {
    if request.source.len() > 16 * 1024 * 1024
        || request.frame_id.is_empty()
        || request.frame_id.len() > 512
    {
        return Err("Слишком большой исходный Canvas".into());
    }
    canvas_export_render::validate(&request.scene)?;
    let source: Value =
        serde_json::from_str(&request.source).map_err(|_| "Исходный Canvas не является JSON")?;
    known_namespace(source.get("brandon"))?;
    if let Some(p) = source.pointer("/brandon/presentation") {
        known_namespace(Some(p))?;
    }
    let nodes = source
        .get("nodes")
        .and_then(Value::as_array)
        .filter(|n| n.len() <= 100000)
        .ok_or("Некорректный список объектов Canvas")?;
    let edges = source
        .get("edges")
        .and_then(Value::as_array)
        .filter(|n| n.len() <= 100000)
        .ok_or("Некорректный список связей Canvas")?;
    let mut ids = BTreeSet::new();
    for node in nodes {
        let id = node
            .get("id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .ok_or("Некорректный ID Canvas")?;
        if !ids.insert(id) {
            return Err("Повторяющийся ID Canvas".into());
        }
        known_namespace(node.get("brandon"))?;
        node_bounds(node)?;
        if let Some(ink) = node.pointer("/brandon/ink") {
            known_namespace(Some(ink))?;
        }
        if let Some(frame) = node.pointer("/brandon/frame") {
            known_namespace(Some(frame))?;
        }
    }
    let frame = nodes
        .iter()
        .find(|n| n.get("id").and_then(Value::as_str) == Some(request.frame_id.as_str()))
        .ok_or("Фрейм не найден")?;
    let f = frame
        .pointer("/brandon/frame")
        .ok_or("Это не фрейм Brandon")?;
    if frame.get("type").and_then(Value::as_str) != Some("group")
        || !matches!(
            f.get("preset").and_then(Value::as_str),
            Some(
                "custom"
                    | "a4"
                    | "letter"
                    | "16:9"
                    | "4:3"
                    | "1:1"
                    | "mobile"
                    | "tablet"
                    | "desktop"
            )
        )
    {
        return Err("Неизвестный фрейм".into());
    }
    let bounds = node_bounds(frame)?;
    if bounds != request.scene.bounds {
        return Err("Границы PNG не совпадают с фреймом".into());
    }
    let visible: Vec<_> = nodes
        .iter()
        .filter(|n| node_bounds(n).is_ok_and(|b| b.intersects(bounds)))
        .collect();
    if visible.len() != request.scene.nodes.len() {
        return Err("Состав PNG не совпадает с исходным фреймом".into());
    }
    let mut matched = BTreeMap::new();
    for (raw, prepared) in visible.into_iter().zip(&request.scene.nodes) {
        let b = node_bounds(raw)?;
        if raw.get("id").and_then(Value::as_str) != Some(prepared.id.as_str())
            || raw.get("type").and_then(Value::as_str) != Some(prepared.kind.as_str())
            || b != (Rect {
                x: prepared.x,
                y: prepared.y,
                width: prepared.width,
                height: prepared.height,
            })
        {
            return Err("Порядок или геометрия PNG не совпадает с источником".into());
        }
        if let Some(image) = &prepared.image
            && raw.get("file").and_then(Value::as_str) != Some(image.path.as_str())
        {
            return Err("PNG ссылается на изображение вне исходного фрейма".into());
        }
        if raw.pointer("/brandon/ink").is_some() != prepared.ink.is_some() {
            return Err("Штрихи PNG не совпадают с источником".into());
        }
        if prepared.kind == "file" {
            let path = raw.get("file").and_then(Value::as_str).unwrap_or("");
            let raster = [".png", ".jpg", ".jpeg", ".gif", ".webp"]
                .iter()
                .any(|suffix| path.to_lowercase().ends_with(suffix));
            if raster != prepared.image.is_some() {
                return Err("Тип вложения PNG не совпадает с источником".into());
            }
            if raw.pointer("/brandon/taskId").is_none() {
                let name = path.rsplit('/').next().unwrap_or("");
                let title = if name.to_lowercase().ends_with(".md") {
                    &name[..name.len() - 3]
                } else {
                    name
                };
                let expected = if raster {
                    name.to_owned()
                } else {
                    format!("{title}\n{path}")
                };
                if expected != prepared.text {
                    return Err("Подпись вложения PNG не совпадает с источником".into());
                }
            }
        }
        if prepared.kind == "text"
            && prepared.ink.is_none()
            && raw.get("text").and_then(Value::as_str).unwrap_or("") != prepared.text
        {
            return Err("Текст PNG не совпадает с источником".into());
        }
        if prepared.kind == "group"
            && raw.get("label").and_then(Value::as_str).unwrap_or("Группа") != prepared.text
        {
            return Err("Название группы PNG не совпадает с источником".into());
        }
        if prepared.kind == "link"
            && raw.get("url").and_then(Value::as_str).unwrap_or("Группа") != prepared.text
        {
            return Err("Ссылка PNG не совпадает с источником".into());
        }
        matched.insert(prepared.id.clone(), raw.clone());
    }
    let edge_ids: BTreeSet<_> = edges
        .iter()
        .filter_map(|e| e.get("id").and_then(Value::as_str))
        .collect();
    if request
        .scene
        .edges
        .iter()
        .any(|e| !edge_ids.contains(e.id.as_str()))
    {
        return Err("Связь PNG отсутствует в исходном Canvas".into());
    }
    Ok((
        frame
            .get("label")
            .and_then(Value::as_str)
            .unwrap_or("Фрейм")
            .into(),
        matched,
    ))
}
fn prepare(
    vault: Vault,
    request: PrepareRequest,
    cancel: &AtomicBool,
) -> Result<PreparedJob, String> {
    let request_fingerprint = digest(
        &serde_json::to_vec(&request).map_err(|_| "Не удалось проверить параметры экспорта")?,
    );
    cancelled(cancel)?;
    let source = vault
        .canvas_export_source(&request.source_path)
        .map_err(|_| "Исходный Canvas недоступен")?;
    if source.content != request.source {
        return Err(
            "Сохраните текущую доску перед экспортом или загрузите внешние изменения".into(),
        );
    }
    let (label, raw) = source_scene(&request)?;
    let mut scene = request.scene;
    let tasks: Vec<String> = raw
        .values()
        .filter_map(|n| {
            n.pointer("/brandon/taskId")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .collect();
    let records = vault
        .task_records_snapshot(&tasks)
        .map_err(|_| "Канонические задачи недоступны; PNG не подготовлен")?;
    let mut task_revisions = BTreeMap::new();
    let mut missing_tasks = Vec::new();
    for node in &mut scene.nodes {
        if let Some(raw) = raw.get(&node.id)
            && let Some(id) = raw.pointer("/brandon/taskId").and_then(Value::as_str)
        {
            let record = records.get(id);
            task_revisions.insert(id.into(), record.map(|r| r.revision()));
            if record.is_none_or(|r| {
                !r.note_path
                    .eq_ignore_ascii_case(raw.get("file").and_then(Value::as_str).unwrap_or(""))
            }) {
                missing_tasks.push(node.id.clone());
            }
            node.text = task_label(
                record,
                raw.get("file").and_then(Value::as_str).unwrap_or(""),
            );
        }
    }
    let mut requested: BTreeMap<String, u32> = BTreeMap::new();
    for node in &scene.nodes {
        if let Some(image) = &node.image {
            let edge = (image.bounds.width.max(image.bounds.height) * scene.scale as f32)
                .ceil()
                .clamp(1., 4096.) as u32;
            requested
                .entry(image.path.clone())
                .and_modify(|e| *e = (*e).max(edge))
                .or_insert(edge);
        }
    }
    if requested.len() > 64 {
        return Err("Фрейм содержит больше 64 разных растровых изображений".into());
    }
    let mut images = HashMap::new();
    let mut asset_revisions = BTreeMap::new();
    let mut asset_report = Vec::new();
    let mut bytes = 0usize;
    for (path, edge) in requested {
        cancelled(cancel)?;
        match vault.attachment_export_raster(&path, edge) {
            Ok(asset) => {
                asset_revisions.insert(path.clone(), Some(asset.fingerprint));
                let dimensions = asset.raster.as_ref().map(|r| [r.width, r.height]);
                asset_report.push(json!({"path":path,"status":if asset.raster.is_some(){"rendered"}else{"unavailable"},"sourceDimensions":[asset.source_width,asset.source_height],"renderedDimensions":dimensions,"resampled":asset.source_width>edge||asset.source_height>edge}));
                if let Some(raster) = asset.raster {
                    bytes += raster.rgba.len();
                    if bytes > canvas_export_render::MAX_RASTER_BYTES {
                        return Err("Изображения превышают бюджет PNG 64 МиБ. Уменьшите масштаб или состав фрейма.".into());
                    }
                    images.insert(
                        path,
                        RasterPixels {
                            width: raster.width,
                            height: raster.height,
                            rgba: raster.rgba,
                        },
                    );
                }
            }
            Err(_) => {
                asset_revisions.insert(path.clone(), None);
                asset_report.push(json!({"path":path,"status":"unavailable","reason":"Источник недоступен или запрещён политикой vault"}));
            }
        }
    }
    let rendered = canvas_export_render::render(&scene, &images, cancel)?;
    if rendered.preview.len() > 4 * 1024 * 1024 {
        return Err("Preview превышает 4 МиБ".into());
    }
    let inert_files: Vec<_> = scene
        .nodes
        .iter()
        .filter(|n| n.kind == "file" && n.image.is_none())
        .map(|n| n.id.clone())
        .collect();
    let report = json!({"format":"brandon.canvas-png-export","version":1,"renderer":"tiny-skia-0.12.0+cosmic-text-0.19.0","sourcePath":request.source_path,"sourceFingerprint":source.fingerprint,"frameId":request.frame_id,"frameLabel":label,"bounds":scene.bounds,"scale":scene.scale,"transparent":scene.transparent,"pngSha256":digest(&rendered.png),"render":rendered.report,"rasters":asset_report,"missingTasks":missing_tasks,"taskRevisions":task_revisions,"inertFileNodes":inert_files,"compatibility":{"class":"flattened-image","lost":["editing","live-links","live-task-state","layers","locks","speaker-notes","presentation-order"],"text":"Plain prepared text with native layout; Markdown styling and property badges from a small rich-card view are not exported.","fonts":"OS fonts are rasterized, not embedded; actual families and missing glyphs are recorded above.","media":"Only permitted bounded raster pixels are embedded. Other file types remain inert captions; no URL or script is fetched."}});
    let job = PreparedJob {
        vault,
        request_id: request.request_id,
        source_path: request.source_path,
        source_fingerprint: source.fingerprint,
        request_fingerprint,
        asset_revisions,
        task_revisions,
        at: Instant::now(),
        rendered,
        report,
    };
    revalidate(&job, cancel)?;
    Ok(job)
}
fn revalidate(job: &PreparedJob, cancel: &AtomicBool) -> Result<(), String> {
    cancelled(cancel)?;
    if job
        .vault
        .canvas_export_source(&job.source_path)
        .map_err(|_| stale())?
        .fingerprint
        != job.source_fingerprint
    {
        return Err(stale());
    }
    for (path, expected) in &job.asset_revisions {
        cancelled(cancel)?;
        if &job.vault.attachment_export_fingerprint(path).ok() != expected {
            return Err(stale());
        }
    }
    let ids: Vec<_> = job.task_revisions.keys().cloned().collect();
    let tasks = job.vault.task_records_snapshot(&ids).map_err(|_| stale())?;
    for (id, expected) in &job.task_revisions {
        if &tasks.get(id).map(|r| r.revision()) != expected {
            return Err(stale());
        }
    }
    Ok(())
}
fn linklike(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_type().is_symlink() || meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
fn destination(vault: &Vault, folder: &Path) -> Result<PathBuf, String> {
    if !folder.is_absolute() {
        return Err("Выберите абсолютный путь к папке экспорта".into());
    }
    let mut current = PathBuf::new();
    for part in folder.components() {
        if matches!(part, Component::ParentDir | Component::CurDir) {
            return Err("Папка экспорта содержит неоднозначные компоненты".into());
        }
        current.push(part);
        if matches!(part, Component::Normal(_))
            && linklike(&fs::symlink_metadata(&current).map_err(|_| "Папка экспорта недоступна")?)
        {
            return Err("Папка экспорта не должна проходить через ссылки или junction".into());
        }
    }
    let folder = folder
        .canonicalize()
        .map_err(|_| "Папка экспорта недоступна")?;
    if !folder.is_dir() || folder.starts_with(vault.root()) {
        return Err("Выберите папку вне исходного vault; оригиналы не изменяются".into());
    }
    Ok(folder)
}
fn publish(
    job: &PreparedJob,
    folder: &Path,
    cancel: &AtomicBool,
    commit: impl FnOnce(&Path, &Path) -> Result<(), String>,
) -> Result<Saved, String> {
    let folder = destination(&job.vault, folder)?;
    revalidate(job, cancel)?;
    let stage = tempfile::Builder::new()
        .prefix(".brandon-export-stage-")
        .tempdir_in(&folder)
        .map_err(|_| "Не удалось подготовить папку экспорта")?;
    let directory = folder.join(format!("Brandon-frame-{}", uuid::Uuid::now_v7()));
    let report =
        serde_json::to_vec_pretty(&job.report).map_err(|_| "Не удалось подготовить отчёт")?;
    if report.len() > 4 * 1024 * 1024 {
        return Err("Отчёт превышает 4 МиБ".into());
    }
    for (name, bytes) in [
        ("frame.png", job.rendered.png.as_slice()),
        ("report.json", report.as_slice()),
    ] {
        cancelled(cancel)?;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(stage.path().join(name))
            .map_err(|_| "Не удалось записать экспорт")?;
        file.write_all(bytes)
            .and_then(|()| file.sync_all())
            .map_err(|_| "Не удалось записать экспорт")?;
    }
    revalidate(job, cancel)?;
    cancelled(cancel)?;
    if destination(&job.vault, &folder)? != folder {
        return Err("Папка назначения изменилась".into());
    }
    commit(stage.path(), &directory)?;
    let _ = stage.keep();
    Ok(Saved {
        png_path: directory.join("frame.png").to_string_lossy().into(),
        report_path: directory.join("report.json").to_string_lossy().into(),
        directory: directory.to_string_lossy().into(),
    })
}

#[tauri::command]
pub async fn canvas_export_prepare(
    window: WebviewWindow,
    request: PrepareRequest,
    vault_state: State<'_, VaultState>,
    jobs: State<'_, CanvasExportState>,
) -> Result<Prepared, String> {
    owner(&window)?;
    validate_id(&request.request_id)?;
    let vault = current_vault(&vault_state)?;
    if vault.root().to_string_lossy() != request.root {
        return Err(stale());
    }
    let id = request.request_id.clone();
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut slot = jobs.inner.lock().map_err(|_| "Экспорт недоступен")?;
        expire(&mut slot);
        let mut early = jobs.early_cancel.lock().map_err(|_| "Экспорт недоступен")?;
        early.retain(|_, (scope, at)| scope.same_instance(&vault) && at.elapsed() <= TTL);
        if early.remove(&id).is_some() {
            return Err("Экспорт отменён".into());
        }
        if let Some(Phase::Ready(job)) = slot.as_ref()
            && job.request_id == id
            && job.vault.same_instance(&vault)
        {
            let fingerprint = digest(
                &serde_json::to_vec(&request)
                    .map_err(|_| "Не удалось проверить параметры экспорта")?,
            );
            if fingerprint != job.request_fingerprint {
                return Err("ID экспорта уже использован для других параметров".into());
            }
            return Ok(Prepared {
                request_id: id,
                report: job.report.clone(),
            });
        }
        if let Some(Phase::Done { id: done, .. }) = slot.as_ref()
            && done == &id
        {
            return Err("Этот экспорт уже сохранён".into());
        }
        if slot
            .as_ref()
            .is_some_and(|p| !matches!(p, Phase::Done { .. }))
        {
            return Err("Завершите или отмените текущий экспорт".into());
        }
        *slot = Some(Phase::Preparing {
            vault: vault.clone(),
            id: id.clone(),
            cancel: cancel.clone(),
            at: Instant::now(),
        });
    }
    let worker_cancel = cancel.clone();
    let result =
        tauri::async_runtime::spawn_blocking(move || prepare(vault, request, &worker_cancel))
            .await
            .map_err(|_| "Процесс подготовки PNG завершился с ошибкой".to_owned())
            .and_then(|r| r);
    let selected = current_vault(&vault_state).ok();
    let mut slot = jobs.inner.lock().map_err(|_| "Экспорт недоступен")?;
    let still_owned = matches!(slot.as_ref(),Some(Phase::Preparing{id:active,..})if active==&id);
    if !still_owned {
        return Err(stale());
    }
    match result {
        Ok(job) => {
            if cancel.load(Ordering::Relaxed)
                || selected
                    .as_ref()
                    .is_none_or(|v| !job.vault.same_instance(v))
            {
                *slot = None;
                return Err(stale());
            }
            let value = Prepared {
                request_id: id,
                report: job.report.clone(),
            };
            *slot = Some(Phase::Ready(Arc::new(job)));
            Ok(value)
        }
        Err(error) => {
            *slot = None;
            Err(error)
        }
    }
}
#[tauri::command]
pub fn canvas_export_preview(
    window: WebviewWindow,
    root: String,
    request_id: String,
    vault_state: State<'_, VaultState>,
    jobs: State<'_, CanvasExportState>,
) -> Result<Response, String> {
    owner(&window)?;
    validate_id(&request_id)?;
    let vault = current_vault(&vault_state)?;
    if vault.root().to_string_lossy() != root {
        return Err(stale());
    }
    let mut slot = jobs.inner.lock().map_err(|_| "Экспорт недоступен")?;
    expire(&mut slot);
    let Some(Phase::Ready(job)) = slot.as_ref() else {
        return Err("Preview недоступен или устарел".into());
    };
    if job.request_id != request_id || !job.vault.same_instance(&vault) {
        return Err(stale());
    }
    Ok(Response::new(job.rendered.preview.clone()))
}
#[tauri::command]
pub fn canvas_export_cancel(
    window: WebviewWindow,
    root: String,
    request_id: String,
    vault_state: State<'_, VaultState>,
    jobs: State<'_, CanvasExportState>,
) -> Result<(), String> {
    owner(&window)?;
    validate_id(&request_id)?;
    let vault = current_vault(&vault_state)?;
    let mut slot = jobs.inner.lock().map_err(|_| "Экспорт недоступен")?;
    expire(&mut slot);
    match slot.as_ref() {
        Some(Phase::Preparing {
            vault: scope,
            id,
            cancel,
            ..
        }) if id == &request_id && scope.root().to_string_lossy() == root => {
            cancel.store(true, Ordering::Relaxed)
        }
        Some(Phase::Writing { job, cancel })
            if job.request_id == request_id && job.vault.root().to_string_lossy() == root =>
        {
            cancel.store(true, Ordering::Relaxed)
        }
        Some(Phase::Ready(job))
            if job.request_id == request_id && job.vault.root().to_string_lossy() == root =>
        {
            *slot = None
        }
        Some(Phase::Done { id, .. }) if id == &request_id => {}
        _ => {
            if vault.root().to_string_lossy() == root {
                let mut early = jobs.early_cancel.lock().map_err(|_| "Экспорт недоступен")?;
                early.retain(|_, (scope, at)| scope.same_instance(&vault) && at.elapsed() <= TTL);
                if early.len() < 128 {
                    early.insert(request_id, (vault, Instant::now()));
                }
            }
        }
    }
    Ok(())
}
#[tauri::command]
pub async fn canvas_export_write(
    window: WebviewWindow,
    root: String,
    request_id: String,
    folder: String,
    vault_state: State<'_, VaultState>,
    jobs: State<'_, CanvasExportState>,
) -> Result<Saved, String> {
    owner(&window)?;
    validate_id(&request_id)?;
    let vault = current_vault(&vault_state)?;
    if vault.root().to_string_lossy() != root {
        return Err(stale());
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let job = {
        let mut slot = jobs.inner.lock().map_err(|_| "Экспорт недоступен")?;
        expire(&mut slot);
        if let Some(Phase::Done {
            vault: scope,
            id,
            result,
            ..
        }) = slot.as_ref()
            && id == &request_id
            && scope.same_instance(&vault)
        {
            return Ok(result.clone());
        }
        let Some(Phase::Ready(job)) = slot.as_ref() else {
            return Err("Подготовьте PNG перед сохранением".into());
        };
        if job.request_id != request_id || !job.vault.same_instance(&vault) {
            return Err(stale());
        }
        let job = job.clone();
        *slot = Some(Phase::Writing {
            job: job.clone(),
            cancel: cancel.clone(),
        });
        job
    };
    let saved_job = job.clone();
    let app = window.app_handle().clone();
    let flag = cancel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        publish(&saved_job, Path::new(&folder), &flag, |from, to| {
            let state = app.state::<VaultState>();
            let guard = state.vault.read().map_err(|_| "Vault недоступен")?;
            if guard
                .as_ref()
                .is_none_or(|v| !v.same_instance(&saved_job.vault))
            {
                return Err(stale());
            }
            cancelled(&flag)?;
            renamore::rename_exclusive(from, to).map_err(|_| {
                "Не удалось опубликовать экспорт без замены существующих файлов".into()
            })
        })
    })
    .await
    .map_err(|_| "Сохранение PNG завершилось с ошибкой".to_owned())
    .and_then(|r| r);
    let mut slot = jobs.inner.lock().map_err(|_| "Экспорт недоступен")?;
    match result {
        Ok(result) => {
            *slot = Some(Phase::Done {
                vault: job.vault.clone(),
                id: request_id,
                at: Instant::now(),
                result: result.clone(),
            });
            Ok(result)
        }
        Err(error) => {
            *slot = if cancel.load(Ordering::Relaxed) {
                None
            } else {
                Some(Phase::Ready(job))
            };
            Err(error)
        }
    }
}
