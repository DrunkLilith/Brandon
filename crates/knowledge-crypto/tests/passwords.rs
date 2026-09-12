use knowledge_crypto::PasswordService;

#[test]
fn argon2id_hash_verifies_only_the_original_password() {
    let passwords = PasswordService::recommended();
    let encoded = passwords
        .hash("correct horse battery staple")
        .expect("password should hash");

    assert!(encoded.starts_with("$argon2id$"));
    assert!(
        passwords
            .verify("correct horse battery staple", &encoded)
            .expect("valid hash should parse")
    );
    assert!(
        !passwords
            .verify("wrong password", &encoded)
            .expect("valid hash should parse")
    );
}

#[test]
fn malformed_password_hash_is_an_error() {
    let passwords = PasswordService::recommended();

    assert!(passwords.verify("password", "not-a-phc-string").is_err());
}
