//! Local desktop UI search only. Agent access requires a separate grant boundary.
use super::{VaultState, current_vault};
use knowledge_vault::{SearchMode, SearchResponse};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use tauri::State;
use tokio::sync::Semaphore;

pub struct SearchState {
    worker: Arc<Semaphore>,
    latest: AtomicU64,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            worker: Arc::new(Semaphore::new(1)),
            latest: AtomicU64::new(0),
        }
    }
}

#[tauri::command]
pub async fn search_vault(
    query: String,
    mode: Option<SearchMode>,
    limit: Option<usize>,
    state: State<'_, VaultState>,
    jobs: State<'_, SearchState>,
) -> Result<SearchResponse, String> {
    let vault = current_vault(&state)?;
    let generation = jobs.latest.fetch_add(1, Ordering::Relaxed) + 1;
    let permit = Arc::clone(&jobs.worker)
        .acquire_owned()
        .await
        .map_err(|_| "Поиск остановлен")?;
    // Rapid typing may leave superseded requests waiting for the worker; they
    // must not each rescan the vault after the newest query has arrived.
    if jobs.latest.load(Ordering::Relaxed) != generation {
        return Err("Запрос заменён новым".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        vault
            .search_local(&query, mode.unwrap_or_default(), limit.unwrap_or(80))
            .map_err(|error| format!("Не удалось выполнить локальный поиск: {error}"))
    })
    .await
    .map_err(|error| error.to_string())?
}
