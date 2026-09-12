//! Cryptographic primitives for encrypted knowledge storage.

mod envelope;
mod keys;
mod passwords;

pub use envelope::{DecryptedBytes, EncryptedEnvelope, EnvelopeCipher, KeyVersion};
pub use keys::{KeyPurpose, Keyring, MasterKey, PurposeKey, WorkspaceKey, WrappedWorkspaceKey};
pub use passwords::PasswordService;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptoError {
    #[error("master key must be exactly 32 bytes encoded as base64")]
    InvalidMasterKey,
    #[error("encrypted envelope uses an unsupported key version")]
    UnsupportedKeyVersion,
    #[error("encrypted envelope is malformed")]
    InvalidEnvelope,
    #[error("authenticated encryption failed")]
    EncryptionFailed,
    #[error("authenticated decryption failed")]
    DecryptionFailed,
    #[error("secure random number generation failed")]
    RandomnessFailed,
    #[error("key derivation failed")]
    KeyDerivationFailed,
    #[error("password hashing failed: {0}")]
    PasswordHash(#[from] argon2::password_hash::Error),
}
