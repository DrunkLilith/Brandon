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
async fn malformed_extractors_return_redacted_problem_details(pool: PgPool) {
    let app = app(pool).await;
    let (cookie, _) = login(&app).await;
    for route in [
        "/api/v1/pages/not-a-uuid",
        "/api/v1/pages?spaceId=not-a-uuid",
        "/api/v1/search?from=private-input",
    ] {
        let response = send(&app, "GET", route, json!({}), Some(&cookie), None).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        assert!(response.headers().contains_key("x-request-id"));
        let problem = body(response).await;
        assert_eq!(problem["code"], "validation_failed");
        assert!(!problem.to_string().contains("private-input"));
    }
}

#[sqlx::test]
async fn bounded_properties_tree_moves_preserve_acyclic_parentage(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login(&app).await;
    let space = create_space(&app, &cookie, &csrf, "Seeded tree").await;
    let mut ids = Vec::new();
    for index in 0..8 {
        let page = create_page(
            &app,
            &cookie,
            &csrf,
            space,
            None,
            &format!("Node {index}"),
            "bounded corpus",
        )
        .await;
        ids.push(page["id"].as_str().unwrap().parse::<Uuid>().unwrap());
    }
    let mut parents = [None; 8];
    let mut seed = 0x51eed_u64;
    for case in 0..64 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let node = (seed % 8) as usize;
        let parent = (((seed >> 8) % 9) as usize).checked_sub(1);
        let mut ancestor = parent;
        let mut cycle = false;
        for _ in 0..8 {
            let Some(index) = ancestor else { break };
            if index == node {
                cycle = true;
                break;
            }
            ancestor = parents[index];
        }
        let response = send(
            &app,
            "POST",
            &format!("/api/v1/pages/{}/move", ids[node]),
            json!({"spaceId":space,"parentId":parent.map(|index|ids[index]),"beforeId":null}),
            Some(&cookie),
            Some(&csrf),
        )
        .await;
        assert_eq!(
            response.status(),
            if cycle {
                StatusCode::CONFLICT
            } else {
                StatusCode::NO_CONTENT
            },
            "seed 0x51eed case {case}"
        );
        if !cycle {
            parents[node] = parent;
        }
        let actual: Vec<(Uuid, Option<Uuid>)> =
            sqlx::query_as("SELECT id,parent_id FROM pages WHERE space_id=$1 ORDER BY id")
                .bind(space)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(actual.len(), 8);
        for (id, parent) in actual {
            let index = ids.iter().position(|candidate| *candidate == id).unwrap();
            assert_eq!(
                parent,
                parents[index].map(|index| ids[index]),
                "seed 0x51eed case {case}"
            );
        }
    }
}

async fn app(pool: PgPool) -> Router {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool);
    let master_key = MasterKey::from_base64(&STANDARD.encode([51_u8; 32])).unwrap();
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

async fn login(app: &Router) -> (String, String) {
    login_as(app, "admin@example.com").await
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
    let invited = send(
        app,
        "POST",
        "/api/v1/invitations",
        json!({"email": email, "workspaceRole": "member"}),
        Some(cookie),
        Some(csrf),
    )
    .await;
    let invited = body(invited).await;
    let accepted = send(
        app,
        "POST",
        &format!(
            "/api/v1/invitations/{}/accept",
            invited["token"].as_str().unwrap()
        ),
        json!({"displayName": "Editor", "password": PASSWORD}),
        None,
        None,
    )
    .await;
    body(accepted).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap()
}

