//! UI-only local search. Index invalidation is not an agent permission grant.
use super::{VaultState, current_vault};
use knowledge_vault::{SearchMode, SearchRebuild, SearchResponse, SearchUpdate, Vault, VaultError};
use notify::{
    Event, EventKind,
    event::{ModifyKind, RenameMode},
};
use std::{
    collections::{BTreeSet, VecDeque},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Semaphore;

const BATCH: usize = 128;
const READ_BUDGET: u64 = 128 * 1024 * 1024;
const RECONCILE_INTERVAL: Duration = Duration::from_secs(60);

#[derive(Clone, Default)]
pub struct SearchState(Arc<Jobs>);
struct Jobs {
    worker: Arc<Semaphore>,
    queue: Mutex<Queue>,
    epoch: AtomicU64,
    latest: AtomicU64,
}
impl Default for Jobs {
    fn default() -> Self {
        Self {
            worker: Arc::new(Semaphore::new(1)),
            queue: Mutex::new(Queue::default()),
            epoch: AtomicU64::new(0),
            latest: AtomicU64::new(0),
        }
    }
}
#[derive(Default)]
struct Queue {
    vault: Option<Vault>,
    prepared: bool,
    dirty: BTreeSet<String>,
    full: bool,
    plan: Option<Plan>,
    active: bool,
    error: Option<String>,
    retry_at: Option<Instant>,
    last_full: Option<Instant>,
}
struct Plan {
    paths: VecDeque<String>,
    remaining: u64,
    skipped: usize,
    failed: usize,
}
enum Work {
    Prepare,
    Inventory,
    Dirty(Vec<String>),
    Batch(Vec<String>, u64),
    Finish(usize, usize),
}

pub fn private_parent_metadata(event: &Event, root: &Path) -> bool {
    // ReadDirectoryChangesW reports directory timestamp changes as Modify(Any).
    // No public sources live under .brandon. Task-file events are still queued,
    // and every query independently checks the canonical task DB fingerprint.
    matches!(
        event.kind,
        EventKind::Modify(ModifyKind::Metadata(_) | ModifyKind::Any)
    ) && !event.paths.is_empty()
        && event
            .paths
            .iter()
            .all(|path| path == &root.join(".brandon"))
}

impl SearchState {
    pub fn metadata_invalidated(&self, epoch: u64, paths: Vec<String>, reset: bool) {
        if reset || paths.len() > 1024 {
            self.require_full(epoch);
        } else if !paths.is_empty() {
            self.dirty(epoch, paths);
        }
    }
    pub fn activate(&self, vault: Vault) -> u64 {
        let mut queue = self
            .0
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let epoch = self.0.epoch.fetch_add(1, Ordering::SeqCst) + 1;
        self.0.latest.fetch_add(1, Ordering::Relaxed);
        *queue = Queue {
            vault: Some(vault),
            full: true,
            ..Queue::default()
        };
        epoch
    }

    pub fn observe(&self, epoch: u64, result: &notify::Result<Event>) {
        let mut queue = self
            .0
            .queue
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if self.0.epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        let Some(root) = queue.vault.as_ref().map(|vault| vault.root().to_path_buf()) else {
            return;
        };
        let Ok(event) = result else {
            queue.full = true;
            return;
        };
        if matches!(event.kind, EventKind::Access(_))
            || (!event.need_rescan() && private_parent_metadata(event, &root))
        {
            return;
        }
        // Filter derivative remove/rename events before their unknown kind can
        // request inventory. Windows reports journal removal as Remove(Any).
        if !event.paths.is_empty()
            && event
                .paths
                .iter()
                .all(|path| super::is_search_cache_path(path))
        {
            if event.need_rescan() {
                queue.full = true;
            }
            return;
        }
        // A single folder event can represent 50k descendants. Expand it via
        // the background inventory, not an unbounded query-time dirty batch.
        let known_removed_files = event.paths.iter().all(|path| {
            path.strip_prefix(&root).ok().is_some_and(|relative| {
                queue.vault.as_ref().and_then(|vault| {
                    vault.metadata_kind(&relative.to_string_lossy().replace('\\', "/"))
                }) == Some(knowledge_vault::VaultEntryKind::File)
            })
        });
        if matches!(
            event.kind,
            EventKind::Create(notify::event::CreateKind::Folder)
                | EventKind::Remove(notify::event::RemoveKind::Folder)
        ) || matches!(
            event.kind,
            EventKind::Remove(notify::event::RemoveKind::Any)
        ) && !known_removed_files
            || event.paths.iter().any(|path| path.is_dir())
        {
            queue.full = true;
            return;
        }
        if event.need_rescan()
            || event.paths.is_empty()
            || matches!(
                event.kind,
                EventKind::Any
                    | EventKind::Other
                    | EventKind::Modify(ModifyKind::Name(
                        RenameMode::From | RenameMode::To | RenameMode::Any | RenameMode::Other
                    ))
            )
        {
            queue.full = true;
        }
        for path in &event.paths {
            if super::is_search_cache_path(path) {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&root) else {
                queue.full = true;
                continue;
            };
            if relative.as_os_str().is_empty() {
                queue.full = true;
                continue;
            }
            queue
                .dirty
                .insert(relative.to_string_lossy().replace('\\', "/"));
        }
        if queue.dirty.len() > 1024 {
            queue.dirty.clear();
            queue.full = true;
        }
    }

    fn scope(&self, vault: &Vault) -> Result<u64, String> {
        let queue = self
            .0
            .queue
            .lock()
            .map_err(|_| "Очередь поиска недоступна")?;
        if queue
            .vault
            .as_ref()
            .is_none_or(|selected| selected.root() != vault.root())
        {
            return Err("Vault изменился; повторите поиск".into());
        }
        Ok(self.0.epoch.load(Ordering::SeqCst))
    }
    fn cancelled(&self, epoch: u64) -> bool {
        self.0.epoch.load(Ordering::SeqCst) != epoch
    }
    pub fn is_current(&self, epoch: u64) -> bool {
        !self.cancelled(epoch)
    }
    fn require_full(&self, epoch: u64) {
        if let Ok(mut queue) = self.0.queue.lock() {
            if self.cancelled(epoch) {
                return;
            }
            queue.full = true;
            queue.retry_at = None;
        }
    }
    fn changed(app: &AppHandle) {
        let _ = app.emit("search-index-updated", ());
    }
    fn dirty(&self, epoch: u64, paths: Vec<String>) {
        if let Ok(mut queue) = self.0.queue.lock() {
            if self.cancelled(epoch) {
                return;
            }
            queue.dirty.extend(paths);
        }
    }

    fn prepare(&self, vault: &Vault, epoch: u64) -> Result<(), VaultError> {
        if self.cancelled(epoch) {
            return Err(VaultError::SearchCancelled);
        }
        let prepared = {
            let queue = self.0.queue.lock().unwrap_or_else(|p| p.into_inner());
            if self.cancelled(epoch) {
                return Err(VaultError::SearchCancelled);
            }
            queue.prepared
        };
        if !prepared {
            vault.prepare_search_cache()?;
            let mut queue = self.0.queue.lock().unwrap_or_else(|p| p.into_inner());
            if self.cancelled(epoch) {
                return Err(VaultError::SearchCancelled);
            }
            queue.prepared = true;
        }
        Ok(())
    }

    fn decorate(&self, mut response: SearchResponse, epoch: u64) -> Result<SearchResponse, String> {
        let queue = self
            .0
            .queue
            .lock()
            .map_err(|_| "Очередь поиска недоступна")?;
        if self.cancelled(epoch) {
            return Err("Vault изменился; запрос отменён".into());
        }
        if let Some(error) = &queue.error {
            return Err(error.clone());
        }
        response.syncing =
            queue.active || queue.full || queue.plan.is_some() || !queue.dirty.is_empty();
        response.pending_sources =
            queue.dirty.len() + queue.plan.as_ref().map_or(0, |plan| plan.paths.len());
        Ok(response)
    }
}

pub fn start_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let mut idle = true;
        loop {
            if idle {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            idle = true;
            let jobs = app.state::<SearchState>().inner().clone();
            let Ok(permit) = Arc::clone(&jobs.0.worker).try_acquire_owned() else {
                continue;
            };
            let selected = {
                let mut queue = jobs.0.queue.lock().unwrap_or_else(|p| p.into_inner());
                if queue.retry_at.is_some_and(|time| time > Instant::now()) {
                    None
                } else if let Some(vault) = queue.vault.clone() {
                    let epoch = jobs.0.epoch.load(Ordering::SeqCst);
                    let work = if !queue.prepared {
                        Some(Work::Prepare)
                    } else if !queue.dirty.is_empty() {
                        let paths: Vec<_> = queue.dirty.iter().take(BATCH).cloned().collect();
                        for path in &paths {
                            queue.dirty.remove(path);
                        }
                        Some(Work::Dirty(paths))
                    } else if let Some(plan) = &queue.plan {
                        if plan.paths.is_empty() {
                            Some(Work::Finish(plan.skipped, plan.failed))
                        } else {
                            Some(Work::Batch(
                                plan.paths.iter().take(BATCH).cloned().collect(),
                                plan.remaining,
                            ))
                        }
                    } else if queue.full
                        || queue
                            .last_full
                            .is_none_or(|last| last.elapsed() >= RECONCILE_INTERVAL)
                    {
                        queue.full = false;
                        Some(Work::Inventory)
                    } else {
                        None
                    };
                    if work.is_some() {
                        queue.active = true;
                    }
                    work.map(|work| (vault, epoch, work))
                } else {
                    None
                }
            };
            let Some((vault, epoch, work)) = selected else {
                drop(permit);
                continue;
            };
            // The semaphore hands queued queries the next turn. Do not add a
            // fixed delay to every one of the 391 batches in a 50k inventory.
            idle = false;
            let app = app.clone();
            let _ = tauri::async_runtime::spawn_blocking(move || {
                let _permit = permit;
                let result = (|| -> Result<(bool, Option<Plan>, usize, u64), VaultError> {
                    if jobs.cancelled(epoch) {
                        return Err(VaultError::SearchCancelled);
                    }
                    match &work {
                        Work::Prepare => {
                            jobs.prepare(&vault, epoch)?;
                            Ok((false, None, 0, 0))
                        }
                        Work::Inventory => {
                            let inventory = vault.search_inventory(&|| jobs.cancelled(epoch))?;
                            Ok((
                                false,
                                Some(Plan {
                                    paths: inventory.paths.into(),
                                    remaining: READ_BUDGET,
                                    skipped: inventory.skipped_sources,
                                    failed: inventory.failed_sources,
                                }),
                                0,
                                0,
                            ))
                        }
                        Work::Dirty(paths) | Work::Batch(paths, _) => {
                            let budget = match &work {
                                Work::Batch(_, budget) => *budget,
                                _ => READ_BUDGET,
                            };
                            let updated = vault.sync_search_batch(
                                SearchUpdate::Paths(paths.clone()),
                                budget,
                                &|| jobs.cancelled(epoch),
                            )?;
                            jobs.dirty(epoch, updated.stale_paths.clone());
                            Ok((
                                updated.changed_sources + updated.removed_sources > 0,
                                None,
                                paths.len(),
                                updated.bytes_read,
                            ))
                        }
                        Work::Finish(skipped, failed) => {
                            vault.mark_search_reconciled(*skipped, *failed)?;
                            Ok((true, None, 0, 0))
                        }
                    }
                })();
                if jobs.cancelled(epoch) {
                    return;
                }
                let emit;
                {
                    let mut queue = jobs.0.queue.lock().unwrap_or_else(|p| p.into_inner());
                    if jobs.cancelled(epoch) {
                        return;
                    }
                    queue.active = false;
                    match result {
                        Ok((changed, plan, count, bytes)) => {
                            emit = changed || queue.error.is_some();
                            queue.error = None;
                            queue.retry_at = None;
                            if let Some(plan) = plan {
                                queue.plan = Some(plan);
                            }
                            if matches!(work, Work::Batch(..))
                                && let Some(plan) = queue.plan.as_mut()
                            {
                                for _ in 0..count {
                                    plan.paths.pop_front();
                                }
                                plan.remaining = plan.remaining.saturating_sub(bytes);
                            }
                            if matches!(work, Work::Finish(..)) {
                                queue.plan = None;
                                queue.last_full = Some(Instant::now());
                            }
                        }
                        Err(error) => {
                            if let Work::Dirty(paths) = &work {
                                queue.dirty.extend(paths.iter().cloned());
                            }
                            if matches!(work, Work::Inventory) {
                                queue.full = true;
                            }
                            let message = format!("Не удалось обновить локальный индекс: {error}");
                            emit = queue.error.as_ref() != Some(&message);
                            queue.error = Some(message);
                            queue.retry_at = Some(Instant::now() + Duration::from_secs(5));
                        }
                    }
                }
                if emit {
                    SearchState::changed(&app);
                }
            })
            .await;
        }
    });
}

