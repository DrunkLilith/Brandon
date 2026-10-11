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

#[sqlx::test]
async fn permission_and_title_reads_revalidate_after_workspace_lock_wait(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let owner = create_user(&app, &cookie, &csrf, "owner@example.com").await;
    let space = create_space(&app, &cookie, &csrf).await;
    grant_space(&app, &cookie, &csrf, space, owner, "owner").await;
    let page = create_page(&app, &cookie, &csrf, space, None, false, "Title").await;
    let workspace: Uuid = sqlx::query_scalar("SELECT id FROM workspaces LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    for uri in [
        format!("/api/v1/pages/{page}/permissions"),
        format!("/api/v1/pages/{page}"),
        format!("/api/v1/pages?spaceId={space}"),
    ] {
        let (owner_cookie, _) = login_as(&app, "owner@example.com").await;
        let mut lock = pool.begin().await.unwrap();
        sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
            .bind(workspace)
            .fetch_one(&mut *lock)
            .await
            .unwrap();
        let pending_app = app.clone();
        let pending = tokio::spawn(async move {
            send(
                &pending_app,
                "GET",
                &uri,
                json!(null),
                Some(&owner_cookie),
                None,
            )
            .await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                let waiting: bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT id FROM workspaces%')").fetch_one(&pool).await.unwrap();
                if waiting {break;}
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        }).await.expect("request should wait for the workspace lock");
        sqlx::query("UPDATE sessions SET revoked_at=now() WHERE user_id=$1")
            .bind(owner)
            .execute(&mut *lock)
            .await
            .unwrap();
        lock.commit().await.unwrap();
        assert_eq!(pending.await.unwrap().status(), StatusCode::UNAUTHORIZED);
    }
}

#[sqlx::test]
async fn workspace_title_disclosures_audit_administrator_access(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let space = create_space(&app, &cookie, &csrf).await;
    let page = create_page(&app, &cookie, &csrf, space, None, false, "Private title").await;
    for uri in [
        format!("/api/v1/pages?spaceId={space}"),
        format!("/api/v1/pages/{page}"),
    ] {
        assert_eq!(
            send(&app, "GET", &uri, json!(null), Some(&cookie), None)
                .await
                .status(),
            StatusCode::OK
        );
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='page.admin_access'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{page}/revisions/1"),
            json!(null),
            Some(&cookie),
            None,
        )
        .await
        .status(),
        StatusCode::OK,
    );
    let revision_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='revision.admin_access'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revision_audits, 1);
}

#[sqlx::test]
async fn workspace_tree_discloses_authorized_titles_and_capabilities(pool: PgPool) {
    let app = app(pool).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let viewer = create_user(&app, &cookie, &csrf, "viewer@example.com").await;
    let space = create_space(&app, &cookie, &csrf).await;
    grant_space(&app, &cookie, &csrf, space, viewer, "viewer").await;
    let page = create_page(&app, &cookie, &csrf, space, None, false, "Visible title").await;
    create_page(&app, &cookie, &csrf, space, None, true, "Secret title").await;
    let (viewer_cookie, _) = login_as(&app, "viewer@example.com").await;
    let response = send(
        &app,
        "GET",
        &format!("/api/v1/pages?spaceId={space}"),
        json!(null),
        Some(&viewer_cookie),
        None,
    )
    .await;
    let tree = body(response).await;
    assert_eq!(tree.as_array().unwrap().len(), 1);
    assert_eq!(tree[0]["title"], "Visible title");
    assert_eq!(
        tree[0]["capabilities"],
        json!({"edit":false,"managePermissions":false})
    );
    assert!(tree[0].get("markdown").is_none());
    let page_response = body(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{page}"),
            json!(null),
            Some(&cookie),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(
        page_response["capabilities"],
        json!({"edit":true,"managePermissions":true})
    );
}

#[sqlx::test]
async fn permission_read_model_is_owner_admin_only_and_filters_inactive_subjects(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let viewer = create_user(&app, &cookie, &csrf, "viewer@example.com").await;
    let space = create_space(&app, &cookie, &csrf).await;
    grant_space(&app, &cookie, &csrf, space, viewer, "viewer").await;
    let page = create_page(&app, &cookie, &csrf, space, None, true, "Restricted").await;
    put_permission(
        &app,
        &cookie,
        &csrf,
        page,
        &format!("user:{viewer}"),
        "viewer",
    )
    .await;
    let uri = format!("/api/v1/pages/{page}/permissions");
    let response = send(&app, "GET", &uri, json!(null), Some(&cookie), None).await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        body(response).await,
        json!([{"subjectType":"user","subjectId":viewer,"role":"viewer"}])
    );
    let (viewer_cookie, _) = login_as(&app, "viewer@example.com").await;
    assert_eq!(
        send(&app, "GET", &uri, json!(null), Some(&viewer_cookie), None)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE users SET status='suspended' WHERE id=$1")
        .bind(viewer)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        body(send(&app, "GET", &uri, json!(null), Some(&cookie), None).await).await,
        json!([])
    );
}