async fn create_space(app: &Router, cookie: &str, csrf: &str, name: &str) -> Uuid {
    let response = send(
        app,
        "POST",
        "/api/v1/spaces",
        json!({"name": name, "description": null}),
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

async fn create_page(
    app: &Router,
    cookie: &str,
    csrf: &str,
    space_id: Uuid,
    parent_id: Option<Uuid>,
    title: &str,
    markdown: &str,
) -> Value {
    let response = send(
        app,
        "POST",
        "/api/v1/pages",
        json!({
            "spaceId": space_id, "parentId": parent_id, "title": title,
            "markdown": markdown, "properties": {}, "restricted": false
        }),
        Some(cookie),
        Some(csrf),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    body(response).await
}

#[sqlx::test]
async fn archived_page_discovery_is_paginated_authorized_and_retention_bounded(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login(&app).await;
    let space = create_space(&app, &cookie, &csrf, "Archive").await;
    let owner = create_user(&app, &cookie, &csrf, "owner@example.com").await;
    let grant = send(
        &app,
        "POST",
        &format!("/api/v1/spaces/{space}/grants"),
        json!({"subjectType":"user","subjectId":owner,"role":"owner"}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(grant.status(), StatusCode::CREATED);
    let mut ids = Vec::new();
    for index in 0..4 {
        let page = create_page(
            &app,
            &cookie,
            &csrf,
            space,
            None,
            &format!("Archived {index}"),
            "retained",
        )
        .await;
        let id: Uuid = page["id"].as_str().unwrap().parse().unwrap();
        ids.push(id);
        assert_eq!(
            send(
                &app,
                "DELETE",
                &format!("/api/v1/pages/{id}"),
                json!({}),
                Some(&cookie),
                Some(&csrf)
            )
            .await
            .status(),
            StatusCode::NO_CONTENT
        );
    }
    sqlx::query("UPDATE pages SET restricted=true WHERE id=$1")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE pages SET archived_at=now()-interval '31 days' WHERE id=$1")
        .bind(ids[1])
        .execute(&pool)
        .await
        .unwrap();
    let (owner_cookie, _) = login_as(&app, "owner@example.com").await;
    let url = format!("/api/v1/spaces/{space}/archived-pages?limit=1");
    let first = send(&app, "GET", &url, json!({}), Some(&owner_cookie), None).await;
    assert_eq!(first.status(), StatusCode::OK);
    let first = body(first).await;
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let cursor = first["nextCursor"].as_str().unwrap();
    let second = body(
        send(
            &app,
            "GET",
            &format!("{url}&after={cursor}"),
            json!({}),
            Some(&owner_cookie),
            None,
        )
        .await,
    )
    .await;
    assert_eq!(second["items"].as_array().unwrap().len(), 1);
    assert!(second["nextCursor"].is_null());
    let mut found = vec![
        first["items"][0]["id"].as_str().unwrap().to_owned(),
        second["items"][0]["id"].as_str().unwrap().to_owned(),
    ];
    found.sort();
    let mut expected = vec![ids[2].to_string(), ids[3].to_string()];
    expected.sort();
    assert_eq!(found, expected);
    sqlx::query("UPDATE space_grants SET role='viewer' WHERE user_id=$1")
        .bind(owner)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(&app, "GET", &url, json!({}), Some(&owner_cookie), None)
            .await
            .status(),
        StatusCode::FORBIDDEN
    );
}

#[sqlx::test]
async fn page_create_stores_only_ciphertext_and_round_trips_exact_markdown(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login(&app).await;
    let space = send(
        &app,
        "POST",
        "/api/v1/spaces",
        json!({"name": "Knowledge", "description": null}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    let space_id = body(space).await["id"].as_str().unwrap().to_owned();
    let title = "TOP SECRET PAGE MARKER";
    let markdown = "# Heading\n\nMARKDOWN SECRET MARKER\n\n- one\n- two";
    let created = send(
        &app,
        "POST",
        "/api/v1/pages",
        json!({
            "spaceId": space_id,
            "parentId": null,
            "title": title,
            "markdown": markdown,
            "properties": {"kind": "manual"},
            "restricted": false
        }),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(created.status(), StatusCode::CREATED);
    let created = body(created).await;
    let page_id: Uuid = created["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(created["version"], 1);

    let (title_ciphertext, body_ciphertext): (Vec<u8>, Vec<u8>) =
        sqlx::query_as("SELECT title_ciphertext, body_ciphertext FROM pages WHERE id = $1")
            .bind(page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !title_ciphertext
            .windows(title.len())
            .any(|part| part == title.as_bytes())
    );
    assert!(
        !body_ciphertext
            .windows(markdown.len())
            .any(|part| part == markdown.as_bytes())
    );
    let revision_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM page_revisions WHERE page_id = $1")
            .bind(page_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(revision_count, 1);

    let fetched = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{page_id}"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(fetched.status(), StatusCode::OK);
    let fetched = body(fetched).await;
    assert_eq!(fetched["title"], title);
    assert_eq!(fetched["markdown"], markdown);
    assert_eq!(fetched["properties"], json!({"kind": "manual"}));
    assert_eq!(fetched["stableKey"], created["stableKey"]);

    let saved = send(
        &app,
        "PATCH",
        &format!("/api/v1/pages/{page_id}"),
        json!({
            "expectedVersion": 1,
            "title": "Version two",
            "markdown": "private version two body",
            "properties": {"kind": "runbook"}
        }),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(saved.status(), StatusCode::OK);
    assert_eq!(body(saved).await["version"], 2);

    let stale = send(
        &app,
        "PATCH",
        &format!("/api/v1/pages/{page_id}"),
        json!({
            "expectedVersion": 1,
            "title": "Stale writer",
            "markdown": "must never be echoed",
            "properties": {}
        }),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(stale.status(), StatusCode::CONFLICT);
    let stale = body(stale).await;
    assert_eq!(stale["currentVersion"], 2);
    assert!(!stale.to_string().contains("must never be echoed"));
}

#[sqlx::test]
async fn tree_history_restore_and_ast_backlinks_are_consistent(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login(&app).await;
    let space_id = create_space(&app, &cookie, &csrf, "Tree").await;
    let root = create_page(&app, &cookie, &csrf, space_id, None, "Root", "root v1").await;
    let root_id: Uuid = root["id"].as_str().unwrap().parse().unwrap();
    let child = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        Some(root_id),
        "Child",
        "child v1",
    )
    .await;
    let child_id: Uuid = child["id"].as_str().unwrap().parse().unwrap();
    let later = create_page(&app, &cookie, &csrf, space_id, None, "Later", "later").await;
    let later_id: Uuid = later["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{later_id}/move"),
            json!({"spaceId": space_id, "parentId": null, "beforeId": root_id}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );

    let cycle = send(
        &app,
        "POST",
        &format!("/api/v1/pages/{root_id}/move"),
        json!({"spaceId": space_id, "parentId": child_id}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(cycle.status(), StatusCode::CONFLICT);

    let tree = send(
        &app,
        "GET",
        &format!("/api/v1/pages?spaceId={space_id}"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(tree.status(), StatusCode::OK);
    let tree = body(tree).await;
    assert_eq!(tree.as_array().unwrap().len(), 3);
    assert_eq!(tree[0]["id"], later_id.to_string());

    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/pages/{root_id}"),
            json!({}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{child_id}"),
            json!({}),
            Some(&cookie),
            None,
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{root_id}/unarchive"),
            json!({}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let destination_space = create_space(&app, &cookie, &csrf, "Destination").await;
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{root_id}/move"),
            json!({"spaceId": destination_space, "parentId": null}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let moved_spaces: Vec<Uuid> =
        sqlx::query_scalar("SELECT space_id FROM pages WHERE id IN ($1, $2) ORDER BY id")
            .bind(root_id)
            .bind(child_id)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(moved_spaces, [destination_space, destination_space]);

    for (expected, title, markdown) in [(1, "Child v2", "body v2"), (2, "Child v3", "body v3")] {
        assert_eq!(send(
            &app, "PATCH", &format!("/api/v1/pages/{child_id}"),
            json!({"expectedVersion": expected, "title": title, "markdown": markdown, "properties": {}}),
            Some(&cookie), Some(&csrf),
        ).await.status(), StatusCode::OK);
    }
    let revisions = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{child_id}/revisions"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(revisions.status(), StatusCode::OK);
    assert_eq!(body(revisions).await.as_array().unwrap().len(), 3);
    let restored = send(
        &app,
        "POST",
        &format!("/api/v1/pages/{child_id}/restore"),
        json!({"version": 1, "expectedVersion": 3}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(restored.status(), StatusCode::OK);
    assert_eq!(body(restored).await["version"], 4);
    let restored_page = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{child_id}"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(body(restored_page).await["markdown"], "child v1");

    let target = create_page(&app, &cookie, &csrf, space_id, None, "Target", "target").await;
    let target_id: Uuid = target["id"].as_str().unwrap().parse().unwrap();
    let stable_key = target["stableKey"].as_str().unwrap();
    let source_markdown =
        format!("[real](/pages/{stable_key})\n\n```md\n[fake](/pages/{stable_key})\n```");
    let source = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        None,
        "Source",
        &source_markdown,
    )
    .await;
    let source_id: Uuid = source["id"].as_str().unwrap().parse().unwrap();
    let link_rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT source_page_id, target_page_id FROM page_links ORDER BY source_page_id",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(link_rows, [(source_id, target_id)]);
    let backlinks = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{target_id}/backlinks"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(backlinks.status(), StatusCode::OK);
    assert_eq!(body(backlinks).await[0]["id"], source_id.to_string());
}

#[sqlx::test]
async fn repeated_front_inserts_trigger_atomic_rank_compaction(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login(&app).await;
    let space_id = create_space(&app, &cookie, &csrf, "Rank compaction").await;
    let anchor = create_page(&app, &cookie, &csrf, space_id, None, "Anchor", "anchor").await;
    let anchor_id: Uuid = anchor["id"].as_str().unwrap().parse().unwrap();

    for index in 0..45 {
        let response = send(
            &app,
            "POST",
            "/api/v1/pages",
            json!({
                "spaceId": space_id,
                "parentId": null,
                "beforeId": anchor_id,
                "title": format!("Before {index}"),
                "markdown": "body",
                "properties": {},
                "restricted": false
            }),
            Some(&cookie),
            Some(&csrf),
        )
        .await;
        assert_eq!(response.status(), StatusCode::CREATED, "insert {index}");
    }

    let (count, distinct_ranks): (i64, i64) =
        sqlx::query_as("SELECT count(*), count(DISTINCT sort_key) FROM pages WHERE space_id = $1")
            .bind(space_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!((count, distinct_ranks), (46, 46));
}

#[sqlx::test]
async fn restricted_ancestor_is_fail_closed_across_every_page_route(pool: PgPool) {
    let app = app(pool).await;
    let (admin_cookie, admin_csrf) = login(&app).await;
    let editor_id = create_user(&app, &admin_cookie, &admin_csrf, "editor@example.com").await;
    let space_id = create_space(&app, &admin_cookie, &admin_csrf, "Restricted").await;
    let grant = send(
        &app,
        "POST",
        &format!("/api/v1/spaces/{space_id}/grants"),
        json!({"subjectType": "user", "subjectId": editor_id, "role": "editor"}),
        Some(&admin_cookie),
        Some(&admin_csrf),
    )
    .await;
    assert_eq!(grant.status(), StatusCode::CREATED);
    let root = send(
        &app, "POST", "/api/v1/pages",
        json!({"spaceId": space_id, "parentId": null, "title": "Restricted root", "markdown": "root", "properties": {}, "restricted": true}),
        Some(&admin_cookie), Some(&admin_csrf),
    ).await;
    let root = body(root).await;
    let root_id: Uuid = root["id"].as_str().unwrap().parse().unwrap();
    let child = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        Some(root_id),
        "Hidden child",
        "hidden",
    )
    .await;
    let child_id: Uuid = child["id"].as_str().unwrap().parse().unwrap();
    let public = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        None,
        "Public",
        "public",
    )
    .await;
    let public_id: Uuid = public["id"].as_str().unwrap().parse().unwrap();
    let public_key = public["stableKey"].as_str().unwrap();
    let outer = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        None,
        "Visible outer",
        "outer",
    )
    .await;
    let outer_id: Uuid = outer["id"].as_str().unwrap().parse().unwrap();
    let nested_restricted = send(
        &app,
        "POST",
        "/api/v1/pages",
        json!({"spaceId": space_id, "parentId": outer_id, "title": "Nested restricted", "markdown": "secret", "properties": {}, "restricted": true}),
        Some(&admin_cookie),
        Some(&admin_csrf),
    )
    .await;
    assert_eq!(nested_restricted.status(), StatusCode::CREATED);
    send(
        &app, "PATCH", &format!("/api/v1/pages/{child_id}"),
        json!({"expectedVersion": 1, "title": "Hidden child", "markdown": format!("[public](/pages/{public_key})"), "properties": {}}),
        Some(&admin_cookie), Some(&admin_csrf),
    ).await;
    let (editor_cookie, editor_csrf) = login_as(&app, "editor@example.com").await;

    for (method, uri, payload) in [
        ("DELETE", format!("/api/v1/pages/{outer_id}"), json!({})),
        (
            "POST",
            format!("/api/v1/pages/{outer_id}/move"),
            json!({"spaceId": space_id, "parentId": null}),
        ),
        (
            "POST",
            format!("/api/v1/pages/{public_id}/move"),
            json!({"spaceId": space_id, "parentId": null, "beforeId": root_id}),
        ),
    ] {
        assert_eq!(
            send(
                &app,
                method,
                &uri,
                payload,
                Some(&editor_cookie),
                Some(&editor_csrf),
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
    }

    for (method, uri, payload, csrf) in [
        ("GET", format!("/api/v1/pages/{child_id}"), json!({}), None),
        (
            "PATCH",
            format!("/api/v1/pages/{child_id}"),
            json!({"expectedVersion": 2, "title": "x", "markdown": "x", "properties": {}}),
            Some(editor_csrf.as_str()),
        ),
        (
            "POST",
            format!("/api/v1/pages/{child_id}/move"),
            json!({"spaceId": space_id, "parentId": null}),
            Some(editor_csrf.as_str()),
        ),
        (
            "DELETE",
            format!("/api/v1/pages/{child_id}"),
            json!({}),
            Some(editor_csrf.as_str()),
        ),
        (
            "GET",
            format!("/api/v1/pages/{child_id}/revisions"),
            json!({}),
            None,
        ),
        (
            "POST",
            format!("/api/v1/pages/{child_id}/restore"),
            json!({"version": 1, "expectedVersion": 2}),
            Some(editor_csrf.as_str()),
        ),
    ] {
        assert_eq!(
            send(&app, method, &uri, payload, Some(&editor_cookie), csrf)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
    }
    let tree = send(
        &app,
        "GET",
        &format!("/api/v1/pages?spaceId={space_id}"),
        json!({}),
        Some(&editor_cookie),
        None,
    )
    .await;
    let tree = body(tree).await;
    let ids: Vec<String> = tree
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str().map(str::to_owned))
        .collect();
    assert!(!ids.contains(&root["id"].as_str().unwrap().to_owned()));
    assert!(!ids.contains(&child["id"].as_str().unwrap().to_owned()));
    let backlinks = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{public_id}/backlinks"),
        json!({}),
        Some(&editor_cookie),
        None,
    )
    .await;
    assert!(body(backlinks).await.as_array().unwrap().is_empty());
}
