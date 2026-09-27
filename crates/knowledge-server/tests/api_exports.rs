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

#[sqlx::test]
async fn active_attachment_download_fences_rotation_and_final_purge(pool: PgPool) {
    let (app, state, pool, objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (_, page) = create_space_page(&app, &cookie, &csrf).await;
    upload_attachment(&app, &cookie, &csrf, page).await;
    let (id, old): (Uuid, Uuid) =
        sqlx::query_as("SELECT id,object_key FROM attachments WHERE page_id=$1")
            .bind(page)
            .fetch_one(&pool)
            .await
            .unwrap();
    let response = send(
        &app,
        "GET",
        &format!("/api/v1/attachments/{id}"),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/v1/admin/key-rotations",
            json!({}),
            Some(&cookie),
            Some(&csrf),
            None
        )
        .await
        .status(),
        StatusCode::ACCEPTED
    );
    let runner = JobRunner::new(Uuid::now_v7());
    for _ in 0..12 {
        runner.run_maintenance_once(&state).await.unwrap();
    }
    assert!(
        objects.size(old).await.is_ok(),
        "an actively streamed old-generation object must remain available"
    );
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
        b"attachment"
    );
    for _ in 0..12 {
        runner.run_maintenance_once(&state).await.unwrap();
    }
    assert!(objects.size(old).await.is_err());
    let object: Uuid = sqlx::query_scalar("SELECT object_key FROM attachments WHERE id=$1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let response = send(
        &app,
        "GET",
        &format!("/api/v1/attachments/{id}"),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    sqlx::query("UPDATE attachments SET archived_at=now()-interval '31 days' WHERE id=$1")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    runner.run_maintenance_once(&state).await.unwrap();
    assert!(objects.size(object).await.is_ok());
    assert_eq!(
        to_bytes(response.into_body(), 1024).await.unwrap().as_ref(),
        b"attachment"
    );
    runner.run_maintenance_once(&state).await.unwrap();
    assert!(objects.size(object).await.is_err());
}

#[sqlx::test]
async fn key_rotation_rejects_a_stale_writer_waiting_for_the_workspace_lock(pool: PgPool) {
    let (app, _, pool, _) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let mut held = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces FOR UPDATE")
        .fetch_one(&mut *held)
        .await
        .unwrap();
    let clone = app.clone();
    let cookie_one = cookie.clone();
    let csrf_one = csrf.clone();
    let rotate = tokio::spawn(async move {
        send(
            &clone,
            "POST",
            "/api/v1/admin/key-rotations",
            json!({}),
            Some(&cookie_one),
            Some(&csrf_one),
            None,
        )
        .await
    });
    wait_for_workspace_waiters(&pool, 1).await;
    let clone = app.clone();
    let cookie_two = cookie.clone();
    let csrf_two = csrf.clone();
    let write = tokio::spawn(async move {
        send(&clone,"POST","/api/v1/pages",json!({"spaceId":space,"title":"New writer","markdown":"encrypted consistently","properties":{},"restricted":false}),Some(&cookie_two),Some(&csrf_two),None).await
    });
    wait_for_workspace_waiters(&pool, 2).await;
    held.commit().await.unwrap();
    assert_eq!(rotate.await.unwrap().status(), StatusCode::ACCEPTED);
    assert_eq!(
        write.await.unwrap().status(),
        StatusCode::INTERNAL_SERVER_ERROR
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 1,
        "stale generation writer must roll back all domain and index changes"
    );
    let (_, page) = create_space_page(&app, &cookie, &csrf).await;
    let response = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{page}"),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
}

