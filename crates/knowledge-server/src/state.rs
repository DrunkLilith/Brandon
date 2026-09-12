use std::sync::Arc;

use ipnet::IpNet;
use knowledge_crypto::{MasterKey, PasswordService};
use knowledge_store::Store;
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
        Ok(Self {
            runtime: Some(Arc::new(RuntimeState {
                store,
                master_key,
                dummy_password_hash,
                public_origin,
                trusted_proxy_cidrs,
            })),
        })
    }

    pub(crate) fn runtime(&self) -> Option<&RuntimeState> {
        self.runtime.as_deref()
    }
}

#[derive(Clone, Copy, Debug, Eq, Error, PartialEq)]
pub enum AppStateError {
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
