use axum::{
    Json, Router,
    extract::{Extension, Path, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use knowledge_core::permissions::AccountStatus;
use knowledge_crypto::{
    EncryptedEnvelope, EnvelopeCipher, KeyPurpose, KeyVersion, PasswordService, WorkspaceKey,
};
use knowledge_store::users::UserRepository;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    CurrentUser,
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    middleware::RequestId,
    problem::ApiProblem,
    routes::auth::{ClientAddress, rate_limit_bucket, unwrap_workspace_key},
    state::AppState,
};

const INVITATION_LIFETIME_DAYS: i64 = 7;
type EncryptedTextFields = (Vec<u8>, Vec<u8>, i32);

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/invitations",
            post(create_invitation).get(list_invitations),
        )
        .route("/api/v1/invitations/{id}", delete(revoke_invitation))
        .route(
            "/api/v1/invitations/{token}/accept",
            post(accept_invitation),
        )
        .route("/api/v1/users", get(list_users))
        .route("/api/v1/users/{id}", patch(update_user))
        .route(
            "/api/v1/users/{id}/revoke-sessions",
            post(revoke_user_sessions),
        )
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateInvitationRequest {
    email: String,
    workspace_role: WorkspaceRoleInput,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum WorkspaceRoleInput {
    Admin,
    Member,
}

impl WorkspaceRoleInput {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::Member => "member",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AcceptInvitationRequest {
    display_name: String,
    password: String,
}

impl Drop for AcceptInvitationRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitationCreatedResponse {
    id: Uuid,
    token: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserCreatedResponse {
    id: Uuid,
    workspace_id: Uuid,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InvitationListItem {
    id: Uuid,
    email: Option<String>,
    workspace_role: String,
    status: String,
    expires_at: chrono::DateTime<Utc>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UserListItem {
    id: Uuid,
    email: Option<String>,
    display_name: String,
    workspace_role: String,
    status: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateUserRequest {
    status: Option<AccountStatusInput>,
    workspace_role: Option<WorkspaceRoleInput>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum AccountStatusInput {
    Active,
    Suspended,
}

impl AccountStatusInput {
    const fn domain(self) -> AccountStatus {
        match self {
            Self::Active => AccountStatus::Active,
            Self::Suspended => AccountStatus::Suspended,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
        }
    }
}

async fn create_invitation(
    State(state): State<AppState>,
    ClientAddress(source): ClientAddress,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    payload: Result<Json<CreateInvitationRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let Json(input) = payload
        .map_err(|_| ApiProblem::validation("body", "expected a valid invitation JSON object"))?;
    let email = input.email.trim().to_lowercase();
    validate_email(&email)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key_record = UserRepository::workspace_key(runtime.store.pool(), current.workspace_id)
        .await
        .map_err(|_| ApiProblem::internal())?
        .ok_or_else(ApiProblem::internal)?;
    let workspace_key = unwrap_workspace_key(&runtime.master_key, &key_record)
        .map_err(|_| ApiProblem::internal())?;
    let search_key = workspace_key
        .derive(KeyPurpose::Search)
        .map_err(|_| ApiProblem::internal())?;
    let email_hmac = search_key.authenticate(email.as_bytes());
    let source_identity = source_identity(source);
    let actor_identity = format!("{}:{}", current.workspace_id, current.user_id);
    let buckets = [
        rate_limit_bucket(b"invitation-create-source", source_identity.as_bytes()),
        rate_limit_bucket(b"invitation-create-actor", actor_identity.as_bytes()),
    ];
    if !UserRepository::consume_login_attempt(runtime.store.pool(), &buckets)
        .await
        .map_err(|_| ApiProblem::internal())?
    {
        return Err(ApiProblem::invitation_rate_limited());
    }
    let invitation_id = Uuid::now_v7();
    let email_cipher = EnvelopeCipher::new(
        KeyVersion(1),
        workspace_key
            .derive(KeyPurpose::Content)
            .map_err(|_| ApiProblem::internal())?,
    );
    let email_aad = format!(
        "workspace:{}:invitation:{invitation_id}:email",
        current.workspace_id
    );
    let encrypted_email = email_cipher
        .encrypt(email_aad.as_bytes(), email.as_bytes())
        .map_err(|_| ApiProblem::internal())?;
    let mut raw_token = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut *raw_token).map_err(|_| ApiProblem::internal())?;
    let token = Zeroizing::new(URL_SAFE_NO_PAD.encode(*raw_token));
    let token_hash: [u8; 32] = Sha256::digest(*raw_token).into();
    let expires_at = Utc::now() + Duration::days(INVITATION_LIFETIME_DAYS);

    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    lock_email_identity(&mut transaction, current.workspace_id, &email_hmac).await?;
    let expired_invitation_id: Option<Uuid> = sqlx::query_scalar(
        "UPDATE invitations SET revoked_at = now()
          WHERE workspace_id = $1 AND email_hmac = $2
            AND accepted_at IS NULL AND revoked_at IS NULL AND expires_at <= now()
          RETURNING id",
    )
    .bind(current.workspace_id)
    .bind(email_hmac.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    if let Some(expired_invitation_id) = expired_invitation_id {
        append_audit_event(
            &mut transaction,
            &workspace_key,
            AuditEvent {
                workspace_id: current.workspace_id,
                actor_user_id: Some(current.user_id),
                session_id: Some(current.session_id),
                event_type: "invitation.expired",
                target_type: "invitation",
                target_id: Some(expired_invitation_id),
                request_id: request_id.as_uuid(),
                metadata: json!({"reason": "superseded"}),
            },
        )
        .await
        .map_err(|_| ApiProblem::internal())?;
    }
    let user_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM users
             WHERE workspace_id = $1 AND email_hmac = $2 AND deleted_at IS NULL
         )",
    )
    .bind(current.workspace_id)
    .bind(email_hmac.as_slice())
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    if user_exists {
        return Err(ApiProblem::conflict(
            "A user with this email already exists",
        ));
    }

    let insert = sqlx::query(
        "INSERT INTO invitations (
            id, workspace_id, email_hmac, email_ciphertext, email_nonce, email_key_version,
            workspace_role, token_hash, invited_by, expires_at
         ) VALUES ($1, $2, $3, $4, $5, 1, $6, $7, $8, $9)",
    )
    .bind(invitation_id)
    .bind(current.workspace_id)
    .bind(email_hmac.as_slice())
    .bind(encrypted_email.ciphertext)
    .bind(encrypted_email.nonce.as_slice())
    .bind(input.workspace_role.as_str())
    .bind(token_hash.as_slice())
    .bind(current.user_id)
    .bind(expires_at)
    .execute(&mut *transaction)
    .await;
    if let Err(error) = insert {
        if error.as_database_error().is_some_and(|database| {
            database.constraint() == Some("invitations_pending_email_unique")
        }) {
            return Err(ApiProblem::conflict("A pending invitation already exists"));
        }
        return Err(ApiProblem::internal());
    }
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "invitation.created",
            target_type: "invitation",
            target_id: Some(invitation_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"workspace_role": input.workspace_role.as_str()}),
        },
    )
    .await
    .map_err(|_| ApiProblem::internal())?;
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;

    let mut response = (
        StatusCode::CREATED,
        Json(InvitationCreatedResponse {
            id: invitation_id,
            token: token.to_string(),
            expires_at,
        }),
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn accept_invitation(
    State(state): State<AppState>,
    ClientAddress(source): ClientAddress,
    Extension(request_id): Extension<RequestId>,
    Path(token): Path<String>,
    payload: Result<Json<AcceptInvitationRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid account details"))?;
    if input.display_name.trim().is_empty() {
        return Err(ApiProblem::validation("displayName", "must not be blank"));
    }
    if input.password.chars().count() < 12 {
        return Err(ApiProblem::validation(
            "password",
            "must contain at least 12 characters",
        ));
    }
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let source_identity = source_identity(source);
    let buckets = [
        rate_limit_bucket(b"invitation-accept-source", source_identity.as_bytes()),
        rate_limit_bucket(b"invitation-accept-token", token.as_bytes()),
    ];
    if !UserRepository::consume_login_attempt(runtime.store.pool(), &buckets)
        .await
        .map_err(|_| ApiProblem::internal())?
    {
        return Err(ApiProblem::invitation_rate_limited());
    }
    let token = Zeroizing::new(token);
    let decoded_token = Zeroizing::new(
        URL_SAFE_NO_PAD
            .decode(token.as_bytes())
            .map_err(|_| ApiProblem::not_found())?,
    );
    let raw_token = Zeroizing::new(
        <[u8; 32]>::try_from(decoded_token.as_slice()).map_err(|_| ApiProblem::not_found())?,
    );
    let token_hash: [u8; 32] = Sha256::digest(*raw_token).into();
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    let identity = sqlx::query_as::<_, (Uuid, Vec<u8>)>(
        "SELECT workspace_id, email_hmac
           FROM invitations
          WHERE token_hash = $1 AND accepted_at IS NULL AND revoked_at IS NULL
            AND expires_at > now()",
    )
    .bind(token_hash.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?
    .ok_or_else(ApiProblem::not_found)?;
    lock_workspace(&mut transaction, identity.0).await?;
    lock_email_identity(&mut transaction, identity.0, &identity.1).await?;
    let invitation = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            Vec<u8>,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            Option<i32>,
            String,
        ),
    >(
        "SELECT id, workspace_id, email_hmac, email_ciphertext, email_nonce,
                email_key_version, workspace_role
           FROM invitations
          WHERE token_hash = $1 AND accepted_at IS NULL AND revoked_at IS NULL
            AND expires_at > now()
          FOR UPDATE",
    )
    .bind(token_hash.as_slice())
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?
    .ok_or_else(ApiProblem::not_found)?;
    let (invitation_id, workspace_id, email_hmac, email_ciphertext, email_nonce, key_version, role) =
        invitation;
    let (email_ciphertext, email_nonce, key_version) =
        complete_envelope(email_ciphertext, email_nonce, key_version)
            .map_err(|_| ApiProblem::not_found())?
            .ok_or_else(ApiProblem::not_found)?;
    let nonce: [u8; 12] = email_nonce.try_into().map_err(|_| ApiProblem::internal())?;
    let key_record = UserRepository::workspace_key(runtime.store.pool(), workspace_id)
        .await
        .map_err(|_| ApiProblem::internal())?
        .ok_or_else(ApiProblem::internal)?;
    let workspace_key = unwrap_workspace_key(&runtime.master_key, &key_record)
        .map_err(|_| ApiProblem::internal())?;
    let content_key = workspace_key
        .derive(KeyPurpose::Content)
        .map_err(|_| ApiProblem::internal())?;
    let invitation_cipher = EnvelopeCipher::new(KeyVersion(key_version as u32), content_key);
    let email_aad = format!("workspace:{workspace_id}:invitation:{invitation_id}:email");
    let decrypted_email = invitation_cipher
        .decrypt(
            email_aad.as_bytes(),
            &EncryptedEnvelope {
                algorithm: "AES-256-GCM".to_owned(),
                key_version: KeyVersion(key_version as u32),
                nonce,
                ciphertext: email_ciphertext,
            },
        )
        .map_err(|_| ApiProblem::internal())?;
    let email =
        std::str::from_utf8(decrypted_email.expose()).map_err(|_| ApiProblem::internal())?;
    let user_id = Uuid::now_v7();
    let user_cipher = EnvelopeCipher::new(
        KeyVersion(1),
        workspace_key
            .derive(KeyPurpose::Content)
            .map_err(|_| ApiProblem::internal())?,
    );
    let encrypted_email = user_cipher
        .encrypt(
            format!("workspace:{workspace_id}:user:{user_id}:email").as_bytes(),
            email.as_bytes(),
        )
        .map_err(|_| ApiProblem::internal())?;
    let encrypted_display_name = user_cipher
        .encrypt(
            format!("workspace:{workspace_id}:user:{user_id}:display-name").as_bytes(),
            input.display_name.trim().as_bytes(),
        )
        .map_err(|_| ApiProblem::internal())?;
    let password_hash = PasswordService::recommended()
        .hash(&input.password)
        .map_err(|_| ApiProblem::internal())?;

    let inserted = sqlx::query(
        "INSERT INTO users (
            id, workspace_id, email_hmac, email_ciphertext, email_nonce, email_key_version,
            display_name_ciphertext, display_name_nonce, display_name_key_version,
            password_hash, status
         ) VALUES ($1, $2, $3, $4, $5, 1, $6, $7, 1, $8, 'active')",
    )
    .bind(user_id)
    .bind(workspace_id)
    .bind(email_hmac)
    .bind(encrypted_email.ciphertext)
    .bind(encrypted_email.nonce.as_slice())
    .bind(encrypted_display_name.ciphertext)
    .bind(encrypted_display_name.nonce.as_slice())
    .bind(password_hash)
    .execute(&mut *transaction)
    .await;
    if let Err(error) = inserted {
        if error
            .as_database_error()
            .is_some_and(|database| database.constraint() == Some("users_active_email_hmac_unique"))
        {
            return Err(ApiProblem::not_found());
        }
        return Err(ApiProblem::internal());
    }
    sqlx::query(
        "INSERT INTO workspace_members (workspace_id, user_id, role)
         VALUES ($1, $2, $3)",
    )
    .bind(workspace_id)
    .bind(user_id)
    .bind(&role)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    sqlx::query("UPDATE invitations SET accepted_at = now() WHERE id = $1")
        .bind(invitation_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ApiProblem::internal())?;
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id,
            actor_user_id: Some(user_id),
            session_id: None,
            event_type: "invitation.accepted",
            target_type: "user",
            target_id: Some(user_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"workspace_role": role}),
        },
    )
    .await
    .map_err(|_| ApiProblem::internal())?;
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;

    Ok((
        StatusCode::CREATED,
        Json(UserCreatedResponse {
            id: user_id,
            workspace_id,
        }),
    )
        .into_response())
}

