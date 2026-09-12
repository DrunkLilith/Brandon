use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, Generate, KeyInit, Payload},
};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::{CryptoError, PurposeKey};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct KeyVersion(pub u32);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct EncryptedEnvelope {
    pub algorithm: String,
    pub key_version: KeyVersion,
    pub nonce: [u8; 12],
    pub ciphertext: Vec<u8>,
}

#[derive(Zeroize, ZeroizeOnDrop)]
pub struct DecryptedBytes(Vec<u8>);

impl DecryptedBytes {
    #[must_use]
    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

pub struct EnvelopeCipher {
    key_version: KeyVersion,
    key: PurposeKey,
}

impl EnvelopeCipher {
    #[must_use]
    pub const fn new(key_version: KeyVersion, key: PurposeKey) -> Self {
        Self { key_version, key }
    }

    pub fn encrypt(&self, aad: &[u8], plaintext: &[u8]) -> Result<EncryptedEnvelope, CryptoError> {
        let cipher = Aes256Gcm::new_from_slice(self.key.expose())
            .map_err(|_| CryptoError::EncryptionFailed)?;
        let nonce = Nonce::generate();
        let ciphertext = cipher
            .encrypt(
                &nonce,
                Payload {
                    msg: plaintext,
                    aad,
                },
            )
            .map_err(|_| CryptoError::EncryptionFailed)?;
        let nonce_bytes: [u8; 12] = nonce.into();

        Ok(EncryptedEnvelope {
            algorithm: "AES-256-GCM".to_owned(),
            key_version: self.key_version,
            nonce: nonce_bytes,
            ciphertext,
        })
    }

    pub fn decrypt(
        &self,
        aad: &[u8],
        envelope: &EncryptedEnvelope,
    ) -> Result<DecryptedBytes, CryptoError> {
        if envelope.algorithm != "AES-256-GCM" {
            return Err(CryptoError::InvalidEnvelope);
        }
        if envelope.key_version != self.key_version {
            return Err(CryptoError::UnsupportedKeyVersion);
        }

        let cipher = Aes256Gcm::new_from_slice(self.key.expose())
            .map_err(|_| CryptoError::DecryptionFailed)?;
        let nonce = Nonce::from(envelope.nonce);
        let plaintext = cipher
            .decrypt(
                &nonce,
                Payload {
                    msg: &envelope.ciphertext,
                    aad,
                },
            )
            .map_err(|_| CryptoError::DecryptionFailed)?;

        Ok(DecryptedBytes(plaintext))
    }
}
