use std::time::Duration;

use axum::{
    Json,
    extract::{Path, Query, State, rejection::JsonRejection, rejection::QueryRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use knowledge_core::permissions::Capability;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use super::{
    auth::{CurrentUser, unwrap_workspace_key},
    spaces::{authorize, resolve_access, revalidate_session},
    users::{
        decrypt_text, load_workspace_key, load_workspace_key_in_transaction,
        lock_and_revalidate_admin,
    },
};
use crate::{
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    middleware::RequestId,
    problem::ApiProblem,
    state::AppState,
};
use axum::Extension;

const DEFAULT_ATTACHMENT_RECOVERY_DAYS: i32 = 30;
const DEFAULT_EXPORT_RETENTION_HOURS: i32 = 24;

api_router! {
    pub(crate) fn router;
    "/api/v1/admin/settings" => [
        get => settings => ("getSettings", Session, 200),
        patch => update_settings => ("updateSettings", Mutation, 200),
    ],
    "/api/v1/admin/status" => [get => status => ("getAdminStatus", Session, 200)],
    "/api/v1/groups/archived" => [get => archived_groups => ("listArchivedGroups", Session, 200)],
    "/api/v1/groups/{id}/restore" => [post => restore_group => ("restoreGroup", Mutation, 204)],
    "/api/v1/spaces/archived" => [get => archived_spaces => ("listArchivedSpaces", Session, 200)],
    "/api/v1/spaces/{id}/restore" => [post => restore_space => ("restoreSpace", Mutation, 204)],
    "/api/v1/spaces/{id}/subjects" => [get => subjects => ("listSubjects", Session, 200)],
    "/api/v1/spaces/{id}/archived-pages" => [get => archived_pages => ("listArchivedPages", Session, 200)],
    "/api/v1/admin/key-rotations" => [
        get => key_rotation_status => ("getKeyRotation", Session, 200),
        post => start_key_rotation => ("startKeyRotation", Mutation, 202),
    ],
}

async fn key_rotation_status(
    State(state): State<AppState>,
    Extension(request): Extension<RequestId>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_and_revalidate_admin(&mut tx, current).await?;
    let key = load_workspace_key_in_transaction(&state, &mut tx, current.workspace_id).await?;
    let row:Option<(i32,i32,String,i64)>=sqlx::query_as("SELECT current_version,previous_version,state,processed FROM key_rotations WHERE workspace_id=$1").bind(current.workspace_id).fetch_optional(&mut *tx).await.map_err(internal)?;
    super::users::audit_disclosure(&mut tx, &key, current, request, "key.admin_access").await?;
    tx.commit().await.map_err(internal)?;
    let (version, previous, phase, processed) =
        row.unwrap_or((key.version().0 as i32, 0, "complete".into(), 0));
    Ok(no_store(Json(json!({"currentVersion":version,"previousVersion":if previous==0 {None} else {Some(previous)},"state":phase,"processed":processed})).into_response()))
}
async fn start_key_rotation(
    State(state): State<AppState>,
    Extension(request): Extension<RequestId>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_and_revalidate_admin(&mut tx, current).await?;
    let busy: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM key_rotations WHERE workspace_id=$1 AND state<>'complete')",
    )
    .bind(current.workspace_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    if busy {
        return Err(ApiProblem::conflict("A key rotation is already running"));
    }
    let old = load_workspace_key_in_transaction(&state, &mut tx, current.workspace_id).await?;
    let key = old.rotated().map_err(internal)?;
    let wrapped =
        knowledge_crypto::Keyring::wrap(&runtime.master_key, &key, runtime.master_key.version())
            .map_err(internal)?;
    sqlx::query("UPDATE workspaces SET current_key_version=$2,wrapped_key_ciphertext=$3,wrapped_key_nonce=$4,wrapped_key_version=$5 WHERE id=$1")
        .bind(current.workspace_id).bind(key.version().0 as i32).bind(wrapped.ciphertext).bind(wrapped.nonce.as_slice()).bind(wrapped.key_version.0 as i32).execute(&mut *tx).await.map_err(internal)?;
    sqlx::query("INSERT INTO key_rotations(workspace_id,current_version,previous_version,state) VALUES($1,$2,$3,'migrating') ON CONFLICT(workspace_id) DO UPDATE SET current_version=EXCLUDED.current_version,previous_version=EXCLUDED.previous_version,state='migrating',after_page=NULL,processed=0,lease_owner=NULL,lease_expires_at=NULL,updated_at=now()")
        .bind(current.workspace_id).bind(key.version().0 as i32).bind(old.version().0 as i32).execute(&mut *tx).await.map_err(internal)?;
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "key.rotation_started",
            target_type: "workspace",
            target_id: Some(current.workspace_id),
            request_id: request.as_uuid(),
            metadata: json!({"operation":"key_rotation"}),
        },
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(no_store((StatusCode::ACCEPTED,Json(json!({"currentVersion":key.version().0,"previousVersion":old.version().0,"state":"migrating","processed":0}))).into_response()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Settings {
    attachment_recovery_days: i32,
    export_retention_hours: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SettingsUpdate {
    attachment_recovery_days: i32,
    export_retention_hours: i32,
}

async fn settings(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_and_revalidate_admin(&mut tx, current).await?;
    let value = read_settings(&mut tx, current.workspace_id).await?;
    let key = load_workspace_key_in_transaction(&state, &mut tx, current.workspace_id).await?;
    super::users::audit_disclosure(&mut tx, &key, current, request_id, "settings.admin_access")
        .await?;
    tx.commit().await.map_err(internal)?;
    Ok(no_store(Json(value).into_response()))
}

async fn update_settings(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    payload: Result<Json<SettingsUpdate>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) = payload.map_err(|_| ApiProblem::validation("body", "expected settings"))?;
    if !(1..=3650).contains(&input.attachment_recovery_days) {
        return Err(ApiProblem::validation(
            "attachmentRecoveryDays",
            "must be between 1 and 3650",
        ));
    }
    if !(1..=720).contains(&input.export_retention_hours) {
        return Err(ApiProblem::validation(
            "exportRetentionHours",
            "must be between 1 and 720",
        ));
    }
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_and_revalidate_admin(&mut tx, current).await?;
    sqlx::query(
        "INSERT INTO operational_settings (
            workspace_id, attachment_recovery_days, export_retention_hours, updated_by, updated_at
         ) VALUES ($1,$2,$3,$4,now())
         ON CONFLICT (workspace_id) DO UPDATE SET
            attachment_recovery_days=EXCLUDED.attachment_recovery_days,
            export_retention_hours=EXCLUDED.export_retention_hours,
            updated_by=EXCLUDED.updated_by, updated_at=now()",
    )
    .bind(current.workspace_id)
    .bind(input.attachment_recovery_days)
    .bind(input.export_retention_hours)
    .bind(current.user_id)
    .execute(&mut *tx)
    .await
    .map_err(internal)?;
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "settings.updated",
            target_type: "workspace",
            target_id: Some(current.workspace_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"operation":"retention_settings"}),
        },
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(no_store(
        Json(Settings {
            attachment_recovery_days: input.attachment_recovery_days,
            export_retention_hours: input.export_retention_hours,
        })
        .into_response(),
    ))
}

