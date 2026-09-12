use serde_json::Value;
use sqlx::{Postgres, Transaction};
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
