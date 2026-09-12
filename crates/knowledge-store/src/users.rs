use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceKeyRecord {
    pub workspace_id: Uuid,
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
    pub key_version: i32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoginUser {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub password_hash: String,
    pub role: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ActiveSession {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub workspace_admin: bool,
}

pub struct UserRepository;

impl UserRepository {
    pub async fn single_workspace_key(
        pool: &PgPool,
    ) -> Result<Option<WorkspaceKeyRecord>, StoreError> {
        let row: Option<(Uuid, Vec<u8>, Vec<u8>, i32)> = sqlx::query_as(
            "SELECT id, wrapped_key_ciphertext, wrapped_key_nonce, wrapped_key_version
               FROM workspaces
              ORDER BY created_at, id
              LIMIT 1",
        )
        .fetch_optional(pool)
        .await?;

        row.map(|(workspace_id, ciphertext, nonce, key_version)| {
            let nonce = nonce
                .try_into()
                .map_err(|_| StoreError::CorruptData("workspace key nonce"))?;
            Ok(WorkspaceKeyRecord {
                workspace_id,
                ciphertext,
                nonce,
                key_version,
            })
        })
        .transpose()
    }

    pub async fn active_login_user(
        pool: &PgPool,
        workspace_id: Uuid,
        email_hmac: &[u8; 32],
    ) -> Result<Option<LoginUser>, StoreError> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, String, String)>(
            "SELECT u.id, u.workspace_id, u.password_hash, wm.role
               FROM users u
               JOIN workspace_members wm
                 ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
              WHERE u.workspace_id = $1
                AND u.email_hmac = $2
                AND u.status = 'active'
                AND u.deleted_at IS NULL
                AND wm.status = 'active'",
        )
        .bind(workspace_id)
        .bind(email_hmac.as_slice())
        .fetch_optional(pool)
        .await?;

        Ok(
            row.map(|(user_id, workspace_id, password_hash, role)| LoginUser {
                user_id,
                workspace_id,
                password_hash,
                role,
            }),
        )
    }

    pub async fn create_session(
        transaction: &mut Transaction<'_, Postgres>,
        session_id: Uuid,
        user: &LoginUser,
        token_hash: &[u8; 32],
        expires_at: DateTime<Utc>,
    ) -> Result<(), StoreError> {
        sqlx::query(
            "INSERT INTO sessions (id, workspace_id, user_id, token_hash, expires_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(session_id)
        .bind(user.workspace_id)
        .bind(user.user_id)
        .bind(token_hash.as_slice())
        .bind(expires_at)
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    pub async fn revoke_by_token_hash(
        transaction: &mut Transaction<'_, Postgres>,
        token_hash: &[u8; 32],
    ) -> Result<(), StoreError> {
        sqlx::query(
            "UPDATE sessions
                SET revoked_at = COALESCE(revoked_at, now())
              WHERE token_hash = $1",
        )
        .bind(token_hash.as_slice())
        .execute(&mut **transaction)
        .await?;
        Ok(())
    }

    pub async fn active_session(
        pool: &PgPool,
        token_hash: &[u8; 32],
    ) -> Result<Option<ActiveSession>, StoreError> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, Uuid, bool)>(
            "SELECT s.id, s.user_id, s.workspace_id, (wm.role = 'admin')
               FROM sessions s
               JOIN users u
                 ON u.workspace_id = s.workspace_id AND u.id = s.user_id
               JOIN workspace_members wm
                 ON wm.workspace_id = s.workspace_id AND wm.user_id = s.user_id
              WHERE s.token_hash = $1
                AND s.revoked_at IS NULL
                AND s.expires_at > now()
                AND u.status = 'active'
                AND u.deleted_at IS NULL
                AND wm.status = 'active'",
        )
        .bind(token_hash.as_slice())
        .fetch_optional(pool)
        .await?;

        Ok(row.map(
            |(session_id, user_id, workspace_id, workspace_admin)| ActiveSession {
                session_id,
                user_id,
                workspace_id,
                workspace_admin,
            },
        ))
    }
}
