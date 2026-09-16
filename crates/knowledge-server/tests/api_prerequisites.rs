use std::sync::Arc;

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use bytes::Bytes;
use knowledge_crypto::MasterKey;
use knowledge_server::{
    bootstrap::{BootstrapInput, BootstrapService},
    build_router,
    state::AppState,
};
use knowledge_store::{
    MIGRATOR, Store,
    attachments::{MemoryObjectStore, ObjectStore, ObjectUpload, StorageFuture},
};
use serde_json::{Value, json};
use sqlx::PgPool;
use tower::ServiceExt;
use uuid::Uuid;

const PASSWORD: &str = "correct horse battery staple";
const ORIGIN: &str = "https://knowledge.local";

struct UnhealthyObjectStore(MemoryObjectStore);

impl ObjectStore for UnhealthyObjectStore {
    fn health(&self) -> StorageFuture<'_, ()> {
        Box::pin(async { Err(knowledge_store::attachments::StorageError) })
    }

    fn start(&self, key: Uuid) -> StorageFuture<'_, Box<dyn ObjectUpload>> {
        self.0.start(key)
    }

    fn read_range(&self, key: Uuid, range: std::ops::Range<u64>) -> StorageFuture<'_, Bytes> {
        self.0.read_range(key, range)
    }

    fn size(&self, key: Uuid) -> StorageFuture<'_, u64> {
        self.0.size(key)
    }

    fn delete(&self, key: Uuid) -> StorageFuture<'_, ()> {
        self.0.delete(key)
    }

    fn abort_upload<'a>(&'a self, key: Uuid, id: &'a str) -> StorageFuture<'a, ()> {
        self.0.abort_upload(key, id)
    }
}

async fn app(pool: PgPool) -> Router {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool);
    let master_key = MasterKey::from_base64(&STANDARD.encode([93_u8; 32])).unwrap();
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
    build_router(
        AppState::new(store, master_key, ORIGIN, "")
            .unwrap()
            .with_object_store(Arc::new(MemoryObjectStore::default())),
    )
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    payload: Value,
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
        .body(Body::from(payload.to_string()))
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
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}

