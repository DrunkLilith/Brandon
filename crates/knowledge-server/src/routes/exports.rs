use axum::{
    Json, Router,
    body::Body,
    extract::{Extension, Path, Query, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use futures_util::stream;
use knowledge_core::{
    ids::{SpaceId, UserId},
    permissions::Capability,
};
use knowledge_crypto::{KeyPurpose, KeyVersion, stream::AttachmentStream};
use knowledge_store::{AccessResolver, attachments::lock_object, users::UserRepository};
use serde::{Deserialize, Serialize};
use serde_json::json;
use uuid::Uuid;

const OP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

use crate::{
    CurrentUser,
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    middleware::RequestId,
    operations::{ExportJobPayload, authorize_export_snapshot, encrypt_job_payload},
    problem::ApiProblem,
    routes::{auth::rate_limit_bucket, spaces::revalidate_session, users::load_workspace_key},
    state::AppState,
};

pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/exports", post(create).get(list))
        .route("/api/v1/exports/{id}/download", get(download))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Create {
    space_id: Uuid,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExportResponse {
    id: Uuid,
    space_id: Uuid,
    status: String,
    expires_at: Option<chrono::DateTime<chrono::Utc>>,
    created_at: chrono::DateTime<chrono::Utc>,
}
#[derive(Deserialize)]
struct ListQuery {
    after: Option<Uuid>,
    limit: Option<usize>,
}

async fn create(
    State(state): State<AppState>,
    current: CurrentUser,
    Extension(request): Extension<RequestId>,
    headers: HeaderMap,
    input: Result<Json<Create>, axum::extract::rejection::JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) = input.map_err(|_| ApiProblem::validation("body", "is invalid"))?;
    let request_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<Uuid>().ok())
        .ok_or_else(|| ApiProblem::validation("Idempotency-Key", "must be a UUID"))?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut tx, current).await?;
    let access = AccessResolver::for_space(
        UserId::from_uuid(current.user_id),
        SpaceId::from_uuid(input.space_id),
        &mut tx,
    )
    .await
    .map_err(internal)?;
    if !access.can(
        Capability::ManageSpace,
        knowledge_core::permissions::ResourceContext::Workspace,
    ) {
        return Err(ApiProblem::not_found());
    }
    if let Some(row)=sqlx::query_as::<_,(Uuid,Uuid,String,Option<chrono::DateTime<chrono::Utc>>,chrono::DateTime<chrono::Utc>)>("SELECT id,space_id,status,expires_at,created_at FROM exports WHERE workspace_id=$1 AND requested_by=$2 AND request_key=$3").bind(current.workspace_id).bind(current.user_id).bind(request_key).fetch_optional(&mut *tx).await.map_err(internal)? {tx.commit().await.map_err(internal)?;return Ok((StatusCode::ACCEPTED,Json(ExportResponse{id:row.0,space_id:row.1,status:row.2,expires_at:row.3,created_at:row.4})).into_response());}
    let bucket = rate_limit_bucket(b"export-actor", current.user_id.as_bytes());
    if !UserRepository::consume_rate_limit_attempt(runtime.store.pool(), &[bucket], 10)
        .await
        .map_err(internal)?
    {
        return Err(ApiProblem::export_rate_limited());
    }
    let export = Uuid::now_v7();
    let job = Uuid::now_v7();
    let payload = ExportJobPayload {
        export_id: export,
        space_id: input.space_id,
        requester_id: current.user_id,
        session_id: current.session_id,
        request_id: request.as_uuid(),
    };
    let encrypted =
        encrypt_job_payload(&key, current.workspace_id, job, &payload).map_err(internal)?;
    sqlx::query("INSERT INTO jobs(id,workspace_id,job_type,payload_ciphertext,payload_nonce,payload_key_version,status) VALUES($1,$2,'export',$3,$4,1,'queued')").bind(job).bind(current.workspace_id).bind(encrypted.0).bind(encrypted.1.as_slice()).execute(&mut *tx).await.map_err(internal)?;
    let created:chrono::DateTime<chrono::Utc>=sqlx::query_scalar("INSERT INTO exports(id,workspace_id,space_id,requested_by,job_id,status,request_key) VALUES($1,$2,$3,$4,$5,'queued',$6) RETURNING created_at").bind(export).bind(current.workspace_id).bind(input.space_id).bind(current.user_id).bind(job).bind(request_key).fetch_one(&mut *tx).await.map_err(internal)?;
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "export.requested",
            target_type: "export",
            target_id: Some(export),
            request_id: request.as_uuid(),
            metadata: json!({"spaceId":input.space_id}),
        },
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok((
        StatusCode::ACCEPTED,
        Json(ExportResponse {
            id: export,
            space_id: input.space_id,
            status: "queued".into(),
            expires_at: None,
            created_at: created,
        }),
    )
        .into_response())
}

