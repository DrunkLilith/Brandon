//! One epoch-scoped metadata lifetime for every desktop projection.
use super::{VaultState, current_vault, is_search_cache_path, search_commands};
use knowledge_vault::{MetadataDelta, Vault};
use notify::{Event, EventKind, event::ModifyKind};
use std::{
    collections::BTreeSet,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Clone, Default)]
pub struct MetadataState(Arc<Mutex<Queue>>);
#[derive(Default)]
struct Queue {
    vault: Option<Vault>,
    epoch: u64,
    paths: BTreeSet<String>,
    reconcile: bool,
    last_reconcile: Option<Instant>,
}

impl MetadataState {
    pub fn activate(&self, vault: Vault, epoch: u64) {
        *self.0.lock().unwrap_or_else(|p| p.into_inner()) = Queue {
            vault: Some(vault),
            epoch,
            last_reconcile: Some(Instant::now()),
            ..Queue::default()
        };
    }
    fn current(&self, epoch: u64) -> bool {
        self.0.lock().is_ok_and(|queue| queue.epoch == epoch)
    }
    pub fn observe(&self, epoch: u64, result: &notify::Result<Event>) {
        let mut queue = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if queue.epoch != epoch {
            return;
        }
        let Some(root) = queue.vault.as_ref().map(|vault| vault.root().to_path_buf()) else {
            return;
        };
        let Ok(event) = result else {
            queue.reconcile = true;
            return;
        };
        if matches!(event.kind, EventKind::Access(_)) {
            return;
        }
        if event.need_rescan() || event.paths.is_empty() {
            queue.reconcile = true;
        }
        if search_commands::private_parent_metadata(event, &root) {
            return;
        }
        for path in &event.paths {
            if is_search_cache_path(path) {
                continue;
            }
            // Directory timestamp churn from an atomic file write does not
            // mean every sibling changed. Create/remove/rename still expand.
            if matches!(
                event.kind,
                EventKind::Modify(
                    ModifyKind::Any
                        | ModifyKind::Metadata(
                            notify::event::MetadataKind::Any
                                | notify::event::MetadataKind::AccessTime
                                | notify::event::MetadataKind::WriteTime
                        )
                )
            ) && path.is_dir()
            {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&root) else {
                queue.reconcile = true;
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            if relative.is_empty() {
                continue;
            }
            if relative.starts_with(".brandon/tasks.sqlite3") {
                queue.paths.insert(".brandon/tasks.sqlite3".into());
            } else if !relative.split('/').any(|part| part.starts_with('.')) {
                queue.paths.insert(relative);
            }
        }
        if matches!(event.kind, EventKind::Any | EventKind::Other) {
            queue.reconcile = true;
        }
        if queue.paths.len() > 2048 {
            queue.paths.clear();
            queue.reconcile = true;
        }
    }
}

pub fn start_worker(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let jobs = app.state::<MetadataState>().inner().clone();
        let mut last = (0, 0, usize::MAX, None);
        let mut emitted = Instant::now();
        let mut forwarded = (0, 1);
        loop {
            let selected = {
                let mut queue = jobs.0.lock().unwrap_or_else(|p| p.into_inner());
                if let Some(vault) = queue.vault.clone() {
                    let paths: Vec<_> = queue.paths.iter().take(128).cloned().collect();
                    for path in &paths {
                        queue.paths.remove(path);
                    }
                    let reconcile = queue.reconcile
                        || queue
                            .last_reconcile
                            .is_none_or(|at| at.elapsed() >= Duration::from_secs(60));
                    if reconcile {
                        queue.reconcile = false;
                        queue.last_reconcile = Some(Instant::now());
                    }
                    Some((vault, queue.epoch, paths, reconcile))
                } else {
                    None
                }
            };
            let Some((vault, epoch, paths, reconcile)) = selected else {
                tokio::time::sleep(Duration::from_millis(25)).await;
                continue;
            };
            let app = app.clone();
            let jobs = jobs.clone();
            let after = if forwarded.0 == epoch { forwarded.1 } else { 1 };
            let next = tauri::async_runtime::spawn_blocking(move || {
                if !jobs.current(epoch) {
                    return None;
                }
                vault.metadata_queue(&paths, reconcile);
                let result = vault.metadata_step(128);
                let generation = vault.snapshot_identity();
                let mut delta = vault
                    .metadata_delta(Some(generation.0), Some(generation.1))
                    .ok()?;
                if let Err(error) = result {
                    delta.error = Some(error.to_string());
                }
                let invalidations = vault.metadata_invalidations(after).ok()?;
                jobs.current(epoch).then_some((delta, epoch, invalidations))
            })
            .await
            .ok()
            .flatten();
            let Some((delta, epoch, (revision, reset, paths))) = next else {
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            };
            app.state::<super::SearchState>()
                .metadata_invalidated(epoch, paths, reset);
            forwarded = (epoch, revision);
            let key = (
                epoch,
                delta.revision,
                delta.stats.pending_sources,
                delta.error.clone(),
            );
            if key != last
                && (emitted.elapsed() >= Duration::from_millis(80)
                    || delta.stats.pending_sources == 0)
            {
                let _=app.emit("metadata-updated",serde_json::json!({"root":delta.root,"generation":delta.generation,"revision":delta.revision,"pendingSources":delta.stats.pending_sources,"error":delta.error}));
                last = key;
                emitted = Instant::now();
            }
            if delta.stats.pending_sources == 0 || delta.error.is_some() {
                tokio::time::sleep(Duration::from_millis(if delta.error.is_some() {
                    1000
                } else {
                    25
                }))
                .await;
            }
        }
    });
}

