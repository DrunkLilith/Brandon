use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::{FromRow, Postgres, Transaction};
use thiserror::Error;
use uuid::Uuid;

use crate::StoreError;

const FORBIDDEN_METADATA_KEYS: [&str; 5] = ["password", "token", "key", "body", "content"];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeAuditMetadata(Value);

impl SafeAuditMetadata {
    pub fn new(value: Value) -> Result<Self, AuditMetadataError> {
        validate_value(&value)?;
        Ok(Self(value))
    }

    #[must_use]
    pub const fn as_value(&self) -> &Value {
        &self.0
    }
}

fn validate_value(value: &Value) -> Result<(), AuditMetadataError> {
    match value {
        Value::Object(object) => {
            for (key, nested) in object {
                let normalized = key.to_ascii_lowercase();
                if FORBIDDEN_METADATA_KEYS.iter().any(|forbidden| {
                    normalized == *forbidden || normalized.ends_with(&format!("_{forbidden}"))
                }) {
                    return Err(AuditMetadataError::ForbiddenKey(key.clone()));
                }
                validate_value(nested)?;
            }
        }
        Value::Array(values) => {
            for nested in values {
                validate_value(nested)?;
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AuditMetadataError {
    #[error("audit metadata key is forbidden: {0}")]
    ForbiddenKey(String),
}

pub struct NewAuditEvent {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub event_type: String,
    pub target_type: String,
    pub target_id: Option<Uuid>,
    pub request_id: Uuid,
    pub metadata_ciphertext: Vec<u8>,
    pub metadata_nonce: [u8; 12],
    pub metadata_key_version: i32,
    pub previous_hash: Option<[u8; 32]>,
    pub event_hash: [u8; 32],
}

pub struct AuditWriter;

#[derive(FromRow)]
pub struct AuditRecord {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub actor_user_id: Option<Uuid>,
    pub session_id: Option<Uuid>,
    pub event_type: String,
    pub target_type: String,
    pub target_id: Option<Uuid>,
    pub request_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub metadata_ciphertext: Vec<u8>,
    pub metadata_nonce: Vec<u8>,
    pub metadata_key_version: i32,
    pub previous_hash: Option<Vec<u8>>,
    pub event_hash: Vec<u8>,
}

#[derive(Default)]
pub struct AuditFilters {
    pub actor: Option<Uuid>,
    pub event_type: Option<String>,
    pub target: Option<Uuid>,
    pub target_type: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub after: Option<Uuid>,
}

pub struct AuditReader;

/// Caller-held trusted checkpoint. A checkpoint read only from the same
/// mutable database is NOT an external tamper-evidence anchor.
pub struct AuditCheckpoint {
    pub count: u64,
    pub head: Option<[u8; 32]>,
}

#[derive(Debug, Error)]
#[error("audit chain verification failed")]
pub struct AuditVerificationError;

/// Verifies the historical v1 format, all links, encrypted metadata and the
/// expected count/head. Legacy timestamps are not authenticated by v1 hashes.
/// A database superuser with the workspace key can rewrite a bare SHA256 chain.
pub fn verify_chain(
    records: &[AuditRecord],
    key: &knowledge_crypto::WorkspaceKey,
    workspace: Uuid,
    checkpoint: &AuditCheckpoint,
) -> Result<(), AuditVerificationError> {
    use knowledge_crypto::{EncryptedEnvelope, EnvelopeCipher, KeyPurpose, KeyVersion};
    use sha2::{Digest, Sha256};
    if records.len() as u64 != checkpoint.count {
        return Err(AuditVerificationError);
    }
    let mut previous: Option<[u8; 32]> = None;
    let mut previous_id = None;
    for r in records {
        if r.workspace_id != workspace
            || previous_id.is_some_and(|id| id >= r.id)
            || r.previous_hash.as_deref() != previous.as_ref().map(|h| h.as_slice())
        {
            return Err(AuditVerificationError);
        }
        let mut digest = Sha256::new();
        digest.update(b"knowledge-platform/audit-event/v1\0");
        if let Some(h) = previous {
            digest.update(h);
        }
        digest.update(r.id.as_bytes());
        digest.update(workspace.as_bytes());
        if let Some(actor) = r.actor_user_id {
            digest.update(actor.as_bytes());
        }
        if let Some(session) = r.session_id {
            digest.update(session.as_bytes());
        }
        digest.update(r.event_type.as_bytes());
        digest.update([0]);
        digest.update(r.target_type.as_bytes());
        if let Some(target) = r.target_id {
            digest.update(target.as_bytes());
        }
        digest.update(r.request_id.as_bytes());
        digest.update(&r.metadata_ciphertext);
        let hash: [u8; 32] = digest.finalize().into();
        if r.event_hash.as_slice() != hash {
            return Err(AuditVerificationError);
        }
        let version =
            KeyVersion(u32::try_from(r.metadata_key_version).map_err(|_| AuditVerificationError)?);
        let cipher = EnvelopeCipher::new(
            version,
            key.derive_for(KeyPurpose::Content, version)
                .map_err(|_| AuditVerificationError)?,
        );
        let envelope = EncryptedEnvelope {
            algorithm: "AES-256-GCM".into(),
            key_version: version,
            nonce: r
                .metadata_nonce
                .as_slice()
                .try_into()
                .map_err(|_| AuditVerificationError)?,
            ciphertext: r.metadata_ciphertext.clone(),
        };
        let plaintext = cipher
            .decrypt(
                format!("workspace:{workspace}:audit:{}:metadata", r.id).as_bytes(),
                &envelope,
            )
            .map_err(|_| AuditVerificationError)?;
        let metadata: Value =
            serde_json::from_slice(plaintext.expose()).map_err(|_| AuditVerificationError)?;
        SafeAuditMetadata::new(metadata).map_err(|_| AuditVerificationError)?;
        previous = Some(hash);
        previous_id = Some(r.id);
    }
    if previous != checkpoint.head {
        return Err(AuditVerificationError);
    }
    Ok(())
}
impl AuditReader {
    pub async fn list(
        transaction: &mut Transaction<'_, Postgres>,
        workspace: Uuid,
        filters: AuditFilters,
        limit: i64,
    ) -> Result<Vec<AuditRecord>, StoreError> {
        Ok(sqlx::query_as("SELECT * FROM audit_events WHERE workspace_id=$1 AND ($2::uuid IS NULL OR actor_user_id=$2) AND ($3::text IS NULL OR event_type=$3) AND ($4::uuid IS NULL OR target_id=$4) AND ($5::text IS NULL OR target_type=$5) AND ($6::timestamptz IS NULL OR created_at >= $6) AND ($7::timestamptz IS NULL OR created_at <= $7) AND ($8::uuid IS NULL OR id > $8) ORDER BY id LIMIT $9")
            .bind(workspace).bind(filters.actor).bind(filters.event_type).bind(filters.target).bind(filters.target_type).bind(filters.from).bind(filters.to).bind(filters.after).bind(limit.clamp(1,100)).fetch_all(&mut **transaction).await?)
    }
}

impl AuditWriter {
    pub async fn append(
        transaction: &mut Transaction<'_, Postgres>,
        event: NewAuditEvent,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO audit_events (
                id, workspace_id, actor_user_id, session_id,
                event_type, target_type, target_id, request_id,
                metadata_ciphertext, metadata_nonce, metadata_key_version,
                previous_hash, event_hash
             ) VALUES (
                $1, $2, $3, $4, $5, $6, $7, $8,
                $9, $10, $11, $12, $13
             )",
        )
        .bind(event.id)
        .bind(event.workspace_id)
        .bind(event.actor_user_id)
        .bind(event.session_id)
        .bind(event.event_type)
        .bind(event.target_type)
        .bind(event.target_id)
        .bind(event.request_id)
        .bind(event.metadata_ciphertext)
        .bind(event.metadata_nonce.as_slice())
        .bind(event.metadata_key_version)
        .bind(event.previous_hash.map(|value| value.to_vec()))
        .bind(event.event_hash.as_slice())
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }
}
