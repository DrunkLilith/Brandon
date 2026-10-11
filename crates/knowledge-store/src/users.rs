use chrono::{DateTime, Utc};
use knowledge_core::permissions::AccountStatus;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkspaceKeyRecord {
    pub workspace_id: Uuid,
    pub ciphertext: Vec<u8>,
    pub nonce: [u8; 12],
    pub key_version: i32,
    pub current_version: i32,
    pub previous: Vec<(i32, Vec<u8>, Vec<u8>, i32)>,
}

type WorkspaceKeyRow = (Uuid, Vec<u8>, Vec<u8>, i32, i32, i32, Vec<u8>, Vec<u8>, i32);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LoginUser {
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub password_hash: String,
    pub role: String,
    pub auth_generation: i64,
    pub membership_generation: i64,
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
    pub async fn set_account_status_and_revoke_sessions(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        user_id: Uuid,
        status: AccountStatus,
    ) -> Result<bool, StoreError> {
        let status = match status {
            AccountStatus::Invited => "invited",
            AccountStatus::Active => "active",
            AccountStatus::Suspended => "suspended",
            AccountStatus::Deleted => "deleted",
        };
        let changed = sqlx::query(
            "UPDATE users SET status = $3, updated_at = now()
              WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(status)
        .execute(&mut **transaction)
        .await?
        .rows_affected()
            == 1;

        if changed && status != "active" {
            sqlx::query(
                "UPDATE sessions SET revoked_at = COALESCE(revoked_at, now())
                  WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL",
            )
            .bind(workspace_id)
            .bind(user_id)
            .execute(&mut **transaction)
            .await?;
        }
        Ok(changed)
    }

    pub async fn change_password_and_revoke_sessions(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        user_id: Uuid,
        password_hash: &str,
    ) -> Result<bool, StoreError> {
        let changed = sqlx::query(
            "UPDATE users SET password_hash = $3, updated_at = now()
              WHERE workspace_id = $1 AND id = $2
                AND status = 'active' AND deleted_at IS NULL",
        )
        .bind(workspace_id)
        .bind(user_id)
        .bind(password_hash)
        .execute(&mut **transaction)
        .await?
        .rows_affected()
            == 1;
        if changed {
            sqlx::query(
                "UPDATE sessions SET revoked_at = COALESCE(revoked_at, now())
                  WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL",
            )
            .bind(workspace_id)
            .bind(user_id)
            .execute(&mut **transaction)
            .await?;
        }
        Ok(changed)
    }

    pub async fn revoke_all_sessions(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        user_id: Uuid,
    ) -> Result<bool, StoreError> {
        let user_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM users
                 WHERE workspace_id = $1 AND id = $2 AND deleted_at IS NULL
             )",
        )
        .bind(workspace_id)
        .bind(user_id)
        .fetch_one(&mut **transaction)
        .await?;
        if user_exists {
            sqlx::query(
                "UPDATE sessions SET revoked_at = COALESCE(revoked_at, now())
                  WHERE workspace_id = $1 AND user_id = $2 AND revoked_at IS NULL",
            )
            .bind(workspace_id)
            .bind(user_id)
            .execute(&mut **transaction)
            .await?;
        }
        Ok(user_exists)
    }

    pub async fn consume_login_attempt(
        pool: &PgPool,
        bucket_keys: &[[u8; 32]],
    ) -> Result<bool, StoreError> {
        Self::consume_rate_limit_attempt(pool, bucket_keys, 5).await
    }

    /// Configurable attempts per the existing 15 minute SQL window. Domain
    /// separation is the caller's responsibility; auth still uses five.
    pub async fn consume_rate_limit_attempt(
        pool: &PgPool,
        bucket_keys: &[[u8; 32]],
        limit: i32,
    ) -> Result<bool, StoreError> {
        if !(1..=10000).contains(&limit) {
            return Err(StoreError::CorruptData("invalid rate limit"));
        }
        let mut transaction = pool.begin().await?;
        sqlx::query(
            "DELETE FROM login_rate_limits
              WHERE updated_at < now() - interval '1 day'",
        )
        .execute(&mut *transaction)
        .await?;

        let mut allowed = true;
        for bucket_key in bucket_keys {
            let bucket_allowed: bool = sqlx::query_scalar(
                "INSERT INTO login_rate_limits (
                    bucket_key, failure_count, window_started_at, updated_at
                 ) VALUES ($1, 1, now(), now())
                 ON CONFLICT (bucket_key) DO UPDATE SET
                    failure_count = CASE
                        WHEN login_rate_limits.window_started_at < now() - interval '15 minutes'
                            THEN 1
                        ELSE LEAST(login_rate_limits.failure_count + 1, $2 + 1)
                    END,
                    window_started_at = CASE
                        WHEN login_rate_limits.window_started_at < now() - interval '15 minutes'
                            THEN now()
                        ELSE login_rate_limits.window_started_at
                    END,
                    updated_at = now()
                 RETURNING failure_count <= $2",
            )
            .bind(bucket_key.as_slice())
            .bind(limit)
            .fetch_one(&mut *transaction)
            .await?;
            allowed &= bucket_allowed;
        }
        transaction.commit().await?;
        Ok(allowed)
    }

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

        if let Some((id, _, _, _)) = row {
            Self::workspace_key(pool, id).await
        } else {
            Ok(None)
        }
    }

    pub async fn workspace_key(
        pool: &PgPool,
        workspace_id: Uuid,
    ) -> Result<Option<WorkspaceKeyRecord>, StoreError> {
        let rows: Vec<WorkspaceKeyRow> = sqlx::query_as(
            "SELECT w.id,w.wrapped_key_ciphertext,w.wrapped_key_nonce,w.wrapped_key_version,w.current_key_version,k.version,k.ciphertext,k.nonce,k.master_version FROM workspaces w JOIN workspace_key_generations k ON k.workspace_id=w.id WHERE w.id=$1 ORDER BY k.version",
        )
        .bind(workspace_id)
        .fetch_all(pool)
        .await?;

        workspace_key_record(rows)
    }

    pub async fn workspace_key_in_transaction(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
    ) -> Result<Option<WorkspaceKeyRecord>, StoreError> {
        let rows: Vec<WorkspaceKeyRow> = sqlx::query_as(
            "SELECT w.id,w.wrapped_key_ciphertext,w.wrapped_key_nonce,w.wrapped_key_version,w.current_key_version,k.version,k.ciphertext,k.nonce,k.master_version FROM workspaces w JOIN workspace_key_generations k ON k.workspace_id=w.id WHERE w.id=$1 ORDER BY k.version",
        )
        .bind(workspace_id)
        .fetch_all(&mut **transaction)
        .await?;

        workspace_key_record(rows)
    }
}