async fn wait_for_workspace_waiters(pool: &PgPool, count: i64) {
    tokio::time::timeout(std::time::Duration::from_secs(5),async {
        loop {
            let waiting:i64=sqlx::query_scalar("SELECT count(*) FROM pg_stat_activity WHERE datname=current_database() AND pid<>pg_backend_pid() AND wait_event_type='Lock' AND query LIKE '%workspaces%'").fetch_one(pool).await.unwrap();
            if waiting>=count {break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }).await.unwrap();
}

#[sqlx::test]
async fn workspace_key_rotation_rewraps_data_and_resumes_search_rebuild(pool: PgPool) {
    let (app, state, pool, objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, page) = create_space_page(&app, &cookie, &csrf).await;
    let file = vec![b'x'; 131_075];
    upload_attachment_bytes(&app, &cookie, &csrf, page, &file).await;
    let created = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    assert_eq!(created.status(), StatusCode::ACCEPTED);
    let export: Uuid = value(created).await["id"]
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
    let archive = to_bytes(
        send(
            &app,
            "GET",
            &format!("/api/v1/exports/{export}/download"),
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await
        .into_body(),
        1_000_000,
    )
    .await
    .unwrap();
    let before: Vec<u8> = sqlx::query_scalar("SELECT body_ciphertext FROM pages WHERE id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    let old_tokens: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT token_hmac FROM blind_index_tokens WHERE page_id=$1 ORDER BY token_hmac",
    )
    .bind(page)
    .fetch_all(&pool)
    .await
    .unwrap();
    let immutable: Vec<(Uuid, Vec<u8>, Vec<u8>)> =
        sqlx::query_as("SELECT id,metadata_ciphertext,event_hash FROM audit_events ORDER BY id")
            .fetch_all(&pool)
            .await
            .unwrap();
    let response = send(
        &app,
        "POST",
        "/api/v1/admin/key-rotations",
        json!({}),
        Some(&cookie),
        Some(&csrf),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let status = value(response).await;
    assert_eq!(status["currentVersion"], 2);
    assert_eq!(status["previousVersion"], 1);
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/v1/admin/key-rotations",
            json!({}),
            Some(&cookie),
            Some(&csrf),
            None
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
    let (_, new_page) = create_space_page(&app, &cookie, &csrf).await;
    let generation: i32 = sqlx::query_scalar("SELECT body_key_version FROM pages WHERE id=$1")
        .bind(new_page)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(generation, 2);
    assert_eq!(
        send(
            &app,
            "GET",
            "/api/v1/search?q=secret",
            json!({}),
            Some(&cookie),
            None,
            None
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    let first = JobRunner::new(Uuid::now_v7());
    first.run_maintenance_once(&state).await.unwrap();
    // A new runner resumes from durable state after a process restart.
    let second = JobRunner::new(Uuid::now_v7());
    for _ in 0..20 {
        second.run_maintenance_once(&state).await.unwrap();
    }
    let status = value(
        send(
            &app,
            "GET",
            "/api/v1/admin/key-rotations",
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await,
    )
    .await;
    assert_eq!(status["state"], "complete");
    let after: Vec<u8> = sqlx::query_scalar("SELECT body_ciphertext FROM pages WHERE id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_ne!(before, after);
    let new_tokens: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT token_hmac FROM blind_index_tokens WHERE page_id=$1 ORDER BY token_hmac",
    )
    .bind(page)
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_ne!(old_tokens, new_tokens);
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{page}"),
            json!({}),
            Some(&cookie),
            None,
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "GET",
            &format!("/api/v1/pages/{page}/revisions/1"),
            json!({}),
            Some(&cookie),
            None,
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    let attachment: Uuid = sqlx::query_scalar("SELECT id FROM attachments WHERE page_id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    let download = send(
        &app,
        "GET",
        &format!("/api/v1/attachments/{attachment}"),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(download.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(download.into_body(), 1_000_000)
            .await
            .unwrap()
            .as_ref(),
        file
    );
    let migrated_archive = to_bytes(
        send(
            &app,
            "GET",
            &format!("/api/v1/exports/{export}/download"),
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await
        .into_body(),
        1_000_000,
    )
    .await
    .unwrap();
    assert_eq!(
        migrated_archive, archive,
        "export bytes must not change when the object is re-encrypted"
    );
    let login_response = send(
        &app,
        "POST",
        "/api/v1/auth/login",
        json!({"email":"admin@example.com","password":"correct horse battery staple"}),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(login_response.status(), StatusCode::OK);
    let search = send(
        &app,
        "GET",
        "/api/v1/search?q=secret",
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(search.status(), StatusCode::OK);
    assert_eq!(
        value(search).await.as_array().unwrap().len(),
        2,
        "the rebuilt index must return old and new-generation pages"
    );
    for (id, ciphertext, hash) in immutable {
        let after: (Vec<u8>, Vec<u8>) =
            sqlx::query_as("SELECT metadata_ciphertext,event_hash FROM audit_events WHERE id=$1")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(after, (ciphertext, hash));
    }
    let old = MasterKey::from_base64(&STANDARD.encode([97; 32])).unwrap();
    let replacement = MasterKey::from_base64(&STANDARD.encode([98; 32]))
        .unwrap()
        .with_versions(2, Some((1, old)))
        .unwrap();
    let replacement_state = AppState::new(Store::from_pool(pool.clone()), replacement, ORIGIN, "")
        .unwrap()
        .with_object_store(objects.clone());
    // Fail after generation one committed, reproducing an interrupted operator
    // batch without mocking the database, cryptography, or rewrap transaction.
    sqlx::raw_sql("CREATE FUNCTION fail_second_rewrap() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF OLD.version=2 THEN RAISE EXCEPTION 'injected failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_second_rewrap BEFORE UPDATE ON workspace_key_generations FOR EACH ROW EXECUTE FUNCTION fail_second_rewrap();").execute(&pool).await.unwrap();
    assert!(
        knowledge_server::operations::rewrap_master_once(&replacement_state)
            .await
            .is_err()
    );
    let checkpoint: Vec<i32> =
        sqlx::query_scalar("SELECT master_version FROM workspace_key_generations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(checkpoint, [2, 1]);
    let ready = send(
        &build_router(replacement_state.clone()),
        "GET",
        "/api/v1/health/ready",
        json!({}),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(
        ready.status(),
        StatusCode::OK,
        "both masters must read the partially committed batch"
    );
    sqlx::raw_sql("DROP TRIGGER fail_second_rewrap ON workspace_key_generations; DROP FUNCTION fail_second_rewrap();").execute(&pool).await.unwrap();
    assert_eq!(
        knowledge_server::operations::rewrap_master_once(&replacement_state)
            .await
            .unwrap(),
        1
    );
    assert_eq!(
        knowledge_server::operations::rewrap_master_once(&replacement_state)
            .await
            .unwrap(),
        0
    );
    let new_only = AppState::new(
        Store::from_pool(pool.clone()),
        MasterKey::from_base64(&STANDARD.encode([98; 32]))
            .unwrap()
            .with_versions(2, None)
            .unwrap(),
        ORIGIN,
        "",
    )
    .unwrap()
    .with_object_store(objects);
    let replacement_app = build_router(new_only);
    assert_eq!(
        send(
            &replacement_app,
            "GET",
            &format!("/api/v1/pages/{page}"),
            json!({}),
            Some(&cookie),
            None,
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &replacement_app,
            "GET",
            "/api/v1/audit-events",
            json!({}),
            Some(&cookie),
            None,
            None
        )
        .await
        .status(),
        StatusCode::OK
    );
    assert_eq!(
        send(
            &app,
            "GET",
            "/api/v1/health/ready",
            json!({}),
            None,
            None,
            None
        )
        .await
        .status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[sqlx::test]
async fn maintenance_failure_is_redacted_observable_and_retryable(pool: PgPool) {
    let fail_key = Arc::new(Mutex::new(None));
    let objects = Arc::new(FailDeleteStore {
        inner: MemoryObjectStore::default(),
        fail_key: fail_key.clone(),
    });
    let (app, state, pool) = setup_with_objects(pool, objects.clone()).await;
    let (cookie, csrf) = login(&app).await;
    let (_, page) = create_space_page(&app, &cookie, &csrf).await;
    upload_attachment(&app, &cookie, &csrf, page).await;
    let object: Uuid = sqlx::query_scalar("SELECT object_key FROM attachments WHERE page_id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE pages SET archived_at=now()-interval '31 days' WHERE id=$1")
        .bind(page)
        .execute(&pool)
        .await
        .unwrap();
    *fail_key.lock().unwrap() = Some(object);
    let runner = JobRunner::new(Uuid::now_v7());
    assert!(runner.run_maintenance_once(&state).await.is_err());
    let health = value(
        send(
            &app,
            "GET",
            "/api/v1/admin/status",
            json!({}),
            Some(&cookie),
            None,
            None,
        )
        .await,
    )
    .await;
    assert_eq!(health["maintenance"]["failures"], 1);
    assert_eq!(health["maintenance"]["lastFailure"]["phase"], "retention");
    assert_eq!(
        health["maintenance"]["lastFailure"]["code"],
        "operation_failed"
    );
    assert!(objects.size(object).await.is_ok());
    *fail_key.lock().unwrap() = None;
    // Simulate an abandoned process lease; a different runner must reclaim it.
    sqlx::query("UPDATE maintenance_leases SET expires_at=now()-interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    let retry = JobRunner::new(Uuid::now_v7());
    for _ in 0..3 {
        retry.run_maintenance_once(&state).await.unwrap();
    }
    assert!(objects.size(object).await.is_err());
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pages WHERE id=$1)")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!exists);
    let audits:i64=sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE event_type='retention.purged' AND target_type='attachment'").fetch_one(&pool).await.unwrap();
    assert_eq!(audits, 1);
}

#[sqlx::test]
async fn key_rotation_resumes_after_interrupted_stream_and_abandoned_lease(pool: PgPool) {
    let stalled = Arc::new(AtomicBool::new(false));
    let entered = Arc::new(AtomicBool::new(false));
    let objects = Arc::new(StallReadStore {
        inner: MemoryObjectStore::default(),
        stalled: stalled.clone(),
        entered: entered.clone(),
    });
    let (app, state, pool) = setup_with_objects(pool, objects.clone()).await;
    let (cookie, csrf) = login(&app).await;
    let (_, page) = create_space_page(&app, &cookie, &csrf).await;
    let file = vec![b'z'; 131_075];
    upload_attachment_bytes(&app, &cookie, &csrf, page, &file).await;
    let old: Uuid = sqlx::query_scalar("SELECT object_key FROM attachments WHERE page_id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/v1/admin/key-rotations",
            json!({}),
            Some(&cookie),
            Some(&csrf),
            None
        )
        .await
        .status(),
        StatusCode::ACCEPTED
    );
    stalled.store(true, Ordering::SeqCst);
    let crashed_state = state.clone();
    let task = tokio::spawn(async move {
        let runner = JobRunner::new(Uuid::now_v7());
        for _ in 0..20 {
            runner.run_maintenance_once(&crashed_state).await.unwrap();
        }
    });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !entered.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    let abandoned: (Uuid, Option<String>) =
        sqlx::query_as("SELECT object_key,remote_upload_id FROM key_rotation_objects")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(abandoned.1.is_some());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    stalled.store(false, Ordering::SeqCst);
    sqlx::query("UPDATE key_rotations SET lease_expires_at=now()-interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE key_rotation_objects SET expires_at=now()-interval '1 second'")
        .execute(&pool)
        .await
        .unwrap();
    let first = JobRunner::new(Uuid::now_v7());
    let second = JobRunner::new(Uuid::now_v7());
    for _ in 0..20 {
        let (a, b) = tokio::join!(
            first.run_maintenance_once(&state),
            second.run_maintenance_once(&state)
        );
        a.unwrap();
        b.unwrap();
    }
    let phase: String = sqlx::query_scalar("SELECT state FROM key_rotations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(phase, "complete");
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM key_rotation_objects")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pending, 0);
    assert!(objects.size(old).await.is_err());
    assert!(objects.size(abandoned.0).await.is_err());
    let id: Uuid = sqlx::query_scalar("SELECT id FROM attachments WHERE page_id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    let response = send(
        &app,
        "GET",
        &format!("/api/v1/attachments/{id}"),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        to_bytes(response.into_body(), 1_000_000)
            .await
            .unwrap()
            .as_ref(),
        file
    );
    let completed: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='key.rotation_completed'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(completed, 1);
}

#[sqlx::test]
async fn key_rotation_retries_published_object_cleanup_without_losing_new_ciphertext(pool: PgPool) {
    let fail_key = Arc::new(Mutex::new(None));
    let objects = Arc::new(FailDeleteStore {
        inner: MemoryObjectStore::default(),
        fail_key: fail_key.clone(),
    });
    let (app, state, pool) = setup_with_objects(pool, objects.clone()).await;
    let (cookie, csrf) = login(&app).await;
    let (_, page) = create_space_page(&app, &cookie, &csrf).await;
    upload_attachment(&app, &cookie, &csrf, page).await;
    let old: Uuid = sqlx::query_scalar("SELECT object_key FROM attachments WHERE page_id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    *fail_key.lock().unwrap() = Some(old);
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/v1/admin/key-rotations",
            json!({}),
            Some(&cookie),
            Some(&csrf),
            None
        )
        .await
        .status(),
        StatusCode::ACCEPTED
    );
    let runner = JobRunner::new(Uuid::now_v7());
    let mut failed = false;
    for _ in 0..20 {
        if runner.run_maintenance_once(&state).await.is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed);
    let (new, version): (Uuid, i32) =
        sqlx::query_as("SELECT object_key,key_version FROM attachments WHERE page_id=$1")
            .bind(page)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_ne!(old, new);
    assert_eq!(version, 2);
    assert!(objects.size(new).await.is_ok());
    assert!(objects.size(old).await.is_ok());
    *fail_key.lock().unwrap() = None;
    let retry = JobRunner::new(Uuid::now_v7());
    for _ in 0..20 {
        retry.run_maintenance_once(&state).await.unwrap();
    }
    assert!(objects.size(old).await.is_err());
    assert!(objects.size(new).await.is_ok());
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM key_rotation_objects")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pending, 0);
}

#[sqlx::test]
async fn expired_page_recovery_fails_before_purge(pool: PgPool) {
    let (app, _, pool, _) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (_, page) = create_space_page(&app, &cookie, &csrf).await;
    sqlx::query("UPDATE pages SET archived_at=now()-interval '31 days' WHERE id=$1")
        .bind(page)
        .execute(&pool)
        .await
        .unwrap();
    let response = send(
        &app,
        "POST",
        &format!("/api/v1/pages/{page}/unarchive"),
        json!({}),
        Some(&cookie),
        Some(&csrf),
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test]
async fn retention_discovery_hides_expired_attachments_and_purge_is_bounded(pool: PgPool) {
    let (app, state, pool, _) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, page) = create_space_page(&app, &cookie, &csrf).await;
    upload_attachment(&app, &cookie, &csrf, page).await;
    sqlx::query("UPDATE attachments SET archived_at=now()-interval '31 days' WHERE page_id=$1")
        .bind(page)
        .execute(&pool)
        .await
        .unwrap();
    let response = send(
        &app,
        "GET",
        &format!("/api/v1/pages/{page}/attachments?archived=true"),
        json!({}),
        Some(&cookie),
        None,
        None,
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        value(response).await.as_array().unwrap().is_empty(),
        "expired recovery item was disclosed"
    );
    for _ in 0..17 {
        let response=send(&app,"POST","/api/v1/pages",json!({"spaceId":space,"title":"Retired","markdown":"bounded","properties":{},"restricted":false}),Some(&cookie),Some(&csrf),None).await;
        assert_eq!(response.status(), StatusCode::CREATED);
    }
    sqlx::query("UPDATE pages SET archived_at=now()-interval '31 days' WHERE id<>$1")
        .bind(page)
        .execute(&pool)
        .await
        .unwrap();
    let runner = JobRunner::new(Uuid::now_v7());
    runner.run_maintenance_once(&state).await.unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pages WHERE archived_at IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        remaining, 1,
        "one pass must process at most 16 page parents"
    );
    runner.run_maintenance_once(&state).await.unwrap();
    let remaining: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pages WHERE archived_at IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 0);
}

#[sqlx::test]
async fn expired_space_purges_unexpired_export_objects_without_fk_failure(pool: PgPool) {
    let (app, state, pool, objects) = setup(pool).await;
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
    assert_eq!(response.status(), StatusCode::ACCEPTED);
    let runner = JobRunner::new(Uuid::now_v7());
    assert!(runner.run_once(&state).await.unwrap());
    let object: Uuid = sqlx::query_scalar("SELECT object_key FROM exports WHERE space_id=$1")
        .bind(space)
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE spaces SET archived_at=now()-interval '31 days' WHERE id=$1")
        .bind(space)
        .execute(&pool)
        .await
        .unwrap();
    for _ in 0..3 {
        runner.run_maintenance_once(&state).await.unwrap();
    }
    let remaining: i64 = sqlx::query_scalar("SELECT count(*) FROM spaces WHERE id=$1")
        .bind(space)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    assert!(objects.size(object).await.is_err());
}

#[sqlx::test]
async fn export_idempotency_key_is_bound_to_space(pool: PgPool) {
    let (app, _, _, _) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (first, _) = create_space_page(&app, &cookie, &csrf).await;
    let (second, _) = create_space_page(&app, &cookie, &csrf).await;
    let key = Uuid::now_v7().to_string();
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/v1/exports",
            json!({"spaceId":first}),
            Some(&cookie),
            Some(&csrf),
            Some(&key)
        )
        .await
        .status(),
        StatusCode::ACCEPTED
    );
    assert_eq!(
        send(
            &app,
            "POST",
            "/api/v1/exports",
            json!({"spaceId":second}),
            Some(&cookie),
            Some(&csrf),
            Some(&key)
        )
        .await
        .status(),
        StatusCode::CONFLICT
    );
}

#[sqlx::test]
async fn retention_purges_domain_rows_objects_and_revisions_once(pool: PgPool) {
    let (app, state, pool, objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, page) = create_space_page(&app, &cookie, &csrf).await;
    let (_, retained_page) = create_space_page(&app, &cookie, &csrf).await;
    upload_attachment(&app, &cookie, &csrf, page).await;
    let object: Uuid = sqlx::query_scalar("SELECT object_key FROM attachments WHERE page_id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    let group = value(
        send(
            &app,
            "POST",
            "/api/v1/groups",
            json!({"name":"retired group"}),
            Some(&cookie),
            Some(&csrf),
            None,
        )
        .await,
    )
    .await;
    let group: Uuid = group["id"].as_str().unwrap().parse().unwrap();
    sqlx::query("UPDATE spaces SET archived_at=now()-interval '31 days' WHERE id=$1")
        .bind(space)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE groups SET archived_at=now()-interval '31 days' WHERE id=$1")
        .bind(group)
        .execute(&pool)
        .await
        .unwrap();
    let first = JobRunner::new(Uuid::now_v7());
    let second = JobRunner::new(Uuid::now_v7());
    for _ in 0..5 {
        let (a, b) = tokio::join!(
            first.run_maintenance_once(&state),
            second.run_maintenance_once(&state)
        );
        a.unwrap();
        b.unwrap();
    }
    for (query, id) in [
        ("SELECT count(*) FROM pages WHERE id=$1", page),
        ("SELECT count(*) FROM spaces WHERE id=$1", space),
        ("SELECT count(*) FROM groups WHERE id=$1", group),
    ] {
        let count: i64 = sqlx::query_scalar(query)
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "retained expired domain row");
    }
    assert!(objects.size(object).await.is_err());
    let retained: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pages WHERE id=$1)")
        .bind(retained_page)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(retained);
    let audits: i64 =
        sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE event_type='retention.purged'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(audits, 4);
    let revisions: i64 = sqlx::query_scalar("SELECT count(*) FROM page_revisions WHERE page_id=$1")
        .bind(page)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(revisions, 0);
}
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
async fn wait_for_locked_query(pool: &PgPool, query_fragment: &str) {
    let pattern = format!("%{query_fragment}%");
    for _ in 0..200 {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM pg_stat_activity
                 WHERE datname=current_database()
                   AND pid<>pg_backend_pid()
                   AND wait_event_type='Lock'
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
    upload_attachment_bytes(app, cookie, csrf, page, b"attachment").await;
}
async fn upload_attachment_bytes(app: &Router, cookie: &str, csrf: &str, page: Uuid, bytes: &[u8]) {
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
async fn export_expiry_uses_the_persisted_operational_setting(pool: PgPool) {
    let (app, state, pool, _) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    assert_eq!(
        send(
            &app,
            "PATCH",
            "/api/v1/admin/settings",
            json!({"attachmentRecoveryDays":30,"exportRetentionHours":36}),
            Some(&cookie),
            Some(&csrf),
            None,
        )
        .await
        .status(),
        StatusCode::OK
    );
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
    let created = send(
        &app,
        "POST",
        "/api/v1/exports",
        json!({"spaceId":space}),
        Some(&cookie),
        Some(&csrf),
        Some(&Uuid::now_v7().to_string()),
    )
    .await;
    assert_eq!(created.status(), StatusCode::ACCEPTED);
    let export_id: Uuid = value(created).await["id"]
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
    let hours: f64 = sqlx::query_scalar(
        "SELECT (EXTRACT(EPOCH FROM (expires_at-ready_at))/3600)::double precision FROM exports WHERE id=$1",
    )
    .bind(export_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        (hours - 36.0).abs() < 0.01,
        "expected 36 hours, got {hours}"
    );
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
async fn export_list_rechecks_admin_role_after_workspace_lock(pool: PgPool) {
    let (app, _state, pool, _objects) = setup(pool).await;
    let (cookie, csrf) = login(&app).await;
    let (space, _) = create_space_page(&app, &cookie, &csrf).await;
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
    let (workspace, user): (Uuid, Uuid) =
        sqlx::query_as("SELECT workspace_id,id FROM users LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM workspaces WHERE id=$1 FOR UPDATE")
        .bind(workspace)
        .fetch_one(&mut *blocker)
        .await
        .unwrap();

    let list_app = app.clone();
    let list_cookie = cookie.clone();
    let list = tokio::spawn(async move {
        send(
            &list_app,
            "GET",
            "/api/v1/exports",
            json!({}),
            Some(&list_cookie),
            None,
            None,
        )
        .await
    });
    wait_for_locked_query(&pool, "SELECT id FROM workspaces WHERE id = $1 FOR UPDATE").await;
    sqlx::query(
        "UPDATE workspace_members SET role='member'
          WHERE workspace_id=$1 AND user_id=$2",
    )
    .bind(workspace)
    .bind(user)
    .execute(&mut *blocker)
    .await
    .unwrap();
    sqlx::query("DELETE FROM space_grants WHERE workspace_id=$1 AND space_id=$2")
        .bind(workspace)
        .bind(space)
        .execute(&mut *blocker)
        .await
        .unwrap();
    blocker.commit().await.unwrap();

    let response = tokio::time::timeout(std::time::Duration::from_secs(5), list)
        .await
        .expect("list must resume after the workspace lock is released")
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = value(response).await;
    assert_eq!(body["exports"].as_array().unwrap().len(), 0);
    let inappropriate_audits: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM audit_events WHERE event_type='export.admin_access'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(inappropriate_audits, 0);
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
