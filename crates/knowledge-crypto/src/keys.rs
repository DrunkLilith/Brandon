use base64::{Engine as _, engine::general_purpose::STANDARD};
use hkdf::Hkdf;
use hmac::{Hmac, KeyInit, Mac};
use sha2::Sha256;
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

use crate::{CryptoError, EncryptedEnvelope, EnvelopeCipher, KeyVersion};

const WORKSPACE_KEY_AAD: &[u8] = b"knowledge-platform/workspace-key/v1";

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct MasterKey {
    bytes: [u8; 32],
    version: u32,
    previous: Option<(u32, [u8; 32])>,
}

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
        Ok(Self {
            bytes,
            version: 1,
            previous: None,
        })
    }

    pub(crate) const fn expose(&self) -> &[u8; 32] {
        &self.bytes
    }
    pub const fn version(&self) -> KeyVersion {
        KeyVersion(self.version)
    }
    pub fn with_versions(
        mut self,
        current: u32,
        previous: Option<(u32, MasterKey)>,
    ) -> Result<Self, CryptoError> {
        if current == 0
            || previous
                .as_ref()
                .is_some_and(|(version, _)| *version == 0 || *version == current)
        {
            return Err(CryptoError::UnsupportedKeyVersion);
        }
        self.version = current;
        self.previous = previous.map(|(version, key)| (version, key.bytes));
        Ok(self)
    }
    fn for_version(&self, version: KeyVersion) -> Result<&[u8; 32], CryptoError> {
        if let Some((old, bytes)) = &self.previous {
            if version.0 == *old {
                return Ok(bytes);
            }
            if version.0 != self.version {
                return Err(CryptoError::UnsupportedKeyVersion);
            }
        }
        Ok(&self.bytes)
    }
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct WorkspaceKey {
    bytes: [u8; 32],
    version: u32,
    previous: Vec<(u32, [u8; 32])>,
}

impl WorkspaceKey {
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self {
            bytes,
            version: 1,
            previous: Vec::new(),
        }
    }

    pub fn generate() -> Result<Self, CryptoError> {
        let mut bytes = [0_u8; 32];
        getrandom::fill(&mut bytes).map_err(|_| CryptoError::RandomnessFailed)?;
        Ok(Self::from_bytes(bytes))
    }

    #[must_use]
    pub fn expose(&self) -> &[u8; 32] {
        &self.bytes
    }

    pub fn derive(&self, purpose: KeyPurpose) -> Result<PurposeKey, CryptoError> {
        self.derive_for(purpose, self.version())
    }
    pub const fn version(&self) -> KeyVersion {
        KeyVersion(self.version)
    }
    pub fn rotated(&self) -> Result<Self, CryptoError> {
        let version = self
            .version
            .checked_add(1)
            .filter(|v| *v <= i32::MAX as u32)
            .ok_or(CryptoError::UnsupportedKeyVersion)?;
        let mut previous = self.previous.clone();
        previous.push((self.version, self.bytes));
        Self::generate()?.with_generation(version, previous)
    }
    pub fn with_generation(
        mut self,
        version: u32,
        previous: Vec<(u32, [u8; 32])>,
    ) -> Result<Self, CryptoError> {
        if version == 0 || previous.iter().any(|(v, _)| *v == 0 || *v == version) {
            return Err(CryptoError::UnsupportedKeyVersion);
        }
        self.version = version;
        self.previous = previous;
        Ok(self)
    }
    /// Stable identity/replay HMACs are not the rotated search index. Generation
    /// one is retained encrypted for these identities and immutable audit reads.
    pub fn derive_identity(&self) -> Result<PurposeKey, CryptoError> {
        self.derive_for(KeyPurpose::Search, KeyVersion(1))
    }
    pub fn derive_for(
        &self,
        purpose: KeyPurpose,
        version: KeyVersion,
    ) -> Result<PurposeKey, CryptoError> {
        let bytes = if version.0 == self.version {
            &self.bytes
        } else {
            &self
                .previous
                .iter()
                .find(|(v, _)| *v == version.0)
                .ok_or(CryptoError::UnsupportedKeyVersion)?
                .1
        };
        let hkdf = Hkdf::<Sha256>::new(None, bytes);
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
            PurposeKey::from_bytes(*master_key.for_version(wrapped.key_version)?),
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

    pub fn wrap(
        master_key: &MasterKey,
        workspace_key: &WorkspaceKey,
        key_version: KeyVersion,
    ) -> Result<WrappedWorkspaceKey, CryptoError> {
        let cipher = EnvelopeCipher::new(key_version, PurposeKey::from_bytes(*master_key.expose()));
        cipher.encrypt(WORKSPACE_KEY_AAD, workspace_key.expose())
    }
}
