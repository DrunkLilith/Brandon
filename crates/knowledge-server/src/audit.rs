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
