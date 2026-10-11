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

async fn app(pool: PgPool) -> Router {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool);
    let master_key = MasterKey::from_base64(&STANDARD.encode([83_u8; 32])).unwrap();
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
        json!({"displayName": "Reader", "password": PASSWORD}),
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

async fn grant_viewer(app: &Router, cookie: &str, csrf: &str, space_id: Uuid, user_id: Uuid) {
    let response = send(
        app,
        "POST",
        &format!("/api/v1/spaces/{space_id}/grants"),
        json!({"subjectType": "user", "subjectId": user_id, "role": "viewer"}),
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
    restricted: bool,
    title: &str,
    markdown: &str,
) -> Uuid {
    let response = send(
        app,
        "POST",
        "/api/v1/pages",
        json!({
            "spaceId": space_id, "parentId": null, "title": title,
            "markdown": markdown, "properties": {}, "restricted": restricted
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

#[sqlx::test]
async fn search_indexes_ciphertext_only_and_tracks_save_archive_unarchive_and_restore(
    pool: PgPool,
) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let space_id = create_space(&app, &cookie, &csrf, "Search").await;
    let page_id = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Original \u{421}\u{415}\u{41a}\u{420}\u{415}\u{422}\u{41d}\u{42b}\u{419}_\u{41c}\u{410}\u{420}\u{41a}\u{415}\u{420}",
        "A private body",
    )
    .await;

    let marker = "\u{421}\u{415}\u{41a}\u{420}\u{415}\u{422}\u{41d}\u{42b}\u{419}_\u{41c}\u{410}\u{420}\u{41a}\u{415}\u{420}".as_bytes();
    let counts: Vec<i64> = sqlx::query_scalar(
        "SELECT count(*) FROM pages
          WHERE position($1::bytea in title_ciphertext) > 0 OR position($1::bytea in body_ciphertext) > 0
         UNION ALL SELECT count(*) FROM page_revisions
          WHERE position($1::bytea in title_ciphertext) > 0 OR position($1::bytea in body_ciphertext) > 0
         UNION ALL SELECT count(*) FROM blind_index_tokens WHERE position($1::bytea in token_hmac) > 0",
    )
    .bind(marker)
    .fetch_all(&pool)
    .await
    .unwrap()
    ;
    let plaintext_rows: i64 = counts.into_iter().sum();
    assert_eq!(
        plaintext_rows, 0,
        "the raw database contains no plaintext search marker"
    );

    let found = send(
        &app,
        "GET",
        "/api/v1/search?q=%D1%81%D0%B5%D0%BA%D1%80%D0%B5%D1%82%D0%BD%D1%8B%D0%B9",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(found.status(), StatusCode::OK);
    assert_eq!(body(found).await[0]["pageId"], page_id.to_string());

    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/pages/{page_id}"),
            json!({}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let archived = send(
        &app,
        "GET",
        "/api/v1/search?q=%D1%81%D0%B5%D0%BA%D1%80%D0%B5%D1%82%D0%BD%D1%8B%D0%B9",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(body(archived).await, json!([]));
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page_id}/unarchive"),
            json!({}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    let restored_to_search = send(
        &app,
        "GET",
        "/api/v1/search?q=%D1%81%D0%B5%D0%BA%D1%80%D0%B5%D1%82%D0%BD%D1%8B%D0%B9",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(
        body(restored_to_search).await[0]["pageId"],
        page_id.to_string()
    );
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/pages/{page_id}"),
            json!({
                "expectedVersion": 1,
                "title": "Replacement marker",
                "markdown": "The revised body",
                "properties": {}
            }),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .status(),
        StatusCode::OK
    );
    let stale_index = send(
        &app,
        "GET",
        "/api/v1/search?q=%D1%81%D0%B5%D0%BA%D1%80%D0%B5%D1%82%D0%BD%D1%8B%D0%B9",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(body(stale_index).await, json!([]));
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page_id}/restore"),
            json!({"version": 1, "expectedVersion": 2}),
            Some(&cookie),
            Some(&csrf)
        )
        .await
        .status(),
        StatusCode::OK
    );
    let restored_revision = send(
        &app,
        "GET",
        "/api/v1/search?q=%D1%81%D0%B5%D0%BA%D1%80%D0%B5%D1%82%D0%BD%D1%8B%D0%B9",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(
        body(restored_revision).await[0]["pageId"],
        page_id.to_string()
    );
}

#[sqlx::test]
async fn search_applies_acl_before_the_candidate_cap_and_rechecks_restricted_pages(pool: PgPool) {
    let app = app(pool).await;
    let (admin_cookie, admin_csrf) = login_as(&app, "admin@example.com").await;
    let reader_id = create_user(&app, &admin_cookie, &admin_csrf, "reader@example.com").await;
    let space_id = create_space(&app, &admin_cookie, &admin_csrf, "ACL search").await;
    grant_viewer(&app, &admin_cookie, &admin_csrf, space_id, reader_id).await;
    let visible_page = create_page(
        &app,
        &admin_cookie,
        &admin_csrf,
        space_id,
        false,
        "Visible cap marker",
        "Visible result",
    )
    .await;
    for number in 0..101 {
        create_page(
            &app,
            &admin_cookie,
            &admin_csrf,
            space_id,
            true,
            &format!("Restricted cap marker {number}"),
            "Must never be decrypted for this reader",
        )
        .await;
    }
    let (reader_cookie, _) = login_as(&app, "reader@example.com").await;

    let response = send(
        &app,
        "GET",
        &format!("/api/v1/search?q=marker&space={space_id}"),
        json!({}),
        Some(&reader_cookie),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let hits = body(response).await;
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["pageId"], visible_page.to_string());
    assert_eq!(hits[0]["title"], "Visible cap marker");
}

#[sqlx::test]
async fn search_excludes_archived_spaces_before_the_candidate_cap(pool: PgPool) {
    let app = app(pool).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let active_space = create_space(&app, &cookie, &csrf, "Active search").await;
    let visible_page = create_page(
        &app,
        &cookie,
        &csrf,
        active_space,
        false,
        "Visible archived-space marker",
        "Active result",
    )
    .await;
    let archived_space = create_space(&app, &cookie, &csrf, "Archived search").await;
    for number in 0..101 {
        create_page(
            &app,
            &cookie,
            &csrf,
            archived_space,
            false,
            &format!("Archived-space marker {number}"),
            "This page belongs to an archived space",
        )
        .await;
    }
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/spaces/{archived_space}"),
            json!({}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );

    let response = send(
        &app,
        "GET",
        "/api/v1/search?q=marker",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let hits = body(response).await;
    assert_eq!(hits.as_array().unwrap().len(), 1);
    assert_eq!(hits[0]["pageId"], visible_page.to_string());
}

#[sqlx::test]
async fn search_ranks_phrase_and_field_matches_then_honors_filters(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let space_id = create_space(&app, &cookie, &csrf, "Ranking").await;
    let title_phrase = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Plan guide",
        "No body match needed",
    )
    .await;
    let title_tokens = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Plan guidebook",
        "No body match needed",
    )
    .await;
    let body_phrase = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Notes",
        "This has a plan guide in its body",
    )
    .await;
    let body_tokens = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Other notes",
        "This has a plan for a revised guide",
    )
    .await;
    let split_fields = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Plan",
        "Guide spread across title and body",
    )
    .await;
    let false_match = create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Plan without the other query word",
        "Unrelated body content",
    )
    .await;

    let response = send(
        &app,
        "GET",
        &format!("/api/v1/search?q=plan+guide&space={space_id}"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let ids: Vec<Uuid> = body(response)
        .await
        .as_array()
        .unwrap()
        .iter()
        .map(|hit| hit["pageId"].as_str().unwrap().parse().unwrap())
        .collect();
    assert_eq!(
        ids,
        [
            title_phrase,
            title_tokens,
            body_phrase,
            body_tokens,
            split_fields
        ]
    );
    assert!(!ids.contains(&false_match));

    let author_id: Uuid = sqlx::query_scalar("SELECT updated_by FROM pages WHERE id = $1")
        .bind(title_phrase)
        .fetch_one(&pool)
        .await
        .unwrap();
    let author = send(
        &app,
        "GET",
        &format!("/api/v1/search?q=plan&space={space_id}&author={author_id}"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(author.status(), StatusCode::OK);
    assert_eq!(body(author).await.as_array().unwrap().len(), 6);

    let after = send(
        &app,
        "GET",
        &format!("/api/v1/search?q=plan&space={space_id}&from=2100-01-01T00%3A00%3A00Z"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(body(after).await, json!([]));
    let before = send(
        &app,
        "GET",
        &format!("/api/v1/search?q=plan&space={space_id}&to=2000-01-01T00%3A00%3A00Z"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(body(before).await, json!([]));
}

#[sqlx::test]
async fn search_snippet_preserves_a_late_nfkc_normalized_match(pool: PgPool) {
    let app = app(pool).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let space_id = create_space(&app, &cookie, &csrf, "Snippet").await;
    let content = format!(
        "{} \u{ff2f}\u{ff26}\u{ff26}\u{ff29}\u{ff23}\u{ff25} remains original",
        "filler ".repeat(30)
    );
    create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Normalized snippet",
        &content,
    )
    .await;

    let response = send(
        &app,
        "GET",
        &format!("/api/v1/search?q=office&space={space_id}"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        body(response).await[0]["snippet"]
            .as_str()
            .unwrap()
            .contains("\u{ff2f}\u{ff26}\u{ff26}\u{ff29}\u{ff23}\u{ff25}")
    );
}

#[sqlx::test]
async fn search_completes_with_only_its_transaction_connection_available(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login_as(&app, "admin@example.com").await;
    let space_id = create_space(&app, &cookie, &csrf, "Saturated pool").await;
    create_page(
        &app,
        &cookie,
        &csrf,
        space_id,
        false,
        "Only connection",
        "searchable marker",
    )
    .await;

    let mut held = Vec::new();
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(250), pool.acquire()).await {
            Ok(Ok(connection)) => held.push(connection),
            Ok(Err(error)) => panic!("failed while saturating the pool: {error}"),
            Err(_) => break,
        }
    }
    assert!(!held.is_empty(), "the test must saturate at least one slot");
    drop(held.pop());

    let response = tokio::time::timeout(
        std::time::Duration::from_secs(2),
        send(
            &app,
            "GET",
            "/api/v1/search?q=searchable",
            json!({}),
            Some(&cookie),
            None,
        ),
    )
    .await
    .expect("search attempted to acquire a second pool connection");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body(response).await.as_array().unwrap().len(), 1);
}