async fn list_invitations(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            Option<i32>,
            String,
            chrono::DateTime<Utc>,
            Option<chrono::DateTime<Utc>>,
            Option<chrono::DateTime<Utc>>,
        ),
    >(
        "SELECT id, email_ciphertext, email_nonce, email_key_version,
                workspace_role, expires_at, accepted_at, revoked_at
           FROM invitations
          WHERE workspace_id = $1
          ORDER BY created_at DESC, id DESC",
    )
    .bind(current.workspace_id)
    .fetch_all(runtime.store.pool())
    .await
    .map_err(|_| ApiProblem::internal())?;
    let mut invitations = Vec::with_capacity(rows.len());
    for (id, ciphertext, nonce, key_version, workspace_role, expires_at, accepted_at, revoked_at) in
        rows
    {
        let email = decrypt_optional_text(
            &workspace_key,
            &format!("workspace:{}:invitation:{id}:email", current.workspace_id),
            ciphertext,
            nonce,
            key_version,
        )?;
        let status = if accepted_at.is_some() {
            "accepted"
        } else if expires_at <= Utc::now() {
            "expired"
        } else if revoked_at.is_some() {
            "revoked"
        } else {
            "pending"
        };
        invitations.push(InvitationListItem {
            id,
            email,
            workspace_role,
            status: status.to_owned(),
            expires_at,
        });
    }
    let mut response = Json(invitations).into_response();
    prevent_caching(&mut response);
    Ok(response)
}

