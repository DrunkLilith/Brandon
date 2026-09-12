use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use knowledge_crypto::MasterKey;
use knowledge_server::{
    bootstrap::{BootstrapInput, BootstrapService},
    build_router,
    state::AppState,
};
use knowledge_store::{MIGRATOR, Store};
use serde_json::{Value, json};
use sqlx::{PgPool, Row};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery staple";

fn master_key() -> MasterKey {
    MasterKey::from_base64(&STANDARD.encode([91_u8; 32])).expect("fixture key should decode")
}

async fn app(pool: PgPool) -> (Router, PgPool) {
    MIGRATOR
        .run(&pool)
        .await
        .expect("test schema should migrate");
    let store = Store::from_pool(pool.clone());
    let master_key = master_key();
    BootstrapService::run(
        &store,
        &master_key,
        BootstrapInput {
            workspace_name: "Командная база".to_owned(),
            email: "admin@example.com".to_owned(),
            display_name: "Главный администратор".to_owned(),
            password: PASSWORD.to_owned(),
        },
    )
    .await
    .expect("bootstrap should succeed");
    (build_router(AppState::new(store, master_key)), pool)
}

fn json_request(method: &str, uri: &str, value: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(value.to_string()))
        .expect("request should build")
}

async fn login(app: &Router, password: &str, cookie: Option<&str>) -> axum::response::Response {
    let mut request = json_request(
        "POST",
        "/api/v1/auth/login",
        json!({"email": " ADMIN@example.com ", "password": password}),
    );
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().expect("cookie should parse"));
    }
    app.clone()
        .oneshot(request)
        .await
        .expect("router should respond")
}

fn session_cookie(response: &axum::response::Response) -> String {
    response
        .headers()
        .get(header::SET_COOKIE)
        .expect("session cookie should be present")
        .to_str()
        .expect("cookie should be text")
        .split(';')
        .next()
        .expect("cookie should have a value")
        .to_owned()
}

async fn csrf_token(app: &Router, cookie: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond");
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body should be readable");
    serde_json::from_slice::<Value>(&bytes).expect("body should be JSON")["token"]
        .as_str()
        .expect("token should be a string")
        .to_owned()
}

#[sqlx::test]
async fn login_sets_hardened_cookie_and_stores_only_token_hash(pool: PgPool) {
    let (app, pool) = app(pool).await;

    let response = login(&app, PASSWORD, None).await;

    assert_eq!(response.status(), StatusCode::OK);
    let set_cookie = response
        .headers()
        .get(header::SET_COOKIE)
        .expect("cookie should be set")
        .to_str()
        .expect("cookie should be text");
    assert!(set_cookie.contains("HttpOnly"));
    assert!(set_cookie.contains("Secure"));
    assert!(set_cookie.contains("SameSite=Strict"));

    let raw_token = session_cookie(&response)
        .split_once('=')
        .expect("cookie should contain equals")
        .1
        .to_owned();
    let stored_hash = sqlx::query("SELECT token_hash FROM sessions")
        .fetch_one(&pool)
        .await
        .expect("session should exist")
        .get::<Vec<u8>, _>("token_hash");
    assert_eq!(stored_hash.len(), 32);
    assert!(
        !stored_hash
            .windows(raw_token.len())
            .any(|window| window == raw_token.as_bytes())
    );
}

#[sqlx::test]
async fn login_rotates_existing_session(pool: PgPool) {
    let (app, _) = app(pool).await;
    let first = login(&app, PASSWORD, None).await;
    let first_cookie = session_cookie(&first);

    let second = login(&app, PASSWORD, Some(&first_cookie)).await;
    let second_cookie = session_cookie(&second);

    assert_eq!(second.status(), StatusCode::OK);
    assert_ne!(first_cookie, second_cookie);
    assert_eq!(
        session_status(&app, &first_cookie).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(session_status(&app, &second_cookie).await, StatusCode::OK);
}

#[sqlx::test]
async fn csrf_is_required_and_logout_revokes_session(pool: PgPool) {
    let (app, _) = app(pool).await;
    let response = login(&app, PASSWORD, None).await;
    let cookie = session_cookie(&response);

    let missing = logout(&app, &cookie, None).await;
    assert_eq!(missing.status(), StatusCode::FORBIDDEN);
    assert_eq!(
        missing.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );

    let csrf = csrf_token(&app, &cookie).await;
    let logged_out = logout(&app, &cookie, Some(&csrf)).await;
    assert_eq!(logged_out.status(), StatusCode::NO_CONTENT);
    assert!(
        logged_out
            .headers()
            .get(header::SET_COOKIE)
            .unwrap()
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    assert_eq!(
        session_status(&app, &cookie).await,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn expired_and_suspended_sessions_are_rejected(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let expired_cookie = session_cookie(&login(&app, PASSWORD, None).await);
    sqlx::query("UPDATE sessions SET expires_at = now() - interval '1 second'")
        .execute(&pool)
        .await
        .expect("session should expire");
    assert_eq!(
        session_status(&app, &expired_cookie).await,
        StatusCode::UNAUTHORIZED
    );

    let active_cookie = session_cookie(&login(&app, PASSWORD, None).await);
    sqlx::query("UPDATE users SET status = 'suspended'")
        .execute(&pool)
        .await
        .expect("user should suspend");
    assert_eq!(
        session_status(&app, &active_cookie).await,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn repeated_failed_logins_are_rate_limited_without_enumeration(pool: PgPool) {
    let (app, _) = app(pool).await;

    for _ in 0..5 {
        let response = login(&app, "a definitely wrong password", None).await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }
    let limited = login(&app, "a definitely wrong password", None).await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);

    let bytes = to_bytes(limited.into_body(), usize::MAX)
        .await
        .expect("body should be readable");
    let problem: Value = serde_json::from_slice(&bytes).expect("body should be JSON");
    assert_eq!(problem["code"], "login_rate_limited");
}

async fn session_status(app: &Router, cookie: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/session")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .expect("request should build"),
        )
        .await
        .expect("router should respond")
        .status()
}

async fn logout(app: &Router, cookie: &str, csrf: Option<&str>) -> axum::response::Response {
    let mut request = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/logout")
        .header(header::COOKIE, cookie)
        .body(Body::empty())
        .expect("request should build");
    if let Some(csrf) = csrf {
        request.headers_mut().insert(
            "x-csrf-token",
            csrf.parse().expect("CSRF token should parse"),
        );
    }
    app.clone()
        .oneshot(request)
        .await
        .expect("router should respond")
}
