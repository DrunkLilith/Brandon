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

async fn setup(pool: PgPool) -> Router {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool);
    let key = MasterKey::from_base64(&STANDARD.encode([91; 32])).unwrap();
    BootstrapService::run(
        &store,
        &key,
        BootstrapInput {
            workspace_name: "Audit".into(),
            email: "admin@example.com".into(),
            display_name: "Admin".into(),
            password: "correct horse battery staple".into(),
        },
    )
    .await
    .unwrap();
    build_router(AppState::new(store, key, "https://knowledge.local", "").unwrap())
}
async fn login(app: &Router) -> String {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header(header::CONTENT_TYPE, "application/json")
                .header(header::HOST, "knowledge.local")
                .header(header::ORIGIN, "https://knowledge.local")
                .header("sec-fetch-site", "same-origin")
                .body(Body::from(
                    json!({"email":"admin@example.com","password":"correct horse battery staple"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    response.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned()
}
async fn get(app: &Router, cookie: &str, uri: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}
#[sqlx::test]
async fn audit_filters_cursor_and_admin_disclosure_are_committed(pool: PgPool) {
    let app = setup(pool.clone()).await;
    let cookie = login(&app).await;
    let response = get(&app, &cookie, "/api/v1/audit-events?limit=1").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let data: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(data["events"].as_array().unwrap().len(), 1);
    let first = data["events"][0]["id"].as_str().unwrap();
    let response = get(
        &app,
        &cookie,
        &format!("/api/v1/audit-events?after={first}&limit=100"),
    )
    .await;
    let data: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert!(
        data["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["id"] != first)
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='audit.admin_access'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 2);
    let response = get(
        &app,
        &cookie,
        "/api/v1/audit-events?type=auth.login_succeeded&from=2100-01-01T00%3A00%3A00Z",
    )
    .await;
    let data: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 65536).await.unwrap()).unwrap();
    assert_eq!(data["events"], json!([]));
    sqlx::query("UPDATE workspace_members SET role='member'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        get(&app, &cookie, "/api/v1/audit-events").await.status(),
        StatusCode::FORBIDDEN
    );
}
