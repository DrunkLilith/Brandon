use base64::{Engine as _, engine::general_purpose::STANDARD};
use knowledge_crypto::{EnvelopeCipher, KeyPurpose, KeyVersion, Keyring, MasterKey, WorkspaceKey};

fn master_key(byte: u8) -> MasterKey {
    MasterKey::from_base64(&STANDARD.encode([byte; 32])).expect("fixture key should decode")
}

fn content_cipher(byte: u8, version: u32) -> EnvelopeCipher {
    let workspace_key = WorkspaceKey::from_bytes([byte; 32]);
    let purpose_key = workspace_key
        .derive(KeyPurpose::Content)
        .expect("fixture key should derive");
    EnvelopeCipher::new(KeyVersion(version), purpose_key)
}

#[test]
fn ciphertext_round_trips_only_with_matching_aad() {
    let cipher = content_cipher(7, 1);
    let encrypted = cipher
        .encrypt(b"workspace:1/page:123", b"secret markdown")
        .expect("encryption should succeed");

    assert_ne!(encrypted.ciphertext, b"secret markdown");
    assert_eq!(
        cipher
            .decrypt(b"workspace:1/page:123", &encrypted)
            .expect("matching AAD should decrypt")
            .expose(),
        b"secret markdown"
    );
    assert!(cipher.decrypt(b"workspace:1/page:124", &encrypted).is_err());
}

#[test]
fn each_encryption_uses_a_distinct_nonce() {
    let cipher = content_cipher(9, 1);
    let first = cipher.encrypt(b"page:1", b"same").expect("encrypt first");
    let second = cipher.encrypt(b"page:1", b"same").expect("encrypt second");

    assert_ne!(first.nonce, second.nonce);
    assert_ne!(first.ciphertext, second.ciphertext);
}

#[test]
fn ciphertext_tampering_is_rejected() {
    let cipher = content_cipher(11, 3);
    let encrypted = cipher.encrypt(b"page:1", b"body").expect("encrypt");
    let mut json = serde_json::to_value(encrypted).expect("serialize envelope");
    let bytes = json["ciphertext"]
        .as_array_mut()
        .expect("ciphertext should serialize as bytes");
    bytes[0] = serde_json::json!(bytes[0].as_u64().expect("byte") ^ 1);
    let tampered = serde_json::from_value(json).expect("deserialize tampered envelope");

    assert!(cipher.decrypt(b"page:1", &tampered).is_err());
}

#[test]
fn key_version_mismatch_is_rejected_before_decryption() {
    let first = content_cipher(4, 1);
    let second = content_cipher(4, 2);
    let encrypted = first.encrypt(b"page:1", b"body").expect("encrypt");

    assert!(second.decrypt(b"page:1", &encrypted).is_err());
}

#[test]
fn purpose_keys_are_cryptographically_separated() {
    let workspace_key = WorkspaceKey::from_bytes([17; 32]);
    let content = workspace_key
        .derive(KeyPurpose::Content)
        .expect("derive content");
    let search = workspace_key
        .derive(KeyPurpose::Search)
        .expect("derive search");
    let attachment = workspace_key
        .derive(KeyPurpose::Attachment)
        .expect("derive attachment");

    assert_ne!(content.expose(), search.expose());
    assert_ne!(content.expose(), attachment.expose());
    assert_ne!(search.expose(), attachment.expose());
}

#[test]
fn rotating_master_key_preserves_workspace_key() {
    let old_master = master_key(1);
    let new_master = master_key(2);
    let wrapped = Keyring::new_workspace(&old_master, KeyVersion(5)).expect("wrap workspace key");
    let original = Keyring::unwrap(&old_master, &wrapped).expect("old key should unwrap");
    let rotated = Keyring::rotate_master(&old_master, &new_master, &wrapped)
        .expect("master rotation should succeed");
    let recovered = Keyring::unwrap(&new_master, &rotated).expect("new key should unwrap");

    assert_eq!(original.expose(), recovered.expose());
    assert!(Keyring::unwrap(&old_master, &rotated).is_err());
    assert_eq!(rotated.key_version, KeyVersion(5));
}

#[test]
fn malformed_master_key_is_rejected() {
    assert!(MasterKey::from_base64("not-base64").is_err());
    assert!(MasterKey::from_base64(&STANDARD.encode([0_u8; 31])).is_err());
}

#[test]
fn purpose_key_authentication_matches_hmac_sha256_vector() {
    let workspace_key = WorkspaceKey::from_bytes([42; 32]);
    let search_key = workspace_key
        .derive(KeyPurpose::Search)
        .expect("derive search key");

    let digest = search_key.authenticate(b"user@example.com");

    assert_eq!(
        hex::encode(digest),
        "f0d36ff906725c6413e578a12afbca2b08b1ceff95e2079614edd8ebd9db0d14"
    );
}