async fn login(app: &Router) -> (String, String) {
    let response = send(
        app,
        "POST",
        "/api/v1/auth/login",
        json!({"email":"admin@example.com","password":PASSWORD}),
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
    let csrf = json_body(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/csrf")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    (cookie, csrf)
}

#[sqlx::test]
async fn persisted_settings_readiness_recovery_subjects_and_revision_payload(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf) = login(&app).await;

    let settings = send(
        &app,
        "GET",
        "/api/v1/admin/settings",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(settings.status(), StatusCode::OK);
    assert_eq!(settings.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(
        json_body(settings).await,
        json!({"attachmentRecoveryDays":30,"exportRetentionHours":24})
    );
    let updated = send(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        json!({"attachmentRecoveryDays":45,"exportRetentionHours":36}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(updated.status(), StatusCode::OK);
    assert_eq!(json_body(updated).await["exportRetentionHours"], 36);
    assert_eq!(
        send(
            &app,
            "PATCH",
            "/api/v1/admin/settings",
            json!({"attachmentRecoveryDays":0,"exportRetentionHours":36}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );

    let ready = send(&app, "GET", "/api/v1/health/ready", json!({}), None, None).await;
    assert_eq!(ready.status(), StatusCode::OK);
    assert_eq!(
        json_body(ready).await,
        json!({"status":"ready","database":true,"migrations":true,"masterKey":true,"objectStorage":true})
    );
    let status = send(
        &app,
        "GET",
        "/api/v1/admin/status",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(status.status(), StatusCode::OK);
    assert_eq!(status.headers()[header::CACHE_CONTROL], "no-store");
    assert!(
        json_body(status).await["components"]["database"]
            .as_bool()
            .unwrap()
    );

    let unhealthy = build_router(
        AppState::new(
            Store::from_pool(pool.clone()),
            MasterKey::from_base64(&STANDARD.encode([93_u8; 32])).unwrap(),
            ORIGIN,
            "",
        )
        .unwrap()
        .with_object_store(Arc::new(UnhealthyObjectStore(MemoryObjectStore::default()))),
    );
    let unhealthy_status = send(
        &unhealthy,
        "GET",
        "/api/v1/admin/status",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(unhealthy_status.status(), StatusCode::OK);
    assert_eq!(
        json_body(unhealthy_status).await["components"]["objectStorage"],
        false
    );

    let unavailable = build_router(
        AppState::new(
            Store::from_pool(pool.clone()),
            MasterKey::from_base64(&STANDARD.encode([93_u8; 32])).unwrap(),
            ORIGIN,
            "",
        )
        .unwrap(),
    );
    let ready = send(
        &unavailable,
        "GET",
        "/api/v1/health/ready",
        json!({}),
        None,
        None,
    )
    .await;
    assert_eq!(ready.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        json_body(ready).await,
        json!({"status":"not_ready","database":true,"migrations":true,"masterKey":true,"objectStorage":false})
    );

    let group = send(
        &app,
        "POST",
        "/api/v1/groups",
        json!({"name":"Archived group","description":null}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    let group_id = json_body(group).await["id"].as_str().unwrap().to_owned();
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
    let archived_groups = send(
        &app,
        "GET",
        "/api/v1/groups/archived",
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(archived_groups.status(), StatusCode::OK);
    assert_eq!(json_body(archived_groups).await[0]["id"], group_id);
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/groups/{group_id}/restore"),
            json!({}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );

    let space = send(
        &app,
        "POST",
        "/api/v1/spaces",
        json!({"name":"Recovery space","description":null}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    let space_id = json_body(space).await["id"].as_str().unwrap().to_owned();
    let invitation = send(
        &app,
        "POST",
        "/api/v1/invitations",
        json!({"email":"owner@example.com","workspaceRole":"member"}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    let token = json_body(invitation).await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let owner = send(
        &app,
        "POST",
        &format!("/api/v1/invitations/{token}/accept"),
        json!({"displayName":"Space owner","password":PASSWORD}),
        None,
        None,
    )
    .await;
    let owner_id = json_body(owner).await["id"].as_str().unwrap().to_owned();
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/spaces/{space_id}/grants"),
            json!({"subjectType":"user","subjectId":owner_id,"role":"owner"}),
            Some(&cookie),
            Some(&csrf),
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    let owner_login = send(
        &app,
        "POST",
        "/api/v1/auth/login",
        json!({"email":"owner@example.com","password":PASSWORD}),
        None,
        None,
    )
    .await;
    let owner_cookie = owner_login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let owner_csrf = json_body(
        app.clone()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/auth/csrf")
                    .header(header::COOKIE, &owner_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap(),
    )
    .await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let subjects = send(
        &app,
        "GET",
        &format!("/api/v1/spaces/{space_id}/subjects"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(subjects.status(), StatusCode::OK);
    let subjects = json_body(subjects).await;
    assert!(
        subjects
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["type"] == "user")
    );
    assert!(
        subjects
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["type"] == "group")
    );

    let page = send(
        &app,
        "POST",
        "/api/v1/pages",
        json!({"spaceId":space_id,"parentId":null,"title":"History title","markdown":"# history body","properties":{"kind":"history"},"restricted":false}),
        Some(&cookie),
        Some(&csrf),
    )
    .await;
    assert_eq!(page.status(), StatusCode::CREATED);
    let page_id = json_body(page).await["id"].as_str().unwrap().to_owned();
    let revision = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{page_id}/revisions/1"),
        json!({}),
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(revision.status(), StatusCode::OK);
    assert_eq!(revision.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(json_body(revision).await["markdown"], "# history body");

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
    let archived_spaces = send(
        &app,
        "GET",
        "/api/v1/spaces/archived",
        json!({}),
        Some(&owner_cookie),
        None,
    )
    .await;
    assert_eq!(archived_spaces.status(), StatusCode::OK);
    assert_eq!(json_body(archived_spaces).await[0]["id"], space_id);
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/spaces/{space_id}/restore"),
            json!({}),
            Some(&owner_cookie),
            Some(&owner_csrf),
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );

    let event_types: Vec<String> =
        sqlx::query_scalar("SELECT event_type FROM audit_events ORDER BY created_at, id")
            .fetch_all(&pool)
            .await
            .unwrap();
    for required in ["settings.updated", "group.restored", "space.restored"] {
        assert!(
            event_types.iter().any(|event| event == required),
            "missing {required}"
        );
    }
    let row: (i32, i32) = sqlx::query_as(
        "SELECT attachment_recovery_days, export_retention_hours FROM operational_settings",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, (45, 36));
    assert!(Uuid::parse_str(&group_id).is_ok());
}
