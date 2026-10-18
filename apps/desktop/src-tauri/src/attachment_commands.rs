use super::{VaultState, current_vault};
use knowledge_vault::{AttachmentImport, AttachmentInfo};
use std::{
    collections::HashMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, State};
use tokio::sync::Semaphore;

pub struct AttachmentState {
    imports: Mutex<ImportRegistry>,
    previews: Arc<Semaphore>,
}
#[derive(Default)]
struct ImportRegistry {
    running: HashMap<String, Arc<AtomicBool>>,
    cancelled: HashMap<String, Instant>,
}
impl Default for AttachmentState {
    fn default() -> Self {
        Self {
            imports: Mutex::new(ImportRegistry::default()),
            previews: Arc::new(Semaphore::new(2)),
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportProgress {
    id: String,
    copied: u64,
    total: u64,
}

#[tauri::command]
pub async fn import_attachment(
    app: AppHandle,
    source_path: String,
    request_id: String,
    state: State<'_, VaultState>,
    jobs: State<'_, AttachmentState>,
) -> Result<AttachmentImport, String> {
    if request_id.is_empty() || request_id.len() > 100 {
        return Err("Некорректный ID импорта".into());
    }
    let vault = current_vault(&state)?;
    let cancel = Arc::new(AtomicBool::new(false));
    {
        let mut imports = jobs.imports.lock().map_err(|_| "Импорт недоступен")?;
        imports
            .cancelled
            .retain(|_, time| time.elapsed() < Duration::from_secs(60));
        if imports.cancelled.remove(&request_id).is_some() {
            return Err("Импорт отменён".into());
        }
        if imports.running.len() >= 4 || imports.running.contains_key(&request_id) {
            return Err("Дождитесь завершения текущего импорта".into());
        }
        imports
            .running
            .insert(request_id.clone(), Arc::clone(&cancel));
    }
    let progress_id = request_id.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let mut published = Instant::now() - Duration::from_secs(1);
        vault
            .import_attachment(Path::new(&source_path), &cancel, |copied, total| {
                if copied == 0
                    || copied == total
                    || published.elapsed() >= Duration::from_millis(150)
                {
                    let _ = app.emit(
                        "attachment-import-progress",
                        ImportProgress {
                            id: progress_id.clone(),
                            copied,
                            total,
                        },
                    );
                    published = Instant::now();
                }
            })
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string());
    jobs.imports
        .lock()
        .map_err(|_| "Импорт недоступен")?
        .running
        .remove(&request_id);
    result?
}

#[tauri::command]
pub fn cancel_attachment_import(
    request_id: String,
    jobs: State<'_, AttachmentState>,
) -> Result<(), String> {
    if request_id.is_empty() || request_id.len() > 100 {
        return Err("Некорректный ID импорта".into());
    }
    let mut imports = jobs.imports.lock().map_err(|_| "Импорт недоступен")?;
    if let Some(cancel) = imports.running.get(&request_id) {
        cancel.store(true, Ordering::Relaxed);
    } else {
        imports
            .cancelled
            .retain(|_, time| time.elapsed() < Duration::from_secs(60));
        if imports.cancelled.len() < 128 {
            imports.cancelled.insert(request_id, Instant::now());
        }
    }
    Ok(())
}

#[tauri::command]
pub async fn attachment_info(
    relative_path: String,
    state: State<'_, VaultState>,
) -> Result<AttachmentInfo, String> {
    let vault = current_vault(&state)?;
    tauri::async_runtime::spawn_blocking(move || {
        vault
            .attachment_info(&relative_path)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn attachment_thumbnail(
    relative_path: String,
    large: Option<bool>,
    state: State<'_, VaultState>,
    jobs: State<'_, AttachmentState>,
) -> Result<tauri::ipc::Response, String> {
    let vault = current_vault(&state)?;
    let permit = Arc::clone(&jobs.previews)
        .acquire_owned()
        .await
        .map_err(|_| "Превью недоступно")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        (if large == Some(true) {
            vault.attachment_preview(&relative_path)
        } else {
            vault.attachment_thumbnail(&relative_path)
        })
        .map(tauri::ipc::Response::new)
        .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())?
}