async fn revoke_invitation(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(invitation_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    let changed = sqlx::query(
        "UPDATE invitations SET revoked_at = now()
          WHERE id = $1 AND workspace_id = $2
            AND accepted_at IS NULL AND revoked_at IS NULL",
    )
    .bind(invitation_id)
    .bind(current.workspace_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?
    .rows_affected();
    if changed != 1 {
        return Err(ApiProblem::not_found());
    }
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "invitation.revoked",
            target_type: "invitation",
            target_id: Some(invitation_id),
            request_id: request_id.as_uuid(),
            metadata: json!({}),
        },
    )
    .await
    .map_err(|_| ApiProblem::internal())?;
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn list_users(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Option<Vec<u8>>,
            Option<Vec<u8>>,
            Option<i32>,
            Vec<u8>,
            Vec<u8>,
            i32,
            String,
            String,
        ),
    >(
        "SELECT u.id, u.email_ciphertext, u.email_nonce, u.email_key_version,
                u.display_name_ciphertext, u.display_name_nonce, u.display_name_key_version,
                wm.role, u.status
           FROM users u
           JOIN workspace_members wm
             ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
          WHERE u.workspace_id = $1 AND u.deleted_at IS NULL
          ORDER BY u.created_at, u.id",
    )
    .bind(current.workspace_id)
    .fetch_all(runtime.store.pool())
    .await
    .map_err(|_| ApiProblem::internal())?;
    let mut users = Vec::with_capacity(rows.len());
    for (
        id,
        email_ciphertext,
        email_nonce,
        email_key_version,
        display_ciphertext,
        display_nonce,
        display_key_version,
        workspace_role,
        status,
    ) in rows
    {
        let email = decrypt_optional_text(
            &workspace_key,
            &format!("workspace:{}:user:{id}:email", current.workspace_id),
            email_ciphertext,
            email_nonce,
            email_key_version,
        )?;
        let display_name = decrypt_text(
            &workspace_key,
            &format!("workspace:{}:user:{id}:display-name", current.workspace_id),
            display_ciphertext,
            display_nonce,
            display_key_version,
        )?;
        users.push(UserListItem {
            id,
            email,
            display_name,
            workspace_role,
            status,
        });
    }
    let mut response = Json(users).into_response();
    prevent_caching(&mut response);
    Ok(response)
}