async fn read_settings(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
) -> Result<Settings, ApiProblem> {
    let row: Option<(i32, i32)> = sqlx::query_as(
        "SELECT attachment_recovery_days, export_retention_hours
           FROM operational_settings WHERE workspace_id=$1",
    )
    .bind(workspace)
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal)?;
    let (attachment_recovery_days, export_retention_hours) = row.unwrap_or((
        DEFAULT_ATTACHMENT_RECOVERY_DAYS,
        DEFAULT_EXPORT_RETENTION_HOURS,
    ));
    Ok(Settings {
        attachment_recovery_days,
        export_retention_hours,
    })
}

async fn status(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let components = readiness(&state).await;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_and_revalidate_admin(&mut tx, current).await?;
    let queued_jobs: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT 1 FROM jobs WHERE workspace_id=$1 AND status IN ('queued','running') LIMIT 1001) bounded",
    )
    .bind(current.workspace_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    let active_exports: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM (SELECT 1 FROM exports WHERE workspace_id=$1 AND status IN ('queued','running','ready') LIMIT 1001) bounded",
    )
    .bind(current.workspace_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal)?;
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "status.admin_access",
            target_type: "workspace",
            target_id: Some(current.workspace_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"operation":"status"}),
        },
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let migration_state = if components.migrations {
        "current"
    } else {
        "pending"
    };
    Ok(no_store(
        Json(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "components": {
                "database":components.database,
                "migrations":components.migrations,
                "masterKey":components.master_key,
                "objectStorage":components.object_storage
            },
            "migrationState":migration_state,
            "queuedJobs":queued_jobs,
            "activeExports":active_exports,
            "maintenance":runtime.maintenance.lock().unwrap_or_else(|error|error.into_inner()).clone()
        }))
        .into_response(),
    ))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchivedItem {
    id: Uuid,
    name: String,
    archived_at: DateTime<Utc>,
}

