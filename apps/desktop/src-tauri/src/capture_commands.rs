use super::{VaultState, current_vault};
use knowledge_vault::{
    CaptureApplied, CaptureChoice, CaptureInput, CaptureLibrary, CapturePreview, Vault,
};
use tauri::State;
fn scoped(root: &str, state: &State<'_, VaultState>) -> Result<Vault, String> {
    let vault = current_vault(state)?;
    if vault.root() != std::path::Path::new(root) {
        return Err("Хранилище сменилось; capture отменён".into());
    }
    Ok(vault)
}
#[tauri::command]
pub fn load_captures(root: String, state: State<'_, VaultState>) -> Result<CaptureLibrary, String> {
    scoped(&root, &state)?
        .load_captures()
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn inspect_capture_import(contents: String) -> Result<Vec<CaptureChoice>, String> {
    knowledge_vault::inspect_capture_import(&contents).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn read_capture_import(
    root: String,
    path: String,
    state: State<'_, VaultState>,
) -> Result<String, String> {
    let _ = scoped(&root, &state)?;
    knowledge_vault::read_capture_import(path).map_err(|e| e.to_string())
}
#[tauri::command]
pub fn import_capture(
    root: String,
    contents: String,
    choice_id: String,
    expected_fingerprint: String,
    state: State<'_, VaultState>,
) -> Result<CaptureLibrary, String> {
    scoped(&root, &state)?
        .import_capture(&contents, &choice_id, &expected_fingerprint)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn preview_capture(
    root: String,
    input: CaptureInput,
    state: State<'_, VaultState>,
) -> Result<CapturePreview, String> {
    scoped(&root, &state)?
        .preview_capture(&input)
        .map_err(|e| e.to_string())
}
#[tauri::command]
pub fn apply_capture(
    app: tauri::AppHandle,
    root: String,
    input: CaptureInput,
    expected_plan: String,
    state: State<'_, VaultState>,
    journal: State<'_, super::WorkspaceState>,
) -> Result<CaptureApplied, String> {
    let preview = scoped(&root, &state)?
        .preview_capture(&input)
        .map_err(|e| e.to_string())?;
    if preview.plan_hash != expected_plan {
        return Err("Файл или настройка изменились после предпросмотра; запись отменена".into());
    }
    if !preview.created {
        super::workspace_commands::capture_checkpoint(
            &app,
            &journal,
            &root,
            &preview.path,
            &preview.before,
        )?;
    }
    scoped(&root, &state)?
        .apply_capture(&input, &expected_plan)
        .map_err(|e| e.to_string())
}
