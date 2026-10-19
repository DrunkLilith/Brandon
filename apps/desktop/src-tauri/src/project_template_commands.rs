use super::VaultState;
use knowledge_vault::{
    TemplateCatalog, TemplateChoice, TemplateCreated, TemplateEdit, TemplateInput, TemplatePreview,
    TemplateRemoved, Vault,
};
use tauri::{AppHandle, Manager, WebviewWindow};
fn owner(window: &WebviewWindow) -> Result<(), String> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err("Шаблоны доступны только в основном окне".into())
    }
}
async fn work<T: Send + 'static>(
    app: AppHandle,
    root: String,
    run: impl FnOnce(&Vault) -> knowledge_vault::Result<T> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<VaultState>();
        let guard = state
            .vault
            .read()
            .map_err(|_| "Vault недоступен".to_owned())?;
        let vault = guard
            .as_ref()
            .filter(|v| v.root().to_string_lossy() == root)
            .ok_or("Хранилище изменилось".to_owned())?;
        run(vault).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Операция с шаблоном завершилась с ошибкой".to_owned())?
}
#[tauri::command]
pub async fn project_template_catalog(
    window: WebviewWindow,
    root: String,
) -> Result<TemplateCatalog, String> {
    owner(&window)?;
    work(window.app_handle().clone(), root, |v| {
        Ok(v.project_template_catalog())
    })
    .await
}
#[tauri::command]
pub async fn project_template_source(
    window: WebviewWindow,
    root: String,
    id: String,
    fingerprint: String,
    copy: bool,
) -> Result<String, String> {
    owner(&window)?;
    work(window.app_handle().clone(), root, move |v| {
        v.project_template_source(&id, &fingerprint, copy)
    })
    .await
}
#[tauri::command]
pub async fn inspect_project_template(
    window: WebviewWindow,
    source: String,
) -> Result<TemplateChoice, String> {
    owner(&window)?;
    tauri::async_runtime::spawn_blocking(move || knowledge_vault::inspect_project_template(&source))
        .await
        .map_err(|_| "Проверка шаблона завершилась с ошибкой".into())
}
#[tauri::command]
pub async fn edit_project_template(
    window: WebviewWindow,
    source: String,
    edit: TemplateEdit,
) -> Result<String, String> {
    owner(&window)?;
    tauri::async_runtime::spawn_blocking(move || {
        knowledge_vault::edit_project_template(&source, edit).map_err(|e| e.to_string())
    })
    .await
    .map_err(|_| "Подготовка шаблона завершилась с ошибкой".to_owned())?
}
#[tauri::command]
pub async fn read_project_template(
    window: WebviewWindow,
    root: String,
    path: String,
) -> Result<String, String> {
    owner(&window)?;
    work(window.app_handle().clone(), root, move |_| {
        knowledge_vault::read_project_template(&path)
    })
    .await
}
#[tauri::command]
pub async fn import_project_template(
    window: WebviewWindow,
    root: String,
    source: String,
    expected_catalog: String,
    replace_fingerprint: Option<String>,
) -> Result<TemplateCatalog, String> {
    owner(&window)?;
    work(window.app_handle().clone(), root, move |v| {
        v.import_project_template(&source, &expected_catalog, replace_fingerprint.as_deref())
    })
    .await
}
#[tauri::command]
pub async fn remove_project_template(
    window: WebviewWindow,
    root: String,
    id: String,
    fingerprint: String,
    expected_catalog: String,
) -> Result<TemplateRemoved, String> {
    owner(&window)?;
    work(window.app_handle().clone(), root, move |v| {
        v.remove_project_template(&id, &fingerprint, &expected_catalog)
    })
    .await
}
#[tauri::command]
pub async fn preview_project_template(
    window: WebviewWindow,
    root: String,
    input: TemplateInput,
) -> Result<TemplatePreview, String> {
    owner(&window)?;
    work(window.app_handle().clone(), root, move |v| {
        v.preview_project_template(&input)
    })
    .await
}
#[tauri::command]
pub async fn apply_project_template(
    window: WebviewWindow,
    root: String,
    input: TemplateInput,
    plan_hash: String,
) -> Result<TemplateCreated, String> {
    owner(&window)?;
    work(window.app_handle().clone(), root, move |v| {
        v.apply_project_template(&input, &plan_hash)
    })
    .await
}
#[tauri::command]
pub async fn export_project_template(
    window: WebviewWindow,
    root: String,
    id: String,
    fingerprint: String,
    folder: String,
) -> Result<String, String> {
    owner(&window)?;
    work(window.app_handle().clone(), root, move |v| {
        v.export_project_template(&id, &fingerprint, &folder)
    })
    .await
}
