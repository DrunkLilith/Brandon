//! Runtime configuration loaded at the process boundary.

use std::{
    collections::HashMap,
    fmt, io,
    net::{AddrParseError, SocketAddr},
    path::Path,
};

const DEFAULT_LISTEN_ADDRESS: &str = "0.0.0.0:3000";

/// Rotation secrets use the same protected-file loader as the primary key.
/// Versions are non-secret metadata. Keep both keys configured during rewrap.
pub fn load_master_key() -> Result<knowledge_crypto::MasterKey, &'static str> {
    let encoded = load_required_secret("KNOWLEDGE_MASTER_KEY")
        .map_err(|_| "KNOWLEDGE_MASTER_KEY_FILE configuration is invalid")?;
    let current = knowledge_crypto::MasterKey::from_base64(&encoded)
        .map_err(|_| "master key configuration is invalid")?;
    let version = std::env::var("KNOWLEDGE_MASTER_KEY_VERSION")
        .unwrap_or_else(|_| "1".into())
        .parse()
        .map_err(|_| "master key version is invalid")?;
    let previous = if let Ok(old_version) = std::env::var("KNOWLEDGE_PREVIOUS_MASTER_KEY_VERSION") {
        let old_version = old_version
            .parse()
            .map_err(|_| "previous master key version is invalid")?;
        let old = load_required_secret("KNOWLEDGE_PREVIOUS_MASTER_KEY")
            .map_err(|_| "previous master key configuration is invalid")?;
        Some((
            old_version,
            knowledge_crypto::MasterKey::from_base64(&old)
                .map_err(|_| "previous master key configuration is invalid")?,
        ))
    } else {
        None
    };
    current
        .with_versions(version, previous)
        .map_err(|_| "master key versions are invalid")
}

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
pub enum S3ConfigError {
    #[error(transparent)]
    Secret(#[from] SecretConfigError),
    #[error("required S3 configuration is missing or invalid")]
    Invalid,
}
impl S3Config {
    pub fn from_env() -> Result<Self, S3ConfigError> {
        Self::from_sources(std::env::vars(), |path| std::fs::read_to_string(path))
    }
    pub fn from_pairs<I, K, V>(pairs: I) -> Result<Self, S3ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
    {
        Self::from_sources(pairs, |path| std::fs::read_to_string(path))
    }
    pub fn from_sources<I, K, V, F>(pairs: I, mut read_file: F) -> Result<Self, S3ConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
        F: FnMut(&Path) -> io::Result<String>,
    {
        let values = collect_pairs(pairs);
        let mut endpoint = None;
        let mut bucket = None;
        let mut region = None;
        let mut allow_http = false;
        for (key, value) in &values {
            match key.as_str() {
                "KNOWLEDGE_S3_ENDPOINT" => endpoint = Some(value.clone()),
                "KNOWLEDGE_S3_BUCKET" => bucket = Some(value.clone()),
                "KNOWLEDGE_S3_REGION" => region = Some(value.clone()),
                "KNOWLEDGE_S3_ALLOW_HTTP" => {
                    allow_http = match value.as_str() {
                        "true" => true,
                        "false" => false,
                        _ => return Err(S3ConfigError::Invalid),
                    }
                }
                _ => {}
            }
        }
        let access_key =
            load_required_secret_from_map("KNOWLEDGE_S3_ACCESS_KEY_ID", &values, &mut read_file)?;
        let secret_key = load_required_secret_from_map(
            "KNOWLEDGE_S3_SECRET_ACCESS_KEY",
            &values,
            &mut read_file,
        )?;
        let config = Self {
            endpoint: endpoint.ok_or(S3ConfigError::Invalid)?,
            bucket: bucket.ok_or(S3ConfigError::Invalid)?,
            region: region.ok_or(S3ConfigError::Invalid)?,
            access_key,
            secret_key,
            allow_http,
        };
        if config.endpoint.is_empty()
            || config.bucket.is_empty()
            || config.region.is_empty()
            || config.access_key.is_empty()
            || config.secret_key.is_empty()
        {
            return Err(S3ConfigError::Invalid);
        }
        let url: axum::http::Uri = config
            .endpoint
            .parse()
            .map_err(|_| S3ConfigError::Invalid)?;
        if url.authority().is_none()
            || !matches!(url.scheme_str(), Some("https"))
                && !(config.allow_http && url.scheme_str() == Some("http"))
        {
            return Err(S3ConfigError::Invalid);
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

#[derive(Debug, thiserror::Error, Eq, PartialEq)]
pub enum SecretConfigError {
    #[error("{0} or {0}_FILE must be set")]
    Missing(&'static str),
    #[error("{0} and {0}_FILE cannot both be set")]
    Conflicting(&'static str),
    #[error("{0} is empty")]
    Empty(String),
    #[error("{0} must contain exactly one line")]
    Multiline(String),
    #[error("{0} could not be read")]
    Unreadable(String),
}

pub fn load_required_secret_from<I, K, V, F>(
    name: &'static str,
    pairs: I,
    mut read_file: F,
) -> Result<zeroize::Zeroizing<String>, SecretConfigError>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
    F: FnMut(&Path) -> io::Result<String>,
{
    let values = collect_pairs(pairs);
    load_required_secret_from_map(name, &values, &mut read_file)
}

pub fn load_required_secret(
    name: &'static str,
) -> Result<zeroize::Zeroizing<String>, SecretConfigError> {
    load_required_secret_from(name, std::env::vars(), |path| std::fs::read_to_string(path))
}

fn load_required_secret_from_map<F>(
    name: &'static str,
    values: &HashMap<String, String>,
    read_file: &mut F,
) -> Result<zeroize::Zeroizing<String>, SecretConfigError>
where
    F: FnMut(&Path) -> io::Result<String>,
{
    let file_name = format!("{name}_FILE");
    let direct = values.get(name);
    let file = values.get(&file_name);
    if direct.is_some() && file.is_some() {
        return Err(SecretConfigError::Conflicting(name));
    }
    let (mut value, source, from_file) = match (direct, file) {
        (Some(value), None) => (value.clone(), name.to_owned(), false),
        (None, Some(path)) if !path.is_empty() => (
            read_file(Path::new(path))
                .map_err(|_| SecretConfigError::Unreadable(file_name.clone()))?,
            file_name,
            true,
        ),
        (None, Some(_)) => return Err(SecretConfigError::Empty(file_name)),
        (None, None) => return Err(SecretConfigError::Missing(name)),
        (Some(_), Some(_)) => unreachable!("conflict handled above"),
    };
    if from_file && value.ends_with('\n') {
        value.pop();
        if value.ends_with('\r') {
            value.pop();
        }
    }
    if value.is_empty() {
        return Err(SecretConfigError::Empty(source));
    }
    if value.contains(['\r', '\n', '\0']) {
        return Err(SecretConfigError::Multiline(source));
    }
    Ok(zeroize::Zeroizing::new(value))
}

#[derive(Debug, thiserror::Error)]
pub enum DatabaseConfigError {
    #[error(transparent)]
    Secret(#[from] SecretConfigError),
    #[error("{0} must be set")]
    Missing(&'static str),
    #[error("database configuration is invalid")]
    Invalid,
    #[error("DATABASE_URL cannot be combined with component database configuration")]
    Conflicting,
}

pub struct DatabaseConfig {
    url: zeroize::Zeroizing<String>,
}

impl fmt::Debug for DatabaseConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DatabaseConfig([REDACTED])")
    }
}

impl DatabaseConfig {
    pub fn from_env() -> Result<Self, DatabaseConfigError> {
        Self::from_sources(std::env::vars(), |path| std::fs::read_to_string(path))
    }

    pub fn from_sources<I, K, V, F>(pairs: I, mut read_file: F) -> Result<Self, DatabaseConfigError>
    where
        I: IntoIterator<Item = (K, V)>,
        K: AsRef<str>,
        V: AsRef<str>,
        F: FnMut(&Path) -> io::Result<String>,
    {
        let values = collect_pairs(pairs);
        let has_url =
            values.contains_key("DATABASE_URL") || values.contains_key("DATABASE_URL_FILE");
        let has_components = values
            .keys()
            .any(|key| key.starts_with("KNOWLEDGE_DATABASE_"));
        if has_url && has_components {
            return Err(DatabaseConfigError::Conflicting);
        }
        if has_url {
            return Ok(Self {
                url: load_required_secret_from_map("DATABASE_URL", &values, &mut read_file)?,
            });
        }
        let required = |name: &'static str| {
            values
                .get(name)
                .filter(|value| !value.is_empty())
                .cloned()
                .ok_or(DatabaseConfigError::Missing(name))
        };
        let host = required("KNOWLEDGE_DATABASE_HOST")?;
        let port = required("KNOWLEDGE_DATABASE_PORT")?
            .parse::<u16>()
            .map_err(|_| DatabaseConfigError::Invalid)?;
        let database = required("KNOWLEDGE_DATABASE_NAME")?;
        let user = required("KNOWLEDGE_DATABASE_USER")?;
        if host.contains(['/', '@', ' ', '\r', '\n']) {
            return Err(DatabaseConfigError::Invalid);
        }
        let password =
            load_required_secret_from_map("KNOWLEDGE_DATABASE_PASSWORD", &values, &mut read_file)?;
        let url = format!(
            "postgres://{}:{}@{}:{}/{}",
            percent_encode(&user),
            percent_encode(&password),
            host,
            port,
            percent_encode(&database)
        );
        Ok(Self {
            url: zeroize::Zeroizing::new(url),
        })
    }

    pub fn as_url(&self) -> &str {
        &self.url
    }
}

fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            use fmt::Write as _;
            write!(encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

fn collect_pairs<I, K, V>(pairs: I) -> HashMap<String, String>
where
    I: IntoIterator<Item = (K, V)>,
    K: AsRef<str>,
    V: AsRef<str>,
{
    pairs
        .into_iter()
        .map(|(key, value)| (key.as_ref().to_owned(), value.as_ref().to_owned()))
        .collect()
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
