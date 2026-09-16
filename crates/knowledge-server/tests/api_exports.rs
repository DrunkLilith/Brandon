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
    operations::JobRunner,
    state::AppState,
};
use knowledge_store::{
    MIGRATOR, Store,
    attachments::{
        MemoryObjectStore, ObjectStore, ObjectUpload, StorageError, StorageFuture, lock_object,
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{
    io::Read,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use tower::ServiceExt;
use uuid::Uuid;

const ORIGIN: &str = "https://knowledge.local";
async fn setup(pool: PgPool) -> (Router, AppState, PgPool, Arc<MemoryObjectStore>) {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool.clone());
    let key = MasterKey::from_base64(&STANDARD.encode([97; 32])).unwrap();
    BootstrapService::run(
        &store,
        &key,
        BootstrapInput {
            workspace_name: "Exports".into(),
            email: "admin@example.com".into(),
            display_name: "Admin".into(),
            password: "correct horse battery staple".into(),
        },
    )
    .await
    .unwrap();
    let objects = Arc::new(MemoryObjectStore::default());
    let state = AppState::new(store, key, ORIGIN, "")
        .unwrap()
        .with_object_store(objects.clone());
    (build_router(state.clone()), state, pool, objects)
}
async fn setup_with_objects(
    pool: PgPool,
    objects: Arc<dyn ObjectStore>,
) -> (Router, AppState, PgPool) {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool.clone());
    let key = MasterKey::from_base64(&STANDARD.encode([97; 32])).unwrap();
    BootstrapService::run(
        &store,
        &key,
        BootstrapInput {
            workspace_name: "Exports".into(),
            email: "admin@example.com".into(),
            display_name: "Admin".into(),
            password: "correct horse battery staple".into(),
        },
    )
    .await
    .unwrap();
    let state = AppState::new(store, key, ORIGIN, "")
        .unwrap()
        .with_object_store(objects);
    (build_router(state.clone()), state, pool)
}

struct FailDeleteStore {
    inner: MemoryObjectStore,
    fail_key: Arc<Mutex<Option<Uuid>>>,
}

struct StallReadStore {
    inner: MemoryObjectStore,
    stalled: Arc<AtomicBool>,
    entered: Arc<AtomicBool>,
}
impl ObjectStore for StallReadStore {
    fn start(&self, key: Uuid) -> StorageFuture<'_, Box<dyn ObjectUpload>> {
        self.inner.start(key)
    }
    fn read_range(
        &self,
        key: Uuid,
        range: std::ops::Range<u64>,
    ) -> StorageFuture<'_, bytes::Bytes> {
        if self.stalled.load(Ordering::SeqCst) {
            let entered = self.entered.clone();
            Box::pin(async move {
                entered.store(true, Ordering::SeqCst);
                std::future::pending::<Result<bytes::Bytes, StorageError>>().await
            })
        } else {
            self.inner.read_range(key, range)
        }
    }
    fn size(&self, key: Uuid) -> StorageFuture<'_, u64> {
        self.inner.size(key)
    }
    fn delete(&self, key: Uuid) -> StorageFuture<'_, ()> {
        self.inner.delete(key)
    }
    fn abort_upload<'a>(&'a self, key: Uuid, id: &'a str) -> StorageFuture<'a, ()> {
        self.inner.abort_upload(key, id)
    }
}
impl ObjectStore for FailDeleteStore {
    fn start(&self, key: Uuid) -> StorageFuture<'_, Box<dyn ObjectUpload>> {
        self.inner.start(key)
    }
    fn read_range(
        &self,
        key: Uuid,
        range: std::ops::Range<u64>,
    ) -> StorageFuture<'_, bytes::Bytes> {
        self.inner.read_range(key, range)
    }
    fn size(&self, key: Uuid) -> StorageFuture<'_, u64> {
        self.inner.size(key)
    }
    fn delete(&self, key: Uuid) -> StorageFuture<'_, ()> {
        if *self.fail_key.lock().unwrap() == Some(key) {
            Box::pin(async { Err(StorageError) })
        } else {
            self.inner.delete(key)
        }
    }
    fn abort_upload<'a>(&'a self, key: Uuid, id: &'a str) -> StorageFuture<'a, ()> {
        self.inner.abort_upload(key, id)
    }
}
async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    value: Value,
    cookie: Option<&str>,
    csrf: Option<&str>,
    key: Option<&str>,
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
    if let Some(v) = cookie {
        request
            .headers_mut()
            .insert(header::COOKIE, v.parse().unwrap());
    }
    if let Some(v) = csrf {
        request
            .headers_mut()
            .insert("x-csrf-token", v.parse().unwrap());
    }
    if let Some(v) = key {
        request
            .headers_mut()
            .insert("idempotency-key", v.parse().unwrap());
    }
    app.clone().oneshot(request).await.unwrap()
}
async fn value(response: axum::response::Response) -> Value {
    serde_json::from_slice(
        &to_bytes(response.into_body(), 2 * 1024 * 1024)
            .await
            .unwrap(),
    )
    .unwrap()
}
async fn login(app: &Router) -> (String, String) {
    let response = send(
        app,
        "POST",
        "/api/v1/auth/login",
        json!({"email":"admin@example.com","password":"correct horse battery staple"}),
        None,
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
    let response = send(
        app,
        "GET",
        "/api/v1/auth/csrf",
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    let csrf = value(response).await["token"].as_str().unwrap().to_owned();
    (cookie, csrf)
}
async fn create_space_page(app: &Router, cookie: &str, csrf: &str) -> (Uuid, Uuid) {
    let response = send(
        app,
        "POST",
        "/api/v1/spaces",
        json!({"name":"Offline Space","description":"Export me"}),
        Some(cookie),
        Some(csrf),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let space: Uuid = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let response=send(app,"POST","/api/v1/pages",json!({"spaceId":space,"title":"Export Page","markdown":"# Portable\n\nsecret body\n","properties":{"tag":"offline"},"restricted":false}),Some(cookie),Some(csrf),None).await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let page = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    (space, page)
}
async fn upload_attachment(app: &Router, cookie: &str, csrf: &str, page: Uuid) {
    let bytes = b"attachment";
    let mut body = b"--files\r\nContent-Disposition: form-data; name=\"file\"; filename=\"file.txt\"\r\nContent-Type: text/plain\r\n\r\n".to_vec();
    body.extend_from_slice(bytes);
    body.extend_from_slice(b"\r\n--files--\r\n");
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!(
                    "/api/v1/pages/{page}/attachments?size={}",
                    bytes.len()
                ))
                .header(header::HOST, "knowledge.local")
                .header(header::ORIGIN, ORIGIN)
                .header("sec-fetch-site", "same-origin")
                .header(header::COOKIE, cookie)
                .header("x-csrf-token", csrf)
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=files")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
}

#[sqlx::test]
async fn export_request_is_idempotent_encrypted_downloadable_audited_and_revocation_safe(
    pool: PgPool,
) {
    let (app, state, pool, objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, page) = create_space_page(&app, &cookie, &csrf).await;
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/v1/exports",
            json!({"spaceId":space}),
            Some(&cookie),
            Some(&csrf),
            None
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let request_key = Uuid::now_v7().to_string();
    let first = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&request_key),
    )
    .await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);
    let export_id: Uuid = value(first).await["id"].as_str().unwrap().parse().unwrap();
    let replay = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&request_key),
    )
    .await;
    assert_eq!(value(replay).await["id"], export_id.to_string());
    assert!(
        JobRunner::new(Uuid::now_v7())
            .run_once(&state)
            .await
            .unwrap()
    );
    let listed = send(
        &app,
        "GET",
        "/api/v1/exports",
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(listed.status(), StatusCode::OK);
    assert_eq!(value(listed).await["exports"][0]["status"], "ready");
    let stored: (Uuid, i64) =
        sqlx::query_as("SELECT object_key,ciphertext_size FROM exports WHERE id=$1")
            .bind(export_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    let encrypted = objects
        .read_range(stored.0, 0..stored.1 as u64)
        .await
        .unwrap();
    assert!(
        !encrypted
            .windows(b"secret body".len())
            .any(|window| window == b"secret body")
    );
    assert!(!encrypted.starts_with(b"space-"));
    let response = send(
        &app,
        "GET",
        &format!("/api/v1/exports/{export_id}/download"),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let archive = to_bytes(response.into_body(), 2 * 1024 * 1024)
        .await
        .unwrap();
    let mut tar = tar::Archive::new(archive.as_ref());
    let mut markdown = String::new();
    for entry in tar.entries().unwrap() {
        let mut entry = entry.unwrap();
        if entry
            .path()
            .unwrap()
            .to_string_lossy()
            .ends_with(&format!("pages/{page}.md"))
        {
            entry.read_to_string(&mut markdown).unwrap();
        }
    }
    assert!(markdown.contains("# Portable"));
    assert!(markdown.contains(&page.to_string()));
    let counts: (i64, i64) =
        sqlx::query_as("SELECT (SELECT count(*) FROM exports),(SELECT count(*) FROM jobs)")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(counts, (1, 1));
    let audits:i64=sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE event_type IN ('export.requested','export.completed','export.downloaded')").fetch_one(&pool).await.unwrap();
    assert_eq!(audits, 3);
    sqlx::query("UPDATE exports SET expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(export_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        JobRunner::new(Uuid::now_v7())
            .run_maintenance_once(&state)
            .await
            .unwrap(),
        1
    );
    let status: String = sqlx::query_scalar("SELECT status FROM exports WHERE id=$1")
        .bind(export_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "expired");
    let expired_audit: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='export.expired' AND target_id=$1",
    )
    .bind(export_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(expired_audit, 1);
    sqlx::query("UPDATE workspace_members SET role='member'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM space_grants WHERE space_id=$1")
        .bind(space)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/exports/{export_id}/download"),
            json!({}),
            Some(&cookie),
            None,
            None
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn export_requests_have_an_independent_actor_rate_limit(pool: PgPool) {
    let (app, _state, _pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    for attempt in 1..=11 {
        let response = send(
            &app,
            "POST",
            "/api/v1/exports",
            json!({"spaceId":space}),
            Some(&cookie),
            Some(&csrf),
            Some(&Uuid::now_v7().to_string()),
        )
        .await;
        assert_eq!(
            response.status(),
            if attempt <= 10 {
                StatusCode::ACCEPTED
            } else {
                StatusCode::TOO_MANY_REQUESTS
            },
            "attempt {attempt}"
        );
    }
}

#[sqlx::test]
async fn corrupt_job_payload_retries_with_fixed_error_then_terminalizes(pool: PgPool) {
    let (app, state, pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let response = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let export: Uuid = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    sqlx::query("UPDATE jobs SET payload_ciphertext=decode('00','hex') WHERE id=(SELECT job_id FROM exports WHERE id=$1)").bind(export).execute(&pool).await.unwrap();
    let runner = JobRunner::new(Uuid::now_v7());
    for attempt in 1..=5 {
        assert!(runner.run_once(&state).await.unwrap());
        let row:(String,i32,Option<String>)=sqlx::query_as("SELECT j.status,j.attempts,j.error_summary FROM jobs j JOIN exports e ON e.job_id=j.id WHERE e.id=$1").bind(export).fetch_one(&pool).await.unwrap();
        assert_eq!(row.1, attempt);
        assert_eq!(row.2.as_deref(), Some("operation_failed"));
        if attempt < 5 {
            assert_eq!(row.0, "queued");
            sqlx::query("UPDATE jobs SET run_after=now()-interval '1 second' WHERE id=(SELECT job_id FROM exports WHERE id=$1)").bind(export).execute(&pool).await.unwrap();
        } else {
            assert_eq!(row.0, "failed");
        }
    }
    let failed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='export.failed' AND target_id=$1",
    )
    .bind(export)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(failed, 1);
}

#[sqlx::test]
async fn export_list_applies_current_space_acl_before_its_limit(pool: PgPool) {
    let (app, _state, pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (visible_space, _) = create_space_page(&app, &cookie, &csrf).await;
    let (hidden_space, _) = create_space_page(&app, &cookie, &csrf).await;
    let visible = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":visible_space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let visible_id = value(visible).await["id"].as_str().unwrap().to_owned();
    let (workspace, user): (Uuid, Uuid) =
        sqlx::query_as("SELECT workspace_id,id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query(
        "WITH generated AS MATERIALIZED (
             SELECT gen_random_uuid() export_id,gen_random_uuid() job_id
               FROM generate_series(1,1001)
         ), inserted_jobs AS (
             INSERT INTO jobs(id,workspace_id,job_type,payload_ciphertext,payload_nonce,payload_key_version,status)
             SELECT job_id,$1,'export',decode('00','hex'),decode('000000000000000000000000','hex'),1,'queued'
               FROM generated RETURNING id
         )
         INSERT INTO exports(id,workspace_id,space_id,requested_by,job_id,status)
         SELECT export_id,$1,$2,$3,job_id,'queued' FROM generated",
    )
    .bind(workspace)
    .bind(hidden_space)
    .bind(user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE workspace_members SET role='member' WHERE workspace_id=$1 AND user_id=$2")
        .bind(workspace)
        .bind(user)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM space_grants WHERE space_id=$1")
        .bind(hidden_space)
        .execute(&pool)
        .await
        .unwrap();

    let response = send(
        &app,
        "GET",
        "/api/v1/exports",
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let body = value(response).await;
    assert_eq!(body["exports"].as_array().unwrap().len(), 1);
    assert_eq!(body["exports"][0]["id"], visible_id);
}

#[sqlx::test]
async fn download_waits_for_object_lock_before_authorization_and_audit(pool: PgPool) {
    let (app, state, pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let response = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let export: Uuid = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        JobRunner::new(Uuid::now_v7())
            .run_once(&state)
            .await
            .unwrap()
    );
    let object: Uuid = sqlx::query_scalar("SELECT object_key FROM exports WHERE id=$1")
        .bind(export)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE workspace_members SET role='member'")
        .execute(&pool)
        .await
        .unwrap();
    let guard = lock_object(&pool, object).await.unwrap();
    let pending_app = app.clone();
    let pending_cookie = cookie.clone();
    let download = tokio::spawn(async move {
        send(
            &pending_app,
            "GET",
            &format!("/api/v1/exports/{export}/download"),
            json!({}),
            Some(&pending_cookie),
            None,
            None,
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert!(
        !download.is_finished(),
        "the object lock must gate disclosure"
    );
    let premature_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='export.downloaded' AND target_id=$1",
    )
    .bind(export)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        premature_audits, 0,
        "download audit must not commit before the lock and final authorization"
    );
    sqlx::query("DELETE FROM space_grants WHERE space_id=$1")
        .bind(space)
        .execute(&pool)
        .await
        .unwrap();
    drop(guard);
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), download)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='export.downloaded' AND target_id=$1",
    )
    .bind(export)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audits, 0);
}

#[sqlx::test]
async fn export_list_uses_an_exclusive_acl_filtered_cursor(pool: PgPool) {
    let (app, _state, _pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    for _ in 0..3 {
        assert_eq!(
            send(
                &app,
                "POST",
                "/api/v1/exports",
                json!({"spaceId":space}),
                Some(&cookie),
                Some(&csrf),
                Some(&Uuid::now_v7().to_string()),
            )
            .await
            .status(),
            StatusCode::ACCEPTED
        );
    }

    let first = value(
        send(
            &app,
            "GET",
            "/api/v1/exports?limit=1",
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await,
    )
    .await;
    assert_eq!(first["exports"].as_array().unwrap().len(), 1);
    let cursor = first["nextCursor"].as_str().unwrap();
    assert_eq!(first["exports"][0]["id"], cursor);
    let second = value(
        send(
            &app,
            "GET",
            &format!("/api/v1/exports?limit=1&after={cursor}"),
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await,
    )
    .await;
    assert_eq!(second["exports"].as_array().unwrap().len(), 1);
    assert_ne!(second["exports"][0]["id"], first["exports"][0]["id"]);
}

#[sqlx::test]
async fn workspace_key_failure_is_fenced_retried_and_terminalized(pool: PgPool) {
    let (app, state, pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let response = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let export: Uuid = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    sqlx::query("UPDATE workspaces SET wrapped_key_ciphertext=decode('00','hex')")
        .execute(&pool)
        .await
        .unwrap();
    let runner = JobRunner::new(Uuid::now_v7());
    for attempt in 1..=5 {
        assert!(runner.run_once(&state).await.unwrap());
        let row: (String, i32, Option<String>, String) = sqlx::query_as(
            "SELECT j.status,j.attempts,j.error_summary,e.status
               FROM jobs j JOIN exports e ON e.job_id=j.id WHERE e.id=$1",
        )
        .bind(export)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.1, attempt);
        assert_eq!(row.2.as_deref(), Some("operation_failed"));
        if attempt < 5 {
            assert_eq!((row.0.as_str(), row.3.as_str()), ("queued", "queued"));
            sqlx::query("UPDATE jobs SET run_after=now()-interval '1 second' WHERE id=(SELECT job_id FROM exports WHERE id=$1)")
                .bind(export)
                .execute(&pool)
                .await
                .unwrap();
        } else {
            assert_eq!((row.0.as_str(), row.3.as_str()), ("failed", "failed"));
        }
    }
    assert!(!runner.run_once(&state).await.unwrap());
}

#[sqlx::test]
async fn maintenance_attempts_expiry_after_an_independent_cleanup_failure(pool: PgPool) {
    let fail_key = Arc::new(Mutex::new(None));
    let objects = Arc::new(FailDeleteStore {
        inner: MemoryObjectStore::default(),
        fail_key: fail_key.clone(),
    });
    let (app, state, pool) = setup_with_objects(pool, objects).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let response = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let export: Uuid = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let runner = JobRunner::new(Uuid::now_v7());
    assert!(runner.run_once(&state).await.unwrap());
    sqlx::query("UPDATE exports SET expires_at=now()-interval '1 second' WHERE id=$1")
        .bind(export)
        .execute(&pool)
        .await
        .unwrap();
    let orphan = Uuid::now_v7();
    let workspace: Uuid = sqlx::query_scalar("SELECT id FROM workspaces")
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO attachment_staging(object_key,workspace_id,attachment_id,expires_at) VALUES($1,$2,$3,now()-interval '1 second')")
        .bind(orphan)
        .bind(workspace)
        .bind(Uuid::now_v7())
        .execute(&pool)
        .await
        .unwrap();
    *fail_key.lock().unwrap() = Some(orphan);

    assert!(runner.run_maintenance_once(&state).await.is_err());
    let status: String = sqlx::query_scalar("SELECT status FROM exports WHERE id=$1")
        .bind(export)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        status, "expired",
        "expiry must run even when attachment cleanup fails"
    );
}

#[sqlx::test]
async fn expiry_skips_an_actively_streamed_object_without_waiting(pool: PgPool) {
    let (app, state, pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let response = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let export: Uuid = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    let runner = JobRunner::new(Uuid::now_v7());
    assert!(runner.run_once(&state).await.unwrap());
    let object: Uuid = sqlx::query_scalar(
        "UPDATE exports SET expires_at=now()-interval '1 second' WHERE id=$1 RETURNING object_key",
    )
    .bind(export)
    .fetch_one(&pool)
    .await
    .unwrap();
    let guard = lock_object(&pool, object).await.unwrap();

    let result = tokio::time::timeout(
        std::time::Duration::from_secs(1),
        runner.run_maintenance_once(&state),
    )
    .await;
    drop(guard);
    assert!(
        result.is_ok(),
        "maintenance must not wait behind an active download/object stream"
    );
    assert_eq!(result.unwrap().unwrap(), 0);
    let status: String = sqlx::query_scalar("SELECT status FROM exports WHERE id=$1")
        .bind(export)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "ready");
}

#[sqlx::test]
async fn download_object_lock_wait_is_bounded(pool: PgPool) {
    let (app, state, pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let response = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let export: Uuid = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        JobRunner::new(Uuid::now_v7())
            .run_once(&state)
            .await
            .unwrap()
    );
    let object: Uuid = sqlx::query_scalar("SELECT object_key FROM exports WHERE id=$1")
        .bind(export)
        .fetch_one(&pool)
        .await
        .unwrap();
    let guard = lock_object(&pool, object).await.unwrap();
    let pending_app = app.clone();
    let mut download = tokio::spawn(async move {
        send(
            &pending_app,
            "GET",
            &format!("/api/v1/exports/{export}/download"),
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert!(!download.is_finished());
    tokio::time::pause();
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    tokio::time::resume();
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), &mut download).await;
    if response.is_err() {
        download.abort();
    }
    drop(guard);
    let response = response.expect("download lock acquisition must time out after 30 seconds");
    assert_eq!(
        response.unwrap().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[sqlx::test]
async fn concurrent_download_streams_preserve_database_pool_headroom(pool: PgPool) {
    let (app, state, pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let runner = JobRunner::new(Uuid::now_v7());
    let mut exports = Vec::new();
    for _ in 0..3 {
        let response = send(
            &app,
            "POST",
            "/api/v1/exports",
            json!({"spaceId":space}),
            Some(&cookie),
            Some(&csrf),
            Some(&Uuid::now_v7().to_string()),
        )
        .await;
        exports.push(
            value(response).await["id"]
                .as_str()
                .unwrap()
                .parse::<Uuid>()
                .unwrap(),
        );
        assert!(runner.run_once(&state).await.unwrap());
    }

    let first = send(
        &app,
        "GET",
        &format!("/api/v1/exports/{}/download", exports[0]),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    let second = send(
        &app,
        "GET",
        &format!("/api/v1/exports/{}/download", exports[1]),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(first.status(), StatusCode::OK);
    assert_eq!(second.status(), StatusCode::OK);

    let excess = send(
        &app,
        "GET",
        &format!("/api/v1/exports/{}/download", exports[2]),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(excess.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        tokio::time::timeout(
            std::time::Duration::from_secs(1),
            sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(&pool),
        )
        .await
        .expect("object streams must leave database pool headroom")
        .unwrap(),
        1
    );
    drop(first);
    drop(second);
}

#[sqlx::test]
async fn download_remote_reads_time_out(pool: PgPool) {
    let stalled = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let objects = Arc::new(StallReadStore {
        inner: MemoryObjectStore::default(),
        stalled: stalled.clone(),
        entered: entered.clone(),
    });
    let (app, state, _pool) = setup_with_objects(pool, objects).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let response = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let export = value(response).await["id"].as_str().unwrap().to_owned();
    assert!(
        JobRunner::new(Uuid::now_v7())
            .run_once(&state)
            .await
            .unwrap()
    );
    stalled.store(true, Ordering::SeqCst);
    let pending_app = app.clone();
    let mut download = tokio::spawn(async move {
        send(
            &pending_app,
            "GET",
            &format!("/api/v1/exports/{export}/download"),
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !entered.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    tokio::time::resume();
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), &mut download).await;
    if response.is_err() {
        download.abort();
    }
    let response = response.expect("download object reads must time out after 30 seconds");
    assert_eq!(
        response.unwrap().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
}

#[sqlx::test]
async fn attachment_source_reads_time_out_and_schedule_a_fenced_retry(pool: PgPool) {
    let stalled = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let objects = Arc::new(StallReadStore {
        inner: MemoryObjectStore::default(),
        stalled: stalled.clone(),
        entered: entered.clone(),
    });
    let (app, state, pool) = setup_with_objects(pool, objects).await;
    let (cookie, csrf) = login(&app).await;
    let (space, page) = create_space_page(&app, &cookie, &csrf).await;
    upload_attachment(&app, &cookie, &csrf, page).await;
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{page}/attachments"),
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );
    let response = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    let export: Uuid = value(response).await["id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    stalled.store(true, Ordering::SeqCst);
    let runner_state = state.clone();
    let run =
        tokio::spawn(async move { JobRunner::new(Uuid::now_v7()).run_once(&runner_state).await });
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !entered.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    tokio::time::resume();
    let run = tokio::time::timeout(std::time::Duration::from_secs(5), run)
        .await
        .expect("attachment source reads must time out after 30 seconds");
    assert!(run.unwrap().unwrap());
    let row: (String, String, i32) = sqlx::query_as(
        "SELECT e.status,j.status,j.attempts
           FROM exports e JOIN jobs j ON j.id=e.job_id WHERE e.id=$1",
    )
    .bind(export)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row, ("queued".into(), "queued".into(), 1));
}
