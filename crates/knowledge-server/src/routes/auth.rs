use axum::{
    Json,
    extract::{ConnectInfo, Extension, FromRequestParts, State, rejection::JsonRejection},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use knowledge_crypto::{EncryptedEnvelope, KeyVersion, Keyring, PasswordService, WorkspaceKey};
use knowledge_store::users::{LoginUser, UserRepository, WorkspaceKeyRecord};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::net::{IpAddr, SocketAddr};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

use crate::{
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    middleware::RequestId,
    problem::ApiProblem,
    state::AppState,
};

const SESSION_COOKIE: &str = "knowledge_session";
const SESSION_SECONDS: i64 = 30 * 24 * 60 * 60;
const CSRF_MESSAGE: &[u8] = b"knowledge-platform/csrf/v1";

api_router! {
    pub(crate) fn router;
    "/api/v1/auth/login" => [post => login => ("login", Public, 200)],
    "/api/v1/auth/logout" => [post => logout => ("logout", Mutation, 204)],
    "/api/v1/auth/session" => [get => session => ("getSession", Session, 200)],
    "/api/v1/auth/csrf" => [get => csrf => ("getCsrf", Session, 200)],
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

pub(crate) struct ClientAddress(pub(crate) Option<SocketAddr>);

impl FromRequestParts<AppState> for ClientAddress {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let address =
            parts
                .extensions
                .get::<ConnectInfo<SocketAddr>>()
                .map(|ConnectInfo(address)| {
                    SocketAddr::new(
                        resolve_client_ip(state, address.ip(), &parts.headers),
                        address.port(),
                    )
                });
        Ok(Self(address))
    }
}

fn resolve_client_ip(state: &AppState, peer_ip: IpAddr, headers: &HeaderMap) -> IpAddr {
    let Some(runtime) = state.runtime() else {
        return peer_ip;
    };
    let mut resolved = peer_ip;
    let Some(forwarded) = headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
    else {
        return resolved;
    };

    for candidate in forwarded
        .split(',')
        .map(str::trim)
        .filter_map(|value| value.parse::<IpAddr>().ok())
        .rev()
    {
        if !runtime
            .trusted_proxy_cidrs
            .iter()
            .any(|network| network.contains(&resolved))
        {
            break;
        }
        resolved = candidate;
    }
    resolved
}

impl Drop for LoginRequest {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUser {
    pub session_id: Uuid,
    pub user_id: Uuid,
    pub workspace_id: Uuid,
    pub workspace_admin: bool,
    #[serde(skip)]
    token_hash: [u8; 32],
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiProblem;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
        let token_hash =
            token_hash_from_headers(&parts.headers).ok_or_else(ApiProblem::unauthorized)?;
        let session = UserRepository::active_session(runtime.store.pool(), &token_hash)
            .await
            .map_err(|_| ApiProblem::internal())?
            .ok_or_else(ApiProblem::unauthorized)?;

        Ok(Self {
            session_id: session.session_id,
            user_id: session.user_id,
            workspace_id: session.workspace_id,
            workspace_admin: session.workspace_admin,
            token_hash,
        })
    }
}

async fn login(
    State(state): State<AppState>,
    ClientAddress(source): ClientAddress,
    headers: HeaderMap,
    Extension(request_id): Extension<RequestId>,
    payload: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) = payload
        .map_err(|_| ApiProblem::validation("body", "expected a valid login JSON object"))?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let normalized_email = input.email.trim().to_lowercase();

    let workspace_record = UserRepository::single_workspace_key(runtime.store.pool())
        .await
        .map_err(|_| ApiProblem::internal())?
        .ok_or_else(ApiProblem::unauthorized)?;
    let workspace_key = unwrap_workspace_key(&runtime.master_key, &workspace_record)
        .map_err(|_| ApiProblem::internal())?;
    let search_key = workspace_key
        .derive_identity()
        .map_err(|_| ApiProblem::internal())?;
    let email_hmac = search_key.authenticate(normalized_email.as_bytes());
    let candidate = UserRepository::active_login_user(
        runtime.store.pool(),
        workspace_record.workspace_id,
        &email_hmac,
    )
    .await
    .map_err(|_| ApiProblem::internal())?;

    let source_identity = source
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "in-process-client".to_owned());
    let source_bucket = rate_limit_bucket(b"login-source", source_identity.as_bytes());
    let account_bucket = rate_limit_bucket(b"login-account", &email_hmac);
    let buckets = [source_bucket, account_bucket];
    let allowed = UserRepository::consume_login_attempt(runtime.store.pool(), &buckets)
        .await
        .map_err(|_| ApiProblem::internal())?;
    if !allowed {
        return Err(ApiProblem::login_rate_limited());
    }

    let valid = verify_login_password(
        candidate.as_ref(),
        &runtime.dummy_password_hash,
        &input.password,
    )
    .map_err(|_| ApiProblem::internal())?;
    let Some(user) = candidate.filter(|_| valid) else {
        return Err(ApiProblem::unauthorized());
    };

    let mut raw_token = Zeroizing::new([0_u8; 32]);
    getrandom::fill(&mut *raw_token).map_err(|_| ApiProblem::internal())?;
    let encoded_token = Zeroizing::new(URL_SAFE_NO_PAD.encode(*raw_token));
    let token_hash: [u8; 32] = Sha256::digest(*raw_token).into();
    let session_id = Uuid::now_v7();
    let expires_at = Utc::now() + Duration::seconds(SESSION_SECONDS);

    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_workspace_for_audit(&mut transaction, user.workspace_id)
        .await
        .map_err(|_| ApiProblem::internal())?;
    // Password hashing is intentionally outside the workspace lock. Bind its
    // result to the exact credential/membership generation before creating a
    // session; reactivation must never resurrect an in-flight old login.
    let still_valid: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM users u JOIN workspace_members wm
           ON wm.workspace_id=u.workspace_id AND wm.user_id=u.id
         WHERE u.workspace_id=$1 AND u.id=$2 AND u.status='active'
           AND u.deleted_at IS NULL AND wm.status='active'
           AND u.auth_generation=$3 AND wm.auth_generation=$4
           AND u.password_hash=$5 AND wm.role=$6)",
    )
    .bind(user.workspace_id)
    .bind(user.user_id)
    .bind(user.auth_generation)
    .bind(user.membership_generation)
    .bind(&user.password_hash)
    .bind(&user.role)
    .fetch_one(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    if !still_valid {
        return Err(ApiProblem::unauthorized());
    }
    if let Some(previous_hash) = token_hash_from_headers(&headers) {
        UserRepository::revoke_by_token_hash(&mut transaction, &previous_hash)
            .await
            .map_err(|_| ApiProblem::internal())?;
    }
    UserRepository::create_session(&mut transaction, session_id, &user, &token_hash, expires_at)
        .await
        .map_err(|_| ApiProblem::internal())?;
    sqlx::query("UPDATE users SET last_login_at = now() WHERE id = $1 AND workspace_id = $2")
        .bind(user.user_id)
        .bind(user.workspace_id)
        .execute(&mut *transaction)
        .await
        .map_err(|_| ApiProblem::internal())?;
    sqlx::query("DELETE FROM login_rate_limits WHERE bucket_key = $1")
        .bind(account_bucket.as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(|_| ApiProblem::internal())?;
    sqlx::query(
        "UPDATE login_rate_limits
            SET failure_count = failure_count - 1, updated_at = now()
          WHERE bucket_key = $1 AND failure_count > 1",
    )
    .bind(source_bucket.as_slice())
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    sqlx::query("DELETE FROM login_rate_limits WHERE bucket_key = $1 AND failure_count = 1")
        .bind(source_bucket.as_slice())
        .execute(&mut *transaction)
        .await
        .map_err(|_| ApiProblem::internal())?;
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id: user.workspace_id,
            actor_user_id: Some(user.user_id),
            session_id: Some(session_id),
            event_type: "auth.login",
            target_type: "user",
            target_id: Some(user.user_id),
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

    let current = CurrentUser {
        session_id,
        user_id: user.user_id,
        workspace_id: user.workspace_id,
        workspace_admin: user.role == "admin",
        token_hash,
    };
    let mut response = Json(current).into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        session_cookie(&encoded_token, SESSION_SECONDS),
    );
    prevent_caching(&mut response);
    Ok(response)
}