type ArchivedEnvelopeRow = (Uuid, Vec<u8>, Vec<u8>, i32, DateTime<Utc>);

#[derive(Deserialize, Default)]
struct UuidListQuery {
    after: Option<Uuid>,
    limit: Option<usize>,
}

async fn archived_pages(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(space_id): Path<Uuid>,
    query: Result<Query<UuidListQuery>, QueryRejection>,
) -> Result<Response, ApiProblem> {
    let Query(query) = query.map_err(|_| ApiProblem::validation("query", "is invalid"))?;
    let limit = query.limit.unwrap_or(100);
    if !(1..=100).contains(&limit) {
        return Err(ApiProblem::validation("limit", "must be between 1 and 100"));
    }
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    let current = super::users::lock_and_refresh_current(&mut tx, current).await?;
    let access = resolve_access(&mut tx, current.user_id, space_id).await?;
    authorize(&access, Capability::ManageSpace)?;
    // Page allowlists can only reduce an owner's capability. No non-admin can
    // manage a restricted boundary, even with an editor allowlist entry. Filter
    // it in SQL before the cap so hidden pages cannot starve or leak via cursors.
    let rows:Vec<ArchivedEnvelopeRow>=sqlx::query_as(
        "SELECT p.id,p.title_ciphertext,p.title_nonce,p.title_key_version,p.archived_at
         FROM pages p WHERE p.workspace_id=$1 AND p.space_id=$2 AND p.archived_at IS NOT NULL
           AND NOT retention_expired($1,p.archived_at) AND ($3::uuid IS NULL OR p.id>$3)
           AND NOT EXISTS (SELECT 1 FROM retention_purge_marks m
                            WHERE m.workspace_id=p.workspace_id AND m.object_type='page' AND m.object_id=p.id)
           AND ($4 OR NOT EXISTS (
             WITH RECURSIVE ancestors AS (
               SELECT id,parent_id,restricted FROM pages WHERE workspace_id=$1 AND id=p.id
               UNION ALL SELECT a.id,a.parent_id,a.restricted FROM pages a JOIN ancestors x ON x.parent_id=a.id WHERE a.workspace_id=$1
             ) SELECT 1 FROM ancestors WHERE restricted
           )) ORDER BY p.id LIMIT $5"
    ).bind(current.workspace_id).bind(space_id).bind(query.after).bind(current.workspace_admin).bind((limit+1) as i64)
        .fetch_all(&mut *tx).await.map_err(internal)?;
    let has_more = rows.len() > limit;
    let mut items = Vec::new();
    for (id, ciphertext, nonce, version, archived_at) in rows.into_iter().take(limit) {
        let name = decrypt_text(
            &key,
            &super::pages::page_aad(current.workspace_id, id, None, "title"),
            ciphertext,
            nonce,
            version,
        )?;
        items.push(ArchivedItem {
            id,
            name,
            archived_at,
        });
    }
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "recovery.listed",
            target_type: "space",
            target_id: Some(space_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"object_type":"page"}),
        },
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let next_cursor = has_more.then(|| items.last().map(|item| item.id)).flatten();
    Ok(no_store(
        Json(json!({"items":items,"nextCursor":next_cursor})).into_response(),
    ))
}