fn workspace_key_record(
    rows: Vec<WorkspaceKeyRow>,
) -> Result<Option<WorkspaceKeyRecord>, StoreError> {
    let first = rows.first().cloned();
    first
        .map(
            |(workspace_id, ciphertext, nonce, key_version, current_version, _, _, _, _)| {
                let nonce = nonce
                    .try_into()
                    .map_err(|_| StoreError::CorruptData("workspace key nonce"))?;
                Ok(WorkspaceKeyRecord {
                    workspace_id,
                    ciphertext,
                    nonce,
                    key_version,
                    current_version,
                    previous: rows
                        .into_iter()
                        .filter(|r| r.5 != current_version)
                        .map(|r| (r.5, r.6, r.7, r.8))
                        .collect(),
                })
            },
        )
        .transpose()
}

impl UserRepository {
    pub async fn active_login_user(
        pool: &PgPool,
        workspace_id: Uuid,
        email_hmac: &[u8; 32],
    ) -> Result<Option<LoginUser>, StoreError> {
        let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, i64, i64)>(
            "SELECT u.id, u.workspace_id, u.password_hash, wm.role, u.auth_generation, wm.auth_generation
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

        Ok(row.map(
            |(
                user_id,
                workspace_id,
                password_hash,
                role,
                auth_generation,
                membership_generation,
            )| LoginUser {
                user_id,
                workspace_id,
                password_hash,
                role,
                auth_generation,
                membership_generation,
            },
        ))
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
