#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod attachment_commands;
mod canvas_export;
use canvas_export::{
    CanvasExportState, canvas_export_cancel, canvas_export_prepare, canvas_export_preview,
    canvas_export_write,
};
mod canvas_export_render;
mod canvas_render_scene;
mod capture_commands;
use capture_commands::{
    apply_capture, import_capture, inspect_capture_import, load_captures, preview_capture,
    read_capture_import,
};
mod linked_resource_commands;
mod metadata_commands;
mod search_commands;
mod task_commands;
mod workspace_commands;
use metadata_commands::{MetadataState, metadata_delta, metadata_status, reconcile_metadata};
use search_commands::{SearchState, rebuild_search_index, reconcile_search_index, search_vault};
use task_commands::{
    backup_tasks, inspect_task_backup, load_task_views, patch_managed_task, restore_task_backup,
    save_task_views,
};
use workspace_commands::{
    WorkspaceState, document_versions, load_workspace, put_draft, remove_draft, save_workspace,
};
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
    #[tauri::command]
    pub fn canvas_gpu_image() -> Result<String, String> {
        Err("Native images are currently available on Windows".into())
    }
}
use canvas_gpu::{
    GpuCanvasState, canvas_gpu_frame, canvas_gpu_image, canvas_gpu_scene, canvas_gpu_start,
    canvas_gpu_stop,
};

use knowledge_vault::{
    CreatedNote, NoteDocument, SaveOutcome, TaskIndexMutation, Vault, VaultIndex, VaultMutation,
    VaultSnapshot,
};
use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc, Mutex, RwLock,
        atomic::{AtomicU64, Ordering},
    },
};
use tauri::{AppHandle, Emitter, Manager, State};

#[tauri::command]
fn canvas_gpu_supported() -> bool {
    cfg!(windows)
}

