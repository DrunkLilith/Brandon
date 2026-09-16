//! Fixed-size independently authenticated frames, with an authenticated end boundary.
//! Consumers must check object length and verify the footer before exposing any bytes.
use crate::{CryptoError, EncryptedEnvelope, EnvelopeCipher, KeyVersion, PurposeKey};

pub const STREAM_CHUNK_SIZE: usize = 64 * 1024;
const FRAME_OVERHEAD: u64 = 28;
const HEADER_SIZE: usize = 20;

pub struct AttachmentStream {
    cipher: EnvelopeCipher,
    identity: Vec<u8>,
    header: [u8; HEADER_SIZE],
    size: u64,
}

impl AttachmentStream {
    pub fn new(
        key: PurposeKey,
        version: KeyVersion,
        workspace: &[u8; 16],
        attachment: &[u8; 16],
        size: u64,
    ) -> Self {
        Self::new_with_domain(
            key,
            version,
            workspace,
            attachment,
            size,
            b"knowledge/attachment-stream/v1",
        )
    }

    /// Uses the attachment stream framing with a separately authenticated
    /// export identity. The wire format and bounded frame sizes stay shared.
    pub fn new_export(
        key: PurposeKey,
        version: KeyVersion,
        workspace: &[u8; 16],
        export: &[u8; 16],
        size: u64,
    ) -> Self {
        Self::new_with_domain(
            key,
            version,
            workspace,
            export,
            size,
            b"knowledge/export-stream/v1",
        )
    }

    fn new_with_domain(
        key: PurposeKey,
        version: KeyVersion,
        workspace: &[u8; 16],
        entity: &[u8; 16],
        size: u64,
        domain: &[u8],
    ) -> Self {
        let mut header = [0; HEADER_SIZE];
        header[..8].copy_from_slice(b"KPAEAD01");
        header[8..12].copy_from_slice(&(STREAM_CHUNK_SIZE as u32).to_be_bytes());
        header[12..].copy_from_slice(&size.to_be_bytes());
        let mut identity = domain.to_vec();
        identity.extend(workspace);
        identity.extend(entity);
        identity.extend(version.0.to_be_bytes());
        Self {
            cipher: EnvelopeCipher::new(version, key),
            identity,
            header,
            size,
        }
    }
    pub fn header(&self) -> &[u8] {
        &self.header
    }
    pub fn chunk_count(&self) -> u64 {
        self.size.div_ceil(STREAM_CHUNK_SIZE as u64)
    }
    pub fn ciphertext_size(&self) -> u64 {
        HEADER_SIZE as u64 + self.size + (self.chunk_count() + 1) * FRAME_OVERHEAD
    }
    pub fn frame_range(&self, index: u64) -> Result<std::ops::Range<u64>, CryptoError> {
        let len = self.chunk_len(index)? as u64;
        let start = HEADER_SIZE as u64 + index * (STREAM_CHUNK_SIZE as u64 + FRAME_OVERHEAD);
        Ok(start..start + len + FRAME_OVERHEAD)
    }
    pub fn footer_range(&self) -> std::ops::Range<u64> {
        self.ciphertext_size() - FRAME_OVERHEAD..self.ciphertext_size()
    }
    fn chunk_len(&self, index: u64) -> Result<usize, CryptoError> {
        if index >= self.chunk_count() {
            return Err(CryptoError::InvalidEnvelope);
        }
        Ok((self.size - index * STREAM_CHUNK_SIZE as u64).min(STREAM_CHUNK_SIZE as u64) as usize)
    }
    fn aad(&self, index: u64, footer: bool) -> Vec<u8> {
        let mut aad = self.identity.clone();
        aad.extend(self.header);
        aad.extend(self.chunk_count().to_be_bytes());
        aad.extend(index.to_be_bytes());
        aad.push(u8::from(footer));
        aad
    }
    fn encrypt(&self, index: u64, footer: bool, bytes: &[u8]) -> Result<Vec<u8>, CryptoError> {
        let envelope = self.cipher.encrypt(&self.aad(index, footer), bytes)?;
        let mut frame = envelope.nonce.to_vec();
        frame.extend(envelope.ciphertext);
        Ok(frame)
    }
    pub fn encrypt_chunk(&self, index: u64, bytes: &[u8]) -> Result<Vec<u8>, CryptoError> {
        if bytes.len() != self.chunk_len(index)? {
            return Err(CryptoError::InvalidEnvelope);
        }
        self.encrypt(index, false, bytes)
    }
    pub fn encrypt_footer(&self) -> Result<Vec<u8>, CryptoError> {
        self.encrypt(self.chunk_count(), true, &[])
    }
    fn decrypt(
        &self,
        index: u64,
        footer: bool,
        frame: &[u8],
        len: usize,
    ) -> Result<Vec<u8>, CryptoError> {
        if frame.len() != len + FRAME_OVERHEAD as usize {
            return Err(CryptoError::InvalidEnvelope);
        }
        let envelope = EncryptedEnvelope {
            algorithm: "AES-256-GCM".into(),
            key_version: self.version(),
            nonce: frame[..12]
                .try_into()
                .map_err(|_| CryptoError::InvalidEnvelope)?,
            ciphertext: frame[12..].to_vec(),
        };
        Ok(self
            .cipher
            .decrypt(&self.aad(index, footer), &envelope)?
            .expose()
            .to_vec())
    }
    fn version(&self) -> KeyVersion {
        KeyVersion(u32::from_be_bytes(
            self.identity[self.identity.len() - 4..]
                .try_into()
                .expect("fixed version bytes"),
        ))
    }
    pub fn decrypt_chunk(&self, index: u64, frame: &[u8]) -> Result<Vec<u8>, CryptoError> {
        self.decrypt(index, false, frame, self.chunk_len(index)?)
    }
    pub fn verify_footer(&self, frame: &[u8]) -> Result<(), CryptoError> {
        self.decrypt(self.chunk_count(), true, frame, 0).map(|_| ())
    }
}
