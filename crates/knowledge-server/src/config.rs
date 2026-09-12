//! Runtime configuration loaded at the process boundary.

use std::net::{AddrParseError, SocketAddr};

const DEFAULT_LISTEN_ADDRESS: &str = "0.0.0.0:3000";

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
