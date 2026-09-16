use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode, header},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use knowledge_crypto::{
    EncryptedEnvelope, EnvelopeCipher, KeyPurpose, KeyVersion, Keyring, MasterKey,
};
use knowledge_server::{
    bootstrap::{BootstrapInput, BootstrapService},
    build_router,
    state::AppState,
};
use knowledge_store::attachments::{
    MemoryObjectStore, ObjectStore, ObjectUpload, StorageError, StorageFuture, cleanup_orphans,
    object_lock_key,
};
use knowledge_store::{MIGRATOR, Store};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

const ORIGIN: &str = "https://knowledge.local";
const PASSWORD: &str = "correct horse battery staple";

async fn app(pool: PgPool) -> Router {
    app_with_objects(pool, Arc::new(MemoryObjectStore::default())).await
}
async fn app_with_objects(pool: PgPool, objects: Arc<dyn ObjectStore>) -> Router {
    MIGRATOR.run(&pool).await.unwrap();
    let store = Store::from_pool(pool);
    let key = MasterKey::from_base64(&STANDARD.encode([51; 32])).unwrap();
    BootstrapService::run(
        &store,
        &key,
        BootstrapInput {
            workspace_name: "Workspace".into(),
            email: "admin@example.com".into(),
            display_name: "Admin".into(),
            password: PASSWORD.into(),
        },
    )
    .await
    .unwrap();
    build_router(
        AppState::new(store, key, ORIGIN, "")
            .unwrap()
            .with_object_store(objects),
    )
}

#[sqlx::test]
async fn cleanup_aborts_failed_upload_and_never_deletes_live_or_active_object(pool: PgPool) {
    let objects = Arc::new(MemoryObjectStore::default());
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/pages/{page}/attachments?size=5"),
        multipart(b"hello", "file.txt", "text/plain"),
        Some(&cookie),
        Some(&csrf),
        "multipart/form-data; boundary=files",
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let (live, workspace): (Uuid, Uuid) =
        sqlx::query_as("SELECT object_key,workspace_id FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap();
    let orphan = Uuid::now_v7();
    let mut upload = objects.start(orphan).await.unwrap();
    let remote = upload.upload_id().to_owned();
    upload
        .write(bytes::Bytes::from_static(b"ciphertext"))
        .await
        .unwrap();
    upload.finish().await.unwrap();
    let active = Uuid::now_v7();
    let guard = knowledge_store::attachments::lock_object(&pool, active)
        .await
        .unwrap();
    for (key, remote) in [(live, None), (orphan, Some(remote)), (active, None)] {
        sqlx::query("INSERT INTO attachment_staging(object_key,workspace_id,attachment_id,remote_upload_id,expires_at) VALUES($1,$2,$3,$4,now()-interval '1 day')").bind(key).bind(workspace).bind(Uuid::now_v7()).bind(remote).execute(&pool).await.unwrap();
    }
    assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 1);
    assert!(objects.size(orphan).await.is_err());
    assert!(objects.size(live).await.is_ok());
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM attachment_staging WHERE object_key=$1)"
        )
        .bind(active)
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    drop(guard);
    assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 1);
    assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 0);
}

#[sqlx::test]
async fn upload_rate_bucket_is_independent_and_allows_more_than_five_files(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    for index in 0..61 {
        let response = send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page}/attachments?size=5"),
            multipart(b"hello", "file.txt", "text/plain"),
            Some(&cookie),
            Some(&csrf),
            "multipart/form-data; boundary=files",
        )
        .await;
        assert_eq!(
            response.status(),
            if index < 60 {
                StatusCode::CREATED
            } else {
                StatusCode::TOO_MANY_REQUESTS
            },
            "upload {index}"
        );
    }
}

