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
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery staple";
const ORIGIN: &str = "https://knowledge.local";

async fn app(pool: PgPool) -> (Router, PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool.clone());
    let master_key = MasterKey::from_base64(&STANDARD.encode([61_u8; 32])).unwrap();
    BootstrapService::run(
        &store,
        &master_key,
        BootstrapInput {
            workspace_name: "Workspace".to_owned(),
            email: "admin@example.com".to_owned(),
            display_name: "Admin".to_owned(),
            password: PASSWORD.to_owned(),
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

async fn body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

async fn login(app: &Router, email: &str) -> (String, String) {
    let response = send(
        app,
        "POST",
        "/api/v1/auth/login",
        json!({"email": email, "password": PASSWORD}),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let cookie = response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf_response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .header(header::COOKIE, &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csrf = body(csrf_response).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}

async fn create_user(app: &Router, cookie: &str, csrf: &str, local: &str) -> Uuid {
    let email = format!("{local}@example.com");
    let invitation = send(
        app,
        "POST",
        "/api/v1/invitations",
        json!({"email": email, "workspaceRole": "member"}),
        Some(cookie),
        Some(csrf),
    )
    .await;
    assert_eq!(invitation.status(), StatusCode::CREATED);
    let invitation = body(invitation).await;
    let accepted = send(
        app,
        "POST",
        &format!(
            "/api/v1/invitations/{}/accept",
            invitation["token"].as_str().unwrap()
        ),
        json!({"displayName": local, "password": PASSWORD}),
        None,
        None,
    )
    .await;
    assert_eq!(accepted.status(), StatusCode::CREATED);
    body(accepted).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn create_space(app: &Router, cookie: &str, csrf: &str) -> Uuid {
    let response = send(
        app,
        "POST",
        "/api/v1/spaces",
        json!({"name": "Операции", "description": "Секретное пространство"}),
        Some(cookie),
        Some(csrf),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn grant(
    app: &Router,
    cookie: &str,
    csrf: &str,
    space_id: Uuid,
    subject_type: &str,
    subject_id: Uuid,
    role: &str,
) -> Uuid {
    let response = send(
        app,
        "POST",
        &format!("/api/v1/spaces/{space_id}/grants"),
        json!({"subjectType": subject_type, "subjectId": subject_id, "role": role}),
        Some(cookie),
        Some(csrf),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

#[sqlx::test]
async fn strongest_direct_or_current_group_grant_wins_without_acl_cache(pool: PgPool) {
    let (app, _) = app(pool).await;
    let (admin_cookie, admin_csrf) = login(&app, "admin@example.com").await;
    let member_id = create_user(&app, &admin_cookie, &admin_csrf, "member").await;
    let group = send(
        &app,
        "POST",
        "/api/v1/groups",
        json!({"name": "Editors", "description": null}),
        Some(&admin_cookie),
        Some(&admin_csrf),
    )
    .await;
    let group_id: Uuid = body(group).await["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        send(
            &app,
            "PUT",
            &format!("/api/v1/groups/{group_id}/members/{member_id}"),
            json!({}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let space_id = create_space(&app, &admin_cookie, &admin_csrf).await;
    grant(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        "user",
        member_id,
        "viewer",
    )
    .await;
    grant(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        "group",
        group_id,
        "editor",
    )
    .await;
    let (member_cookie, _) = login(&app, "member@example.com").await;

    let before = send(
        &app,
        "GET",
        &format!("/api/v1/spaces/{space_id}"),
        json!({}),
        Some(&member_cookie),
        None,
    )
    .await;
    assert_eq!(before.status(), StatusCode::OK);
    assert_eq!(body(before).await["effectiveRole"], "editor");

    send(
        &app,
        "DELETE",
        &format!("/api/v1/groups/{group_id}/members/{member_id}"),
        json!({}),
        Some(&admin_cookie),
        Some(&admin_csrf),
    )
    .await;
    let after = send(
        &app,
        "GET",
        &format!("/api/v1/spaces/{space_id}"),
        json!({}),
        Some(&member_cookie),
        None,
    )
    .await;
    assert_eq!(after.status(), StatusCode::OK);
    assert_eq!(body(after).await["effectiveRole"], "viewer");
}

#[sqlx::test]
async fn owner_editor_viewer_and_no_access_matrix_is_enforced(pool: PgPool) {
    let (app, pool) = app(pool).await;
    let (admin_cookie, admin_csrf) = login(&app, "admin@example.com").await;
    let owner_id = create_user(&app, &admin_cookie, &admin_csrf, "owner").await;
    let editor_id = create_user(&app, &admin_cookie, &admin_csrf, "editor").await;
    let viewer_id = create_user(&app, &admin_cookie, &admin_csrf, "viewer").await;
    let none_id = create_user(&app, &admin_cookie, &admin_csrf, "none").await;
    let space_id = create_space(&app, &admin_cookie, &admin_csrf).await;
    for (user_id, role) in [
        (owner_id, "owner"),
        (editor_id, "editor"),
        (viewer_id, "viewer"),
    ] {
        grant(
            &app,
            &admin_cookie,
            &admin_csrf,
            space_id,
            "user",
            user_id,
            role,
        )
        .await;
    }
    let (owner_cookie, owner_csrf) = login(&app, "owner@example.com").await;
    let (editor_cookie, editor_csrf) = login(&app, "editor@example.com").await;
    let (viewer_cookie, viewer_csrf) = login(&app, "viewer@example.com").await;
    let (none_cookie, none_csrf) = login(&app, "none@example.com").await;

    for (cookie, expected) in [
        (&owner_cookie, StatusCode::OK),
        (&editor_cookie, StatusCode::OK),
        (&viewer_cookie, StatusCode::OK),
        (&none_cookie, StatusCode::NOT_FOUND),
    ] {
        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/spaces/{space_id}"),
                json!({}),
                Some(cookie),
                None,
            )
            .await
            .status(),
            expected
        );
    }
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/spaces/{space_id}"),
            json!({"name": "Owner renamed"}),
            Some(&owner_cookie),
            Some(&owner_csrf),
        )
        .await
        .status(),
        StatusCode::OK
    );
    for (cookie, csrf, expected) in [
        (&editor_cookie, &editor_csrf, StatusCode::FORBIDDEN),
        (&viewer_cookie, &viewer_csrf, StatusCode::FORBIDDEN),
        (&none_cookie, &none_csrf, StatusCode::NOT_FOUND),
    ] {
        assert_eq!(
            send(
                &app,
                "PATCH",
                &format!("/api/v1/spaces/{space_id}"),
                json!({"name": "Denied"}),
                Some(cookie),
                Some(csrf),
            )
            .await
            .status(),
            expected
        );
    }
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/spaces/{space_id}/grants"),
            json!({"subjectType": "user", "subjectId": none_id, "role": "viewer"}),
            Some(&editor_cookie),
            Some(&editor_csrf),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    let managed_grant = grant(
        &app,
        &owner_cookie,
        &owner_csrf,
        space_id,
        "user",
        none_id,
        "viewer",
    )
    .await;
    let grants = send(
        &app,
        "GET",
        &format!("/api/v1/spaces/{space_id}/grants"),
        json!({}),
        Some(&owner_cookie),
        None,
    )
    .await;
    assert_eq!(grants.status(), StatusCode::OK);
    assert!(
        body(grants)
            .await
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == managed_grant.to_string())
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/spaces/{space_id}/grants/{managed_grant}"),
            json!({"role": "editor"}),
            Some(&owner_cookie),
            Some(&owner_csrf),
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/spaces/{space_id}/grants/{managed_grant}"),
            json!({}),
            Some(&owner_cookie),
            Some(&owner_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/spaces/{space_id}"),
            json!({}),
            Some(&owner_cookie),
            Some(&owner_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/spaces/{space_id}"),
            json!({}),
            Some(&admin_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/spaces/{space_id}/grants"),
            json!({}),
            Some(&admin_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let name_ciphertext: Vec<u8> = sqlx::query_scalar("SELECT name_ciphertext FROM spaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        !name_ciphertext
            .windows("Owner renamed".len())
            .any(|part| part == b"Owner renamed")
    );
    let events: Vec<String> = sqlx::query_scalar(
        "SELECT event_type FROM audit_events WHERE event_type LIKE 'space.%' ORDER BY created_at, id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert!(events.contains(&"space.created".to_owned()));
    assert!(events.contains(&"space.updated".to_owned()));
    assert!(events.contains(&"space.grant_created".to_owned()));
    assert!(events.contains(&"space.grant_updated".to_owned()));
    assert!(events.contains(&"space.grant_deleted".to_owned()));
    assert!(events.contains(&"space.archived".to_owned()));
}
