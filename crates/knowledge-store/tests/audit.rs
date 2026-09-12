use knowledge_store::audit::{AuditWriter, NewAuditEvent, SafeAuditMetadata};
use serde_json::json;
use sqlx::{PgPool, Row};
use uuid::Uuid;

async fn seed_workspace_and_user(pool: &PgPool) -> (Uuid, Uuid) {
    let workspace_id = Uuid::now_v7();
    let user_id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO workspaces (
            id, name_ciphertext, name_nonce, name_key_version,
            wrapped_key_ciphertext, wrapped_key_nonce, wrapped_key_version
         ) VALUES ($1, $2, $3, 1, $4, $5, 1)",
    )
    .bind(workspace_id)
    .bind(b"workspace".as_slice())
    .bind([1_u8; 12].as_slice())
    .bind(b"wrapped".as_slice())
    .bind([2_u8; 12].as_slice())
    .execute(pool)
    .await
    .expect("workspace fixture should insert");
    sqlx::query(
        "INSERT INTO users (
            id, workspace_id, email_hmac, display_name_ciphertext,
            display_name_nonce, display_name_key_version, status
         ) VALUES ($1, $2, $3, $4, $5, 1, 'active')",
    )
    .bind(user_id)
    .bind(workspace_id)
    .bind([3_u8; 32].as_slice())
    .bind(b"user".as_slice())
    .bind([4_u8; 12].as_slice())
    .execute(pool)
    .await
    .expect("user fixture should insert");
    (workspace_id, user_id)
}

#[test]
fn audit_metadata_rejects_secret_bearing_keys_at_any_depth() {
    let safe = SafeAuditMetadata::new(json!({
        "changedFields": ["role"],
        "subject": {"kind": "group"}
    }));
    let secret = SafeAuditMetadata::new(json!({
        "subject": {"body": "plaintext must not enter audit"}
    }));

    assert!(safe.is_ok());
    assert!(secret.is_err());
}

#[sqlx::test]
async fn failed_audit_append_rolls_back_the_domain_mutation(pool: PgPool) {
    let (workspace_id, user_id) = seed_workspace_and_user(&pool).await;
    let group_id = Uuid::now_v7();
    let mut transaction = pool.begin().await.expect("transaction should begin");

    sqlx::query(
        "INSERT INTO groups (
            id, workspace_id, name_ciphertext, name_nonce, name_key_version, created_by
         ) VALUES ($1, $2, $3, $4, 1, $5)",
    )
    .bind(group_id)
    .bind(workspace_id)
    .bind(b"encrypted group".as_slice())
    .bind([5_u8; 12].as_slice())
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .expect("domain mutation should initially succeed");

    let invalid_actor = Uuid::now_v7();
    let audit_result = AuditWriter::append(
        &mut transaction,
        NewAuditEvent {
            id: Uuid::now_v7(),
            workspace_id,
            actor_user_id: Some(invalid_actor),
            session_id: None,
            event_type: "group.created".to_owned(),
            target_type: "group".to_owned(),
            target_id: Some(group_id),
            request_id: Uuid::now_v7(),
            metadata_ciphertext: b"encrypted metadata".to_vec(),
            metadata_nonce: [6_u8; 12],
            metadata_key_version: 1,
            previous_hash: None,
            event_hash: [7_u8; 32],
        },
    )
    .await;
    assert!(audit_result.is_err());
    drop(transaction);

    let stored_groups: i64 = sqlx::query("SELECT count(*) AS count FROM groups WHERE id = $1")
        .bind(group_id)
        .fetch_one(&pool)
        .await
        .expect("group count should be readable")
        .get("count");
    assert_eq!(stored_groups, 0);
}
