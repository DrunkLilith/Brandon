use knowledge_crypto::{EnvelopeCipher, KeyPurpose, KeyVersion, WorkspaceKey};

// Fixed seed/case bound: reproducible adversarial corpus, never secret output.
#[test]
fn bounded_properties_envelopes_reject_all_mutated_dimensions() {
    let mut seed = 0x51eed_u64;
    for case in 0..128 {
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let key = WorkspaceKey::from_bytes(std::array::from_fn(|_| next() as u8));
        let data: Vec<u8> = (0..next() % 1024).map(|_| next() as u8).collect();
        let aad = next().to_le_bytes();
        let cipher = EnvelopeCipher::new(KeyVersion(1), key.derive(KeyPurpose::Content).unwrap());
        let encrypted = cipher.encrypt(&aad, &data).unwrap();
        assert_eq!(
            cipher.decrypt(&aad, &encrypted).unwrap().expose(),
            data,
            "case {case}"
        );
        for dimension in 0..5 {
            let mut bad = encrypted.clone();
            let mut context = aad;
            match dimension {
                0 => bad.nonce[0] ^= 1,
                1 => bad.ciphertext[0] ^= 1,
                2 => {
                    bad.ciphertext.pop();
                }
                3 => bad.key_version = KeyVersion(2),
                _ => context[0] ^= 1,
            }
            assert!(
                cipher.decrypt(&context, &bad).is_err(),
                "case {case} dimension {dimension}"
            );
        }
    }
}
