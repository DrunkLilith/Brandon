use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use knowledge_crypto::MasterKey;
use knowledge_store::Store;

const LOGIN_WINDOW: Duration = Duration::from_secs(15 * 60);
const MAX_LOGIN_FAILURES: u32 = 5;

#[derive(Clone)]
pub struct AppState {
    runtime: Option<Arc<RuntimeState>>,
    login_limiter: Arc<LoginLimiter>,
}

pub(crate) struct RuntimeState {
    pub store: Store,
    pub master_key: MasterKey,
}

impl AppState {
    #[must_use]
    pub fn new(store: Store, master_key: MasterKey) -> Self {
        Self {
            runtime: Some(Arc::new(RuntimeState { store, master_key })),
            login_limiter: Arc::new(LoginLimiter::default()),
        }
    }

    pub(crate) fn runtime(&self) -> Option<&RuntimeState> {
        self.runtime.as_deref()
    }

    pub(crate) fn login_is_limited(&self, key: &[u8; 32]) -> bool {
        self.login_limiter.is_limited(key)
    }

    pub(crate) fn record_login_failure(&self, key: [u8; 32]) {
        self.login_limiter.record_failure(key);
    }

    pub(crate) fn clear_login_failures(&self, key: &[u8; 32]) {
        self.login_limiter.clear(key);
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            runtime: None,
            login_limiter: Arc::new(LoginLimiter::default()),
        }
    }
}

#[derive(Default)]
struct LoginLimiter {
    failures: Mutex<HashMap<[u8; 32], FailureBucket>>,
}

#[derive(Clone, Copy)]
struct FailureBucket {
    count: u32,
    started_at: Instant,
}

impl LoginLimiter {
    fn is_limited(&self, key: &[u8; 32]) -> bool {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        match failures.get(key).copied() {
            Some(bucket) if bucket.started_at.elapsed() < LOGIN_WINDOW => {
                bucket.count >= MAX_LOGIN_FAILURES
            }
            Some(_) => {
                failures.remove(key);
                false
            }
            None => false,
        }
    }

    fn record_failure(&self, key: [u8; 32]) {
        let mut failures = self
            .failures
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let bucket = failures.entry(key).or_insert(FailureBucket {
            count: 0,
            started_at: Instant::now(),
        });
        if bucket.started_at.elapsed() >= LOGIN_WINDOW {
            *bucket = FailureBucket {
                count: 0,
                started_at: Instant::now(),
            };
        }
        bucket.count = bucket.count.saturating_add(1);
    }

    fn clear(&self, key: &[u8; 32]) {
        self.failures
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(key);
    }
}
