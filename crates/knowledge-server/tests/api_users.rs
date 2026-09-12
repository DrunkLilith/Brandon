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
use sqlx::PgPool;
use tower::ServiceExt;

const ADMIN_PASSWORD: &str = "correct horse battery staple";
const ORIGIN: &str = "https://knowledge.local";

fn master_key() -> MasterKey {
    MasterKey::from_base64(&STANDARD.encode([91_u8; 32])).unwrap()
}

async fn app(pool: PgPool) -> (Router, PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool.clone());
    let master_key = master_key();
    BootstrapService::run(
        &store,
        &master_key,
        BootstrapInput {
            workspace_name: "Командная база".to_owned(),
            email: "admin@example.com".to_owned(),
            display_name: "Главный администратор".to_owned(),
            password: ADMIN_PASSWORD.to_owned(),
        },
    )
    .await
    .unwrap();
    (
        build_router(AppState::new(store, master_key, ORIGIN, "").unwrap()),
        pool,
    )
}

fn request(method: &str, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::HOST, "knowledge.local")
        .header(header::ORIGIN, ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    cookie: Option<&str>,
    csrf: Option<&str>,
) -> axum::response::Response {
    let mut request = request(method, uri, body);
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert(header::COOKIE, cookie.parse().unwrap());
    }
    if let Some(csrf) = csrf {
        request
            .headers_mut()
            .insert("x-csrf-token", csrf.parse().unwrap());
    }
    app.clone().oneshot(request).await.unwrap()
}

async fn json_body(response: axum::response::Response) -> Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

async fn wait_for_locked_query(pool: &PgPool, query_fragment: &str) {
    let pattern = format!("%{query_fragment}%");
    for _ in 0..200 {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM pg_stat_activity
                 WHERE datname = current_database()
                   AND pid <> pg_backend_pid()
                   AND wait_event_type = 'Lock'
                   AND query LIKE $1
            )",
        )
        .bind(&pattern)
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("query did not reach the expected lock wait: {query_fragment}");
}

