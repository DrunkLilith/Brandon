//! Runtime configuration loaded at the process boundary.

use std::net::{AddrParseError, SocketAddr};

const DEFAULT_LISTEN_ADDRESS: &str = "0.0.0.0:3000";

/// Explicit production storage credentials; intentionally has no Debug impl.
pub struct S3Config {
    endpoint: String,
    bucket: String,
    region: String,
    access_key: zeroize::Zeroizing<String>,
    secret_key: zeroize::Zeroizing<String>,
    allow_http: bool,
}
#[derive(Debug, thiserror::Error)]
#[error("required S3 configuration is missing or invalid")]
pub struct S3ConfigError;
impl S3Config {
    pub fn from_env() -> Result<Self, S3ConfigError> {
        Self::from_pairs(std::env::vars())
    }
    pub fn from_pairs<I, K, V>(pairs: I) -> Result<Self, S3ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let mut endpoint = None;
        let mut bucket = None;
        let mut region = None;
        let mut access_key = None;
        let mut secret_key = None;
        let mut allow_http = false;
        for (key, value) in pairs {
            match key.as_ref() {
                "KNOWLEDGE_S3_ENDPOINT" => endpoint = Some(value.as_ref().to_owned()),
                "KNOWLEDGE_S3_BUCKET" => bucket = Some(value.as_ref().to_owned()),
                "KNOWLEDGE_S3_REGION" => region = Some(value.as_ref().to_owned()),
                "KNOWLEDGE_S3_ACCESS_KEY_ID" => {
                    access_key = Some(zeroize::Zeroizing::new(value.as_ref().to_owned()))
                }
                "KNOWLEDGE_S3_SECRET_ACCESS_KEY" => {
                    secret_key = Some(zeroize::Zeroizing::new(value.as_ref().to_owned()))
                }
                "KNOWLEDGE_S3_ALLOW_HTTP" => {
                    allow_http = match value.as_ref() {
                        "true" => true,
                        "false" => false,
                        _ => return Err(S3ConfigError),
                    }
                }
                _ => {}
            }
        }
        let config = Self {
            endpoint: endpoint.ok_or(S3ConfigError)?,
            bucket: bucket.ok_or(S3ConfigError)?,
            region: region.ok_or(S3ConfigError)?,
            access_key: access_key.ok_or(S3ConfigError)?,
            secret_key: secret_key.ok_or(S3ConfigError)?,
            allow_http,
        };
        if config.endpoint.is_empty()
            || config.bucket.is_empty()
            || config.region.is_empty()
            || config.access_key.is_empty()
            || config.secret_key.is_empty()
        {
            return Err(S3ConfigError);
        }
        let url: axum::http::Uri = config.endpoint.parse().map_err(|_| S3ConfigError)?;
        if url.authority().is_none()
            || !matches!(url.scheme_str(), Some("https"))
                && !(config.allow_http && url.scheme_str() == Some("http"))
        {
            return Err(S3ConfigError);
        }
        Ok(config)
    }
    pub fn build(
        self,
    ) -> Result<
        knowledge_store::attachments::S3ObjectStore,
        knowledge_store::attachments::StorageError,
    > {
        knowledge_store::attachments::S3ObjectStore::new(
            &self.endpoint,
            &self.bucket,
            &self.region,
            &self.access_key,
            &self.secret_key,
            self.allow_http,
        )
    }
}

/// Network settings required to start the HTTP process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServerConfig {
    pub listen_address: SocketAddr,
}

impl ServerConfig {
    /// Loads configuration from the current process environment.
    pub fn from_env() -> Result<Self, AddrParseError> {
        Self::from_pairs(std::env::vars())
    }

    /// Loads configuration from key/value pairs, allowing deterministic tests.
    pub fn from_pairs<I, K, V>(pairs: I) -> Result<Self, AddrParseError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        let listen_address = match pairs
            .into_iter()
            .find(|(key, _)| key.as_ref() == "KNOWLEDGE_LISTEN_ADDRESS")
        {
            Some((_, value)) => value.as_ref().parse()?,
            None => DEFAULT_LISTEN_ADDRESS.parse()?,
        };

        Ok(Self { listen_address })
    }
}