async fn archived_groups(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    query: Result<Query<UuidListQuery>, QueryRejection>,
) -> Result<Response, ApiProblem> {
    archived_items(&state, current, request_id, "groups", query).await
}

async fn archived_spaces(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    query: Result<Query<UuidListQuery>, QueryRejection>,
) -> Result<Response, ApiProblem> {
    archived_items(&state, current, request_id, "spaces", query).await
}

async fn archived_items(
    state: &AppState,
    current: CurrentUser,
    request_id: RequestId,
    table: &'static str,
    query: Result<Query<UuidListQuery>, QueryRejection>,
) -> Result<Response, ApiProblem> {
    let Query(query) = query.map_err(|_| ApiProblem::validation("query", "is invalid"))?;
    let limit = query.limit.unwrap_or(100);
    if !(1..=100).contains(&limit) {
        return Err(ApiProblem::validation("limit", "must be between 1 and 100"));
    }
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    if table == "groups" {
        lock_and_revalidate_admin(&mut tx, current).await?;
    } else {
        lock_workspace_for_audit(&mut tx, current.workspace_id)
            .await
            .map_err(internal)?;
        revalidate_session(&mut tx, current).await?;
    }
    let settings = read_settings(&mut tx, current.workspace_id).await?;
    let rows: Vec<ArchivedEnvelopeRow> = if table == "groups" {
        sqlx::query_as(
            "SELECT id,name_ciphertext,name_nonce,name_key_version,archived_at
               FROM groups
              WHERE workspace_id=$1 AND archived_at IS NOT NULL
                AND archived_at > clock_timestamp()-make_interval(days=>$2)
                AND NOT EXISTS (SELECT 1 FROM retention_purge_marks m
                                 WHERE m.workspace_id=groups.workspace_id AND m.object_type='group' AND m.object_id=groups.id)
                AND ($3::uuid IS NULL OR id>$3)
              ORDER BY id LIMIT $4",
        )
        .bind(current.workspace_id)
        .bind(settings.attachment_recovery_days)
        .bind(query.after)
        .bind(i64::try_from(limit + 1).map_err(internal)?)
        .fetch_all(&mut *tx)
        .await
        .map_err(internal)?
    } else {
        sqlx::query_as(
            "SELECT s.id,s.name_ciphertext,s.name_nonce,s.name_key_version,s.archived_at
           FROM spaces s
          WHERE s.workspace_id=$1 AND s.archived_at IS NOT NULL
            AND s.archived_at > clock_timestamp()-make_interval(days=>$2)
            AND NOT EXISTS (SELECT 1 FROM retention_purge_marks m
                             WHERE m.workspace_id=s.workspace_id AND m.object_type='space' AND m.object_id=s.id)
            AND ($4::uuid IS NULL OR s.id>$4)
            AND EXISTS (
              SELECT 1 FROM users u
              JOIN workspace_members wm ON wm.workspace_id=u.workspace_id AND wm.user_id=u.id
              WHERE u.workspace_id=$1 AND u.id=$3 AND u.status='active' AND u.deleted_at IS NULL
                AND wm.status='active' AND (
                  wm.role='admin'
                  OR EXISTS (SELECT 1 FROM space_grants sg WHERE sg.workspace_id=$1 AND sg.space_id=s.id AND sg.user_id=$3 AND sg.role='owner')
                  OR EXISTS (
                    SELECT 1 FROM space_grants sg
                    JOIN groups g ON g.workspace_id=sg.workspace_id AND g.id=sg.group_id AND g.archived_at IS NULL
                    JOIN group_members gm ON gm.workspace_id=sg.workspace_id AND gm.group_id=sg.group_id AND gm.user_id=$3
                    WHERE sg.workspace_id=$1 AND sg.space_id=s.id AND sg.role='owner'
                  )
                )
            )
          ORDER BY s.id LIMIT $5",
        )
        .bind(current.workspace_id)
        .bind(settings.attachment_recovery_days)
        .bind(current.user_id)
        .bind(query.after)
        .bind(i64::try_from(limit + 1).map_err(internal)?)
        .fetch_all(&mut *tx)
        .await
        .map_err(internal)?
    };
    let has_more = rows.len() > limit;
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "recovery.listed",
            target_type: "workspace",
            target_id: Some(current.workspace_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"object_type": if table=="groups" {"group"} else {"space"}}),
        },
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    let singular = if table == "groups" { "group" } else { "space" };
    let values = rows
        .into_iter()
        .take(limit)
        .map(|(id, ciphertext, nonce, version, archived_at)| {
            let name = decrypt_text(
                &key,
                &format!("workspace:{}:{singular}:{id}:name", current.workspace_id),
                ciphertext,
                nonce,
                version,
            )?;
            Ok(ArchivedItem {
                id,
                name,
                archived_at,
            })
        })
        .collect::<Result<Vec<_>, ApiProblem>>()?;
    let next_cursor = has_more
        .then(|| values.last().map(|item| item.id))
        .flatten();
    Ok(no_store(
        Json(json!({"items":values,"nextCursor":next_cursor})).into_response(),
    ))
}

