use super::{VaultState, current_vault};
use knowledge_vault::{ManagedTaskPatch, TaskBackupSummary, TaskChange, TaskViewsDocument, Vault};
use tauri::State;
fn scoped(root: &str, state: &State<'_, VaultState>) -> Result<Vault, String> {
    let vault = current_vault(state)?;
    if vault.root() != std::path::Path::new(root) {
        return Err("Хранилище сменилось; операция с задачами отменена".into());
    }
    Ok(vault)
}

#[tauri::command]
pub fn patch_managed_task(
    root: String,
    task_id: String,
    expected_revision: String,
    patch: ManagedTaskPatch,
    state: State<'_, VaultState>,
) -> Result<TaskChange, String> {
    scoped(&root, &state)?
        .patch_managed_task(&task_id, &expected_revision, &patch)
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub fn load_task_views(
    root: String,
    state: State<'_, VaultState>,
) -> Result<TaskViewsDocument, String> {
    scoped(&root, &state)?
        .load_task_views()
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub fn save_task_views(
    root: String,
    expected_fingerprint: String,
    value: serde_json::Value,
    state: State<'_, VaultState>,
) -> Result<TaskViewsDocument, String> {
    scoped(&root, &state)?
        .save_task_views(&expected_fingerprint, &value)
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub async fn backup_tasks(
    root: String,
    parent: String,
    state: State<'_, VaultState>,
) -> Result<TaskBackupSummary, String> {
    let vault = scoped(&root, &state)?;
    tauri::async_runtime::spawn_blocking(move || vault.backup_tasks(parent))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub async fn inspect_task_backup(
    root: String,
    path: String,
    state: State<'_, VaultState>,
) -> Result<TaskBackupSummary, String> {
    let _ = scoped(&root, &state)?;
    tauri::async_runtime::spawn_blocking(move || Vault::inspect_task_backup(path))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
#[tauri::command]
pub async fn restore_task_backup(
    root: String,
    path: String,
    parent: String,
    state: State<'_, VaultState>,
) -> Result<TaskBackupSummary, String> {
    let vault = scoped(&root, &state)?;
    tauri::async_runtime::spawn_blocking(move || vault.restore_task_backup(path, parent))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
