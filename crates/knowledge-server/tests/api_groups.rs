use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use knowledge_core::ids::{GroupId, UserId};
use knowledge_crypto::MasterKey;
use knowledge_server::{
    bootstrap::{BootstrapInput, BootstrapService},
    build_router,
    state::AppState,
};
use knowledge_store::{GroupRepository, MIGRATOR, Store};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const ADMIN_PASSWORD: &str = "correct horse battery staple";
const MEMBER_PASSWORD: &str = "another correct horse battery staple";
const ORIGIN: &str = "https://knowledge.local";

async fn app(pool: PgPool) -> (Router, PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool.clone());
    let master_key = MasterKey::from_base64(&STANDARD.encode([71_u8; 32])).unwrap();
    BootstrapService::run(
        &store,
        &master_key,
        BootstrapInput {
            workspace_name: "Командная база".to_owned(),
            email: "admin@example.com".to_owned(),
            display_name: "Администратор".to_owned(),
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

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    body: Value,
    cookie: Option<&str>,
    csrf: Option<&str>,
) -> axum::response::Response {
    let mut request = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::HOST, "knowledge.local")
        .header(header::ORIGIN, ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .body(Body::from(body.to_string()))
        .unwrap();
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

async fn create_group(app: &Router, cookie: &str, csrf: &str, name: &str) -> (StatusCode, Value) {
    let response = send(
        app,
        "POST",
        "/api/v1/groups",
        json!({"name": name, "description": "Закрытое описание"}),
        Some(cookie),
        Some(csrf),
    )
    .await;
    let status = response.status();
    (status, json_body(response).await)
}

async fn create_member(app: &Router, admin_cookie: &str, admin_csrf: &str) -> Uuid {
    let invitation = send(
        app,
        "POST",
        "/api/v1/invitations",
        json!({"email": "member@example.com", "workspaceRole": "member"}),
        Some(admin_cookie),
        Some(admin_csrf),
    )
    .await;
    let invitation = json_body(invitation).await;
    let accepted = send(
        app,
        "POST",
        &format!(
            "/api/v1/invitations/{}/accept",
            invitation["token"].as_str().unwrap()
        ),
        json!({"displayName": "Участник", "password": MEMBER_PASSWORD}),
        None,
        None,
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::CREATED);
    json_body(accepted).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[sqlx::test]
async fn administrator_can_create_list_rename_and_archive_encrypted_groups(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let csrf = csrf(&app, &cookie).await;
    let (status, created) = create_group(&app, &cookie, &csrf, "Разведка").await;
    assert_eq!(status, StatusCode::CREATED);
    let group_id = created["id"].as_str().unwrap();

    let (name_ciphertext, description_ciphertext): (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT name_ciphertext, description_ciphertext FROM groups")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !name_ciphertext
            .windows(16)
            .any(|part| part == "Разведка".as_bytes())
    );
    assert!(
        !description_ciphertext
            .windows(33)
            .any(|part| part == "Закрытое описание".as_bytes())
    );

    let listed = send(
        &app,
        "GET",
        "/api/v1/groups",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(json_body(listed).await[0]["name"], "Разведка");

    let renamed = send(
        &app,
        "PATCH",
        &format!("/api/v1/groups/{group_id}"),
        json!({"name": "Исследователи", "description": null}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(renamed.status(), StatusCode::OK);
    let renamed = json_body(renamed).await;
    assert_eq!(renamed["name"], "Исследователи");
    assert_eq!(renamed["description"], Value::Null);

    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/groups/{group_id}"),
            json!({}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let archived_at: Option<chrono::DateTime<chrono::Utc>> =
        sqlx::query_scalar("SELECT archived_at FROM groups WHERE id = $1")
            .bind(group_id.parse::<Uuid>().unwrap())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(archived_at.is_some());
    let events: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM audit_events
          WHERE event_type LIKE 'group.%' ORDER BY created_at, id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(events, ["group.created", "group.updated", "group.archived"]);
}

#[sqlx::test]
async fn membership_changes_are_idempotent_and_reject_suspended_users(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let csrf = csrf(&app, &cookie).await;
    let member_id = create_member(&app, &cookie, &csrf).await;
    let (_, first_group) = create_group(&app, &cookie, &csrf, "Первая").await;
    let first_group_id = first_group["id"].as_str().unwrap();
    let member_uri = format!("/api/v1/groups/{first_group_id}/members/{member_id}");

    for _ in 0..2 {
        assert_eq!(
            send(
                &app,
                "PUT",
                &member_uri,
                json!({}),
                Some(&cookie),
                Some(&csrf)
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    let memberships: i64 = sqlx::query_scalar("SELECT count(*) FROM group_members")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(memberships, 1);
    assert_eq!(
        GroupRepository::subject_groups(&pool, UserId::from_uuid(member_id))
            .await
            .unwrap(),
        [GroupId::from_uuid(first_group_id.parse().unwrap())]
    );

    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/users/{member_id}"),
            json!({"status": "suspended"}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::OK
    );
    let (_, second_group) = create_group(&app, &cookie, &csrf, "Вторая").await;
    assert_eq!(
        send(
            &app,
            "PUT",
            &format!(
                "/api/v1/groups/{}/members/{member_id}",
                second_group["id"].as_str().unwrap()
            ),
            json!({}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );

    for _ in 0..2 {
        assert_eq!(
            send(
                &app,
                "DELETE",
                &member_uri,
                json!({}),
                Some(&cookie),
                Some(&csrf)
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    let memberships: i64 = sqlx::query_scalar("SELECT count(*) FROM group_members")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(memberships, 0);
    assert!(
        GroupRepository::subject_groups(&pool, UserId::from_uuid(member_id))
            .await
            .unwrap()
            .is_empty()
    );
}

#[sqlx::test]
async fn group_routes_require_admin_and_audit_only_real_mutations(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let admin_cookie = login(&app, "admin@example.com", ADMIN_PASSWORD).await;
    let admin_csrf = csrf(&app, &admin_cookie).await;
    let member_id = create_member(&app, &admin_cookie, &admin_csrf).await;
    let member_cookie = login(&app, "member@example.com", MEMBER_PASSWORD).await;
    let member_csrf = csrf(&app, &member_cookie).await;

    assert_eq!(
        create_group(&app, &member_cookie, &member_csrf, "Запрещено")
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            "GET",
            "/api/v1/groups",
            json!({}),
            Some(&member_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    let (_, group) = create_group(&app, &admin_cookie, &admin_csrf, "Аудит").await;
    let group_id = group["id"].as_str().unwrap();
    let member_uri = format!("/api/v1/groups/{group_id}/members/{member_id}");
    for _ in 0..2 {
        send(
            &app,
            "PUT",
            &member_uri,
            json!({}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await;
    }
    for _ in 0..2 {
        send(
            &app,
            "DELETE",
            &member_uri,
            json!({}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await;
    }

    let events: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM audit_events
          WHERE event_type LIKE 'group.%' ORDER BY created_at, id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(
        events,
        [
            "group.created",
            "group.member_added",
            "group.member_removed"
        ]
    );
}