async fn restore_group(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    restore(
        &state,
        current,
        request_id,
        id,
        "groups",
        "group.restored",
        true,
    )
    .await
}

async fn restore_space(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    restore(
        &state,
        current,
        request_id,
        id,
        "spaces",
        "space.restored",
        false,
    )
    .await
}

async fn restore(
    state: &AppState,
    current: CurrentUser,
    request_id: RequestId,
    id: Uuid,
    table: &'static str,
    event: &'static str,
    admin_only: bool,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut tx, current).await?;
    if admin_only {
        lock_and_revalidate_admin(&mut tx, current).await?;
    } else {
        let authorized: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1
                  FROM users u
                  JOIN workspace_members wm ON wm.workspace_id=u.workspace_id AND wm.user_id=u.id
                  JOIN sessions se ON se.workspace_id=u.workspace_id AND se.user_id=u.id
                 WHERE u.workspace_id=$1 AND u.id=$2 AND se.id=$3
                   AND u.status='active' AND u.deleted_at IS NULL
                   AND wm.status='active' AND se.revoked_at IS NULL AND se.expires_at>now()
                   AND (
                     wm.role='admin'
                     OR EXISTS (SELECT 1 FROM space_grants sg WHERE sg.workspace_id=$1 AND sg.space_id=$4 AND sg.user_id=$2 AND sg.role='owner')
                     OR EXISTS (
                       SELECT 1 FROM space_grants sg
                       JOIN groups g ON g.workspace_id=sg.workspace_id AND g.id=sg.group_id AND g.archived_at IS NULL
                       JOIN group_members gm ON gm.workspace_id=sg.workspace_id AND gm.group_id=sg.group_id AND gm.user_id=$2
                       WHERE sg.workspace_id=$1 AND sg.space_id=$4 AND sg.role='owner'
                     )
                   )
            )",
        )
        .bind(current.workspace_id).bind(current.user_id).bind(current.session_id).bind(id)
        .fetch_one(&mut *tx).await.map_err(internal)?;
        if !authorized {
            return Err(ApiProblem::not_found());
        }
    }
    let settings = read_settings(&mut tx, current.workspace_id).await?;
    let query = if table == "groups" {
        "UPDATE groups SET archived_at=NULL,updated_at=now() WHERE workspace_id=$1 AND id=$2 AND archived_at IS NOT NULL AND archived_at > clock_timestamp()-make_interval(days=>$3) AND NOT EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=$1 AND m.object_type='group' AND m.object_id=$2)"
    } else {
        "UPDATE spaces SET archived_at=NULL,updated_at=now() WHERE workspace_id=$1 AND id=$2 AND archived_at IS NOT NULL AND archived_at > clock_timestamp()-make_interval(days=>$3) AND NOT EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=$1 AND m.object_type='space' AND m.object_id=$2)"
    };
    let changed = sqlx::query(query)
        .bind(current.workspace_id)
        .bind(id)
        .bind(settings.attachment_recovery_days)
        .execute(&mut *tx)
        .await
        .map_err(internal)?
        .rows_affected();
    if changed != 1 {
        return Err(ApiProblem::not_found());
    }
    append_audit_event(
        &mut tx,
        &key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: event,
            target_type: if admin_only { "group" } else { "space" },
            target_id: Some(id),
            request_id: request_id.as_uuid(),
            metadata: json!({"operation":"restore"}),
        },
    )
    .await
    .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