async fn update_user(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(user_id): Path<Uuid>,
    payload: Result<Json<UpdateUserRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid user changes"))?;
    if input.status.is_none() && input.workspace_role.is_none() {
        return Err(ApiProblem::validation("body", "no changes were supplied"));
    }
    if user_id == current.user_id
        && (matches!(input.status, Some(AccountStatusInput::Suspended))
            || matches!(input.workspace_role, Some(WorkspaceRoleInput::Member)))
    {
        return Err(ApiProblem::conflict(
            "The current administrator cannot suspend or demote itself",
        ));
    }

    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    let target: Option<(String, String)> = sqlx::query_as(
        "SELECT u.status, wm.role
           FROM users u
           JOIN workspace_members wm
             ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
          WHERE u.workspace_id = $1 AND u.id = $2 AND u.deleted_at IS NULL
          FOR UPDATE OF u, wm",
    )
    .bind(current.workspace_id)
    .bind(user_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    let Some((mut status, mut role)) = target else {
        return Err(ApiProblem::not_found());
    };

    let removes_active_admin = status == "active"
        && role == "admin"
        && (matches!(input.status, Some(AccountStatusInput::Suspended))
            || matches!(input.workspace_role, Some(WorkspaceRoleInput::Member)));
    if removes_active_admin {
        let active_admins: i64 = sqlx::query_scalar(
            "SELECT count(*)
               FROM users u
               JOIN workspace_members wm
                 ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
              WHERE u.workspace_id = $1 AND u.status = 'active' AND u.deleted_at IS NULL
                AND wm.status = 'active' AND wm.role = 'admin'",
        )
        .bind(current.workspace_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|_| ApiProblem::internal())?;
        if active_admins <= 1 {
            return Err(ApiProblem::conflict(
                "A workspace must keep at least one active administrator",
            ));
        }
    }

    if let Some(new_status) = input.status {
        UserRepository::set_account_status_and_revoke_sessions(
            &mut transaction,
            current.workspace_id,
            user_id,
            new_status.domain(),
        )
        .await
        .map_err(|_| ApiProblem::internal())?;
        status = new_status.as_str().to_owned();
    }
    if let Some(new_role) = input.workspace_role {
        sqlx::query(
            "UPDATE workspace_members SET role = $3
              WHERE workspace_id = $1 AND user_id = $2",
        )
        .bind(current.workspace_id)
        .bind(user_id)
        .bind(new_role.as_str())
        .execute(&mut *transaction)
        .await
        .map_err(|_| ApiProblem::internal())?;
        role = new_role.as_str().to_owned();
    }

    let event_type = match input.status {
        Some(AccountStatusInput::Suspended) => "user.suspended",
        Some(AccountStatusInput::Active) => "user.reactivated",
        None => "user.role_changed",
    };
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type,
            target_type: "user",
            target_id: Some(user_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"status": status, "workspace_role": role}),
        },
    )
    .await
    .map_err(|_| ApiProblem::internal())?;
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    Ok(Json(json!({
        "id": user_id,
        "status": status,
        "workspaceRole": role
    }))
    .into_response())
}