async fn session(current: CurrentUser) -> Response {
    let mut response = Json(current).into_response();
    prevent_caching(&mut response);
    response
}

async fn csrf(current: CurrentUser) -> Response {
    let mut response = Json(CsrfResponse {
        token: csrf_token(&current.token_hash),
    })
    .into_response();
    prevent_caching(&mut response);
    response
}

async fn logout(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_record =
        UserRepository::workspace_key(runtime.store.pool(), current.workspace_id)
            .await
            .map_err(|_| ApiProblem::internal())?
            .ok_or_else(ApiProblem::internal)?;
    let workspace_key = unwrap_workspace_key(&runtime.master_key, &workspace_record)
        .map_err(|_| ApiProblem::internal())?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(|_| ApiProblem::internal())?;
    UserRepository::revoke_by_token_hash(&mut transaction, &current.token_hash)
        .await
        .map_err(|_| ApiProblem::internal())?;
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "auth.logout",
            target_type: "user",
            target_id: Some(current.user_id),
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

    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(
        header::SET_COOKIE,
        HeaderValue::from_static(
            "knowledge_session=; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age=0",
        ),
    );
    Ok(response)
}

#[derive(Serialize)]
struct CsrfResponse {
    token: String,
}

fn verify_login_password(
    candidate: Option<&LoginUser>,
    dummy_password_hash: &str,
    supplied_password: &str,
) -> Result<bool, knowledge_crypto::CryptoError> {
    let passwords = PasswordService::recommended();
    let encoded = candidate.map_or(dummy_password_hash, |user| user.password_hash.as_str());
    let verified = passwords.verify(supplied_password, encoded)?;
    Ok(candidate.is_some() && verified)
}

