use base64::{Engine as _, engine::general_purpose::STANDARD};
use knowledge_crypto::MasterKey;
use knowledge_server::bootstrap::{BootstrapError, BootstrapInput, BootstrapService};
use knowledge_store::{MIGRATOR, Store};
use sqlx::{PgPool, Row};
use std::process::Command;

fn master_key() -> MasterKey {
    MasterKey::from_base64(&STANDARD.encode([91_u8; 32])).expect("fixture key should decode")
}

fn bootstrap_input() -> BootstrapInput {
    BootstrapInput {
        workspace_name: "Командная база".to_owned(),
        email: "  ADMIN@Example.COM ".to_owned(),
        display_name: "Главный администратор".to_owned(),
        password: "correct horse battery staple".to_owned(),
    }
}

#[test]
fn bootstrap_cli_rejects_password_arguments_without_echoing_secret() {
    let output = Command::new(env!("CARGO_BIN_EXE_bootstrap"))
        .args([
            "--email",
            "admin@example.com",
            "--password",
            "do-not-print-this-secret",
        ])
        .output()
        .expect("bootstrap CLI should execute");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("password arguments are not accepted"));
    assert!(!stderr.contains("do-not-print-this-secret"));
}

#[test]
fn bootstrap_input_debug_redacts_password() {
    let rendered = format!("{:?}", bootstrap_input());

    assert!(!rendered.contains("correct horse battery staple"));
    assert!(rendered.contains("[REDACTED]"));
}

#[sqlx::test]
async fn bootstrap_is_atomic_and_can_run_only_once(pool: PgPool) {
    MIGRATOR
        .run(&pool)
        .await
        .expect("test schema should migrate");
    let store = Store::from_pool(pool.clone());
    let master_key = master_key();

    let result = BootstrapService::run(&store, &master_key, bootstrap_input())
        .await
        .expect("first bootstrap should succeed");
    let second = BootstrapService::run(&store, &master_key, bootstrap_input()).await;

    assert!(matches!(second, Err(BootstrapError::AlreadyCompleted)));
    let workspace_count: i64 = sqlx::query_scalar("SELECT count(*) FROM workspaces")
        .fetch_one(&pool)
        .await
        .expect("workspace count should be readable");
    let admin_count: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM workspace_members
          WHERE workspace_id = $1 AND user_id = $2 AND role = 'admin'",
    )
    .bind(result.workspace_id.into_uuid())
    .bind(result.user_id.into_uuid())
    .fetch_one(&pool)
    .await
    .expect("admin count should be readable");
    let audit_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type = 'system.bootstrap'",
    )
    .fetch_one(&pool)
    .await
    .expect("audit count should be readable");

    assert_eq!(workspace_count, 1);
    assert_eq!(admin_count, 1);
    assert_eq!(audit_count, 1);
}

#[sqlx::test]
async fn bootstrap_stores_no_workspace_or_user_plaintext(pool: PgPool) {
    MIGRATOR
        .run(&pool)
        .await
        .expect("test schema should migrate");
    let store = Store::from_pool(pool.clone());
    BootstrapService::run(&store, &master_key(), bootstrap_input())
        .await
        .expect("bootstrap should succeed");

    let workspace = sqlx::query("SELECT name_ciphertext FROM workspaces")
        .fetch_one(&pool)
        .await
        .expect("workspace should exist")
        .get::<Vec<u8>, _>("name_ciphertext");
    let user = sqlx::query("SELECT display_name_ciphertext, email_hmac, password_hash FROM users")
        .fetch_one(&pool)
        .await
        .expect("user should exist");
    let display_name = user.get::<Vec<u8>, _>("display_name_ciphertext");
    let email_hmac = user.get::<Vec<u8>, _>("email_hmac");
    let password_hash = user.get::<String, _>("password_hash");

    assert!(
        !workspace
            .windows("Командная база".len())
            .any(|window| { window == "Командная база".as_bytes() })
    );
    assert!(
        !display_name
            .windows("Главный администратор".len())
            .any(|window| { window == "Главный администратор".as_bytes() })
    );
    assert_ne!(email_hmac, b"admin@example.com");
    assert!(password_hash.starts_with("$argon2id$"));
}
