#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod attachment_commands;
#[cfg(windows)]
mod canvas_gpu;
use attachment_commands::{
    AttachmentState, attachment_info, attachment_thumbnail, cancel_attachment_import,
    import_attachment,
};
#[cfg(not(windows))]
mod canvas_gpu {
    #[derive(Default)]
    pub struct GpuCanvasState {}
    #[tauri::command]
    pub fn canvas_gpu_start() -> Result<(), String> {
        Err("Native canvas is currently available on Windows".into())
    }
    #[tauri::command]
    pub fn canvas_gpu_scene(delta: serde_json::Value) {
        let _ = delta;
    }
    #[tauri::command]
    pub fn canvas_gpu_frame(frame: serde_json::Value) {
        let _ = frame;
    }
    #[tauri::command]
    pub fn canvas_gpu_stop() {}
}
use canvas_gpu::{
    GpuCanvasState, canvas_gpu_frame, canvas_gpu_scene, canvas_gpu_start, canvas_gpu_stop,
};

use knowledge_vault::{
    CreatedNote, NoteDocument, SaveOutcome, SearchHit, TaskIndexMutation, Vault, VaultIndex,
    VaultMutation, VaultSnapshot,
};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{Mutex, RwLock},
};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
fn canvas_gpu_supported() -> bool {
    cfg!(windows)
}

#[derive(Default)]
struct VaultState {
    vault: RwLock<Option<Vault>>,
    watcher: Mutex<Option<RecommendedWatcher>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecentVault {
    path: PathBuf,
}

fn user_error(context: &str, error: impl std::fmt::Display) -> String {
    format!("{context}: {error}")
}

fn set_vault(state: &State<'_, VaultState>, vault: Vault) -> Result<(), String> {
    let mut current = state
        .vault
        .write()
        .map_err(|_| "Не удалось получить доступ к открытому vault".to_owned())?;
    *current = Some(vault);
    Ok(())
}

fn current_vault(state: &State<'_, VaultState>) -> Result<Vault, String> {
    state
        .vault
        .read()
        .map_err(|_| "Не удалось получить доступ к открытому vault".to_owned())?
        .clone()
        .ok_or_else(|| "Сначала откройте папку с Markdown".to_owned())
}

fn start_watcher(
    app: &AppHandle,
    state: &State<'_, VaultState>,
    root: &Path,
) -> Result<(), String> {
    let app_handle = app.clone();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<notify::Event>| {
            if let Ok(event) = result {
                let paths: Vec<String> = event
                    .paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                let _ = app_handle.emit("vault-changed", paths);
            }
        },
        Config::default(),
    )
    .map_err(|error| user_error("Не удалось запустить наблюдение за vault", error))?;
    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(|error| user_error("Не удалось наблюдать за vault", error))?;
    let mut current = state
        .watcher
        .lock()
        .map_err(|_| "Не удалось обновить наблюдение за vault".to_owned())?;
    *current = Some(watcher);
    Ok(())
}

fn recent_vault_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|directory| directory.join("recent-vault.json"))
        .map_err(|error| user_error("Не удалось определить папку настроек", error))
}

fn remember_vault(app: &AppHandle, root: &Path) -> Result<(), String> {
    let target = recent_vault_path(app)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| user_error("Не удалось создать папку настроек", error))?;
    }
    let payload = serde_json::to_vec_pretty(&RecentVault {
        path: root.to_owned(),
    })
    .map_err(|error| user_error("Не удалось подготовить настройки", error))?;
    fs::write(target, payload).map_err(|error| user_error("Не удалось запомнить vault", error))
}

fn activate_vault(
    app: &AppHandle,
    state: &State<'_, VaultState>,
    vault: Vault,
) -> Result<VaultSnapshot, String> {
    let snapshot = vault.snapshot().map_err(|error| error.to_string())?;
    remember_vault(app, vault.root())?;
    start_watcher(app, state, vault.root())?;
    set_vault(state, vault)?;
    Ok(snapshot)
}

#[tauri::command]
fn open_vault(
    app: AppHandle,
    path: String,
    state: State<'_, VaultState>,
) -> Result<VaultSnapshot, String> {
    let vault = Vault::open(path).map_err(|error| error.to_string())?;
    activate_vault(&app, &state, vault)
}

#[tauri::command]
fn restore_vault(
    app: AppHandle,
    state: State<'_, VaultState>,
) -> Result<Option<VaultSnapshot>, String> {
    let target = recent_vault_path(&app)?;
    let payload = match fs::read(&target) {
        Ok(payload) => payload,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(user_error("Не удалось прочитать настройки", error)),
    };
    let recent: RecentVault = serde_json::from_slice(&payload)
        .map_err(|error| user_error("Настройки vault повреждены", error))?;
    let vault = match Vault::open(recent.path) {
        Ok(vault) => vault,
        Err(_) => return Ok(None),
    };
    activate_vault(&app, &state, vault).map(Some)
}