#[tauri::command]
pub async fn metadata_delta(
    root: String,
    generation: Option<u64>,
    after: Option<u64>,
    state: State<'_, VaultState>,
) -> Result<MetadataDelta, String> {
    let vault = current_vault(&state)?;
    if vault.root() != Path::new(&root) {
        return Err("Vault изменился; metadata запрос отменён".into());
    }
    let identity = vault.snapshot_identity();
    let delta = tauri::async_runtime::spawn_blocking(move || {
        vault
            .metadata_delta(generation, after)
            .map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    if current_vault(&state)?.snapshot_identity().0 != identity.0 {
        return Err("Metadata generation изменилась; запрос отменён".into());
    }
    Ok(delta)
}

#[tauri::command]
pub async fn metadata_status(
    root: String,
    state: State<'_, VaultState>,
) -> Result<MetadataDelta, String> {
    let vault = current_vault(&state)?;
    if vault.root() != Path::new(&root) {
        return Err("Vault изменился; metadata запрос отменён".into());
    }
    let identity = vault.snapshot_identity();
    let delta = tauri::async_runtime::spawn_blocking(move || {
        vault.metadata_status().map_err(|error| error.to_string())
    })
    .await
    .map_err(|error| error.to_string())??;
    if current_vault(&state)?.snapshot_identity().0 != identity.0 {
        return Err("Metadata generation изменилась; запрос отменён".into());
    }
    Ok(delta)
}

#[tauri::command]
pub fn reconcile_metadata(
    root: String,
    state: State<'_, VaultState>,
    jobs: State<'_, MetadataState>,
) -> Result<(), String> {
    if current_vault(&state)?.root() != Path::new(&root) {
        return Err("Vault изменился; сверка отменена".into());
    }
    jobs.0
        .lock()
        .map_err(|_| "Metadata очередь недоступна")?
        .reconcile = true;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_directory_permission_changes_are_not_filtered_as_timestamp_noise() {
        let (root, state, epoch) = fixture();
        state.observe(
            epoch,
            &Ok(Event::new(EventKind::Modify(ModifyKind::Metadata(
                notify::event::MetadataKind::Permissions,
            )))
            .add_path(root.path().canonicalize().unwrap().join("Folder"))),
        );
        let queue = state.0.lock().unwrap();
        assert!(queue.paths.contains("Folder"));
        assert!(!queue.reconcile);
    }
    fn fixture() -> (tempfile::TempDir, MetadataState, u64) {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("Folder")).unwrap();
        std::fs::write(root.path().join("Folder/A.md"), "# A\n").unwrap();
        let state = MetadataState::default();
        state.activate(Vault::open(root.path()).unwrap().incremental().unwrap(), 7);
        (root, state, 7)
    }
    #[test]
    fn metadata_epochs_and_overflow_are_explicit() {
        let (root, state, epoch) = fixture();
        state.observe(epoch - 1, &Err(notify::Error::generic("old")));
        assert!(!state.0.lock().unwrap().reconcile);
        state.observe(epoch, &Err(notify::Error::generic("lost events")));
        assert!(state.0.lock().unwrap().reconcile);
        let mut event = Event::new(EventKind::Modify(ModifyKind::Data(
            notify::event::DataChange::Any,
        )));
        for n in 0..2050 {
            event = event.add_path(root.path().canonicalize().unwrap().join(format!("N{n}.md")));
        }
        state.observe(epoch, &Ok(event));
        let queue = state.0.lock().unwrap();
        assert!(queue.reconcile);
        assert!(queue.paths.is_empty());
    }
    #[test]
    fn directory_timestamp_and_derivative_events_do_not_requeue_all_siblings() {
        let (root, state, epoch) = fixture();
        let root = root.path().canonicalize().unwrap();
        for event in [
            Event::new(EventKind::Modify(ModifyKind::Any)).add_path(root.join("Folder")),
            Event::new(EventKind::Modify(ModifyKind::Any))
                .add_path(root.join(".brandon/search-v1.sqlite3")),
            Event::new(EventKind::Access(notify::event::AccessKind::Any))
                .add_path(root.join("Folder/A.md")),
        ] {
            state.observe(epoch, &Ok(event));
        }
        let queue = state.0.lock().unwrap();
        assert!(queue.paths.is_empty());
        assert!(!queue.reconcile);
    }
    #[test]
    fn file_and_task_events_queue_only_their_relative_paths() {
        let (root, state, epoch) = fixture();
        let root = root.path().canonicalize().unwrap();
        state.observe(
            epoch,
            &Ok(Event::new(EventKind::Modify(ModifyKind::Data(
                notify::event::DataChange::Content,
            )))
            .add_path(root.join("Folder/A.md"))),
        );
        state.observe(
            epoch,
            &Ok(Event::new(EventKind::Modify(ModifyKind::Any))
                .add_path(root.join(".brandon/tasks.sqlite3-wal"))),
        );
        let queue = state.0.lock().unwrap();
        assert_eq!(queue.paths.len(), 2);
        assert!(queue.paths.contains("Folder/A.md"));
        assert!(queue.paths.contains(".brandon/tasks.sqlite3"));
        assert!(!queue.reconcile);
    }
}
