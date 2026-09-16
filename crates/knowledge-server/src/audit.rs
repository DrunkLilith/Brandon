use knowledge_crypto::{CryptoError, EnvelopeCipher, KeyPurpose, KeyVersion, WorkspaceKey};
use knowledge_store::{
    StoreError,
    audit::{AuditMetadataError, AuditWriter, NewAuditEvent, SafeAuditMetadata},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug)]
pub(crate) struct AuditEvent<'a> {
    pub workspace_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub event_type: &'a str,
    pub target_type: &'a str,
    pub target_id: Option<Uuid>,
    pub request_id: Uuid,
    pub metadata: Value,
}

#[derive(Debug, Error)]
pub(crate) enum ServerAuditError {
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Metadata(#[from] AuditMetadataError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("stored audit hash is malformed")]
    CorruptHash,
}

pub(crate) async fn append_audit_event(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_key: &WorkspaceKey,
    event: AuditEvent<'_>,
) -> Result<(), ServerAuditError> {
    lock_workspace_for_audit(transaction, event.workspace_id).await?;
    let previous_hash = sqlx::query_scalar::<_, Vec<u8>>(
        "SELECT candidate.event_hash
           FROM audit_events candidate
          WHERE candidate.workspace_id = $1
            AND NOT EXISTS (
                SELECT 1
                  FROM audit_events successor
                 WHERE successor.workspace_id = candidate.workspace_id
                   AND successor.previous_hash = candidate.event_hash
            )
          LIMIT 1",
    )
    .bind(event.workspace_id)
    .fetch_optional(&mut **transaction)
    .await?
    .map(|bytes| bytes.try_into().map_err(|_| ServerAuditError::CorruptHash))
    .transpose()?;

    let id = Uuid::now_v7();
    let metadata = SafeAuditMetadata::new(event.metadata.clone())?;
    let serialized_metadata = serde_json::to_vec(metadata.as_value())?;
    let cipher = EnvelopeCipher::new(KeyVersion(1), workspace_key.derive(KeyPurpose::Content)?);
    let aad = format!("workspace:{}:audit:{id}:metadata", event.workspace_id);
    let encrypted_metadata = cipher.encrypt(aad.as_bytes(), &serialized_metadata)?;
    let event_hash = event_hash(
        id,
        &event,
        previous_hash.as_ref(),
        &encrypted_metadata.ciphertext,
    );

    AuditWriter::append(
        transaction,
        NewAuditEvent {
            id,
            workspace_id: event.workspace_id,
            actor_user_id: event.actor_user_id,
            session_id: event.session_id,
            event_type: event.event_type.to_owned(),
            target_type: event.target_type.to_owned(),
            target_id: event.target_id,
            request_id: event.request_id,
            metadata_ciphertext: encrypted_metadata.ciphertext,
            metadata_nonce: encrypted_metadata.nonce,
            metadata_key_version: 1,
            previous_hash,
            event_hash,
        },
    )
    .await?;
    Ok(())
}

pub(crate) async fn lock_workspace_for_audit(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
) -> Result<(), ServerAuditError> {
    sqlx::query("SELECT id FROM workspaces WHERE id = $1 FOR UPDATE")
        .bind(workspace_id)
        .fetch_one(&mut **transaction)
        .await?;
    Ok(())
}

fn event_hash(
    id: Uuid,
    event: &AuditEvent<'_>,
    previous_hash: Option<&[u8; 32]>,
    metadata_ciphertext: &[u8],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"knowledge-platform/audit-event/v1\0");
    if let Some(previous_hash) = previous_hash {
        digest.update(previous_hash);
    }
    digest.update(id.as_bytes());
    digest.update(event.workspace_id.as_bytes());
    if let Some(actor_user_id) = event.actor_user_id {
        digest.update(actor_user_id.as_bytes());
    }
    if let Some(session_id) = event.session_id {
        digest.update(session_id.as_bytes());
    }
    digest.update(event.event_type.as_bytes());
    digest.update([0]);
    digest.update(event.target_type.as_bytes());
    if let Some(target_id) = event.target_id {
        digest.update(target_id.as_bytes());
    }
    digest.update(event.request_id.as_bytes());
    digest.update(metadata_ciphertext);
    digest.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use knowledge_store::audit::{AuditCheckpoint, AuditRecord, verify_chain};

    #[test]
    fn chain_rejects_deletion_reordering_metadata_and_removed_valid_suffix() {
        let key = WorkspaceKey::generate().unwrap();
        let workspace = Uuid::now_v7();
        let mut records = Vec::new();
        for _ in 0..3 {
            let id = Uuid::now_v7();
            let event = AuditEvent {
                workspace_id: workspace,
                actor_user_id: None,
                session_id: None,
                event_type: "fixture",
                target_type: "fixture",
                target_id: None,
                request_id: Uuid::now_v7(),
                metadata: serde_json::json!({"operation":"fixture"}),
            };
            let cipher =
                EnvelopeCipher::new(KeyVersion(1), key.derive(KeyPurpose::Content).unwrap());
            let envelope = cipher
                .encrypt(
                    format!("workspace:{workspace}:audit:{id}:metadata").as_bytes(),
                    br#"{"operation":"fixture"}"#,
                )
                .unwrap();
            let previous = records
                .last()
                .map(|r: &AuditRecord| r.event_hash.clone().try_into().unwrap());
            let hash = event_hash(id, &event, previous.as_ref(), &envelope.ciphertext);
            records.push(AuditRecord {
                id,
                workspace_id: workspace,
                actor_user_id: None,
                session_id: None,
                event_type: "fixture".into(),
                target_type: "fixture".into(),
                target_id: None,
                request_id: event.request_id,
                created_at: Utc::now(),
                metadata_ciphertext: envelope.ciphertext,
                metadata_nonce: envelope.nonce.to_vec(),
                metadata_key_version: 1,
                previous_hash: previous.map(|h: [u8; 32]| h.to_vec()),
                event_hash: hash.to_vec(),
            });
        }
        let checkpoint = AuditCheckpoint {
            count: 3,
            head: Some(records[2].event_hash.clone().try_into().unwrap()),
        };
        assert!(verify_chain(&records, &key, workspace, &checkpoint).is_ok());
        assert!(
            verify_chain(&records[..2], &key, workspace, &checkpoint).is_err(),
            "trusted checkpoint detects removed suffix"
        );
        records.swap(0, 1);
        assert!(verify_chain(&records, &key, workspace, &checkpoint).is_err());
        records.swap(0, 1);
        records[1].metadata_nonce[0] ^= 1;
        assert!(verify_chain(&records, &key, workspace, &checkpoint).is_err());
        records[1].metadata_nonce[0] ^= 1;
        records.remove(1);
        assert!(verify_chain(&records, &key, workspace, &checkpoint).is_err());
    }
}