#[tauri::command]
fn refresh_vault(state: State<'_, VaultState>) -> Result<VaultSnapshot, String> {
    current_vault(&state)?
        .snapshot()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn read_note(relative_path: String, state: State<'_, VaultState>) -> Result<NoteDocument, String> {
    current_vault(&state)?
        .read_note(&relative_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn read_resource(
    relative_path: String,
    state: State<'_, VaultState>,
) -> Result<NoteDocument, String> {
    current_vault(&state)?
        .read_resource(&relative_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_note(
    relative_path: String,
    contents: String,
    expected_fingerprint: Option<String>,
    state: State<'_, VaultState>,
) -> Result<SaveOutcome, String> {
    current_vault(&state)?
        .save_note(&relative_path, &contents, expected_fingerprint.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn save_resource(
    relative_path: String,
    contents: String,
    expected_fingerprint: Option<String>,
    state: State<'_, VaultState>,
) -> Result<SaveOutcome, String> {
    current_vault(&state)?
        .save_resource(&relative_path, &contents, expected_fingerprint.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_note(
    title: String,
    folder: Option<String>,
    state: State<'_, VaultState>,
) -> Result<CreatedNote, String> {
    current_vault(&state)?
        .create_note(folder.as_deref(), &title)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_resource(
    title: String,
    folder: Option<String>,
    extension: String,
    contents: String,
    state: State<'_, VaultState>,
) -> Result<VaultMutation, String> {
    current_vault(&state)?
        .create_resource(folder.as_deref(), &title, &extension, &contents)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_folder(
    name: String,
    parent: Option<String>,
    state: State<'_, VaultState>,
) -> Result<VaultMutation, String> {
    current_vault(&state)?
        .create_folder(parent.as_deref(), &name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn rename_entry(
    relative_path: String,
    new_name: String,
    update_links: bool,
    state: State<'_, VaultState>,
) -> Result<VaultMutation, String> {
    current_vault(&state)?
        .rename_entry(&relative_path, &new_name, update_links)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn move_entry(
    relative_path: String,
    destination_folder: Option<String>,
    update_links: bool,
    state: State<'_, VaultState>,
) -> Result<VaultMutation, String> {
    current_vault(&state)?
        .move_entry(&relative_path, destination_folder.as_deref(), update_links)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn trash_entry(
    relative_path: String,
    state: State<'_, VaultState>,
) -> Result<VaultSnapshot, String> {
    current_vault(&state)?
        .trash_entry(&relative_path)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_vault_index(state: State<'_, VaultState>) -> Result<VaultIndex, String> {
    current_vault(&state)?
        .index()
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn create_task(
    title: String,
    scheduled: Option<String>,
    state: State<'_, VaultState>,
) -> Result<TaskIndexMutation, String> {
    current_vault(&state)?
        .create_task(&title, scheduled.as_deref())
        .map_err(|error| error.to_string())
}

#[tauri::command]
#[expect(
    clippy::too_many_arguments,
    reason = "The Tauri IPC contract uses named task fields"
)]
fn update_task(
    task_id: String,
    relative_path: String,
    line: usize,
    block_id: Option<String>,
    completed: Option<bool>,
    scheduled: Option<String>,
    clear_scheduled: Option<bool>,
    due: Option<String>,
    clear_due: Option<bool>,
    state: State<'_, VaultState>,
) -> Result<TaskIndexMutation, String> {
    current_vault(&state)?
        .update_task(
            &task_id,
            &relative_path,
            line,
            block_id.as_deref(),
            completed,
            scheduled.as_deref(),
            clear_scheduled.unwrap_or(false),
            due.as_deref(),
            clear_due.unwrap_or(false),
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn search_vault(
    query: String,
    limit: Option<usize>,
    state: State<'_, VaultState>,
) -> Result<Vec<SearchHit>, String> {
    current_vault(&state)?
        .search(&query, limit.unwrap_or(50))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn reveal_entry(relative_path: String, state: State<'_, VaultState>) -> Result<(), String> {
    let path = current_vault(&state)?
        .absolute_entry_path(&relative_path)
        .map_err(|error| error.to_string())?;
    #[cfg(target_os = "windows")]
    Command::new("explorer")
        .arg(format!("/select,{}", path.to_string_lossy()))
        .spawn()
        .map_err(|error| user_error("Не удалось открыть Проводник", error))?;
    #[cfg(target_os = "macos")]
    Command::new("open")
        .args(["-R", &path.to_string_lossy()])
        .spawn()
        .map_err(|error| user_error("Не удалось открыть Finder", error))?;
    #[cfg(all(unix, not(target_os = "macos")))]
    Command::new("xdg-open")
        .arg(path.parent().unwrap_or(&path))
        .spawn()
        .map_err(|error| user_error("Не удалось открыть файловый менеджер", error))?;
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(VaultState::default())
        .manage(GpuCanvasState::default())
        .manage(AttachmentState::default())
        .invoke_handler(tauri::generate_handler![
            canvas_gpu_supported,
            attachment_info,
            attachment_thumbnail,
            import_attachment,
            cancel_attachment_import,
            open_vault,
            restore_vault,
            refresh_vault,
            read_note,
            read_resource,
            save_note,
            save_resource,
            create_note,
            create_resource,
            create_folder,
            rename_entry,
            move_entry,
            trash_entry,
            get_vault_index,
            create_task,
            update_task,
            search_vault,
            reveal_entry,
            canvas_gpu_start,
            canvas_gpu_scene,
            canvas_gpu_frame,
            canvas_gpu_stop,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Brandon desktop");
}
