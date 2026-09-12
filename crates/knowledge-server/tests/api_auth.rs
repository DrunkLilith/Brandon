use axum::{
    Router,
    body::{Body, to_bytes},
    extract::ConnectInfo,
    http::{Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use knowledge_core::permissions::AccountStatus;
use knowledge_crypto::{MasterKey, PasswordService};
use knowledge_server::{
    bootstrap::{BootstrapInput, BootstrapService},
    build_router,
    state::AppState,
};
use knowledge_store::{MIGRATOR, Store, users::UserRepository};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tower::ServiceExt;

const PASSWORD: &str = "correct horse battery staple";

fn master_key() -> MasterKey {
    MasterKey::from_base64(&STANDARD.encode([91_u8; 32])).expect("fixture key should decode")
}

async fn app(pool: PgPool) -> (Router, PgPool) {
    app_with_trusted_proxies(pool, "").await
}

async fn app_with_trusted_proxies(pool: PgPool, trusted_proxy_cidrs: &str) -> (Router, PgPool) {
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
    (
        build_router(
            AppState::new(
                store,
                master_key,
                "https://knowledge.local",
                trusted_proxy_cidrs,
            )
            .expect("public origin should be valid"),
        ),
        pool,
    )
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
    login_as(app, " ADMIN@example.com ", password, cookie).await
}

async fn login_as(
    app: &Router,
    email: &str,
    password: &str,
    cookie: Option<&str>,
) -> axum::response::Response {
    login_as_source(app, email, password, cookie, None).await
}

async fn login_as_source(
    app: &Router,
    email: &str,
    password: &str,
    cookie: Option<&str>,
    source: Option<Ipv4Addr>,
) -> axum::response::Response {
    login_as_network(app, email, password, cookie, source, None).await
}

async fn login_as_network(
    app: &Router,
    email: &str,
    password: &str,
    cookie: Option<&str>,
    source: Option<Ipv4Addr>,
    forwarded_for: Option<&str>,
) -> axum::response::Response {
    let mut request = json_request(
        "POST",
        "/api/v1/auth/login",
        json!({"email": email, "password": password}),
    );
    request
        .headers_mut()
        .insert(header::HOST, "knowledge.local".parse().unwrap());
    request
        .headers_mut()
        .insert(header::ORIGIN, "https://knowledge.local".parse().unwrap());
    request
        .headers_mut()
        .insert("sec-fetch-site", "same-origin".parse().unwrap());
    if let Some(source) = source {
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::new(IpAddr::V4(source), 43210)));
    }
    if let Some(forwarded_for) = forwarded_for {
        request.headers_mut().insert(
            "x-forwarded-for",
            forwarded_for
                .parse()
                .expect("forwarded address should parse"),
        );
    }
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
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store"
    );
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
    let forged = logout(&app, &cookie, Some("forged-csrf-token")).await;
    assert_eq!(forged.status(), StatusCode::FORBIDDEN);

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