async fn revoke_user_sessions(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(user_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    if !UserRepository::revoke_all_sessions(&mut transaction, current.workspace_id, user_id)
        .await
        .map_err(|_| ApiProblem::internal())?
    {
        return Err(ApiProblem::not_found());
    }
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "user.sessions_revoked",
            target_type: "user",
            target_id: Some(user_id),
            request_id: request_id.as_uuid(),
            metadata: json!({}),
        },
    )
    .await
    .map_err(|_| ApiProblem::internal())?;
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn load_workspace_key(
    state: &AppState,
    workspace_id: Uuid,
) -> Result<WorkspaceKey, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let record = UserRepository::workspace_key(runtime.store.pool(), workspace_id)
        .await
        .map_err(|_| ApiProblem::internal())?
        .ok_or_else(ApiProblem::internal)?;
    unwrap_workspace_key(&runtime.master_key, &record).map_err(|_| ApiProblem::internal())
}

async fn lock_and_revalidate_admin(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
) -> Result<(), ApiProblem> {
    lock_workspace(transaction, current.workspace_id).await?;
    let still_authorized: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1
              FROM users u
              JOIN workspace_members wm
                ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
              JOIN sessions s
                ON s.workspace_id = u.workspace_id AND s.user_id = u.id
             WHERE u.workspace_id = $1 AND u.id = $2 AND s.id = $3
               AND u.status = 'active' AND u.deleted_at IS NULL
               AND wm.status = 'active' AND wm.role = 'admin'
               AND s.revoked_at IS NULL AND s.expires_at > now()
        )",
    )
    .bind(current.workspace_id)
    .bind(current.user_id)
    .bind(current.session_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    if still_authorized {
        Ok(())
    } else {
        Err(ApiProblem::forbidden())
    }
}