async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    Extension(request): Extension<RequestId>,
    query: Result<Query<ListQuery>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, ApiProblem> {
    let Query(query) = query.map_err(|_| ApiProblem::validation("query", "is invalid"))?;
    let limit = query.limit.unwrap_or(100);
    if !(1..=100).contains(&limit) {
        return Err(ApiProblem::validation("limit", "must be between 1 and 100"));
    }
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut tx, current).await?;
    let rows = sqlx::query_as::<
        _,
        (
            Uuid,
            Uuid,
            String,
            Option<chrono::DateTime<chrono::Utc>>,
            chrono::DateTime<chrono::Utc>,
        ),
    >(
        "SELECT e.id,e.space_id,e.status,e.expires_at,e.created_at
           FROM exports e
           JOIN spaces s ON s.workspace_id=e.workspace_id AND s.id=e.space_id
          WHERE e.workspace_id=$1 AND s.archived_at IS NULL
            AND ($4::uuid IS NULL OR e.id<$4)
            AND ($3 OR EXISTS (
                SELECT 1 FROM space_grants sg
                LEFT JOIN groups g
                  ON g.workspace_id=sg.workspace_id AND g.id=sg.group_id
                LEFT JOIN group_members gm
                  ON gm.workspace_id=sg.workspace_id AND gm.group_id=sg.group_id
                 AND gm.user_id=$2
                WHERE sg.workspace_id=e.workspace_id AND sg.space_id=e.space_id
                  AND sg.role='owner'
                  AND (sg.user_id=$2 OR (
                      sg.group_id IS NOT NULL AND g.archived_at IS NULL
                      AND gm.user_id IS NOT NULL
                  ))
            ))
          ORDER BY e.id DESC LIMIT $5",
    )
    .bind(current.workspace_id)
    .bind(current.user_id)
    .bind(current.workspace_admin)
    .bind(query.after)
    .bind(i64::try_from(limit + 1).map_err(internal)?)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    let has_more = rows.len() > limit;
    let output: Vec<_> = rows
        .into_iter()
        .take(limit)
        .map(|row| ExportResponse {
            id: row.0,
            space_id: row.1,
            status: row.2,
            expires_at: row.3,
            created_at: row.4,
        })
        .collect();
    let next_cursor = has_more
        .then(|| output.last().map(|item| item.id))
        .flatten();
    if current.workspace_admin {
        append_audit_event(
            &mut tx,
            &key,
            AuditEvent {
                workspace_id: current.workspace_id,
                actor_user_id: Some(current.user_id),
                session_id: Some(current.session_id),
                event_type: "export.admin_access",
                target_type: "export",
                target_id: None,
                request_id: request.as_uuid(),
                metadata: json!({"operation":"list"}),
            },
        )
        .await
        .map_err(internal)?;
    }
    tx.commit().await.map_err(internal)?;
    let mut response = Json(json!({"exports":output,"nextCursor":next_cursor})).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn download(
    State(state): State<AppState>,
    current: CurrentUser,
    Extension(request): Extension<RequestId>,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let objects = runtime
        .objects
        .clone()
        .ok_or_else(ApiProblem::unavailable)?;
    let stream_permit = runtime
        .object_streams
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiProblem::unavailable())?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let object: Uuid = sqlx::query_scalar(
        "SELECT object_key FROM exports
          WHERE workspace_id=$1 AND id=$2 AND status='ready' AND expires_at>now()",
    )
    .bind(current.workspace_id)
    .bind(id)
    .fetch_optional(runtime.store.pool())
    .await
    .map_err(internal)?
    .ok_or_else(ApiProblem::not_found)?;
    let object_guard = tokio::time::timeout(OP_TIMEOUT, lock_object(runtime.store.pool(), object))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut tx, current).await?;
    let row=sqlx::query_as::<_,(Uuid,Uuid,i64,i64,i32)>("SELECT object_key,space_id,plaintext_size,ciphertext_size,key_version FROM exports WHERE workspace_id=$1 AND id=$2 AND object_key=$3 AND status='ready' AND expires_at>now()").bind(current.workspace_id).bind(id).bind(object).fetch_optional(&mut *tx).await.map_err(internal)?.ok_or_else(ApiProblem::not_found)?;
    authorize_export_snapshot(&mut tx, &key, current, id, row.1).await?;
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "export.downloaded",
            target_type: "export",
            target_id: Some(id),
            request_id: request.as_uuid(),
            metadata: json!({"operation":"download"}),
        },
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let plaintext = u64::try_from(row.2).map_err(internal)?;
    let expected = u64::try_from(row.3).map_err(internal)?;
    let actual = tokio::time::timeout(OP_TIMEOUT, objects.size(row.0))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    if actual != expected {
        return Err(ApiProblem::internal());
    }
    let stream_cipher = AttachmentStream::new_export(
        key.derive(KeyPurpose::Attachment).map_err(internal)?,
        KeyVersion(u32::try_from(row.4).map_err(internal)?),
        current.workspace_id.as_bytes(),
        id.as_bytes(),
        plaintext,
    );
    let footer = tokio::time::timeout(
        OP_TIMEOUT,
        objects.read_range(row.0, stream_cipher.footer_range()),
    )
    .await
    .map_err(internal)?
    .map_err(internal)?;
    stream_cipher.verify_footer(&footer).map_err(internal)?;
    let body_stream = stream::try_unfold(
        (
            objects,
            stream_cipher,
            row.0,
            0_u64,
            object_guard,
            stream_permit,
        ),
        |(objects, cipher, object, index, guard, permit)| async move {
            if index >= cipher.chunk_count() {
                return Ok::<_, knowledge_store::attachments::StorageError>(None);
            }
            let range = cipher
                .frame_range(index)
                .map_err(|_| knowledge_store::attachments::StorageError)?;
            let frame = tokio::time::timeout(OP_TIMEOUT, objects.read_range(object, range))
                .await
                .map_err(|_| knowledge_store::attachments::StorageError)??;
            let clear = cipher
                .decrypt_chunk(index, &frame)
                .map_err(|_| knowledge_store::attachments::StorageError)?;
            Ok(Some((
                bytes::Bytes::from(clear),
                (objects, cipher, object, index + 1, guard, permit),
            )))
        },
    );
    let mut response = Body::from_stream(body_stream).into_response();
    *response.status_mut() = StatusCode::OK;
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-tar"),
    );
    response.headers_mut().insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=export-{id}.tar"))
            .map_err(internal)?,
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response.headers_mut().insert(
        header::CONTENT_LENGTH,
        HeaderValue::from_str(&plaintext.to_string()).map_err(internal)?,
    );
    Ok(response)
}
fn internal<E>(_error: E) -> ApiProblem {
    ApiProblem::internal()
}