#[sqlx::test]
async fn recovery_uses_persisted_retention_immediately_after_bootstrap(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let attachment = json_body(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page}/attachments?size=5"),
            multipart(b"hello", "file.txt", "text/plain"),
            Some(&cookie),
            Some(&csrf),
            "multipart/form-data; boundary=files",
        )
        .await,
    )
    .await;
    let id: Uuid = attachment["id"].as_str().unwrap().parse().unwrap();
    sqlx::query("UPDATE attachments SET archived_at=now()-interval '2 days' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    let store = Store::from_pool(pool.clone());
    let key = MasterKey::from_base64(&STANDARD.encode([51; 32])).unwrap();
    let app = build_router(
        AppState::new(store, key, ORIGIN, "")
            .unwrap()
            .with_object_store(Arc::new(MemoryObjectStore::default()))
            .with_attachment_retention(1)
            .unwrap(),
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/attachments/{id}/unarchive"),
            vec![],
            Some(&cookie),
            Some(&csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    bytes: Vec<u8>,
    cookie: Option<&str>,
    csrf: Option<&str>,
    content_type: &str,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::HOST, "knowledge.local")
        .header(header::ORIGIN, ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .header(header::CONTENT_TYPE, content_type);
    if method == "POST" && uri == "/api/v1/invitations" {
        builder = builder.header("idempotency-key", Uuid::now_v7().to_string());
    }
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }
    if let Some(csrf) = csrf {
        builder = builder.header("x-csrf-token", csrf);
    }
    app.clone()
        .oneshot(builder.body(Body::from(bytes)).unwrap())
        .await
        .unwrap()
}
async fn json_body(response: axum::response::Response) -> Value {
    serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap()
}
async fn setup(app: &Router) -> (String, String, Uuid) {
    let login = send(
        app,
        "POST",
        "/api/v1/auth/login",
        json!({"email":"admin@example.com","password":PASSWORD})
            .to_string()
            .into_bytes(),
        None,
        None,
        "application/json",
    )
    .await;
    let cookie = login.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf = json_body(
        send(
            app,
            "GET",
            "/api/v1/auth/csrf",
            vec![],
            Some(&cookie),
            None,
            "application/json",
        )
        .await,
    )
    .await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    let space = json_body(
        send(
            app,
            "POST",
            "/api/v1/spaces",
            json!({"name":"Files","description":null})
                .to_string()
                .into_bytes(),
            Some(&cookie),
            Some(&csrf),
            "application/json",
        )
        .await,
    )
    .await;
    let page=json_body(send(app,"POST","/api/v1/pages",json!({"spaceId":space["id"],"title":"Files","markdown":"body","properties":{},"restricted":false}).to_string().into_bytes(),Some(&cookie),Some(&csrf),"application/json").await).await;
    (cookie, csrf, page["id"].as_str().unwrap().parse().unwrap())
}
fn multipart(bytes: &[u8], name: &str, mime: &str) -> Vec<u8> {
    let mut data=format!("--files\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: {mime}\r\n\r\n").into_bytes();
    data.extend(bytes);
    data.extend(b"\r\n--files--\r\n");
    data
}

// Removing registration/encryption/finalization breaks this exact-byte API contract.
#[sqlx::test]
async fn multichunk_upload_roundtrips_without_plaintext_metadata(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let bytes = b"ATTACHMENT SECRET MARKER\n".repeat(10000);
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/pages/{page}/attachments?size={}", bytes.len()),
        multipart(&bytes, "SECRET-NAME.txt", "text/plain"),
        Some(&cookie),
        Some(&csrf),
        "multipart/form-data; boundary=files",
    )
    .await;
    let status = response.status();
    let attachment = json_body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{attachment}");
    let id: Uuid = attachment["id"].as_str().unwrap().parse().unwrap();
    let (name, mime): (Vec<u8>, Vec<u8>) = sqlx::query_as(
        "SELECT original_name_ciphertext,mime_ciphertext FROM attachments WHERE id=$1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!name.windows(11).any(|x| x == b"SECRET-NAME"));
    assert!(!mime.windows(10).any(|x| x == b"text/plain"));
    let fetched = send(
        &app,
        "GET",
        &format!("/api/v1/attachments/{id}"),
        vec![],
        Some(&cookie),
        None,
        "application/json",
    )
    .await;
    assert_eq!(fetched.status(), StatusCode::OK);
    assert_eq!(fetched.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(fetched.headers()["x-content-type-options"], "nosniff");
    assert_eq!(
        &to_bytes(fetched.into_body(), usize::MAX).await.unwrap()[..],
        bytes
    );
}

// A missing durable multipart identity makes crash cleanup impossible.
#[sqlx::test]
async fn failed_sql_commit_keeps_durable_remote_upload_identity(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    sqlx::query("CREATE FUNCTION reject_attachment() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_attachment BEFORE INSERT ON attachments FOR EACH ROW EXECUTE FUNCTION reject_attachment()").execute(&pool).await.unwrap();
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/pages/{page}/attachments?size=5"),
        multipart(b"hello", "file.txt", "text/plain"),
        Some(&cookie),
        Some(&csrf),
        "multipart/form-data; boundary=files",
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    let staging: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM attachment_staging s")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(
        staging["remote_upload_id"].is_string(),
        "multipart identity must survive SQL failure"
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM attachments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

// MIME validation must cover later bytes, not only the first sniffing window.
#[sqlx::test]
async fn text_mime_rejects_binary_after_first_authenticated_chunk(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let mut bytes = vec![b'a'; 65536];
    bytes.extend(b"\0binary");
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/pages/{page}/attachments?size={}", bytes.len()),
        multipart(&bytes, "file.txt", "text/plain"),
        Some(&cookie),
        Some(&csrf),
        "multipart/form-data; boundary=files",
    )
    .await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[derive(Clone, Copy)]
enum GateAt {
    Created,
    Write,
    Finalized,
    FinalizeFailure,
}
struct Gate {
    at: GateAt,
    entered: tokio::sync::Notify,
    resume: tokio::sync::Notify,
    writes: std::sync::atomic::AtomicUsize,
}
struct GateStore {
    memory: MemoryObjectStore,
    gate: Arc<Gate>,
}
struct GateUpload {
    inner: Box<dyn ObjectUpload>,
    gate: Arc<Gate>,
}
impl GateStore {
    fn new(at: GateAt) -> Self {
        Self {
            memory: MemoryObjectStore::default(),
            gate: Arc::new(Gate {
                at,
                entered: tokio::sync::Notify::new(),
                resume: tokio::sync::Notify::new(),
                writes: std::sync::atomic::AtomicUsize::new(0),
            }),
        }
    }
}
impl ObjectUpload for GateUpload {
    fn upload_id(&self) -> &str {
        self.inner.upload_id()
    }
    fn write(&mut self, bytes: bytes::Bytes) -> StorageFuture<'_, ()> {
        Box::pin(async move {
            if matches!(self.gate.at, GateAt::Write) {
                self.gate.entered.notify_one();
                self.gate.resume.notified().await;
            }
            self.gate
                .writes
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.write(bytes).await
        })
    }
    fn finish(self: Box<Self>) -> StorageFuture<'static, ()> {
        Box::pin(async move {
            self.inner.finish().await?;
            if matches!(self.gate.at, GateAt::FinalizeFailure) {
                return Err(StorageError);
            }
            if matches!(self.gate.at, GateAt::Finalized) {
                self.gate.entered.notify_one();
                self.gate.resume.notified().await;
            }
            Ok(())
        })
    }
    fn abort(self: Box<Self>) -> StorageFuture<'static, ()> {
        self.inner.abort()
    }
}
impl ObjectStore for GateStore {
    fn start(&self, key: Uuid) -> StorageFuture<'_, Box<dyn ObjectUpload>> {
        Box::pin(async move {
            let inner = self.memory.start(key).await?;
            if matches!(self.gate.at, GateAt::Created) {
                self.gate.entered.notify_one();
                self.gate.resume.notified().await;
            }
            Ok(Box::new(GateUpload {
                inner,
                gate: self.gate.clone(),
            }) as Box<dyn ObjectUpload>)
        })
    }
    fn read_range(
        &self,
        key: Uuid,
        range: std::ops::Range<u64>,
    ) -> StorageFuture<'_, bytes::Bytes> {
        self.memory.read_range(key, range)
    }
    fn size(&self, key: Uuid) -> StorageFuture<'_, u64> {
        self.memory.size(key)
    }
    fn delete(&self, key: Uuid) -> StorageFuture<'_, ()> {
        self.memory.delete(key)
    }
    fn abort_upload<'a>(&'a self, key: Uuid, id: &'a str) -> StorageFuture<'a, ()> {
        self.memory.abort_upload(key, id)
    }
}
async fn paused_upload(
    app: Router,
    cookie: String,
    csrf: String,
    page: Uuid,
) -> tokio::task::JoinHandle<axum::response::Response> {
    tokio::spawn(async move {
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page}/attachments?size=5"),
            multipart(b"hello", "file.txt", "text/plain"),
            Some(&cookie),
            Some(&csrf),
            "multipart/form-data; boundary=files",
        )
        .await
    })
}
async fn entered(gate: &Gate) {
    tokio::time::timeout(std::time::Duration::from_secs(5), gate.entered.notified())
        .await
        .unwrap();
}