pub(crate) fn rate_limit_bucket(domain: &[u8], identity: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"knowledge-platform/rate-limit/v1\0");
    digest.update(domain);
    digest.update(b"\0");
    digest.update(identity);
    digest.finalize().into()
}

pub(crate) fn unwrap_workspace_key(
    master_key: &knowledge_crypto::MasterKey,
    record: &WorkspaceKeyRecord,
) -> Result<WorkspaceKey, knowledge_crypto::CryptoError> {
    let version = u32::try_from(record.key_version)
        .map_err(|_| knowledge_crypto::CryptoError::InvalidEnvelope)?;
    let current = Keyring::unwrap(
        master_key,
        &EncryptedEnvelope {
            algorithm: "AES-256-GCM".to_owned(),
            key_version: KeyVersion(version),
            nonce: record.nonce,
            ciphertext: record.ciphertext.clone(),
        },
    )?;
    let mut previous = Vec::new();
    for (generation, ciphertext, nonce, master_version) in &record.previous {
        let old = Keyring::unwrap(
            master_key,
            &EncryptedEnvelope {
                algorithm: "AES-256-GCM".into(),
                key_version: KeyVersion(
                    u32::try_from(*master_version)
                        .map_err(|_| knowledge_crypto::CryptoError::InvalidEnvelope)?,
                ),
                nonce: nonce
                    .as_slice()
                    .try_into()
                    .map_err(|_| knowledge_crypto::CryptoError::InvalidEnvelope)?,
                ciphertext: ciphertext.clone(),
            },
        )?;
        previous.push((
            u32::try_from(*generation)
                .map_err(|_| knowledge_crypto::CryptoError::InvalidEnvelope)?,
            *old.expose(),
        ));
    }
    current.with_generation(
        u32::try_from(record.current_version)
            .map_err(|_| knowledge_crypto::CryptoError::InvalidEnvelope)?,
        previous,
    )
}

fn session_cookie(token: &str, max_age: i64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}"
    ))
    .expect("URL-safe base64 token always forms a valid cookie")
}

fn prevent_caching(response: &mut Response) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

fn token_hash_from_headers(headers: &HeaderMap) -> Option<[u8; 32]> {
    let cookies = headers.get(header::COOKIE)?.to_str().ok()?;
    let token = cookies.split(';').find_map(|cookie| {
        let (name, value) = cookie.trim().split_once('=')?;
        (name == SESSION_COOKIE).then_some(value)
    })?;
    let decoded = URL_SAFE_NO_PAD.decode(token).ok()?;
    let raw_token: [u8; 32] = decoded.try_into().ok()?;
    Some(Sha256::digest(raw_token).into())
}

fn csrf_token(token_hash: &[u8; 32]) -> String {
    let mut mac =
        Hmac::<Sha256>::new_from_slice(token_hash).expect("HMAC-SHA-256 accepts a 32-byte key");
    mac.update(CSRF_MESSAGE);
    URL_SAFE_NO_PAD.encode(mac.finalize().into_bytes())
}

pub(crate) fn verify_request_csrf(headers: &HeaderMap) -> Result<(), ApiProblem> {
    let token_hash = token_hash_from_headers(headers).ok_or_else(ApiProblem::csrf)?;
    let supplied = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .ok_or_else(ApiProblem::csrf)?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(&token_hash).expect("HMAC-SHA-256 accepts a 32-byte key");
    mac.update(CSRF_MESSAGE);
    mac.verify_slice(&supplied).map_err(|_| ApiProblem::csrf())
}