#[derive(Serialize)]
struct Subject {
    id: Uuid,
    #[serde(rename = "type")]
    kind: &'static str,
    label: String,
    status: &'static str,
}

type SubjectEnvelopeRow = (String, Uuid, Vec<u8>, Vec<u8>, i32);

#[derive(Deserialize, Default)]
struct SubjectListQuery {
    after: Option<String>,
    limit: Option<usize>,
}

async fn subjects(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(space_id): Path<Uuid>,
    query: Result<Query<SubjectListQuery>, QueryRejection>,
) -> Result<Response, ApiProblem> {
    let Query(query) = query.map_err(|_| ApiProblem::validation("query", "is invalid"))?;
    let limit = query.limit.unwrap_or(100);
    if !(1..=100).contains(&limit) {
        return Err(ApiProblem::validation("limit", "must be between 1 and 100"));
    }
    let cursor = query
        .after
        .as_deref()
        .map(parse_subject_cursor)
        .transpose()?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    let current = super::users::lock_and_refresh_current(&mut tx, current).await?;
    let access = resolve_access(&mut tx, current.user_id, space_id).await?;
    authorize(&access, Capability::ManageSpace)?;
    let cursor_kind = cursor.as_ref().map(|value| value.0.as_str());
    let cursor_id = cursor.as_ref().map(|value| value.1);
    let rows: Vec<SubjectEnvelopeRow> = sqlx::query_as(
        "SELECT kind,id,ciphertext,nonce,key_version FROM (
            SELECT 'group'::text AS kind,id,name_ciphertext AS ciphertext,
                   name_nonce AS nonce,name_key_version AS key_version
              FROM groups WHERE workspace_id=$1 AND archived_at IS NULL
            UNION ALL
            SELECT 'user'::text AS kind,u.id,u.display_name_ciphertext,
                   u.display_name_nonce,u.display_name_key_version
              FROM users u JOIN workspace_members wm
                ON wm.workspace_id=u.workspace_id AND wm.user_id=u.id
             WHERE u.workspace_id=$1 AND u.status='active'
               AND u.deleted_at IS NULL AND wm.status='active'
        ) subjects
        WHERE $2::text IS NULL OR kind>$2 OR (kind=$2 AND id>$3)
        ORDER BY kind,id LIMIT $4",
    )
    .bind(current.workspace_id)
    .bind(cursor_kind)
    .bind(cursor_id)
    .bind(i64::try_from(limit + 1).map_err(internal)?)
    .fetch_all(&mut *tx)
    .await
    .map_err(internal)?;
    super::users::audit_disclosure(&mut tx, &key, current, request_id, "subject.admin_access")
        .await?;
    tx.commit().await.map_err(internal)?;
    let has_more = rows.len() > limit;
    let mut result = Vec::with_capacity(limit.min(rows.len()));
    for (kind, id, c, n, v) in rows.into_iter().take(limit) {
        let (kind, aad) = if kind == "group" {
            (
                "group",
                format!("workspace:{}:group:{id}:name", current.workspace_id),
            )
        } else {
            (
                "user",
                format!("workspace:{}:user:{id}:display-name", current.workspace_id),
            )
        };
        result.push(Subject {
            id,
            kind,
            label: decrypt_text(&key, &aad, c, n, v)?,
            status: "active",
        });
    }
    let next_cursor = has_more
        .then(|| {
            result
                .last()
                .map(|item| format!("{}:{}", item.kind, item.id))
        })
        .flatten();
    Ok(no_store(
        Json(json!({"items":result,"nextCursor":next_cursor})).into_response(),
    ))
}

fn parse_subject_cursor(value: &str) -> Result<(String, Uuid), ApiProblem> {
    let (kind, id) = value
        .split_once(':')
        .ok_or_else(|| ApiProblem::validation("after", "is invalid"))?;
    if !matches!(kind, "group" | "user") {
        return Err(ApiProblem::validation("after", "is invalid"));
    }
    let id = id
        .parse()
        .map_err(|_| ApiProblem::validation("after", "is invalid"))?;
    Ok((kind.to_owned(), id))
}