// Returning a cancelled upload's locked session to the pool would fail try-lock.
#[sqlx::test]
async fn cancellation_closes_lock_and_never_writes_before_durable_identity(pool: PgPool) {
    for at in [GateAt::Created, GateAt::Write] {
        // Separate bootstrap databases are unnecessary: reuse same app account.
        let objects = Arc::new(GateStore::new(at));
        let app = if matches!(at, GateAt::Created) {
            app_with_objects(pool.clone(), objects.clone()).await
        } else {
            let store = Store::from_pool(pool.clone());
            let key = MasterKey::from_base64(&STANDARD.encode([51; 32])).unwrap();
            build_router(
                AppState::new(store, key, ORIGIN, "")
                    .unwrap()
                    .with_object_store(objects.clone()),
            )
        };
        let (cookie, csrf, page) = setup(&app).await;
        let handle = paused_upload(app, cookie, csrf, page).await;
        entered(&objects.gate).await;
        let (key,remote):(Uuid,Option<String>)=sqlx::query_as("SELECT object_key,remote_upload_id FROM attachment_staging ORDER BY expires_at DESC LIMIT 1").fetch_one(&pool).await.unwrap();
        assert_eq!(remote.is_some(), matches!(at, GateAt::Write));
        assert_eq!(
            objects
                .gate
                .writes
                .load(std::sync::atomic::Ordering::SeqCst),
            0
        );
        handle.abort();
        assert!(handle.await.unwrap_err().is_cancelled());
        let mut connection = pool.acquire().await.unwrap();
        connection.close_on_drop();
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
                    .bind(object_lock_key(key))
                    .fetch_one(&mut *connection)
                    .await
                    .unwrap();
                if locked {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        drop(connection);
        sqlx::query(
            "UPDATE attachment_staging SET expires_at=now()-interval '1 day' WHERE object_key=$1",
        )
        .bind(key)
        .execute(&pool)
        .await
        .unwrap();
        assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 1);
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
    }
}

#[sqlx::test]
async fn sql_identity_persistence_failure_aborts_without_ciphertext_writes(pool: PgPool) {
    let objects = Arc::new(GateStore::new(GateAt::Finalized));
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    sqlx::query("CREATE FUNCTION reject_identity() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_identity BEFORE UPDATE ON attachment_staging FOR EACH ROW EXECUTE FUNCTION reject_identity()").execute(&pool).await.unwrap();
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/pages/{page}/attachments?size=5"),
        multipart(b"hello", "file.txt", "text/plain"),
        Some(&cookie),
        Some(&csrf),
        "multipart/form-data; boundary=files",
    )
    .await;
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        objects
            .gate
            .writes
            .load(std::sync::atomic::Ordering::SeqCst),
        0
    );
}