async fn login(app: &Router, email: &str, password: &str) -> String {
    let response = send(
        app,
        "POST",
        "/api/v1/auth/login",
        json!({"email": email, "password": password}),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}

async fn csrf(app: &Router, cookie: &str) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    json_body(response).await["token"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[sqlx::test]
async fn invitation_is_returned_once_and_creates_an_active_user(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let admin_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let admin_csrf = csrf(&app, &admin_cookie).await;

    let created = send(
        &app,
        "POST",
        "/api/v1/invitations",
        json!({"email": " New.Member@Example.com ", "workspaceRole": "member"}),
        Some(&admin_cookie),
        Some(&admin_csrf),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let invitation = json_body(created).await;
    let token = invitation["token"].as_str().unwrap();

    let (stored_token_hash, stored_email_hmac, stored_email_ciphertext): (
        Vec<u8>,
        Vec<u8>,
        Vec<u8>,
    ) = sqlx::query_as("SELECT token_hash, email_hmac, email_ciphertext FROM invitations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_token_hash.len(), 32);
    assert_eq!(stored_email_hmac.len(), 32);
    assert!(
        !stored_token_hash
            .windows(token.len())
            .any(|window| window == token.as_bytes())
    );
    assert!(
        !stored_email_ciphertext
            .windows("new.member@example.com".len())
            .any(|window| window == b"new.member@example.com")
    );

    let accepted = send(
        &app,
        "POST",
        &format!("/api/v1/invitations/{token}/accept"),
        json!({
            "displayName": "Новый участник",
            "password": "another correct horse battery staple"
        }),
        None,
        None,
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::CREATED);

    let reused = send(
        &app,
        "POST",
        &format!("/api/v1/invitations/{token}/accept"),
        json!({
            "displayName": "Повтор",
            "password": "another correct horse battery staple"
        }),
        None,
        None,
    )
    .await;
    assert_eq!(reused.status(), StatusCode::NOT_FOUND);
    let member_cookie = login(
        &app,
        "new.member@example.com",
        "another correct horse battery staple",
    )
    .await;
    assert!(!member_cookie.is_empty());

    let event_types: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM audit_events WHERE event_type LIKE 'invitation.%' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(event_types, ["invitation.created", "invitation.accepted"]);
}

async fn create_invitation(
    app: &Router,
    cookie: &str,
    csrf: &str,
    email: &str,
) -> (StatusCode, Value) {
    let response = send(
        app,
        "POST",
        "/api/v1/invitations",
        json!({"email": email, "workspaceRole": "member"}),
        Some(cookie),
        Some(csrf),
    )
    .await;
    let status = response.status();
    (status, json_body(response).await)
}

async fn accept_invitation(app: &Router, token: &str, display_name: &str) -> StatusCode {
    send(
        app,
        "POST",
        &format!("/api/v1/invitations/{token}/accept"),
        json!({
            "displayName": display_name,
            "password": "another correct horse battery staple"
        }),
        None,
        None,
    )
    .await
    .status()
}

#[sqlx::test]
async fn invitation_duplicate_expiry_revocation_and_authorization_are_enforced(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let admin_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let admin_csrf = csrf(&app, &admin_cookie).await;
    let (status, first) =
        create_invitation(&app, &admin_cookie, &admin_csrf, "member@example.com").await;
    assert_eq!(status, StatusCode::CREATED);
    let first_token = first["token"].as_str().unwrap();

    assert_eq!(
        create_invitation(&app, &admin_cookie, &admin_csrf, "MEMBER@example.com")
            .await
            .0,
        StatusCode::CONFLICT
    );
    sqlx::query("UPDATE invitations SET expires_at = now() - interval '1 second' WHERE id = $1")
        .bind(first["id"].as_str().unwrap().parse::<uuid::Uuid>().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        accept_invitation(&app, first_token, "Expired member").await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        create_invitation(&app, &admin_cookie, &admin_csrf, "member@example.com")
            .await
            .0,
        StatusCode::CREATED
    );
    let expiry_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type = 'invitation.expired'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(expiry_events, 1);

    let (_, revoked) =
        create_invitation(&app, &admin_cookie, &admin_csrf, "revoked@example.com").await;
    let revoked_id = revoked["id"].as_str().unwrap();
    let revoked_token = revoked["token"].as_str().unwrap();
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/invitations/{revoked_id}"),
            json!({}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        accept_invitation(&app, revoked_token, "Revoked member").await,
        StatusCode::NOT_FOUND
    );

    let (_, accepted) = create_invitation(
        &app,
        &admin_cookie,
        &admin_csrf,
        "active.member@example.com",
    )
    .await;
    assert_eq!(
        accept_invitation(&app, accepted["token"].as_str().unwrap(), "Active member").await,
        StatusCode::CREATED
    );
    let member_cookie = login(
        &app,
        "active.member@example.com",
        "another correct horse battery staple",
    )
    .await;
    let member_csrf = csrf(&app, &member_cookie).await;
    assert_eq!(
        create_invitation(&app, &member_cookie, &member_csrf, "forbidden@example.com")
            .await
            .0,
        StatusCode::FORBIDDEN
    );

    let listed = send(
        &app,
        "GET",
        "/api/v1/invitations",
        json!({}),
        Some(&admin_cookie),
        None,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    let listed = json_body(listed).await;
    assert!(
        listed
            .as_array()
            .unwrap()
            .iter()
            .any(|item| { item["email"] == "revoked@example.com" && item.get("token").is_none() })
    );
}

#[sqlx::test]
async fn administrators_can_list_suspend_reactivate_and_revoke_sessions(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let admin_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let admin_csrf = csrf(&app, &admin_cookie).await;
    let (_, invitation) =
        create_invitation(&app, &admin_cookie, &admin_csrf, "member@example.com").await;
    assert_eq!(
        accept_invitation(&app, invitation["token"].as_str().unwrap(), "Участник").await,
        StatusCode::CREATED
    );
    let member_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM users WHERE email_hmac <> (SELECT email_hmac FROM users ORDER BY created_at LIMIT 1)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let member_cookie = login(
        &app,
        "member@example.com",
        "another correct horse battery staple",
    )
    .await;

    let users = send(
        &app,
        "GET",
        "/api/v1/users",
        json!({}),
        Some(&admin_cookie),
        None,
    )
    .await;
    assert_eq!(users.status(), StatusCode::OK);
    let users = json_body(users).await;
    assert!(users.as_array().unwrap().iter().any(|user| {
        user["email"] == "member@example.com"
            && user["displayName"] == "Участник"
            && user["workspaceRole"] == "member"
    }));
    assert_eq!(
        send(
            &app,
            "GET",
            "/api/v1/users",
            json!({}),
            Some(&member_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/users/{member_id}"),
            json!({"status": "suspended"}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await
        .status(),
        StatusCode::OK
    );
    let member_session = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/session")
                .header(header::COOKIE, &member_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(member_session.status(), StatusCode::UNAUTHORIZED);

    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/users/{member_id}"),
            json!({"status": "active"}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await
        .status(),
        StatusCode::OK
    );
    let fresh_member_cookie = login(
        &app,
        "member@example.com",
        "another correct horse battery staple",
    )
    .await;
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/users/{member_id}/revoke-sessions"),
            json!({}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let member_session = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/session")
                .header(header::COOKIE, fresh_member_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(member_session.status(), StatusCode::UNAUTHORIZED);

    let user_events: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM audit_events WHERE event_type LIKE 'user.%' ORDER BY id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        user_events,
        [
            "user.suspended",
            "user.reactivated",
            "user.sessions_revoked"
        ]
    );
}

#[sqlx::test]
async fn legacy_rows_without_an_email_envelope_are_listed_as_redacted(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let admin_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let admin_csrf = csrf(&app, &admin_cookie).await;
    let (_, invitation) =
        create_invitation(&app, &admin_cookie, &admin_csrf, "legacy@example.com").await;

    sqlx::query(
        "UPDATE users
            SET email_ciphertext = NULL, email_nonce = NULL, email_key_version = NULL",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "UPDATE invitations
            SET email_ciphertext = NULL, email_nonce = NULL, email_key_version = NULL
          WHERE id = $1",
    )
    .bind(
        invitation["id"]
            .as_str()
            .unwrap()
            .parse::<uuid::Uuid>()
            .unwrap(),
    )
    .execute(&pool)
    .await
    .unwrap();

    let users = send(
        &app,
        "GET",
        "/api/v1/users",
        json!({}),
        Some(&admin_cookie),
        None,
    )
    .await;
    assert_eq!(users.status(), StatusCode::OK);
    assert_eq!(json_body(users).await[0]["email"], Value::Null);

    let invitations = send(
        &app,
        "GET",
        "/api/v1/invitations",
        json!({}),
        Some(&admin_cookie),
        None,
    )
    .await;
    assert_eq!(invitations.status(), StatusCode::OK);
    assert_eq!(json_body(invitations).await[0]["email"], Value::Null);
    assert_eq!(
        accept_invitation(&app, invitation["token"].as_str().unwrap(), "Legacy member").await,
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn invitation_creation_and_acceptance_are_serialized_per_email(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let admin_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let admin_csrf = csrf(&app, &admin_cookie).await;
    let (_, invitation) =
        create_invitation(&app, &admin_cookie, &admin_csrf, "concurrent@example.com").await;
    let token = invitation["token"].as_str().unwrap().to_owned();

    let create = create_invitation(&app, &admin_cookie, &admin_csrf, "concurrent@example.com");
    let accept = accept_invitation(&app, &token, "Concurrent member");
    let ((create_status, _), accept_status) = tokio::join!(create, accept);
    assert_eq!(create_status, StatusCode::CONFLICT);
    assert_eq!(accept_status, StatusCode::CREATED);

    let user_count: i64 = sqlx::query_scalar("SELECT count(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    let pending_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM invitations
          WHERE accepted_at IS NULL AND revoked_at IS NULL AND expires_at > now()",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(user_count, 2);
    assert_eq!(pending_count, 0);
}

#[sqlx::test]
async fn concurrent_administrators_cannot_remove_every_active_admin(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let first_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let first_csrf = csrf(&app, &first_cookie).await;
    let created = send(
        &app,
        "POST",
        "/api/v1/invitations",
        json!({"email": "second.admin@example.com", "workspaceRole": "admin"}),
        Some(&first_cookie),
        Some(&first_csrf),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let invitation = json_body(created).await;
    let accepted = send(
        &app,
        "POST",
        &format!(
            "/api/v1/invitations/{}/accept",
            invitation["token"].as_str().unwrap()
        ),
        json!({
            "displayName": "Second admin",
            "password": "another correct horse battery staple"
        }),
        None,
        None,
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::CREATED);
    let second_id: uuid::Uuid = json_body(accepted).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let first_id: uuid::Uuid = sqlx::query_scalar(
        "SELECT u.id
           FROM users u
           JOIN workspace_members wm ON wm.user_id = u.id AND wm.workspace_id = u.workspace_id
          WHERE wm.role = 'admin' AND u.id <> $1",
    )
    .bind(second_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let second_cookie = login(
        &app,
        "second.admin@example.com",
        "another correct horse battery staple",
    )
    .await;
    let second_csrf = csrf(&app, &second_cookie).await;

    let second_user_uri = format!("/api/v1/users/{second_id}");
    let first_user_uri = format!("/api/v1/users/{first_id}");
    let first_demotes_second = send(
        &app,
        "PATCH",
        &second_user_uri,
        json!({"workspaceRole": "member"}),
        Some(&first_cookie),
        Some(&first_csrf),
    );
    let second_demotes_first = send(
        &app,
        "PATCH",
        &first_user_uri,
        json!({"workspaceRole": "member"}),
        Some(&second_cookie),
        Some(&second_csrf),
    );
    let (first_result, second_result) = tokio::join!(first_demotes_second, second_demotes_first);
    let mut statuses = [first_result.status(), second_result.status()];
    statuses.sort_by_key(|status| status.as_u16());
    assert_eq!(statuses, [StatusCode::OK, StatusCode::FORBIDDEN]);

    let active_admins: i64 = sqlx::query_scalar(
        "SELECT count(*)
           FROM users u
           JOIN workspace_members wm ON wm.user_id = u.id AND wm.workspace_id = u.workspace_id
          WHERE u.status = 'active' AND u.deleted_at IS NULL
            AND wm.status = 'active' AND wm.role = 'admin'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(active_admins, 1);
}

#[sqlx::test]
async fn public_invitation_acceptance_is_rate_limited(pool: PgPool) {
    let (app, _) = app(pool).await;
    for attempt in 0..6 {
        let status = accept_invitation(&app, &format!("invalid-token-{attempt}"), "Nobody").await;
        if attempt < 5 {
            assert_eq!(status, StatusCode::NOT_FOUND);
        } else {
            assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
        }
    }
}

#[sqlx::test]
async fn login_and_admin_update_use_the_same_workspace_first_lock_order(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let admin_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let admin_csrf = csrf(&app, &admin_cookie).await;
    let (_, invitation) =
        create_invitation(&app, &admin_cookie, &admin_csrf, "locking@example.com").await;
    let accepted = send(
        &app,
        "POST",
        &format!(
            "/api/v1/invitations/{}/accept",
            invitation["token"].as_str().unwrap()
        ),
        json!({
            "displayName": "Locking member",
            "password": "another correct horse battery staple"
        }),
        None,
        None,
    )
    .await;
    let member_id: uuid::Uuid = json_body(accepted).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();

    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM users WHERE id = $1 FOR UPDATE")
        .bind(member_id)
        .fetch_one(&mut *blocker)
        .await
        .unwrap();

    let login_app = app.clone();
    let login_task = tokio::spawn(async move {
        login(
            &login_app,
            "locking@example.com",
            "another correct horse battery staple",
        )
        .await
    });
    wait_for_locked_query(&pool, "INSERT INTO sessions").await;

    let update_app = app.clone();
    let update_cookie = admin_cookie.clone();
    let update_csrf = admin_csrf.clone();
    let update_task = tokio::spawn(async move {
        send(
            &update_app,
            "PATCH",
            &format!("/api/v1/users/{member_id}"),
            json!({"status": "suspended"}),
            Some(&update_cookie),
            Some(&update_csrf),
        )
        .await
        .status()
    });
    wait_for_locked_query(&pool, "SELECT id FROM workspaces WHERE id = $1 FOR UPDATE").await;
    blocker.commit().await.unwrap();

    let (login_result, update_result) =
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(login_task, update_task)
        })
        .await
        .expect("login/update must not deadlock");
    assert!(!login_result.unwrap().is_empty());
    assert_eq!(update_result.unwrap(), StatusCode::OK);
}

#[sqlx::test]
async fn logout_and_session_revoke_use_the_same_workspace_first_lock_order(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let admin_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let admin_csrf = csrf(&app, &admin_cookie).await;
    let (_, invitation) =
        create_invitation(&app, &admin_cookie, &admin_csrf, "logout-lock@example.com").await;
    let accepted = send(
        &app,
        "POST",
        &format!(
            "/api/v1/invitations/{}/accept",
            invitation["token"].as_str().unwrap()
        ),
        json!({
            "displayName": "Logout lock member",
            "password": "another correct horse battery staple"
        }),
        None,
        None,
    )
    .await;
    let member_id: uuid::Uuid = json_body(accepted).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let member_cookie = login(
        &app,
        "logout-lock@example.com",
        "another correct horse battery staple",
    )
    .await;
    let member_csrf = csrf(&app, &member_cookie).await;

    let mut blocker = pool.begin().await.unwrap();
    sqlx::query(
        "SELECT id FROM sessions
          WHERE user_id = $1 AND revoked_at IS NULL
          ORDER BY created_at DESC LIMIT 1 FOR UPDATE",
    )
    .bind(member_id)
    .fetch_one(&mut *blocker)
    .await
    .unwrap();

    let logout_app = app.clone();
    let logout_cookie = member_cookie.clone();
    let logout_csrf = member_csrf.clone();
    let logout_task = tokio::spawn(async move {
        send(
            &logout_app,
            "POST",
            "/api/v1/auth/logout",
            json!({}),
            Some(&logout_cookie),
            Some(&logout_csrf),
        )
        .await
        .status()
    });
    wait_for_locked_query(&pool, "UPDATE sessions").await;

    let revoke_app = app.clone();
    let revoke_cookie = admin_cookie.clone();
    let revoke_csrf = admin_csrf.clone();
    let revoke_task = tokio::spawn(async move {
        send(
            &revoke_app,
            "POST",
            &format!("/api/v1/users/{member_id}/revoke-sessions"),
            json!({}),
            Some(&revoke_cookie),
            Some(&revoke_csrf),
        )
        .await
        .status()
    });
    wait_for_locked_query(&pool, "SELECT id FROM workspaces WHERE id = $1 FOR UPDATE").await;
    blocker.commit().await.unwrap();

    let (logout_result, revoke_result) =
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(logout_task, revoke_task)
        })
        .await
        .expect("logout/revoke-sessions must not deadlock");
    assert_eq!(logout_result.unwrap(), StatusCode::NO_CONTENT);
    assert_eq!(revoke_result.unwrap(), StatusCode::NO_CONTENT);
}
