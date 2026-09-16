use std::sync::Arc;

use ipnet::IpNet;
use knowledge_crypto::{MasterKey, PasswordService};
use knowledge_store::Store;
use knowledge_store::attachments::ObjectStore;
use thiserror::Error;

#[derive(Clone, Default)]
pub struct AppState {
    runtime: Option<Arc<RuntimeState>>,
}

pub(crate) struct RuntimeState {
    pub store: Store,
    pub master_key: MasterKey,
    pub dummy_password_hash: String,
    pub public_origin: String,
    pub trusted_proxy_cidrs: Vec<IpNet>,
    pub objects: Option<Arc<dyn ObjectStore>>,
    pub object_streams: Arc<tokio::sync::Semaphore>,
    pub attachment_retention_days: u32,
}

impl AppState {
    pub fn new(
        store: Store,
        master_key: MasterKey,
        public_origin: &str,
        trusted_proxy_cidrs: &str,
    ) -> Result<Self, AppStateError> {
        let public_origin = canonical_origin(public_origin)?;
        let trusted_proxy_cidrs = trusted_proxy_cidrs
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| {
                value
                    .parse()
                    .map_err(|_| AppStateError::InvalidTrustedProxy)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let dummy_password_hash = PasswordService::recommended()
            .hash("an intentionally invalid password")
            .expect("the operating system must provide randomness at startup");
        let object_stream_slots = store
            .pool()
            .options()
            .get_max_connections()
            .saturating_sub(3)
            .min(2) as usize;
        Ok(Self {
            runtime: Some(Arc::new(RuntimeState {
                store,
                master_key,
                dummy_password_hash,
                public_origin,
                trusted_proxy_cidrs,
                objects: None,
                object_streams: Arc::new(tokio::sync::Semaphore::new(object_stream_slots)),
                attachment_retention_days: 30,
            })),
        })
    }

    /// Explicit dependency injection; unconfigured uploads fail closed (503).
    pub fn with_object_store(mut self, objects: Arc<dyn ObjectStore>) -> Self {
        if let Some(runtime) = self.runtime.as_mut() {
            Arc::get_mut(runtime)
                .expect("configure before cloning application state")
                .objects = Some(objects);
        }
        self
    }
    pub fn with_attachment_retention(mut self, days: u32) -> Result<Self, AppStateError> {
        if !(1..=3650).contains(&days) {
            return Err(AppStateError::InvalidAttachmentRetention);
        }
        if let Some(runtime) = self.runtime.as_mut() {
            Arc::get_mut(runtime)
                .expect("configure before cloning application state")
                .attachment_retention_days = days;
        }
        Ok(self)
    }

    pub(crate) fn runtime(&self) -> Option<&RuntimeState> {
        self.runtime.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AppStateError {
    #[error("attachment retention must be between 1 and 3650 days")]
    InvalidAttachmentRetention,
    #[error("KNOWLEDGE_PUBLIC_ORIGIN must be an origin such as https://knowledge.example")]
    InvalidPublicOrigin,
    #[error("KNOWLEDGE_TRUSTED_PROXY_CIDRS must contain comma-separated IP networks")]
    InvalidTrustedProxy,
}

pub(crate) fn canonical_origin(value: &str) -> Result<String, AppStateError> {
    let uri: axum::http::Uri = value
        .parse()
        .map_err(|_| AppStateError::InvalidPublicOrigin)?;
    let scheme = uri.scheme_str().ok_or(AppStateError::InvalidPublicOrigin)?;
    let authority = uri.authority().ok_or(AppStateError::InvalidPublicOrigin)?;
    if uri
        .path_and_query()
        .is_some_and(|path| path.as_str() != "/")
    {
        return Err(AppStateError::InvalidPublicOrigin);
    }
    Ok(format!(
        "{}://{}",
        scheme.to_ascii_lowercase(),
        authority.as_str().to_ascii_lowercase()
    ))
}