#[sqlx::test]
async fn archived_space_during_upload_prevents_metadata_and_audit_commit(pool: PgPool) {
    let objects = Arc::new(GateStore::new(GateAt::Finalized));
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let handle = paused_upload(app, cookie, csrf, page).await;
    entered(&objects.gate).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    sqlx::query(
        "UPDATE spaces SET archived_at=now() WHERE id=(SELECT space_id FROM pages WHERE id=$1)",
    )
    .bind(page)
    .execute(&pool)
    .await
    .unwrap();
    objects.gate.resume.notify_one();
    assert_eq!(handle.await.unwrap().status(), StatusCode::NOT_FOUND);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE event_type='attachment.uploaded'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

#[sqlx::test]
async fn size_mime_and_interrupted_uploads_never_publish_metadata(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    for (size, bytes, mime) in [
        (0, vec![], "text/plain"),
        (104857601, b"hello".to_vec(), "text/plain"),
        (6, b"hello".to_vec(), "text/plain"),
        (4, b"hello".to_vec(), "text/plain"),
        (5, b"hello".to_vec(), "image/png"),
    ] {
        let response = send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page}/attachments?size={size}"),
            multipart(&bytes, "file.txt", mime),
            Some(&cookie),
            Some(&csrf),
            "multipart/form-data; boundary=files",
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    let mut interrupted = multipart(b"hello", "file.txt", "text/plain");
    interrupted.truncate(interrupted.len() - 12);
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page}/attachments?size=5"),
            interrupted,
            Some(&cookie),
            Some(&csrf),
            "multipart/form-data; boundary=files"
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

async fn upload_file(app: &Router, cookie: &str, csrf: &str, page: Uuid, bytes: &[u8]) -> Uuid {
    let response = send(
        app,
        "POST",
        &format!("/api/v1/pages/{page}/attachments?size={}", bytes.len()),
        multipart(bytes, "secret-file.txt", "text/plain"),
        Some(cookie),
        Some(csrf),
        "multipart/form-data; boundary=files",
    )
    .await;
    let status = response.status();
    let body = json_body(response).await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    body["id"].as_str().unwrap().parse().unwrap()
}
async fn range_request(
    app: &Router,
    cookie: &str,
    id: Uuid,
    range: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/attachments/{id}"))
                .header(header::HOST, "knowledge.local")
                .header(header::COOKIE, cookie)
                .header(header::RANGE, range)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}
#[sqlx::test]
async fn ranges_only_return_authenticated_chunks_and_reject_unsupported_bounds(pool: PgPool) {
    let app = app(pool).await;
    let (cookie, csrf, page) = setup(&app).await;
    let bytes = vec![b'a'; 131079];
    let id = upload_file(&app, &cookie, &csrf, page, &bytes).await;
    for (range, start, end) in [
        ("bytes=0-65535", 0, 65536),
        ("bytes=65536-131071", 65536, 131072),
        ("bytes=131072-", 131072, 131079),
    ] {
        let response = range_request(&app, &cookie, id, range).await;
        assert_eq!(response.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            &to_bytes(response.into_body(), usize::MAX).await.unwrap()[..],
            &bytes[start..end]
        );
    }
    for range in [
        "bytes=1-65535",
        "bytes=0-10",
        "bytes=-65536",
        "bytes=0-65535,65536-131071",
        "bytes=131079-",
        "bytes=0-999999",
        "bytes=5-2",
    ] {
        let response = range_request(&app, &cookie, id, range).await;
        assert_eq!(
            response.status(),
            StatusCode::RANGE_NOT_SATISFIABLE,
            "{range}"
        );
        assert_eq!(response.headers()[header::CONTENT_RANGE], "bytes */131079");
    }
}
async fn raw_object(objects: &dyn ObjectStore, key: Uuid) -> Vec<u8> {
    let size = objects.size(key).await.unwrap();
    let mut bytes = vec![];
    let mut offset = 0;
    while offset < size {
        let end = (offset + 65536).min(size);
        bytes.extend_from_slice(&objects.read_range(key, offset..end).await.unwrap());
        offset = end;
    }
    bytes
}
async fn replace_object(objects: &dyn ObjectStore, key: Uuid, bytes: &[u8]) {
    let mut writer = objects.start(key).await.unwrap();
    for part in bytes.chunks(65536) {
        writer
            .write(bytes::Bytes::copy_from_slice(part))
            .await
            .unwrap();
    }
    writer.finish().await.unwrap();
}
#[sqlx::test]
async fn ciphertext_objects_reject_truncation_appends_reordering_and_tampering(pool: PgPool) {
    let objects = Arc::new(MemoryObjectStore::default());
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let bytes = b"OBJECT SECRET MARKER\n".repeat(10000);
    let id = upload_file(&app, &cookie, &csrf, page, &bytes).await;
    let key: Uuid = sqlx::query_scalar("SELECT object_key FROM attachments WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let original = raw_object(&*objects, key).await;
    use sha2::Digest;
    let checksum: Vec<u8> = sqlx::query_scalar("SELECT checksum FROM attachments WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(checksum, sha2::Sha256::digest(&original).to_vec());
    assert!(
        !original
            .windows(b"OBJECT SECRET MARKER".len())
            .any(|part| part == b"OBJECT SECRET MARKER")
    );
    assert!(!original.windows(15).any(|part| part == b"secret-file.txt"));
    for corrupted in [
        original[..original.len() - 1].to_vec(),
        [original.clone(), vec![0]].concat(),
    ] {
        replace_object(&*objects, key, &corrupted).await;
        assert_eq!(
            send(
                &app,
                "GET",
                &format!("/api/v1/attachments/{id}"),
                vec![],
                Some(&cookie),
                None,
                "application/json"
            )
            .await
            .status(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }
    let mut reordered = original.clone();
    let frame_len = 65536 + 28;
    reordered[20..20 + frame_len].copy_from_slice(&original[20 + frame_len..20 + frame_len * 2]);
    let mut altered = original.clone();
    altered[40] ^= 1;
    for corrupted in [reordered, altered] {
        replace_object(&*objects, key, &corrupted).await;
        let response = send(
            &app,
            "GET",
            &format!("/api/v1/attachments/{id}"),
            vec![],
            Some(&cookie),
            None,
            "application/json",
        )
        .await;
        assert!(to_bytes(response.into_body(), usize::MAX).await.is_err());
    }
}
#[sqlx::test]
async fn archive_list_and_restore_preserve_objects_and_do_not_cache_names(pool: PgPool) {
    let objects = Arc::new(MemoryObjectStore::default());
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let id = upload_file(&app, &cookie, &csrf, page, b"hello").await;
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/attachments/{id}"),
            vec![],
            Some(&cookie),
            Some(&csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/attachments/{id}"),
            vec![],
            Some(&cookie),
            None,
            "application/json"
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    let listed = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{page}/attachments?archived=true"),
        vec![],
        Some(&cookie),
        None,
        "application/json",
    )
    .await;
    assert_eq!(
        listed
            .headers()
            .get(header::CACHE_CONTROL)
            .and_then(|x| x.to_str().ok()),
        Some("no-store")
    );
    assert_eq!(json_body(listed).await[0]["id"], id.to_string());
    assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 0);
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/attachments/{id}/unarchive"),
            vec![],
            Some(&cookie),
            Some(&csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        &to_bytes(
            send(
                &app,
                "GET",
                &format!("/api/v1/attachments/{id}"),
                vec![],
                Some(&cookie),
                None,
                "application/json"
            )
            .await
            .into_body(),
            usize::MAX
        )
        .await
        .unwrap()[..],
        b"hello"
    );
}

async fn member(
    app: &Router,
    pool: &PgPool,
    admin_cookie: &str,
    admin_csrf: &str,
    page: Uuid,
) -> (String, String, Uuid, Uuid) {
    let invitation = json_body(
        send(
            app,
            "POST",
            "/api/v1/invitations",
            json!({"email":"editor@example.com","workspaceRole":"member"})
                .to_string()
                .into_bytes(),
            Some(admin_cookie),
            Some(admin_csrf),
            "application/json",
        )
        .await,
    )
    .await;
    let user = json_body(
        send(
            app,
            "POST",
            &format!(
                "/api/v1/invitations/{}/accept",
                invitation["token"].as_str().unwrap()
            ),
            json!({"displayName":"Editor","password":PASSWORD})
                .to_string()
                .into_bytes(),
            None,
            None,
            "application/json",
        )
        .await,
    )
    .await;
    let space: Uuid = sqlx::query_scalar("SELECT space_id FROM pages WHERE id=$1")
        .bind(page)
        .fetch_one(pool)
        .await
        .unwrap();
    let grant = json_body(
        send(
            app,
            "POST",
            &format!("/api/v1/spaces/{space}/grants"),
            json!({"subjectType":"user","subjectId":user["id"],"role":"editor"})
                .to_string()
                .into_bytes(),
            Some(admin_cookie),
            Some(admin_csrf),
            "application/json",
        )
        .await,
    )
    .await;
    let logged = send(
        app,
        "POST",
        "/api/v1/auth/login",
        json!({"email":"editor@example.com","password":PASSWORD})
            .to_string()
            .into_bytes(),
        None,
        None,
        "application/json",
    )
    .await;
    let cookie = logged.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf = json_body(
        send(
            app,
            "GET",
            "/api/v1/auth/csrf",
            vec![],
            Some(&cookie),
            None,
            "application/json",
        )
        .await,
    )
    .await["token"]
        .as_str()
        .unwrap()
        .to_owned();
    (
        cookie,
        csrf,
        space,
        grant["id"].as_str().unwrap().parse().unwrap(),
    )
}
#[sqlx::test]
async fn revoked_page_access_during_object_finalize_is_rechecked_before_commit(pool: PgPool) {
    let objects = Arc::new(GateStore::new(GateAt::Finalized));
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (admin_cookie, admin_csrf, page) = setup(&app).await;
    let (cookie, csrf, space, grant) = member(&app, &pool, &admin_cookie, &admin_csrf, page).await;
    let handle = paused_upload(app.clone(), cookie, csrf, page).await;
    entered(&objects.gate).await;
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/spaces/{space}/grants/{grant}"),
            vec![],
            Some(&admin_cookie),
            Some(&admin_csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    objects.gate.resume.notify_one();
    assert_eq!(handle.await.unwrap().status(), StatusCode::NOT_FOUND);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
#[sqlx::test]
async fn restricted_pages_deny_every_attachment_route_without_enumeration(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (admin_cookie, admin_csrf, page) = setup(&app).await;
    let (cookie, csrf, _, _) = member(&app, &pool, &admin_cookie, &admin_csrf, page).await;
    let id = upload_file(&app, &admin_cookie, &admin_csrf, page, b"hello").await;
    sqlx::query("UPDATE pages SET restricted=true WHERE id=$1")
        .bind(page)
        .execute(&pool)
        .await
        .unwrap();
    for (method, uri) in [
        ("GET", format!("/api/v1/attachments/{id}")),
        ("DELETE", format!("/api/v1/attachments/{id}")),
        ("GET", format!("/api/v1/pages/{page}/attachments")),
        (
            "GET",
            format!("/api/v1/pages/{page}/attachments?archived=true"),
        ),
    ] {
        assert_eq!(
            send(
                &app,
                method,
                &uri,
                vec![],
                Some(&cookie),
                Some(&csrf),
                "application/json"
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
    }
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page}/attachments?size=5"),
            multipart(b"hello", "file.txt", "text/plain"),
            Some(&cookie),
            Some(&csrf),
            "multipart/form-data; boundary=files"
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/attachments/{id}"),
            vec![],
            Some(&admin_cookie),
            Some(&admin_csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/attachments/{id}/unarchive"),
            vec![],
            Some(&cookie),
            Some(&csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NOT_FOUND
    );
}

#[test]
fn production_s3_configuration_is_required_and_redacts_errors() {
    let error = knowledge_server::config::S3Config::from_pairs([(
        "KNOWLEDGE_S3_SECRET_ACCESS_KEY",
        "TOP-SECRET",
    )])
    .err()
    .expect("missing endpoint/bucket/access ID must fail closed");
    assert!(!error.to_string().contains("TOP-SECRET"));
    let config = knowledge_server::config::S3Config::from_pairs([
        ("KNOWLEDGE_S3_ENDPOINT", "http://localhost:9000"),
        ("KNOWLEDGE_S3_BUCKET", "encrypted"),
        ("KNOWLEDGE_S3_REGION", "us-east-1"),
        ("KNOWLEDGE_S3_ACCESS_KEY_ID", "test"),
        ("KNOWLEDGE_S3_SECRET_ACCESS_KEY", "TOP-SECRET"),
        ("KNOWLEDGE_S3_ALLOW_HTTP", "true"),
    ])
    .unwrap();
    assert!(config.build().is_ok());
}

#[sqlx::test]
async fn ambiguous_finalize_failure_leaves_cleanup_ledger_not_visible_metadata(pool: PgPool) {
    let objects = Arc::new(GateStore::new(GateAt::FinalizeFailure));
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page}/attachments?size=5"),
            multipart(b"hello", "file.txt", "text/plain"),
            Some(&cookie),
            Some(&csrf),
            "multipart/form-data; boundary=files"
        )
        .await
        .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let key: Uuid = sqlx::query_scalar("SELECT object_key FROM attachment_staging")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(objects.size(key).await.is_ok());
    sqlx::query("UPDATE attachment_staging SET expires_at=now()-interval '1 day'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 1);
    assert!(objects.size(key).await.is_err());
}

#[sqlx::test]
async fn text_unicode_spanning_chunk_boundaries_roundtrips(pool: PgPool) {
    let app = app(pool).await;
    let (cookie, csrf, page) = setup(&app).await;
    let mut bytes = vec![b'a'; 65535];
    bytes.extend("я🙂".as_bytes());
    let id = upload_file(&app, &cookie, &csrf, page, &bytes).await;
    assert_eq!(
        &to_bytes(
            send(
                &app,
                "GET",
                &format!("/api/v1/attachments/{id}"),
                vec![],
                Some(&cookie),
                None,
                "application/json"
            )
            .await
            .into_body(),
            usize::MAX
        )
        .await
        .unwrap()[..],
        bytes
    );
}

async fn keyed_upload(
    app: &Router,
    cookie: &str,
    csrf: &str,
    page: Uuid,
    bytes: &[u8],
    name: &str,
    key: Uuid,
) -> axum::response::Response {
    keyed_upload_body(
        app,
        cookie,
        csrf,
        page,
        bytes.len(),
        Body::from(multipart(bytes, name, "text/plain")),
        key,
    )
    .await
}
async fn keyed_upload_body(
    app: &Router,
    cookie: &str,
    csrf: &str,
    page: Uuid,
    size: usize,
    body: Body,
    key: Uuid,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/pages/{page}/attachments?size={}", size))
                .header(header::HOST, "knowledge.local")
                .header(header::ORIGIN, ORIGIN)
                .header("sec-fetch-site", "same-origin")
                .header(header::COOKIE, cookie)
                .header("x-csrf-token", csrf)
                .header("idempotency-key", key.to_string())
                .header(header::CONTENT_TYPE, "multipart/form-data; boundary=files")
                .body(body)
                .unwrap(),
        )
        .await
        .unwrap()
}
#[sqlx::test]
async fn idempotent_replay_has_one_object_metadata_and_audit_and_rejects_mismatch(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let key = Uuid::now_v7();
    let first =
        json_body(keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key).await).await;
    let replay =
        json_body(keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key).await).await;
    assert_eq!(first["id"], replay["id"]);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE event_type='attachment.uploaded'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    for (bytes, name) in [
        (b"world".as_slice(), "file.txt"),
        (b"hello".as_slice(), "other.txt"),
        (b"longer".as_slice(), "file.txt"),
    ] {
        assert_eq!(
            keyed_upload(&app, &cookie, &csrf, page, bytes, name, key)
                .await
                .status(),
            StatusCode::CONFLICT
        );
    }
}
// Missing committed admin-access events must fail these disclosure regressions.
#[sqlx::test]
async fn administrative_disclosures_commit_correlated_redacted_audit(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let request_key = Uuid::now_v7();
    let first = json_body(
        keyed_upload(
            &app,
            &cookie,
            &csrf,
            page,
            b"private body",
            "private-name.txt",
            request_key,
        )
        .await,
    )
    .await;
    let id = first["id"].as_str().unwrap();
    let key_record = knowledge_store::users::UserRepository::workspace_key(
        &pool,
        sqlx::query_scalar("SELECT id FROM workspaces")
            .fetch_one(&pool)
            .await
            .unwrap(),
    )
    .await
    .unwrap()
    .unwrap();
    let master = MasterKey::from_base64(&STANDARD.encode([51; 32])).unwrap();
    let key = Keyring::unwrap(
        &master,
        &EncryptedEnvelope {
            algorithm: "AES-256-GCM".into(),
            key_version: KeyVersion(key_record.key_version as u32),
            nonce: key_record.nonce,
            ciphertext: key_record.ciphertext,
        },
    )
    .unwrap();
    let cipher = EnvelopeCipher::new(KeyVersion(1), key.derive(KeyPurpose::Content).unwrap());
    for (operation, target_type, target, response) in [
        (
            "list",
            "page",
            page,
            send(
                &app,
                "GET",
                &format!("/api/v1/pages/{page}/attachments"),
                vec![],
                Some(&cookie),
                None,
                "application/json",
            )
            .await,
        ),
        (
            "download",
            "attachment",
            id.parse().unwrap(),
            send(
                &app,
                "GET",
                &format!("/api/v1/attachments/{id}"),
                vec![],
                Some(&cookie),
                None,
                "application/json",
            )
            .await,
        ),
        (
            "upload_replay",
            "attachment",
            id.parse().unwrap(),
            keyed_upload(
                &app,
                &cookie,
                &csrf,
                page,
                b"private body",
                "private-name.txt",
                request_key,
            )
            .await,
        ),
    ] {
        assert!(response.status().is_success());
        let request: Uuid = response.headers()["x-request-id"]
            .to_str()
            .unwrap()
            .parse()
            .unwrap();
        let (event_id, workspace, stored_type, stored_target, actor, session, ciphertext, nonce):(Uuid,Uuid,String,Uuid,Uuid,Uuid,Vec<u8>,Vec<u8>) = sqlx::query_as("SELECT id,workspace_id,target_type,target_id,actor_user_id,session_id,metadata_ciphertext,metadata_nonce FROM audit_events WHERE event_type='attachment.admin_access' AND request_id=$1").bind(request).fetch_one(&pool).await.expect("admin use must be committed before disclosure");
        assert_eq!(stored_type, target_type);
        assert_eq!(stored_target, target);
        assert_ne!(actor, Uuid::nil());
        assert_ne!(session, Uuid::nil());
        let metadata = cipher
            .decrypt(
                format!("workspace:{workspace}:audit:{event_id}:metadata").as_bytes(),
                &EncryptedEnvelope {
                    algorithm: "AES-256-GCM".into(),
                    key_version: KeyVersion(1),
                    nonce: nonce.try_into().unwrap(),
                    ciphertext,
                },
            )
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(metadata.expose()).unwrap(),
            json!({"operation":operation})
        );
        let _ = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE event_type='attachment.uploaded'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
}

#[sqlx::test]
async fn administrative_audit_failure_blocks_list_download_and_replay_disclosure(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let key = Uuid::now_v7();
    let first = json_body(
        keyed_upload(
            &app,
            &cookie,
            &csrf,
            page,
            b"private body",
            "private-name.txt",
            key,
        )
        .await,
    )
    .await;
    let id = first["id"].as_str().unwrap();
    sqlx::query("CREATE FUNCTION reject_admin_access() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='attachment.admin_access' THEN RAISE EXCEPTION 'audit unavailable'; END IF; RETURN NEW; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_admin_access BEFORE INSERT ON audit_events FOR EACH ROW EXECUTE FUNCTION reject_admin_access()").execute(&pool).await.unwrap();
    for response in [
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{page}/attachments"),
            vec![],
            Some(&cookie),
            None,
            "application/json",
        )
        .await,
        send(
            &app,
            "GET",
            &format!("/api/v1/attachments/{id}"),
            vec![],
            Some(&cookie),
            None,
            "application/json",
        )
        .await,
        keyed_upload(
            &app,
            &cookie,
            &csrf,
            page,
            b"private body",
            "private-name.txt",
            key,
        )
        .await,
    ] {
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = String::from_utf8(
            to_bytes(response.into_body(), usize::MAX)
                .await
                .unwrap()
                .to_vec(),
        )
        .unwrap();
        assert!(
            !body.contains("private body")
                && !body.contains("private-name.txt")
                && !body.contains("audit unavailable")
        );
    }
}

#[sqlx::test]
async fn ordinary_member_disclosures_do_not_record_administrative_access(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (admin_cookie, admin_csrf, page) = setup(&app).await;
    let (cookie, csrf, _, _) = member(&app, &pool, &admin_cookie, &admin_csrf, page).await;
    let key = Uuid::now_v7();
    let first =
        json_body(keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key).await).await;
    let id = first["id"].as_str().unwrap();
    for response in [
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{page}/attachments"),
            vec![],
            Some(&cookie),
            None,
            "application/json",
        )
        .await,
        send(
            &app,
            "GET",
            &format!("/api/v1/attachments/{id}"),
            vec![],
            Some(&cookie),
            None,
            "application/json",
        )
        .await,
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key).await,
    ] {
        assert!(response.status().is_success());
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE event_type='attachment.admin_access'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}

// Bypassing or double-charging the body bucket breaks the hand-derived 60/61 boundary.
#[sqlx::test]
async fn successful_replay_body_attempts_saturate_once_without_duplicate_mutations(pool: PgPool) {
    let objects = Arc::new(MemoryObjectStore::default());
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (admin_cookie, admin_csrf, page) = setup(&app).await;
    let (cookie, csrf, _, _) = member(&app, &pool, &admin_cookie, &admin_csrf, page).await;
    let key = Uuid::now_v7();
    for attempt in 1..=60 {
        let response = keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key).await;
        assert_eq!(
            response.status(),
            StatusCode::CREATED,
            "streamed attempt {attempt}"
        );
    }
    let polled = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = polled.clone();
    let body = Body::from_stream(futures_util::stream::once(async move {
        observed.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok::<_, std::io::Error>(bytes::Bytes::from(multipart(
            b"hello",
            "file.txt",
            "text/plain",
        )))
    }));
    assert_eq!(
        keyed_upload_body(&app, &cookie, &csrf, page, 5, body, key)
            .await
            .status(),
        StatusCode::TOO_MANY_REQUESTS
    );
    assert!(
        !polled.load(std::sync::atomic::Ordering::SeqCst),
        "saturated replay must not process body"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachment_staging")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM audit_events WHERE event_type LIKE 'attachment.%'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    let object: Uuid = sqlx::query_scalar("SELECT object_key FROM attachments")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(objects.size(object).await.is_ok());
}
#[sqlx::test]
async fn concurrent_idempotency_reservation_conflicts_then_replays_without_object_writes(
    pool: PgPool,
) {
    let objects = Arc::new(GateStore::new(GateAt::Finalized));
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let key = Uuid::now_v7();
    let clone = app.clone();
    let cookie_clone = cookie.clone();
    let csrf_clone = csrf.clone();
    let pending = tokio::spawn(async move {
        keyed_upload(
            &clone,
            &cookie_clone,
            &csrf_clone,
            page,
            b"hello",
            "file.txt",
            key,
        )
        .await
    });
    entered(&objects.gate).await;
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    objects.gate.resume.notify_one();
    assert_eq!(pending.await.unwrap().status(), StatusCode::CREATED);
    let before = objects
        .gate
        .writes
        .load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        objects
            .gate
            .writes
            .load(std::sync::atomic::Ordering::SeqCst),
        before
    );
}
#[sqlx::test]
async fn idempotency_replay_never_bypasses_revoked_page_access(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (admin_cookie, admin_csrf, page) = setup(&app).await;
    let (cookie, csrf, space, grant) = member(&app, &pool, &admin_cookie, &admin_csrf, page).await;
    let key = Uuid::now_v7();
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/spaces/{space}/grants/{grant}"),
            vec![],
            Some(&admin_cookie),
            Some(&admin_csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
}

#[sqlx::test]
async fn visible_restricted_page_owner_can_recover_attachment(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (admin_cookie, admin_csrf, page) = setup(&app).await;
    let (cookie, csrf, space, grant) = member(&app, &pool, &admin_cookie, &admin_csrf, page).await;
    let member: Uuid = sqlx::query_scalar("SELECT user_id FROM space_grants WHERE id=$1")
        .bind(grant)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            &app,
            "PATCH",
            &format!("/api/v1/spaces/{space}/grants/{grant}"),
            json!({"role":"owner"}).to_string().into_bytes(),
            Some(&admin_cookie),
            Some(&admin_csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::OK
    );
    let id = upload_file(&app, &admin_cookie, &admin_csrf, page, b"hello").await;
    sqlx::query("UPDATE pages SET restricted=true WHERE id=$1")
        .bind(page)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            &app,
            "PUT",
            &format!("/api/v1/pages/{page}/permissions/user:{member}"),
            json!({"role":"editor"}).to_string().into_bytes(),
            Some(&admin_cookie),
            Some(&admin_csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "DELETE",
            &format!("/api/v1/attachments/{id}"),
            vec![],
            Some(&cookie),
            Some(&csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/attachments/{id}/unarchive"),
            vec![],
            Some(&cookie),
            Some(&csrf),
            "application/json"
        )
        .await
        .status(),
        StatusCode::NO_CONTENT
    );
}

#[sqlx::test]
async fn failed_idempotent_request_is_reclaimed_only_by_safe_expired_cleanup(pool: PgPool) {
    let objects = Arc::new(GateStore::new(GateAt::FinalizeFailure));
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let key = Uuid::now_v7();
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 0);
    sqlx::query("UPDATE attachment_staging SET expires_at=now()-interval '1 day'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 1);
    let state = AppState::new(
        Store::from_pool(pool.clone()),
        MasterKey::from_base64(&STANDARD.encode([51; 32])).unwrap(),
        ORIGIN,
        "",
    )
    .unwrap()
    .with_object_store(Arc::new(MemoryObjectStore::default()));
    let app = build_router(state);
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachment_upload_requests")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}
#[sqlx::test]
async fn idempotency_is_actor_scoped_and_rejects_cross_page_reuse(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (admin_cookie, admin_csrf, page) = setup(&app).await;
    let (cookie, csrf, space, _) = member(&app, &pool, &admin_cookie, &admin_csrf, page).await;
    let key = Uuid::now_v7();
    assert_eq!(
        keyed_upload(
            &app,
            &admin_cookie,
            &admin_csrf,
            page,
            b"hello",
            "file.txt",
            key
        )
        .await
        .status(),
        StatusCode::CREATED
    );
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, page, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::CREATED
    );
    let second=json_body(send(&app,"POST","/api/v1/pages",json!({"spaceId":space,"title":"Other","markdown":"body","properties":{},"restricted":false}).to_string().into_bytes(),Some(&admin_cookie),Some(&admin_csrf),"application/json").await).await;
    let second: Uuid = second["id"].as_str().unwrap().parse().unwrap();
    assert_eq!(
        keyed_upload(&app, &cookie, &csrf, second, b"hello", "file.txt", key)
            .await
            .status(),
        StatusCode::CONFLICT
    );
    let fingerprints: Vec<Vec<u8>> =
        sqlx::query_scalar("SELECT fingerprint FROM attachment_upload_requests")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(fingerprints.len(), 2);
    assert_ne!(fingerprints[0], fingerprints[1]);
}
#[sqlx::test]
async fn authenticated_stream_stops_at_tampered_chunk_and_emits_no_later_bytes(pool: PgPool) {
    use futures_util::StreamExt;
    let objects = Arc::new(MemoryObjectStore::default());
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let bytes = vec![b'x'; 196608];
    let id = upload_file(&app, &cookie, &csrf, page, &bytes).await;
    let key: Uuid = sqlx::query_scalar("SELECT object_key FROM attachments WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut raw = raw_object(&*objects, key).await;
    raw[20 + 65536 + 28 + 12] ^= 1;
    replace_object(&*objects, key, &raw).await;
    let mut stream = send(
        &app,
        "GET",
        &format!("/api/v1/attachments/{id}"),
        vec![],
        Some(&cookie),
        None,
        "application/json",
    )
    .await
    .into_body()
    .into_data_stream();
    assert_eq!(&stream.next().await.unwrap().unwrap()[..], &bytes[..65536]);
    assert!(stream.next().await.unwrap().is_err());
    assert!(stream.next().await.is_none());
}

#[sqlx::test]
async fn oversized_multipart_header_is_rejected_before_object_finalization(pool: PgPool) {
    let app = app(pool.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let bytes=format!("--files\r\nContent-Disposition: form-data; name=\"file\"; filename=\"file.txt\"\r\nContent-Type: text/plain\r\nX-Padding: {}\r\n\r\nhello\r\n--files--\r\n","x".repeat(10000)).into_bytes();
    assert_eq!(
        send(
            &app,
            "POST",
            &format!("/api/v1/pages/{page}/attachments?size=5"),
            bytes,
            Some(&cookie),
            Some(&csrf),
            "multipart/form-data; boundary=files"
        )
        .await
        .status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
#[sqlx::test]
async fn malformed_upload_query_and_multipart_return_redacted_problem_details(pool: PgPool) {
    let app = app(pool).await;
    let (cookie, csrf, page) = setup(&app).await;
    for (uri, mime) in [
        (
            format!("/api/v1/pages/{page}/attachments?size=RAW-SECRET"),
            "multipart/form-data; boundary=files",
        ),
        (
            format!("/api/v1/pages/{page}/attachments?size=5"),
            "multipart/form-data",
        ),
    ] {
        let response = send(&app, "POST", &uri, vec![], Some(&cookie), Some(&csrf), mime).await;
        assert_eq!(
            response.headers()[header::CONTENT_TYPE],
            "application/problem+json"
        );
        assert!(!json_body(response).await.to_string().contains("RAW-SECRET"));
    }
}

#[sqlx::test]
async fn stalled_first_multipart_header_times_out_and_releases_upload_resources(pool: PgPool) {
    use futures_util::StreamExt;
    let objects = Arc::new(MemoryObjectStore::default());
    let app = app_with_objects(pool.clone(), objects.clone()).await;
    let (cookie, csrf, page) = setup(&app).await;
    let stalled = Arc::new(tokio::sync::Notify::new());
    let signal = stalled.clone();
    let stream = futures_util::stream::once(async {
        Ok::<_, std::io::Error>(bytes::Bytes::from_static(b"--files\r\nContent-Disposit"))
    })
    .chain(futures_util::stream::once(async move {
        signal.notify_one();
        std::future::pending::<Result<bytes::Bytes, std::io::Error>>().await
    }));
    let request = Request::builder()
        .method("POST")
        .uri(format!("/api/v1/pages/{page}/attachments?size=5"))
        .header(header::HOST, "knowledge.local")
        .header(header::ORIGIN, ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .header(header::COOKIE, &cookie)
        .header("x-csrf-token", &csrf)
        .header(header::CONTENT_TYPE, "multipart/form-data; boundary=files")
        .body(Body::from_stream(stream))
        .unwrap();
    let clone = app.clone();
    let handle = tokio::spawn(async move { clone.oneshot(request).await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(5), stalled.notified())
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(31)).await;
    for _ in 0..20 {
        tokio::task::yield_now().await;
    }
    let finished = handle.is_finished();
    tokio::time::resume();
    if !finished {
        handle.abort();
    }
    assert!(
        finished,
        "first multipart header must time out in 30 seconds, not wait the 5 minute body deadline"
    );
    assert_eq!(
        handle.await.unwrap().status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
    sqlx::query("UPDATE attachment_staging SET expires_at=now()-interval '1 day'")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(cleanup_orphans(&pool, &*objects).await.unwrap(), 1);
    upload_file(&app, &cookie, &csrf, page, b"hello").await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM attachments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test]
async fn non_ascii_or_duplicate_range_headers_return_416(pool: PgPool) {
    let app = app(pool).await;
    let (cookie, csrf, page) = setup(&app).await;
    let id = upload_file(&app, &cookie, &csrf, page, b"hello").await;
    for duplicate in [false, true] {
        let mut request = Request::builder()
            .uri(format!("/api/v1/attachments/{id}"))
            .header(header::HOST, "knowledge.local")
            .header(header::COOKIE, &cookie)
            .body(Body::empty())
            .unwrap();
        if duplicate {
            request
                .headers_mut()
                .append(header::RANGE, "bytes=0-4".parse().unwrap());
            request
                .headers_mut()
                .append(header::RANGE, "bytes=1-4".parse().unwrap());
        } else {
            request.headers_mut().insert(
                header::RANGE,
                axum::http::HeaderValue::from_bytes(&[255]).unwrap(),
            );
        }
        assert_eq!(
            app.clone().oneshot(request).await.unwrap().status(),
            StatusCode::RANGE_NOT_SATISFIABLE
        );
    }
}
