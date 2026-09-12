use base64::{Engine as _, engine::general_purpose::STANDARD};
use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{CryptoError, EncryptedEnvelope, EnvelopeCipher, KeyVersion};

const WORKSPACE_KEY_AAD: &[u8] = b"knowledge-platform/workspace-key/v1";

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey([u8; 32]);

impl MasterKey {
    pub fn from_base64(encoded: &str) -> Result<Self, CryptoError> {
        let decoded = Zeroizing::new(
            STANDARD
                .decode(encoded)
                .map_err(|_| CryptoError::InvalidMasterKey)?,
        );
        let bytes = decoded
            .as_slice()
            .try_into()
            .map_err(|_| CryptoError::InvalidMasterKey)?;
        Ok(Self(bytes))
    }

    pub(crate) const fn expose(&self) -> &[u8; 32] {
        &self.0
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct WorkspaceKey([u8; 32]);

impl WorkspaceKey {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn generate() -> Result<Self, CryptoError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| CryptoError::RandomnessFailed)?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    pub fn derive(&self, purpose: KeyPurpose) -> Result<PurposeKey, CryptoError> {
        let hkdf = Hkdf::<Sha256>::new(None, &self.0);
        let mut output = [0_u8; 32];
        hkdf.expand(purpose.info(), &mut output)
            .map_err(|_| CryptoError::KeyDerivationFailed)?;
        Ok(PurposeKey(output))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeyPurpose {
    Content,
    Search,
    Attachment,
}

impl KeyPurpose {
    const fn info(self) -> &'static [u8] {
        match self {
            Self::Content => b"knowledge-platform/content/v1",
            Self::Search => b"knowledge-platform/search/v1",
            Self::Attachment => b"knowledge-platform/attachment/v1",
        }
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct PurposeKey([u8; 32]);

impl PurposeKey {
    #[must_use]
    pub fn expose(&self) -> &[u8; 32] {
        &self.0
    }

    #[must_use]
    pub fn authenticate(&self, value: &[u8]) -> [u8; 32] {
        let mut mac =
            Hmac::<Sha256>::new_from_slice(&self.0).expect("HMAC-SHA-256 accepts a 32-byte key");
        mac.update(value);
        mac.finalize().into_bytes().into()
    }

    const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

pub type WrappedWorkspaceKey = EncryptedEnvelope;

pub struct Keyring;

impl Keyring {
    pub fn new_workspace(
        master_key: &MasterKey,
        key_version: KeyVersion,
    ) -> Result<WrappedWorkspaceKey, CryptoError> {
        let workspace_key = WorkspaceKey::generate()?;
        Self::wrap(master_key, &workspace_key, key_version)
    }

    pub fn unwrap(
        master_key: &MasterKey,
        wrapped: &WrappedWorkspaceKey,
    ) -> Result<WorkspaceKey, CryptoError> {
        let cipher = EnvelopeCipher::new(
            wrapped.key_version,
            PurposeKey::from_bytes(*master_key.expose()),
        );
        let decrypted = cipher.decrypt(WORKSPACE_KEY_AAD, wrapped)?;
        let bytes = decrypted
            .expose()
            .try_into()
            .map_err(|_| CryptoError::InvalidEnvelope)?;
        Ok(WorkspaceKey::from_bytes(bytes))
    }

    pub fn rotate_master(
        old_master_key: &MasterKey,
        new_master_key: &MasterKey,
        wrapped: &WrappedWorkspaceKey,
    ) -> Result<WrappedWorkspaceKey, CryptoError> {
        let workspace_key = Self::unwrap(old_master_key, wrapped)?;
        Self::wrap(new_master_key, &workspace_key, wrapped.key_version)
    }

    fn wrap(
        master_key: &MasterKey,
        workspace_key: &WorkspaceKey,
        key_version: KeyVersion,
    ) -> Result<WrappedWorkspaceKey, CryptoError> {
        let cipher = EnvelopeCipher::new(key_version, PurposeKey::from_bytes(*master_key.expose()));
        cipher.encrypt(WORKSPACE_KEY_AAD, workspace_key.expose())
    }
}
