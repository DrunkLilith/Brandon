use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use knowledge_server::config::{DatabaseConfig, ServerConfig, load_required_secret_from};

#[test]
fn explicit_listen_address_is_used() {
    let config = ServerConfig::from_pairs([("KNOWLEDGE_LISTEN_ADDRESS", "127.0.0.1:4100")])
        .expect("valid address should load");

    assert_eq!(
        config.listen_address,
        SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4100)
    );
}

#[test]
fn file_secrets_reject_conflicts_and_empty_files_without_trimming_secret_bytes() {
    let directory = std::env::temp_dir().join(format!("knowledge-config-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&directory).unwrap();
    let secret_path = directory.join("secret value");
    std::fs::write(&secret_path, b"  keep spaces  \r\n").unwrap();
    let loaded = load_required_secret_from(
        "EXAMPLE_SECRET",
        [("EXAMPLE_SECRET_FILE", secret_path.to_str().unwrap())],
        |path| std::fs::read_to_string(path),
    )
    .unwrap();
    assert_eq!(&*loaded, "  keep spaces  ");

    let conflict = load_required_secret_from(
        "EXAMPLE_SECRET",
        [
            ("EXAMPLE_SECRET", "direct"),
            ("EXAMPLE_SECRET_FILE", secret_path.to_str().unwrap()),
        ],
        |path| std::fs::read_to_string(path),
    )
    .unwrap_err();
    assert_eq!(
        conflict.to_string(),
        "EXAMPLE_SECRET and EXAMPLE_SECRET_FILE cannot both be set"
    );

    std::fs::write(&secret_path, b"\n").unwrap();
    let empty = load_required_secret_from(
        "EXAMPLE_SECRET",
        [("EXAMPLE_SECRET_FILE", secret_path.to_str().unwrap())],
        |path| std::fs::read_to_string(path),
    )
    .unwrap_err();
    assert_eq!(empty.to_string(), "EXAMPLE_SECRET_FILE is empty");

    let direct_newline = load_required_secret_from(
        "EXAMPLE_SECRET",
        [("EXAMPLE_SECRET", "must-not-be-normalized\n")],
        |_| unreachable!(),
    )
    .unwrap_err();
    assert_eq!(
        direct_newline.to_string(),
        "EXAMPLE_SECRET must contain exactly one line"
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn database_config_requires_and_encodes_the_password_secret() {
    let missing = DatabaseConfig::from_sources(
        [
            ("KNOWLEDGE_DATABASE_HOST", "postgres"),
            ("KNOWLEDGE_DATABASE_PORT", "5432"),
            ("KNOWLEDGE_DATABASE_NAME", "knowledge"),
            ("KNOWLEDGE_DATABASE_USER", "knowledge"),
        ],
        |_| unreachable!(),
    )
    .unwrap_err();
    assert_eq!(
        missing.to_string(),
        "KNOWLEDGE_DATABASE_PASSWORD or KNOWLEDGE_DATABASE_PASSWORD_FILE must be set"
    );

    let config = DatabaseConfig::from_sources(
        [
            ("KNOWLEDGE_DATABASE_HOST", "postgres"),
            ("KNOWLEDGE_DATABASE_PORT", "5432"),
            ("KNOWLEDGE_DATABASE_NAME", "knowledge"),
            ("KNOWLEDGE_DATABASE_USER", "knowledge"),
            ("KNOWLEDGE_DATABASE_PASSWORD_FILE", "C:/secrets/db"),
        ],
        |_| Ok("p@ss word\r\n".to_owned()),
    )
    .unwrap();
    assert_eq!(
        config.as_url(),
        "postgres://knowledge:p%40ss%20word@postgres:5432/knowledge"
    );
}
