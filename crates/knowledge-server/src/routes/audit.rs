use crate::{
    CurrentUser,
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    middleware::RequestId,
    problem::ApiProblem,
    routes::{
        spaces::revalidate_session,
        users::{decrypt_text, load_workspace_key},
    },
    state::AppState,
};
use axum::{
    Json,
    extract::{Extension, Query, State},
    http::{HeaderValue, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use knowledge_store::audit::{AuditFilters, AuditReader};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

api_router! {
    pub(crate) fn router;
    "/api/v1/audit-events" => [get => list => ("listAuditEvents", Session, 200)],
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Filters {
    actor: Option<Uuid>,
    #[serde(rename = "type")]
    event_type: Option<String>,
    target: Option<Uuid>,
    target_type: Option<String>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    after: Option<Uuid>,
    limit: Option<i64>,
}
async fn list(
    State(state): State<AppState>,
    current: CurrentUser,
    Extension(request): Extension<RequestId>,
    query: Result<Query<Filters>, axum::extract::rejection::QueryRejection>,
) -> Result<Response, ApiProblem> {
    let Query(q) = query.map_err(|_| ApiProblem::validation("query", "is invalid"))?;
    if q.limit.is_some_and(|n| !(1..=100).contains(&n))
        || q.from.is_some_and(|a| q.to.is_some_and(|b| a > b))
        || q.event_type.as_ref().is_some_and(|s| s.len() > 128)
        || q.target_type.as_ref().is_some_and(|s| s.len() > 128)
    {
        return Err(ApiProblem::validation("query", "is invalid"));
    }
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(|_| ApiProblem::internal())?;
    revalidate_session(&mut tx, current).await?;
    let admin:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users u JOIN workspace_members m ON m.workspace_id=u.workspace_id AND m.user_id=u.id WHERE u.workspace_id=$1 AND u.id=$2 AND u.status='active' AND m.status='active' AND m.role='admin')").bind(current.workspace_id).bind(current.user_id).fetch_one(&mut *tx).await.map_err(|_|ApiProblem::internal())?;
    if !admin {
        return Err(ApiProblem::forbidden());
    }
    let records = AuditReader::list(
        &mut tx,
        current.workspace_id,
        AuditFilters {
            actor: q.actor,
            event_type: q.event_type,
            target: q.target,
            target_type: q.target_type,
            from: q.from,
            to: q.to,
            after: q.after,
        },
        q.limit.unwrap_or(100),
    )
    .await
    .map_err(|_| ApiProblem::internal())?;
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "audit.admin_access",
            target_type: "audit",
            target_id: None,
            request_id: request.as_uuid(),
            metadata: json!({"operation":"list"}),
        },
    )
    .await
    .map_err(|_| ApiProblem::internal())?;
    tx.commit().await.map_err(|_| ApiProblem::internal())?;
    let mut events = Vec::new();
    for r in records {
        let metadata: serde_json::Value = serde_json::from_str(&decrypt_text(
            &key,
            &format!("workspace:{}:audit:{}:metadata", r.workspace_id, r.id),
            r.metadata_ciphertext,
            r.metadata_nonce,
            r.metadata_key_version,
        )?)
        .map_err(|_| ApiProblem::internal())?;
        events.push(json!({"id":r.id,"actorUserId":r.actor_user_id,"sessionId":r.session_id,"type":r.event_type,"targetType":r.target_type,"targetId":r.target_id,"requestId":r.request_id,"createdAt":r.created_at,"metadata":metadata}));
    }
    let after = events.last().map(|e| e["id"].clone());
    let mut response = Json(json!({"events":events,"nextCursor":after})).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}
