use knowledge_core::ids::{UserId, WorkspaceId};
use knowledge_crypto::{
    CryptoError, EnvelopeCipher, KeyPurpose, KeyVersion, Keyring, MasterKey, PasswordService,
};
use knowledge_store::{
    Store, StoreError,
    audit::{AuditWriter, NewAuditEvent, SafeAuditMetadata},
};
use serde_json::json;
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;
use zeroize::Zeroize;

const BOOTSTRAP_LOCK_ID: i64 = 0x4b_4e_4f_57_4c_45_44;
const KEY_VERSION: KeyVersion = KeyVersion(1);

#[derive(Eq, PartialEq)]
pub struct BootstrapInput {
    pub workspace_name: String,
    pub email: String,
    pub display_name: String,
    pub password: String,
}

impl std::fmt::Debug for BootstrapInput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BootstrapInput")
            .field("workspace_name", &self.workspace_name)
            .field("email", &self.email)
            .field("display_name", &self.display_name)
            .field("password", &"[REDACTED]")
            .finish()
    }
}

impl Drop for BootstrapInput {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BootstrapResult {
    pub workspace_id: WorkspaceId,
    pub user_id: UserId,
}

#[derive(Debug, Error)]
pub enum BootstrapError {
    #[error("the installation has already been bootstrapped")]
    AlreadyCompleted,
    #[error("{0} must not be blank")]
    BlankField(&'static str),
    #[error("password must contain at least 12 characters")]
    WeakPassword,
    #[error(transparent)]
    Crypto(#[from] CryptoError),
    #[error(transparent)]
    Database(#[from] sqlx::Error),
    #[error(transparent)]
    Store(#[from] StoreError),
}

pub struct BootstrapService;

impl BootstrapService {
    pub async fn run(
        store: &Store,
        master_key: &MasterKey,
        input: BootstrapInput,
    ) -> Result<BootstrapResult, BootstrapError> {
        validate_input(&input)?;

        let normalized_email = input.email.trim().to_lowercase();
        let workspace_name = input.workspace_name.trim();
        let display_name = input.display_name.trim();

        let mut transaction = store.pool().begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock($1)")
            .bind(BOOTSTRAP_LOCK_ID)
            .execute(&mut *transaction)
            .await?;

        let already_completed: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM workspaces)")
                .fetch_one(&mut *transaction)
                .await?;
        if already_completed {
            return Err(BootstrapError::AlreadyCompleted);
        }

        let workspace_id = WorkspaceId::new();
        let user_id = UserId::new();
        let wrapped_key = Keyring::new_workspace(master_key, KEY_VERSION)?;
        let workspace_key = Keyring::unwrap(master_key, &wrapped_key)?;
        let content_key = workspace_key.derive(KeyPurpose::Content)?;
        let search_key = workspace_key.derive(KeyPurpose::Search)?;
        let content_cipher = EnvelopeCipher::new(KEY_VERSION, content_key);

        let workspace_name_aad = format!("workspace:{workspace_id}:name");
        let display_name_aad = format!("workspace:{workspace_id}:user:{user_id}:display-name");
        let encrypted_workspace_name =
            content_cipher.encrypt(workspace_name_aad.as_bytes(), workspace_name.as_bytes())?;
        let encrypted_display_name =
            content_cipher.encrypt(display_name_aad.as_bytes(), display_name.as_bytes())?;
        let email_hmac = search_key.authenticate(normalized_email.as_bytes());
        let password_hash = PasswordService::recommended().hash(&input.password)?;

        sqlx::query(
            "INSERT INTO workspaces (
                id, name_ciphertext, name_nonce, name_key_version,
                wrapped_key_ciphertext, wrapped_key_nonce, wrapped_key_version
             ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(workspace_id.as_uuid())
        .bind(encrypted_workspace_name.ciphertext)
        .bind(encrypted_workspace_name.nonce.as_slice())
        .bind(KEY_VERSION.0 as i32)
        .bind(wrapped_key.ciphertext)
        .bind(wrapped_key.nonce.as_slice())
        .bind(KEY_VERSION.0 as i32)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            "INSERT INTO users (
                id, workspace_id, email_hmac,
                display_name_ciphertext, display_name_nonce, display_name_key_version,
                password_hash, status
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, 'active')",
        )
        .bind(user_id.as_uuid())
        .bind(workspace_id.as_uuid())
        .bind(email_hmac.as_slice())
        .bind(encrypted_display_name.ciphertext)
        .bind(encrypted_display_name.nonce.as_slice())
        .bind(KEY_VERSION.0 as i32)
        .bind(password_hash)
        .execute(&mut *transaction)
        .await?;

        sqlx::query(
            "INSERT INTO workspace_members (workspace_id, user_id, role)
             VALUES ($1, $2, 'admin')",
        )
        .bind(workspace_id.as_uuid())
        .bind(user_id.as_uuid())
        .execute(&mut *transaction)
        .await?;

        let audit_id = Uuid::now_v7();
        let request_id = Uuid::now_v7();
        let metadata =
            SafeAuditMetadata::new(json!({})).expect("empty bootstrap metadata is always safe");
        let serialized_metadata = serde_json::to_vec(metadata.as_value())
            .expect("JSON values always have a serializable representation");
        let metadata_aad = format!("workspace:{workspace_id}:audit:{audit_id}:metadata");
        let encrypted_metadata =
            content_cipher.encrypt(metadata_aad.as_bytes(), &serialized_metadata)?;
        let event_hash = audit_hash(
            workspace_id,
            user_id,
            audit_id,
            request_id,
            &encrypted_metadata.ciphertext,
        );

        AuditWriter::append(
            &mut transaction,
            NewAuditEvent {
                id: audit_id,
                workspace_id: workspace_id.as_uuid(),
                actor_user_id: Some(user_id.as_uuid()),
                session_id: None,
                event_type: "system.bootstrap".to_owned(),
                target_type: "workspace".to_owned(),
                target_id: Some(workspace_id.as_uuid()),
                request_id,
                metadata_ciphertext: encrypted_metadata.ciphertext,
                metadata_nonce: encrypted_metadata.nonce,
                metadata_key_version: KEY_VERSION.0 as i32,
                previous_hash: None,
                event_hash,
            },
        )
        .await?;

        transaction.commit().await?;
        Ok(BootstrapResult {
            workspace_id,
            user_id,
        })
    }
}

fn validate_input(input: &BootstrapInput) -> Result<(), BootstrapError> {
    for (name, value) in [
        ("workspace name", input.workspace_name.as_str()),
        ("email", input.email.as_str()),
        ("display name", input.display_name.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(BootstrapError::BlankField(name));
        }
    }
    if input.password.chars().count() < 12 {
        return Err(BootstrapError::WeakPassword);
    }
    Ok(())
}

fn audit_hash(
    workspace_id: WorkspaceId,
    user_id: UserId,
    audit_id: Uuid,
    request_id: Uuid,
    metadata_ciphertext: &[u8],
) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(workspace_id.as_uuid().as_bytes());
    digest.update(user_id.as_uuid().as_bytes());
    digest.update(audit_id.as_bytes());
    digest.update(request_id.as_bytes());
    digest.update(b"system.bootstrap\0workspace");
    digest.update(metadata_ciphertext);
    digest.finalize().into()
}
