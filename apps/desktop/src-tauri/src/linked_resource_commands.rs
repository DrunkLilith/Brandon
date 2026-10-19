use super::VaultState;
use knowledge_vault::{NoteDocument, SaveOutcome, VaultMutation};
use tauri::{State, WebviewWindow};
#[tauri::command]
pub fn create_task_format(
    window: WebviewWindow,
    root: String,
    folder: Option<String>,
    title: String,
    contents: String,
    state: State<'_, VaultState>,
) -> Result<VaultMutation, String> {
    if contents.len() > 2 * 1024 * 1024 {
        return Err("Определение превышает 2 МиБ".into());
    }
    let raw: serde_json::Value =
        serde_json::from_str(&contents).map_err(|_| "Новое определение должно быть JSON")?;
    if raw
        .pointer("/brandon/version")
        .and_then(serde_json::Value::as_u64)
        != Some(1)
        || raw
            .pointer("/brandon/format")
            .and_then(serde_json::Value::as_str)
            != Some("task-view")
    {
        return Err("Неизвестная версия связанного формата".into());
    }
    let guard = state.vault.read().map_err(|_| "Vault недоступен")?;
    let vault = guard
        .as_ref()
        .filter(|v| window.label() == "main" && v.root().to_string_lossy() == root)
        .ok_or("Хранилище или окно изменилось")?;
    vault
        .create_resource(folder.as_deref(), &title, "base", &contents)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn read_linked_resource(
    window: WebviewWindow,
    root: String,
    path: String,
    state: State<'_, VaultState>,
) -> Result<NoteDocument, String> {
    let guard = state.vault.read().map_err(|_| "Vault недоступен")?;
    let vault = guard
        .as_ref()
        .filter(|v| window.label() == "main" && v.root().to_string_lossy() == root)
        .ok_or("Хранилище или окно изменилось")?;
    vault.read_linked_resource(&path).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn save_linked_resource(
    window: WebviewWindow,
    root: String,
    path: String,
    expected: String,
    contents: String,
    state: State<'_, VaultState>,
) -> Result<SaveOutcome, String> {
    let guard = state.vault.read().map_err(|_| "Vault недоступен")?;
    let vault = guard
        .as_ref()
        .filter(|v| window.label() == "main" && v.root().to_string_lossy() == root)
        .ok_or("Хранилище или окно изменилось")?;
    vault
        .save_linked_resource(&path, &expected, &contents)
        .map_err(|e| e.to_string())
}