#[sqlx::test]
async fn login_rejects_cross_origin_browser_requests(pool: PgPool) {
    let (app, _) = app(pool).await;
    let mut request = json_request(
        "POST",
        "/api/v1/auth/login",
        json!({"email": "admin@example.com", "password": PASSWORD}),
    );
    request
        .headers_mut()
        .insert(header::HOST, "knowledge.local".parse().unwrap());
    request
        .headers_mut()
        .insert(header::ORIGIN, "https://attacker.example".parse().unwrap());
    request
        .headers_mut()
        .insert("sec-fetch-site", "cross-site".parse().unwrap());

    let response = app.oneshot(request).await.expect("router should respond");
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[sqlx::test]
async fn login_rejects_cross_scheme_and_missing_origin(pool: PgPool) {
    let (app, _) = app(pool).await;
    let missing = json_request(
        "POST",
        "/api/v1/auth/login",
        json!({"email": "admin@example.com", "password": PASSWORD}),
    );
    assert_eq!(
        app.clone().oneshot(missing).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );

    let mut cross_scheme = json_request(
        "POST",
        "/api/v1/auth/login",
        json!({"email": "admin@example.com", "password": PASSWORD}),
    );
    cross_scheme
        .headers_mut()
        .insert(header::HOST, "knowledge.local".parse().unwrap());
    cross_scheme
        .headers_mut()
        .insert(header::ORIGIN, "http://knowledge.local".parse().unwrap());
    cross_scheme
        .headers_mut()
        .insert("sec-fetch-site", "same-site".parse().unwrap());
    assert_eq!(
        app.oneshot(cross_scheme).await.unwrap().status(),
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test]
async fn login_and_logout_are_audited(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let response = login(&app, PASSWORD, None).await;
    let cookie = session_cookie(&response);
    let csrf = csrf_token(&app, &cookie).await;
    assert_eq!(
        logout(&app, &cookie, Some(&csrf)).await.status(),
        StatusCode::NO_CONTENT
    );

    let event_types: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM audit_events WHERE event_type LIKE 'auth.%' ORDER BY created_at",
    )
    .fetch_all(&pool)
    .await
    .expect("audit events should be readable");
    assert_eq!(event_types, ["auth.login", "auth.logout"]);
}

#[sqlx::test]
async fn failed_login_audit_rolls_back_session_and_last_login(pool: PgPool) {
    let (app, pool) = app(pool).await;
    sqlx::raw_sql(
        "CREATE FUNCTION reject_auth_login_audit() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
           IF NEW.event_type = 'auth.login' THEN
             RAISE EXCEPTION 'forced audit failure';
           END IF;
           RETURN NEW;
         END;
         $$;
         CREATE TRIGGER reject_auth_login_audit
           BEFORE INSERT ON audit_events
           FOR EACH ROW EXECUTE FUNCTION reject_auth_login_audit();",
    )
    .execute(&pool)
    .await
    .expect("failure trigger should install");

    let response = login(&app, PASSWORD, None).await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let session_count: i64 = sqlx::query_scalar("SELECT count(*) FROM sessions")
        .fetch_one(&pool)
        .await
        .unwrap();
    let last_login_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM users WHERE last_login_at IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(session_count, 0);
    assert_eq!(last_login_count, 0);
}

#[sqlx::test]
async fn failed_logout_audit_keeps_session_active(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let response = login(&app, PASSWORD, None).await;
    let cookie = session_cookie(&response);
    let csrf = csrf_token(&app, &cookie).await;
    sqlx::raw_sql(
        "CREATE FUNCTION reject_auth_logout_audit() RETURNS trigger LANGUAGE plpgsql AS $$
         BEGIN
           IF NEW.event_type = 'auth.logout' THEN
             RAISE EXCEPTION 'forced audit failure';
           END IF;
           RETURN NEW;
         END;
         $$;
         CREATE TRIGGER reject_auth_logout_audit
           BEFORE INSERT ON audit_events
           FOR EACH ROW EXECUTE FUNCTION reject_auth_logout_audit();",
    )
    .execute(&pool)
    .await
    .expect("failure trigger should install");

    assert_eq!(
        logout(&app, &cookie, Some(&csrf)).await.status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(session_status(&app, &cookie).await, StatusCode::OK);
}

#[sqlx::test]
async fn malformed_login_json_uses_problem_details(pool: PgPool) {
    let (app, _) = app(pool).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::HOST, "knowledge.local")
                .header(header::ORIGIN, "https://knowledge.local")
                .header("sec-fetch-site", "same-origin")
                .body(Body::from("{"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/problem+json"
    );
}

#[sqlx::test]
async fn login_limit_survives_new_application_state(pool: PgPool) {
    let (first_app, pool) = app(pool).await;
    for _ in 0..5 {
        assert_eq!(
            login(&first_app, "a definitely wrong password", None)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    let restarted_app = build_router(
        AppState::new(
            Store::from_pool(pool),
            master_key(),
            "https://knowledge.local",
            "",
        )
        .unwrap(),
    );
    assert_eq!(
        login(&restarted_app, "a definitely wrong password", None)
            .await
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test]
async fn suspended_session_does_not_resurrect_after_reactivation(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let cookie = session_cookie(&login(&app, PASSWORD, None).await);
    let (workspace_id, user_id): (uuid::Uuid, uuid::Uuid) =
        sqlx::query_as("SELECT workspace_id, id FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();

    let mut transaction = pool.begin().await.unwrap();
    UserRepository::set_account_status_and_revoke_sessions(
        &mut transaction,
        workspace_id,
        user_id,
        AccountStatus::Suspended,
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(
        session_status(&app, &cookie).await,
        StatusCode::UNAUTHORIZED
    );

    let mut transaction = pool.begin().await.unwrap();
    UserRepository::set_account_status_and_revoke_sessions(
        &mut transaction,
        workspace_id,
        user_id,
        AccountStatus::Active,
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(
        session_status(&app, &cookie).await,
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn changing_password_revokes_old_sessions(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let cookie = session_cookie(&login(&app, PASSWORD, None).await);
    let (workspace_id, user_id): (uuid::Uuid, uuid::Uuid) =
        sqlx::query_as("SELECT workspace_id, id FROM users")
            .fetch_one(&pool)
            .await
            .unwrap();
    let new_password = "a newer correct horse battery staple";
    let password_hash = PasswordService::recommended().hash(new_password).unwrap();

    let mut transaction = pool.begin().await.unwrap();
    UserRepository::change_password_and_revoke_sessions(
        &mut transaction,
        workspace_id,
        user_id,
        &password_hash,
    )
    .await
    .unwrap();
    transaction.commit().await.unwrap();

    assert_eq!(
        session_status(&app, &cookie).await,
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, PASSWORD, None).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        login(&app, new_password, None).await.status(),
        StatusCode::OK
    );
}

#[sqlx::test]
async fn rotating_unknown_emails_cannot_bypass_source_limit(pool: PgPool) {
    let (app, _) = app(pool).await;
    for attempt in 0..5 {
        assert_eq!(
            login_as(
                &app,
                &format!("unknown-{attempt}@example.com"),
                "a definitely wrong password",
                None,
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        login_as(
            &app,
            "one-more-unknown@example.com",
            "a definitely wrong password",
            None,
        )
        .await
        .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[sqlx::test]
async fn concurrent_failed_logins_cannot_overshoot_limit(pool: PgPool) {
    let (app, _) = app(pool).await;
    let mut attempts = tokio::task::JoinSet::new();
    for _ in 0..6 {
        let app = app.clone();
        attempts.spawn(async move {
            login(&app, "a definitely wrong password", None)
                .await
                .status()
        });
    }

    let mut unauthorized = 0;
    let mut limited = 0;
    while let Some(result) = attempts.join_next().await {
        match result.unwrap() {
            StatusCode::UNAUTHORIZED => unauthorized += 1,
            StatusCode::TOO_MANY_REQUESTS => limited += 1,
            status => panic!("unexpected login status: {status}"),
        }
    }
    assert_eq!(unauthorized, 5);
    assert_eq!(limited, 1);
}

#[sqlx::test]
async fn known_and_unknown_accounts_have_identical_distributed_limits(pool: PgPool) {
    let (app, _) = app(pool).await;
    for octet in 1..=5 {
        let source = Ipv4Addr::new(192, 0, 2, octet);
        assert_eq!(
            login_as_source(
                &app,
                "admin@example.com",
                "a definitely wrong password",
                None,
                Some(source),
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            login_as_source(
                &app,
                "unknown@example.com",
                "a definitely wrong password",
                None,
                Some(source),
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    for email in ["admin@example.com", "unknown@example.com"] {
        assert_eq!(
            login_as_source(
                &app,
                email,
                "a definitely wrong password",
                None,
                Some(Ipv4Addr::new(192, 0, 2, 99)),
            )
            .await
            .status(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }
}

#[sqlx::test]
async fn concurrent_logins_form_one_linear_audit_chain(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let mut logins = tokio::task::JoinSet::new();
    for octet in 1..=5 {
        let app = app.clone();
        logins.spawn(async move {
            login_as_source(
                &app,
                "admin@example.com",
                PASSWORD,
                None,
                Some(Ipv4Addr::new(198, 51, 100, octet)),
            )
            .await
            .status()
        });
    }
    while let Some(result) = logins.join_next().await {
        assert_eq!(result.unwrap(), StatusCode::OK);
    }

    let (events, roots, successors): (i64, i64, i64) = sqlx::query_as(
        "SELECT
            count(*),
            count(*) FILTER (WHERE previous_hash IS NULL),
            count(DISTINCT previous_hash) FILTER (WHERE previous_hash IS NOT NULL)
           FROM audit_events",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((events, roots, successors), (6, 1, 5));
}

#[sqlx::test]
async fn bootstrap_and_auth_events_share_one_hash_algorithm(pool: PgPool) {
    let (app, pool) = app(pool).await;
    assert_eq!(login(&app, PASSWORD, None).await.status(), StatusCode::OK);
    let events = sqlx::query_as::<
        _,
        (
            uuid::Uuid,
            uuid::Uuid,
            Option<uuid::Uuid>,
            Option<uuid::Uuid>,
            String,
            String,
            Option<uuid::Uuid>,
            uuid::Uuid,
            Vec<u8>,
            Option<Vec<u8>>,
            Vec<u8>,
        ),
    >(
        "SELECT id, workspace_id, actor_user_id, session_id, event_type,
                target_type, target_id, request_id, metadata_ciphertext,
                previous_hash, event_hash
           FROM audit_events
          ORDER BY created_at, id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let mut expected_previous: Option<[u8; 32]> = None;
    for (
        id,
        workspace_id,
        actor_user_id,
        session_id,
        event_type,
        target_type,
        target_id,
        request_id,
        metadata_ciphertext,
        previous_hash,
        stored_hash,
    ) in events
    {
        assert_eq!(
            previous_hash.as_deref(),
            expected_previous.as_ref().map(<[u8; 32]>::as_slice)
        );
        let mut digest = Sha256::new();
        digest.update(b"knowledge-platform/audit-event/v1\0");
        if let Some(previous) = expected_previous {
            digest.update(previous);
        }
        digest.update(id.as_bytes());
        digest.update(workspace_id.as_bytes());
        if let Some(actor_user_id) = actor_user_id {
            digest.update(actor_user_id.as_bytes());
        }
        if let Some(session_id) = session_id {
            digest.update(session_id.as_bytes());
        }
        digest.update(event_type.as_bytes());
        digest.update([0]);
        digest.update(target_type.as_bytes());
        if let Some(target_id) = target_id {
            digest.update(target_id.as_bytes());
        }
        digest.update(request_id.as_bytes());
        digest.update(metadata_ciphertext);
        let computed: [u8; 32] = digest.finalize().into();
        assert_eq!(stored_hash, computed);
        expected_previous = Some(computed);
    }
}

#[sqlx::test]
async fn database_rejects_a_second_audit_root_or_successor(pool: PgPool) {
    let (app, pool) = app(pool).await;
    assert_eq!(login(&app, PASSWORD, None).await.status(), StatusCode::OK);
    let workspace_id: uuid::Uuid = sqlx::query_scalar("SELECT id FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    let root_hash: Vec<u8> =
        sqlx::query_scalar("SELECT event_hash FROM audit_events WHERE previous_hash IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();

    for (kind, previous_hash) in [("root", None), ("successor", Some(root_hash))] {
        let result = sqlx::query(
            "INSERT INTO audit_events (
                id, workspace_id, event_type, target_type, request_id,
                metadata_ciphertext, metadata_nonce, metadata_key_version,
                previous_hash, event_hash
             ) VALUES ($1, $2, 'test.invalid', 'workspace', $3, $4, $5, 1, $6, $7)",
        )
        .bind(uuid::Uuid::now_v7())
        .bind(workspace_id)
        .bind(uuid::Uuid::now_v7())
        .bind(Vec::<u8>::new())
        .bind([0_u8; 12].as_slice())
        .bind(previous_hash)
        .bind([7_u8; 32].as_slice())
        .execute(&pool)
        .await;
        assert!(result.is_err(), "duplicate {kind} unexpectedly succeeded");
    }
}

#[sqlx::test]
async fn trusted_proxy_forwarding_keeps_client_source_buckets_separate(pool: PgPool) {
    let (app, _) = app_with_trusted_proxies(pool, "10.0.0.1/32").await;
    let proxy = Some(Ipv4Addr::new(10, 0, 0, 1));
    for attempt in 0..5 {
        assert_eq!(
            login_as_network(
                &app,
                &format!("first-client-{attempt}@example.com"),
                "a definitely wrong password",
                None,
                proxy,
                Some("203.0.113.10"),
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    assert_eq!(
        login_as_network(
            &app,
            "second-client@example.com",
            "a definitely wrong password",
            None,
            proxy,
            Some("203.0.113.11"),
        )
        .await
        .status(),
        StatusCode::UNAUTHORIZED
    );
}

#[sqlx::test]
async fn direct_clients_cannot_spoof_forwarded_source(pool: PgPool) {
    let (app, _) = app_with_trusted_proxies(pool, "10.0.0.1/32").await;
    let direct_peer = Some(Ipv4Addr::new(198, 51, 100, 40));
    for attempt in 0..5 {
        assert_eq!(
            login_as_network(
                &app,
                &format!("spoof-attempt-{attempt}@example.com"),
                "a definitely wrong password",
                None,
                direct_peer,
                Some(&format!("203.0.113.{attempt}")),
            )
            .await
            .status(),
            StatusCode::UNAUTHORIZED
        );
    }

    assert_eq!(
        login_as_network(
            &app,
            "spoof-attempt-last@example.com",
            "a definitely wrong password",
            None,
            direct_peer,
            Some("203.0.113.99"),
        )
        .await
        .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
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
        .header(header::HOST, "knowledge.local")
        .header(header::ORIGIN, "https://knowledge.local")
        .header("sec-fetch-site", "same-origin")
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
