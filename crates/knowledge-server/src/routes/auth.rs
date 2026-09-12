use axum::{
    Json, Router,
    extract::{FromRequestParts, State},
    http::{HeaderMap, HeaderValue, StatusCode, header, request::Parts},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration, Utc};
use hmac::{Hmac, KeyInit, Mac};
use knowledge_crypto::{
    EncryptedEnvelope, KeyPurpose, KeyVersion, Keyring, PasswordService, WorkspaceKey,
};
use knowledge_store::users::{LoginUser, UserRepository, WorkspaceKeyRecord};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::{problem::ApiProblem, state::AppState};

const SESSION_COOKIE: &str = "knowledge_session";
const SESSION_SECONDS: i64 = 30 * 24 * 60 * 60;
const CSRF_MESSAGE: &[u8] = b"knowledge-platform/csrf/v1";

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/session", get(session))
        .route("/api/v1/auth/csrf", get(csrf))
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
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
    headers: HeaderMap,
    Json(input): Json<LoginRequest>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let normalized_email = input.email.trim().to_lowercase();
    let limiter_key: [u8; 32] = Sha256::digest(normalized_email.as_bytes()).into();
    if state.login_is_limited(&limiter_key) {
        return Err(ApiProblem::login_rate_limited());
    }

    let workspace_record = UserRepository::single_workspace_key(runtime.store.pool())
        .await
        .map_err(|_| ApiProblem::internal())?
        .ok_or_else(ApiProblem::unauthorized)?;
    let workspace_key = unwrap_workspace_key(&runtime.master_key, &workspace_record)
        .map_err(|_| ApiProblem::internal())?;
    let search_key = workspace_key
        .derive(KeyPurpose::Search)
        .map_err(|_| ApiProblem::internal())?;
    let email_hmac = search_key.authenticate(normalized_email.as_bytes());
    let candidate = UserRepository::active_login_user(
        runtime.store.pool(),
        workspace_record.workspace_id,
        &email_hmac,
    )
    .await
    .map_err(|_| ApiProblem::internal())?;

    let valid = verify_login_password(candidate.as_ref(), &input.password)
        .map_err(|_| ApiProblem::internal())?;
    let Some(user) = candidate.filter(|_| valid) else {
        state.record_login_failure(limiter_key);
        return Err(ApiProblem::unauthorized());
    };

    let mut raw_token = [0_u8; 32];
    getrandom::fill(&mut raw_token).map_err(|_| ApiProblem::internal())?;
    let encoded_token = URL_SAFE_NO_PAD.encode(raw_token);
    let token_hash: [u8; 32] = Sha256::digest(raw_token).into();
    let session_id = Uuid::now_v7();
    let expires_at = Utc::now() + Duration::seconds(SESSION_SECONDS);

    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
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
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    state.clear_login_failures(&limiter_key);

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
    Ok(response)
}

async fn session(current: CurrentUser) -> Json<CurrentUser> {
    Json(current)
}

async fn csrf(current: CurrentUser) -> Json<CsrfResponse> {
    Json(CsrfResponse {
        token: csrf_token(&current.token_hash),
    })
}

async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    verify_csrf(&headers, &current.token_hash)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    UserRepository::revoke_by_token_hash(&mut transaction, &current.token_hash)
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
    supplied_password: &str,
) -> Result<bool, knowledge_crypto::CryptoError> {
    let passwords = PasswordService::recommended();
    match candidate {
        Some(user) => passwords.verify(supplied_password, &user.password_hash),
        None => {
            let dummy_hash = passwords.hash("an intentionally invalid password")?;
            let _ = passwords.verify(supplied_password, &dummy_hash)?;
            Ok(false)
        }
    }
}

fn unwrap_workspace_key(
    master_key: &knowledge_crypto::MasterKey,
    record: &WorkspaceKeyRecord,
) -> Result<WorkspaceKey, knowledge_crypto::CryptoError> {
    let version = u32::try_from(record.key_version)
        .map_err(|_| knowledge_crypto::CryptoError::InvalidEnvelope)?;
    Keyring::unwrap(
        master_key,
        &EncryptedEnvelope {
            algorithm: "AES-256-GCM".to_owned(),
            key_version: KeyVersion(version),
            nonce: record.nonce,
            ciphertext: record.ciphertext.clone(),
        },
    )
}

fn session_cookie(token: &str, max_age: i64) -> HeaderValue {
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={token}; Path=/; HttpOnly; Secure; SameSite=Strict; Max-Age={max_age}"
    ))
    .expect("URL-safe base64 token always forms a valid cookie")
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

fn verify_csrf(headers: &HeaderMap, token_hash: &[u8; 32]) -> Result<(), ApiProblem> {
    let supplied = headers
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .ok_or_else(ApiProblem::csrf)?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(token_hash).expect("HMAC-SHA-256 accepts a 32-byte key");
    mac.update(CSRF_MESSAGE);
    mac.verify_slice(&supplied).map_err(|_| ApiProblem::csrf())
}