async fn app(pool: PgPool) -> Router {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool);
    let master_key = MasterKey::from_base64(&STANDARD.encode([71_u8; 32])).unwrap();
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
    build_router(AppState::new(store, master_key, ORIGIN, "").unwrap())
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    value: Value,
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
        .body(Body::from(value.to_string()))
        .unwrap();
    if method == "POST" && uri == "/api/v1/invitations" {
        request.headers_mut().insert(
            "idempotency-key",
            Uuid::now_v7().to_string().parse().unwrap(),
        );
    }
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

async fn login_as(app: &Router, email: &str) -> (String, String) {
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

async fn create_user(app: &Router, cookie: &str, csrf: &str, email: &str) -> Uuid {
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
    let token = body(invitation).await["token"].as_str().unwrap().to_owned();
    let accepted = send(
        app,
        "POST",
        &format!("/api/v1/invitations/{token}/accept"),
        json!({"displayName": email, "password": PASSWORD}),
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
        json!({"name": "Restricted", "description": null}),
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

async fn grant_space(
    app: &Router,
    cookie: &str,
    csrf: &str,
    space_id: Uuid,
    user_id: Uuid,
    role: &str,
) {
    let response = send(
        app,
        "POST",
        &format!("/api/v1/spaces/{space_id}/grants"),
        json!({"subjectType": "user", "subjectId": user_id, "role": role}),
        Some(cookie),
        Some(csrf),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
}

async fn create_page(
    app: &Router,
    cookie: &str,
    csrf: &str,
    space_id: Uuid,
    parent_id: Option<Uuid>,
    restricted: bool,
    title: &str,
) -> Uuid {
    let response = send(
        app,
        "POST",
        "/api/v1/pages",
        json!({
            "spaceId": space_id,
            "parentId": parent_id,
            "title": title,
            "markdown": format!("body for {title}"),
            "properties": {},
            "restricted": restricted
        }),
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

async fn put_permission(
    app: &Router,
    cookie: &str,
    csrf: &str,
    page_id: Uuid,
    subject: &str,
    role: &str,
) -> axum::response::Response {
    send(
        app,
        "PUT",
        &format!("/api/v1/pages/{page_id}/permissions/{subject}"),
        json!({"role": role}),
        Some(cookie),
        Some(csrf),
    )
    .await
}

#[sqlx::test]
async fn nearest_restricted_boundary_supports_direct_and_group_allowlists(pool: PgPool) {
    let app = app(pool).await;
    let (admin_cookie, admin_csrf) = login_as(&app, "admin@example.com").await;
    let owner_id = create_user(&app, &admin_cookie, &admin_csrf, "owner@example.com").await;
    let editor_id = create_user(&app, &admin_cookie, &admin_csrf, "editor@example.com").await;
    let isolated_id = create_user(&app, &admin_cookie, &admin_csrf, "isolated@example.com").await;
    let space_id = create_space(&app, &admin_cookie, &admin_csrf).await;
    grant_space(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        owner_id,
        "owner",
    )
    .await;
    let destination_space = create_space(&app, &admin_cookie, &admin_csrf).await;
    grant_space(
        &app,
        &admin_cookie,
        &admin_csrf,
        destination_space,
        editor_id,
        "editor",
    )
    .await;
    grant_space(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        editor_id,
        "editor",
    )
    .await;

    let public = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        None,
        false,
        "Public",
    )
    .await;
    let boundary = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        Some(public),
        true,
        "Boundary",
    )
    .await;
    let child = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        Some(boundary),
        false,
        "Child",
    )
    .await;
    let leaf = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        Some(boundary),
        false,
        "Movable leaf",
    )
    .await;
    let nested = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        Some(child),
        true,
        "Nested boundary",
    )
    .await;
    let grandchild = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        Some(nested),
        false,
        "Grandchild",
    )
    .await;
    let cross_space_boundary = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        None,
        true,
        "Cross-space boundary",
    )
    .await;
    assert_eq!(
        put_permission(
            &app,
            &admin_cookie,
            &admin_csrf,
            cross_space_boundary,
            &format!("user:{editor_id}"),
            "editor",
        )
        .await
        .status(),
        StatusCode::OK
    );
    let (owner_cookie, owner_csrf) = login_as(&app, "owner@example.com").await;
    let (editor_cookie, editor_csrf) = login_as(&app, "editor@example.com").await;
    let (isolated_cookie, _) = login_as(&app, "isolated@example.com").await;

    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{cross_space_boundary}/move"),
            json!({"spaceId": destination_space, "parentId": null}),
            Some(&editor_cookie),
            Some(&editor_csrf),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );

    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{child}"),
            json!({}),
            Some(&owner_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        put_permission(
            &app,
            &owner_cookie,
            &owner_csrf,
            boundary,
            &format!("user:{editor_id}"),
            "viewer",
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        put_permission(
            &app,
            &admin_cookie,
            &admin_csrf,
            public,
            &format!("user:{editor_id}"),
            "viewer",
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(
        put_permission(
            &app,
            &admin_cookie,
            &admin_csrf,
            boundary,
            &format!("user:{editor_id}"),
            "owner",
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        put_permission(
            &app,
            &admin_cookie,
            &admin_csrf,
            boundary,
            &format!("user:{editor_id}"),
            "viewer",
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{child}"),
            json!({}),
            Some(&editor_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{leaf}/move"),
            json!({"spaceId": space_id, "parentId": null}),
            Some(&editor_cookie),
            Some(&editor_csrf),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/pages/{child}"),
            json!({"expectedVersion": 1, "title": "Denied", "markdown": "denied", "properties": {}}),
            Some(&editor_cookie),
            Some(&editor_csrf),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        put_permission(
            &app,
            &admin_cookie,
            &admin_csrf,
            boundary,
            &format!("user:{editor_id}"),
            "editor",
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{leaf}/move"),
            json!({"spaceId": space_id, "parentId": null}),
            Some(&editor_cookie),
            Some(&editor_csrf),
        )
        .await
        .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/pages/{child}"),
            json!({"expectedVersion": 1, "title": "Allowed", "markdown": "allowed", "properties": {}}),
            Some(&editor_cookie),
            Some(&editor_csrf),
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{grandchild}"),
            json!({}),
            Some(&editor_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );

    assert_eq!(
        put_permission(
            &app,
            &admin_cookie,
            &admin_csrf,
            boundary,
            &format!("user:{isolated_id}"),
            "viewer",
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{child}"),
            json!({}),
            Some(&isolated_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );

    let group = send(
        &app,
        "POST",
        "/api/v1/groups",
        json!({"name": "Allowed", "description": null}),
        Some(&admin_cookie),
        Some(&admin_csrf),
    )
    .await;
    assert_eq!(group.status(), StatusCode::CREATED);
    let group_id: Uuid = body(group).await["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        send(
            &app,
            "PUT",
            &format!("/api/v1/groups/{group_id}/members/{editor_id}"),
            json!({}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/pages/{boundary}/permissions/user:{editor_id}"),
            json!({}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        put_permission(
            &app,
            &admin_cookie,
            &admin_csrf,
            nested,
            &format!("group:{group_id}"),
            "viewer",
        )
        .await
        .status(),
        StatusCode::OK
    );
    let nested_response = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{nested}"),
        json!({}),
        Some(&editor_cookie),
        None,
    )
    .await;
    assert_eq!(nested_response.status(), StatusCode::OK);
    assert_eq!(body(nested_response).await["parentId"], Value::Null);
    let grandchild_response = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{grandchild}"),
        json!({}),
        Some(&editor_cookie),
        None,
    )
    .await;
    assert_eq!(grandchild_response.status(), StatusCode::OK);
    assert_eq!(
        body(grandchild_response).await["parentId"],
        nested.to_string()
    );
    let tree = send(
        &app,
        "GET",
        &format!("/api/v1/pages?spaceId={space_id}"),
        json!({}),
        Some(&editor_cookie),
        None,
    )
    .await;
    assert_eq!(tree.status(), StatusCode::OK);
    let tree = body(tree).await;
    let nested_node = tree
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == nested.to_string())
        .unwrap();
    assert_eq!(nested_node["parentId"], Value::Null);
    assert!(
        !tree
            .as_array()
            .unwrap()
            .iter()
            .any(|node| node["id"] == child.to_string())
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{grandchild}"),
            json!({}),
            Some(&editor_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/pages/{nested}/permissions/group:{group_id}"),
            json!({}),
            Some(&admin_cookie),
            Some(&admin_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{grandchild}"),
            json!({}),
            Some(&editor_cookie),
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn hidden_and_absent_pages_are_indistinguishable_on_read_routes(pool: PgPool) {
    let app = app(pool).await;
    let (admin_cookie, admin_csrf) = login_as(&app, "admin@example.com").await;
    let viewer_id = create_user(&app, &admin_cookie, &admin_csrf, "viewer@example.com").await;
    let space_id = create_space(&app, &admin_cookie, &admin_csrf).await;
    grant_space(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        viewer_id,
        "viewer",
    )
    .await;
    let hidden = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        None,
        true,
        "Hidden marker",
    )
    .await;
    let (viewer_cookie, viewer_csrf) = login_as(&app, "viewer@example.com").await;
    let missing = Uuid::now_v7();

    for suffix in ["", "/revisions", "/backlinks", "/permissions"] {
        let hidden_response = send(
            &app,
            "GET",
            &format!("/api/v1/pages/{hidden}{suffix}"),
            json!({}),
            Some(&viewer_cookie),
            None,
        )
        .await;
        let missing_response = send(
            &app,
            "GET",
            &format!("/api/v1/pages/{missing}{suffix}"),
            json!({}),
            Some(&viewer_cookie),
            None,
        )
        .await;
        assert_eq!(hidden_response.status(), StatusCode::NOT_FOUND);
        assert_eq!(missing_response.status(), StatusCode::NOT_FOUND);
        assert_eq!(body(hidden_response).await, body(missing_response).await);
    }

    let hidden_mutation = send(
        &app,
        "PATCH",
        &format!("/api/v1/pages/{hidden}"),
        json!({"expectedVersion": 1, "title": "x", "markdown": "x", "properties": {}}),
        Some(&viewer_cookie),
        Some(&viewer_csrf),
    )
    .await;
    let missing_mutation = send(
        &app,
        "PATCH",
        &format!("/api/v1/pages/{missing}"),
        json!({"expectedVersion": 1, "title": "x", "markdown": "x", "properties": {}}),
        Some(&viewer_cookie),
        Some(&viewer_csrf),
    )
    .await;
    assert_eq!(hidden_mutation.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing_mutation.status(), StatusCode::NOT_FOUND);
    assert_eq!(body(hidden_mutation).await, body(missing_mutation).await);

    let tree = send(
        &app,
        "GET",
        &format!("/api/v1/pages?spaceId={space_id}"),
        json!({}),
        Some(&viewer_cookie),
        None,
    )
    .await;
    assert_eq!(tree.status(), StatusCode::OK);
    assert!(body(tree).await.as_array().unwrap().is_empty());
}