pub(crate) struct Readiness {
    pub database: bool,
    pub migrations: bool,
    pub master_key: bool,
    pub object_storage: bool,
}

impl Readiness {
    pub const fn ready(&self) -> bool {
        self.database && self.migrations && self.master_key && self.object_storage
    }
}

pub(crate) async fn readiness(state: &AppState) -> Readiness {
    let Some(runtime) = state.runtime() else {
        return Readiness {
            database: false,
            migrations: false,
            master_key: false,
            object_storage: false,
        };
    };
    let database = tokio::time::timeout(
        Duration::from_secs(2),
        sqlx::query_scalar::<_, i32>("SELECT 1").fetch_one(runtime.store.pool()),
    )
    .await
    .is_ok_and(|r| r.is_ok());
    let migrations = if database {
        tokio::time::timeout(Duration::from_secs(2), async {
            let applied: Vec<(i64, Vec<u8>, bool)> = sqlx::query_as(
                "SELECT version,checksum,success FROM _sqlx_migrations ORDER BY version",
            )
            .fetch_all(runtime.store.pool())
            .await?;
            let expected: Vec<(i64, &[u8])> = knowledge_store::MIGRATOR
                .iter()
                .map(|migration| (migration.version, migration.checksum.as_ref()))
                .collect();
            let exact = applied.len() == expected.len()
                && applied.iter().zip(expected).all(
                    |((actual_version, actual_checksum, success), (version, checksum))| {
                        *success
                            && *actual_version == version
                            && actual_checksum.as_slice() == checksum
                    },
                );
            let schema: bool = sqlx::query_scalar(
                "SELECT NOT EXISTS (
                        SELECT 1 FROM (VALUES
                            ('workspaces','wrapped_key_ciphertext'),
                            ('users','status'),
                            ('workspace_members','role'),
                            ('sessions','revoked_at'),
                            ('operational_settings','attachment_recovery_days'),
                            ('pages','archived_at'),
                            ('page_revisions','body_ciphertext'),
                            ('attachments','archived_at'),
                            ('jobs','status'),
                            ('exports','object_key'),
                            ('audit_events','event_hash')
                        ) AS required(table_name,column_name)
                        WHERE NOT EXISTS (
                            SELECT 1 FROM information_schema.columns actual
                             WHERE actual.table_schema='public'
                               AND actual.table_name=required.table_name
                               AND actual.column_name=required.column_name
                        )
                    )",
            )
            .fetch_one(runtime.store.pool())
            .await?;
            Ok::<bool, sqlx::Error>(exact && schema)
        })
        .await
        .is_ok_and(|result| result.unwrap_or(false))
    } else {
        false
    };
    let master_key = if database {
        tokio::time::timeout(Duration::from_secs(2), async {
            let records: Vec<Uuid> =
                sqlx::query_scalar("SELECT id FROM workspaces ORDER BY id LIMIT 1001")
                    .fetch_all(runtime.store.pool())
                    .await
                    .map_err(|_| ())?;
            if records.is_empty() || records.len() > 1000 {
                return Ok::<bool, ()>(false);
            }
            for workspace_id in records {
                let record = knowledge_store::users::UserRepository::workspace_key(
                    runtime.store.pool(),
                    workspace_id,
                )
                .await
                .map_err(|_| ())?
                .ok_or(())?;
                if unwrap_workspace_key(&runtime.master_key, &record).is_err() {
                    return Ok(false);
                }
            }
            Ok(true)
        })
        .await
        .is_ok_and(|result| result.unwrap_or(false))
    } else {
        false
    };
    let object_storage = match runtime.objects.as_ref() {
        Some(objects) => tokio::time::timeout(Duration::from_secs(2), objects.health())
            .await
            .is_ok_and(|r| r.is_ok()),
        None => false,
    };
    Readiness {
        database,
        migrations,
        master_key,
        object_storage,
    }
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}
fn internal<E>(_error: E) -> ApiProblem {
    ApiProblem::internal()
}