/// Final phase of the UI's save/session flush. A renderer reload can leave
/// native JS-close listener IDs behind; destroy only the requesting main window.
#[tauri::command]
fn complete_workspace_close(
    window: tauri::WebviewWindow,
    state: State<'_, VaultState>,
    root: String,
) -> Result<(), String> {
    if window.label() != "main" || current_vault(&state)?.root().to_string_lossy() != root {
        return Err("Окно или vault изменились; закрытие отменено".into());
    }
    window.destroy().map_err(|error| error.to_string())
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
    root: &Path,
    generation: Arc<AtomicU64>,
) -> Result<RecommendedWatcher, String> {
    let app_handle = app.clone();
    let search = app.state::<SearchState>().inner().clone();
    let metadata = app.state::<MetadataState>().inner().clone();
    let watcher_root = root.to_path_buf();
    let mut watcher = RecommendedWatcher::new(
        move |result: notify::Result<notify::Event>| {
            let epoch = generation.load(Ordering::SeqCst);
            if epoch == 0 || !search.is_current(epoch) {
                return;
            }
            search.observe(epoch, &result);
            metadata.observe(epoch, &result);
            if let Ok(event) = result {
                if matches!(event.kind, notify::EventKind::Access(_))
                    || search_commands::private_parent_metadata(&event, &watcher_root)
                {
                    return;
                }
                let paths: Vec<String> = event
                    .paths
                    .iter()
                    .filter(|path| !is_search_cache_path(path))
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect();
                if !paths.is_empty() {
                    let _ = app_handle.emit("vault-changed", paths);
                }
            } else {
                let _ = app_handle.emit("vault-changed", Vec::<String>::new());
            }
        },
        Config::default(),
    )
    .map_err(|error| user_error("Не удалось запустить наблюдение за vault", error))?;
    watcher
        .watch(root, RecursiveMode::Recursive)
        .map_err(|error| user_error("Не удалось наблюдать за vault", error))?;
    Ok(watcher)
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
    let vault = vault.incremental().map_err(|error| error.to_string())?;
    let snapshot = vault.snapshot().map_err(|error| error.to_string())?;
    let mut current = state
        .vault
        .write()
        .map_err(|_| "Не удалось обновить vault")?;
    let mut watcher_slot = state
        .watcher
        .lock()
        .map_err(|_| "Не удалось обновить наблюдение за vault")?;
    // Stage fallible setup before changing the selected root or cancelling its
    // search worker. Initial reconciliation covers events before activation.
    let generation = Arc::new(AtomicU64::new(0));
    let watcher = start_watcher(app, vault.root(), Arc::clone(&generation))?;
    remember_vault(app, vault.root())?;
    let epoch = app.state::<SearchState>().activate(vault.clone());
    app.state::<MetadataState>().activate(vault.clone(), epoch);
    *current = Some(vault);
    generation.store(epoch, Ordering::SeqCst);
    *watcher_slot = Some(watcher);
    drop(watcher_slot);
    drop(current);
    app.state::<CanvasExportState>().scope_changed();
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
    // A renderer reload is not a new vault lifetime. Keep its watcher, revision
    // and background plans instead of cancelling and inventorying it again.
    if let Some(vault) = state.vault.read().map_err(|_| "Vault недоступен")?.clone() {
        return vault
            .snapshot()
            .map(Some)
            .map_err(|error| error.to_string());
    }
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
fn inspect_note_properties(contents: String) -> knowledge_vault::PropertyInspection {
    knowledge_vault::inspect_properties(&contents)
}

#[tauri::command]
fn patch_properties_source(
    contents: String,
    patch: knowledge_vault::PropertyPatch,
) -> Result<String, String> {
    knowledge_vault::patch_property(&contents, &patch)
}

#[tauri::command]
fn parse_structured_yaml(contents: String) -> Result<serde_json::Value, String> {
    knowledge_vault::parse_yaml_mapping(&contents)
}

#[tauri::command]
fn patch_note_property(
    root: String,
    relative_path: String,
    expected_fingerprint: String,
    patch: knowledge_vault::PropertyPatch,
    state: State<'_, VaultState>,
) -> Result<SaveOutcome, String> {
    let vault = current_vault(&state)?;
    if vault.root() != std::path::Path::new(&root) {
        return Err("Хранилище сменилось; правка свойства отменена".into());
    }
    vault
        .patch_note_property(&relative_path, &expected_fingerprint, &patch)
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
    root: Option<String>,
    state: State<'_, VaultState>,
) -> Result<knowledge_vault::TaskChange, String> {
    let vault = current_vault(&state)?;
    if root
        .as_deref()
        .is_some_and(|root| vault.root() != std::path::Path::new(root))
    {
        return Err("Хранилище сменилось; создание задачи отменено".into());
    }
    vault
        .create_task_record(&title, scheduled.as_deref())
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
        .manage(SearchState::default())
        .manage(WorkspaceState::default())
        .manage(MetadataState::default())
        .manage(CanvasExportState::default())
        .setup(|app| {
            search_commands::start_worker(app.handle().clone());
            metadata_commands::start_worker(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            workspace_commands::promote_linked_draft,
            linked_resource_commands::read_linked_resource,
            linked_resource_commands::create_task_format,
            linked_resource_commands::save_linked_resource,
            canvas_export_prepare,
            canvas_export_preview,
            canvas_export_cancel,
            canvas_export_write,
            canvas_gpu_supported,
            canvas_gpu_image,
            attachment_info,
            attachment_thumbnail,
            import_attachment,
            cancel_attachment_import,
            open_vault,
            restore_vault,
            complete_workspace_close,
            load_workspace,
            save_workspace,
            put_draft,
            remove_draft,
            document_versions,
            refresh_vault,
            metadata_delta,
            metadata_status,
            reconcile_metadata,
            read_note,
            read_resource,
            save_note,
            save_resource,
            inspect_note_properties,
            patch_properties_source,
            parse_structured_yaml,
            patch_note_property,
            create_note,
            create_resource,
            create_folder,
            rename_entry,
            move_entry,
            trash_entry,
            get_vault_index,
            create_task,
            patch_managed_task,
            load_task_views,
            save_task_views,
            backup_tasks,
            inspect_task_backup,
            restore_task_backup,
            load_captures,
            inspect_capture_import,
            read_capture_import,
            import_capture,
            preview_capture,
            apply_capture,
            update_task,
            search_vault,
            reconcile_search_index,
            rebuild_search_index,
            reveal_entry,
            canvas_gpu_start,
            canvas_gpu_scene,
            canvas_gpu_frame,
            canvas_gpu_stop,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Brandon desktop");
}

fn is_search_cache_path(path: &Path) -> bool {
    if path.ancestors().any(|ancestor| {
        ancestor
            .parent()
            .and_then(Path::file_name)
            .is_some_and(|name| name == ".brandon")
            && ancestor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| {
                    name.strip_prefix("search-recovery-").is_some_and(|suffix| {
                        suffix.len() == 6 && suffix.bytes().all(|byte| byte.is_ascii_alphanumeric())
                    })
                })
    }) {
        return true;
    }
    path.parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == ".brandon")
        && path.file_name().is_some_and(|name| {
            matches!(
                name.to_str(),
                Some(
                    "search-v1.sqlite3"
                        | "search-v1.sqlite3-journal"
                        | "search-v1.sqlite3-wal"
                        | "search-v1.sqlite3-shm"
                )
            )
        })
}

#[cfg(test)]
mod search_watcher_tests {
    use super::*;
    #[test]
    fn ignores_only_derivative_search_cache_not_tasks_or_notes() {
        for name in [
            "search-v1.sqlite3",
            "search-v1.sqlite3-journal",
            "search-v1.sqlite3-wal",
            "search-v1.sqlite3-shm",
        ] {
            assert!(is_search_cache_path(
                &Path::new("vault").join(".brandon").join(name)
            ));
        }
        for path in [
            "vault/.brandon/tasks.sqlite3",
            "vault/Note.md",
            "vault/search-v1.sqlite3",
            "vault/Board.canvas",
            "vault/.brandon/search-recovery-tasks.sqlite3",
            "vault/search-recovery-Ab123c/Note.md",
        ] {
            assert!(!is_search_cache_path(Path::new(path)));
        }
        for path in [
            "vault/.brandon/search-recovery-Ab123c",
            "vault/.brandon/search-recovery-Ab123c/search-v1.sqlite3",
        ] {
            assert!(is_search_cache_path(Path::new(path)));
        }
    }
}
