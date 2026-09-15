use knowledge_crypto::{
    KeyPurpose, KeyVersion, WorkspaceKey,
    stream::{AttachmentStream, STREAM_CHUNK_SIZE},
};
fn stream(workspace: u8, id: u8, size: u64) -> AttachmentStream {
    AttachmentStream::new(
        WorkspaceKey::from_bytes([91; 32])
            .derive(KeyPurpose::Attachment)
            .unwrap(),
        KeyVersion(1),
        &[workspace; 16],
        &[id; 16],
        size,
    )
}
#[test]
fn frames_bind_workspace_attachment_index_size_and_complete_stream() {
    let size = STREAM_CHUNK_SIZE as u64 + 3;
    let original = stream(1, 2, size);
    let bytes = vec![b'x'; STREAM_CHUNK_SIZE];
    let first = original.encrypt_chunk(0, &bytes).unwrap();
    let last = original.encrypt_chunk(1, b"end").unwrap();
    let footer = original.encrypt_footer().unwrap();
    assert_eq!(original.decrypt_chunk(0, &first).unwrap(), bytes);
    assert_eq!(original.decrypt_chunk(1, &last).unwrap(), b"end");
    assert!(original.verify_footer(&footer).is_ok());
    for different in [
        stream(3, 2, size),
        stream(1, 4, size),
        stream(1, 2, size + 1),
    ] {
        assert!(different.decrypt_chunk(0, &first).is_err());
        assert!(different.verify_footer(&footer).is_err());
    }
    assert!(original.decrypt_chunk(1, &first).is_err());
    assert!(original.decrypt_chunk(0, &last).is_err());
    let mut altered = first.clone();
    altered[20] ^= 1;
    assert!(original.decrypt_chunk(0, &altered).is_err());
    assert!(
        original
            .decrypt_chunk(0, &first[..first.len() - 1])
            .is_err()
    );
    assert!(
        original
            .decrypt_chunk(0, &[first, vec![0]].concat())
            .is_err()
    );
    assert!(original.verify_footer(&last).is_err());
    assert!(original.verify_footer(&footer[..footer.len() - 1]).is_err());
    let last_range = original.frame_range(1).unwrap();
    assert_eq!(last_range.start, 65584);
    assert_eq!(last_range.end, 65615);
    assert_eq!(original.footer_range(), 65615..65643);
    assert_eq!(original.ciphertext_size(), 65643);
}
#[test]
fn empty_stream_still_requires_an_authenticated_footer() {
    let original = stream(1, 2, 0);
    assert_eq!(original.chunk_count(), 0);
    assert!(original.encrypt_chunk(0, b"").is_err());
    assert_eq!(original.ciphertext_size(), 48);
    assert!(
        original
            .verify_footer(&original.encrypt_footer().unwrap())
            .is_ok()
    );
}