async fn lock_workspace(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
) -> Result<(), ApiProblem> {
    lock_workspace_for_audit(transaction, workspace_id)
        .await
        .map_err(|_| ApiProblem::not_found())
}

async fn lock_email_identity(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    email_hmac: &[u8],
) -> Result<(), ApiProblem> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(
            hashtextextended($1::text || ':' || encode($2::bytea, 'hex'), 0)
        )",
    )
    .bind(workspace_id)
    .bind(email_hmac)
    .execute(&mut **transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    Ok(())
}

fn source_identity(source: Option<std::net::SocketAddr>) -> String {
    source
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "in-process-client".to_owned())
}

fn decrypt_text(
    workspace_key: &WorkspaceKey,
    aad: &str,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    key_version: i32,
) -> Result<String, ApiProblem> {
    let nonce: [u8; 12] = nonce.try_into().map_err(|_| ApiProblem::internal())?;
    let key_version = u32::try_from(key_version).map_err(|_| ApiProblem::internal())?;
    let cipher = EnvelopeCipher::new(
        KeyVersion(key_version),
        workspace_key
            .derive(KeyPurpose::Content)
            .map_err(|_| ApiProblem::internal())?,
    );
    let decrypted = cipher
        .decrypt(
            aad.as_bytes(),
            &EncryptedEnvelope {
                algorithm: "AES-256-GCM".to_owned(),
                key_version: KeyVersion(key_version),
                nonce,
                ciphertext,
            },
        )
        .map_err(|_| ApiProblem::internal())?;
    String::from_utf8(decrypted.expose().to_vec()).map_err(|_| ApiProblem::internal())
}

fn complete_envelope(
    ciphertext: Option<Vec<u8>>,
    nonce: Option<Vec<u8>>,
    key_version: Option<i32>,
) -> Result<Option<EncryptedTextFields>, ()> {
    match (ciphertext, nonce, key_version) {
        (Some(ciphertext), Some(nonce), Some(key_version)) => {
            Ok(Some((ciphertext, nonce, key_version)))
        }
        (None, None, None) => Ok(None),
        _ => Err(()),
    }
}

fn decrypt_optional_text(
    workspace_key: &WorkspaceKey,
    aad: &str,
    ciphertext: Option<Vec<u8>>,
    nonce: Option<Vec<u8>>,
    key_version: Option<i32>,
) -> Result<Option<String>, ApiProblem> {
    complete_envelope(ciphertext, nonce, key_version)
        .map_err(|_| ApiProblem::internal())?
        .map(|(ciphertext, nonce, key_version)| {
            decrypt_text(workspace_key, aad, ciphertext, nonce, key_version)
        })
        .transpose()
}

fn prevent_caching(response: &mut Response) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

fn require_admin(current: CurrentUser) -> Result<(), ApiProblem> {
    if current.workspace_admin {
        Ok(())
    } else {
        Err(ApiProblem::forbidden())
    }
}

fn validate_email(email: &str) -> Result<(), ApiProblem> {
    if email.len() > 320 || !email.contains('@') || email.starts_with('@') || email.ends_with('@') {
        Err(ApiProblem::validation(
            "email",
            "must be a valid email address",
        ))
    } else {
        Ok(())
    }
}