#[tauri::command]
pub async fn reconcile_search_index(
    root: String,
    state: State<'_, VaultState>,
    jobs: State<'_, SearchState>,
) -> Result<(), String> {
    let vault = current_vault(&state)?;
    if vault.root().to_string_lossy() != root {
        return Err("Vault изменился; повторите сверку".into());
    }
    jobs.require_full(jobs.scope(&vault)?);
    Ok(())
}

#[tauri::command]
pub async fn rebuild_search_index(
    root: String,
    query: String,
    mode: Option<SearchMode>,
    limit: Option<usize>,
    state: State<'_, VaultState>,
    jobs: State<'_, SearchState>,
) -> Result<SearchRebuild, String> {
    let vault = current_vault(&state)?;
    if vault.root().to_string_lossy() != root {
        return Err("Vault изменился; повторите восстановление в нужной папке".into());
    }
    let epoch = jobs.scope(&vault)?;
    jobs.0.latest.fetch_add(1, Ordering::Relaxed);
    let jobs = jobs.inner().clone();
    let permit = Arc::clone(&jobs.0.worker)
        .acquire_owned()
        .await
        .map_err(|_| "Поиск остановлен")?;
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        if jobs.cancelled(epoch) {
            return Err("Vault изменился; восстановление отменено".into());
        }
        let result = vault
            .rebuild_search_cancellable(
                &query,
                mode.unwrap_or_default(),
                limit.unwrap_or(80),
                &|| jobs.cancelled(epoch),
            )
            .map_err(|error| format!("Не удалось восстановить локальный поиск: {error}"))?;
        if jobs.cancelled(epoch) {
            return Err("Vault изменился; восстановление отменено".into());
        }
        {
            let mut queue = jobs
                .0
                .queue
                .lock()
                .map_err(|_| "Очередь поиска недоступна")?;
            if jobs.cancelled(epoch) {
                return Err("Vault изменился; восстановление отменено".into());
            }
            queue.prepared = true;
            queue.plan = None;
            queue.error = None;
            queue.retry_at = None;
            queue.last_full = Some(Instant::now());
        }
        Ok(result)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn search_vault(
    root: Option<String>,
    query: String,
    mode: Option<SearchMode>,
    limit: Option<usize>,
    state: State<'_, VaultState>,
    jobs: State<'_, SearchState>,
) -> Result<SearchResponse, String> {
    let vault = current_vault(&state)?;
    if root.is_some_and(|root| root != vault.root().to_string_lossy()) {
        return Err("Vault изменился; повторите поиск".into());
    }
    let epoch = jobs.scope(&vault)?;
    let generation = jobs.0.latest.fetch_add(1, Ordering::Relaxed) + 1;
    let jobs = jobs.inner().clone();
    let permit = Arc::clone(&jobs.0.worker)
        .acquire_owned()
        .await
        .map_err(|_| "Поиск остановлен")?;
    if jobs.0.latest.load(Ordering::Relaxed) != generation {
        return Err("Запрос заменён новым".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        let started = Instant::now();
        let result = (|| -> Result<SearchResponse, VaultError> {
            jobs.prepare(&vault, epoch)?;
            let paths = {
                let mut queue = jobs.0.queue.lock().unwrap_or_else(|p| p.into_inner());
                if jobs.cancelled(epoch) {
                    return Err(VaultError::SearchCancelled);
                }
                let paths: Vec<_> = queue.dirty.iter().take(BATCH).cloned().collect();
                for path in &paths {
                    queue.dirty.remove(path);
                }
                paths
            };
            let mut synced = SearchResponse::default();
            if !paths.is_empty() {
                match vault.sync_search_batch(
                    SearchUpdate::Paths(paths.clone()),
                    READ_BUDGET,
                    &|| jobs.cancelled(epoch),
                ) {
                    Ok(updated) => {
                        jobs.dirty(epoch, updated.stale_paths.clone());
                        synced = updated;
                    }
                    Err(error) => {
                        if !jobs.cancelled(epoch) {
                            jobs.dirty(epoch, paths);
                        }
                        return Err(error);
                    }
                }
            }
            let mut response =
                vault.search_cached(&query, mode.unwrap_or_default(), limit.unwrap_or(80))?;
            if !response.stale_paths.is_empty() {
                let updated = vault.sync_search_batch(
                    SearchUpdate::Paths(response.stale_paths.iter().take(BATCH).cloned().collect()),
                    READ_BUDGET,
                    &|| jobs.cancelled(epoch),
                )?;
                jobs.dirty(epoch, updated.stale_paths.clone());
                synced.changed_sources += updated.changed_sources;
                synced.removed_sources += updated.removed_sources;
                synced.scanned_sources += updated.scanned_sources;
                response =
                    vault.search_cached(&query, mode.unwrap_or_default(), limit.unwrap_or(80))?;
                jobs.dirty(epoch, response.stale_paths.clone());
            }
            response.changed_sources = synced.changed_sources;
            response.removed_sources = synced.removed_sources;
            response.scanned_sources = synced.scanned_sources;
            Ok(response)
        })()
        .map_err(|error| format!("Не удалось выполнить локальный поиск: {error}"))?;
        let mut result = jobs.decorate(result, epoch)?;
        result.elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        Ok(result)
    })
    .await
    .map_err(|error| error.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn known_file_remove_any_does_not_request_a_whole_vault_inventory() {
        let root = tempfile::tempdir().unwrap();
        std::fs::write(root.path().join("A.md"), "# A\n").unwrap();
        let vault = Vault::open(root.path()).unwrap().incremental().unwrap();
        let state = SearchState::default();
        let epoch = state.activate(vault);
        state.0.queue.lock().unwrap().full = false;
        state.observe(
            epoch,
            &Ok(
                Event::new(EventKind::Remove(notify::event::RemoveKind::Any))
                    .add_path(root.path().canonicalize().unwrap().join("A.md")),
            ),
        );
        let queue = state.0.queue.lock().unwrap();
        assert!(!queue.full);
        assert!(queue.dirty.contains("A.md"));
    }
    fn state() -> (SearchState, u64) {
        let state = SearchState::default();
        let epoch = state.activate(Vault::open(std::env::temp_dir()).unwrap());
        state.0.queue.lock().unwrap().full = false;
        (state, epoch)
    }
    fn event(kind: EventKind, paths: &[&str]) -> notify::Result<Event> {
        let mut event = Event::new(kind);
        for path in paths {
            event = event.add_path(std::env::temp_dir().canonicalize().unwrap().join(path));
        }
        Ok(event)
    }
    #[test]
    fn old_watcher_epochs_cannot_invalidate_a_new_vault() {
        let (state, epoch) = state();
        state.observe(epoch - 1, &Err(notify::Error::generic("old watcher")));
        assert!(!state.0.queue.lock().unwrap().full);
        state.observe(epoch, &Err(notify::Error::generic("lost watcher events")));
        assert!(state.0.queue.lock().unwrap().full);
    }
    #[test]
    fn stale_commands_cannot_mutate_the_new_queue() {
        let (state, epoch) = state();
        state.require_full(epoch - 1);
        state.dirty(epoch - 1, vec!["old.md".into()]);
        assert!(!state.0.queue.lock().unwrap().full);
        assert!(state.0.queue.lock().unwrap().dirty.is_empty());
    }
    #[test]
    fn rename_pairs_keep_both_paths_but_incomplete_renames_require_inventory() {
        let (state, epoch) = state();
        state.observe(
            epoch,
            &event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &["Old", "New"],
            ),
        );
        assert_eq!(state.0.queue.lock().unwrap().dirty.len(), 2);
        assert!(!state.0.queue.lock().unwrap().full);
        state.observe(
            epoch,
            &event(
                EventKind::Modify(ModifyKind::Name(RenameMode::From)),
                &["Lost.md"],
            ),
        );
        assert!(state.0.queue.lock().unwrap().full);
    }
    #[test]
    fn derivative_writes_are_ignored_but_canonical_tasks_are_not() {
        let (state, epoch) = state();
        let kind = EventKind::Modify(ModifyKind::Data(notify::event::DataChange::Any));
        state.observe(
            epoch,
            &event(
                kind,
                &[
                    ".brandon/search-v1.sqlite3",
                    ".brandon/search-v1.sqlite3-journal",
                    ".brandon/search-recovery-Ab123c/search-v1.sqlite3",
                ],
            ),
        );
        assert!(state.0.queue.lock().unwrap().dirty.is_empty());
        state.observe(epoch, &event(kind, &[".brandon/tasks.sqlite3"]));
        assert_eq!(state.0.queue.lock().unwrap().dirty.len(), 1);
        state.observe(
            epoch,
            &event(
                EventKind::Modify(ModifyKind::Metadata(notify::event::MetadataKind::Any)),
                &[".brandon"],
            ),
        );
        assert_eq!(state.0.queue.lock().unwrap().dirty.len(), 1);
        assert!(!state.0.queue.lock().unwrap().full);
        state.observe(
            epoch,
            &event(EventKind::Modify(ModifyKind::Any), &[".brandon"]),
        );
        state.observe(
            epoch,
            &event(
                EventKind::Remove(notify::event::RemoveKind::Any),
                &[".brandon/search-v1.sqlite3-journal"],
            ),
        );
        assert!(!state.0.queue.lock().unwrap().full);
        assert_eq!(state.0.queue.lock().unwrap().dirty.len(), 1);
    }
    #[test]
    fn overflow_and_unknown_notifications_request_full_reconciliation() {
        let (state, epoch) = state();
        let paths: Vec<_> = (0..1025).map(|n| format!("Note-{n}.md")).collect();
        let paths: Vec<_> = paths.iter().map(String::as_str).collect();
        state.observe(
            epoch,
            &event(EventKind::Create(notify::event::CreateKind::File), &paths),
        );
        assert!(state.0.queue.lock().unwrap().full);
        assert!(state.0.queue.lock().unwrap().dirty.is_empty());
        state.0.queue.lock().unwrap().full = false;
        state.observe(epoch, &event(EventKind::Other, &[]));
        assert!(state.0.queue.lock().unwrap().full);
    }
    #[test]
    fn access_events_do_not_create_feedback_work() {
        let (state, epoch) = state();
        state.observe(
            epoch,
            &event(
                EventKind::Access(notify::event::AccessKind::Any),
                &["Note.md"],
            ),
        );
        assert!(!state.0.queue.lock().unwrap().full);
        assert!(state.0.queue.lock().unwrap().dirty.is_empty());
    }
    #[test]
    fn folder_events_use_background_inventory_not_query_time_subtree_reads() {
        let (state, epoch) = state();
        state.observe(
            epoch,
            &event(
                EventKind::Create(notify::event::CreateKind::Folder),
                &["LargeFolder"],
            ),
        );
        assert!(state.0.queue.lock().unwrap().full);
        assert!(state.0.queue.lock().unwrap().dirty.is_empty());
    }
}
