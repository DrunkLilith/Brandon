use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use knowledge_crypto::{EncryptedEnvelope, EnvelopeCipher, KeyPurpose, KeyVersion, WorkspaceKey};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use uuid::Uuid;

use crate::{
    CurrentUser,
    audit::{AuditEvent, append_audit_event},
    middleware::RequestId,
    problem::ApiProblem,
    routes::users::{decrypt_text, load_workspace_key, lock_and_revalidate_admin, require_admin},
    state::AppState,
};

api_router! {
    pub(crate) fn router;
    "/api/v1/groups" => [
        post => create_group => ("createGroup", Mutation, 201),
        get => list_groups => ("listGroups", Session, 200),
    ],
    "/api/v1/groups/{id}" => [
        get => get_group => ("getGroup", Session, 200),
        patch => update_group => ("updateGroup", Mutation, 200),
        delete => archive_group => ("archiveGroup", Mutation, 204),
    ],
    "/api/v1/groups/{group_id}/members/{user_id}" => [
        put => add_member => ("addGroupMember", Mutation, 204),
        delete => remove_member => ("removeGroupMember", Mutation, 204),
    ],
}

#[derive(Deserialize)]
struct CreateGroupRequest {
    name: String,
    description: Option<String>,
}

#[derive(Default)]
enum PatchField<T> {
    #[default]
    Missing,
    Present(Option<T>),
}

fn deserialize_patch_field<'de, D, T>(deserializer: D) -> Result<PatchField<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(PatchField::Present)
}

#[derive(Deserialize)]
struct UpdateGroupRequest {
    name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    description: PatchField<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GroupResponse {
    id: Uuid,
    name: String,
    description: Option<String>,
    member_ids: Vec<Uuid>,
    archived_at: Option<DateTime<Utc>>,
}

async fn create_group(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    payload: Result<Json<CreateGroupRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid group details"))?;
    let name = normalize_name(&input.name)?;
    let description = normalize_description(input.description)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let group_id = Uuid::now_v7();
    let name_envelope = encrypt_text(
        &workspace_key,
        &format!("workspace:{}:group:{group_id}:name", current.workspace_id),
        &name,
    )?;
    let description_envelope = description
        .as_deref()
        .map(|description| {
            encrypt_text(
                &workspace_key,
                &format!(
                    "workspace:{}:group:{group_id}:description",
                    current.workspace_id
                ),
                description,
            )
        })
        .transpose()?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    sqlx::query(
        "INSERT INTO groups (
            id, workspace_id, name_ciphertext, name_nonce, name_key_version,
            description_ciphertext, description_nonce, description_key_version, created_by
         ) VALUES ($1, $2, $3, $4, 1, $5, $6, $7, $8)",
    )
    .bind(group_id)
    .bind(current.workspace_id)
    .bind(name_envelope.ciphertext)
    .bind(name_envelope.nonce.as_slice())
    .bind(
        description_envelope
            .as_ref()
            .map(|value| value.ciphertext.as_slice()),
    )
    .bind(
        description_envelope
            .as_ref()
            .map(|value| value.nonce.as_slice()),
    )
    .bind(description_envelope.as_ref().map(|_| 1_i32))
    .bind(current.user_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    append_group_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "group.created",
        group_id,
        json!({}),
    )
    .await?;
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    Ok((
        StatusCode::CREATED,
        Json(GroupResponse {
            id: group_id,
            name,
            description,
            member_ids: Vec::new(),
            archived_at: None,
        }),
    )
        .into_response())
}

async fn list_groups(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let ids: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM groups WHERE workspace_id = $1 ORDER BY created_at, id")
            .bind(current.workspace_id)
            .fetch_all(runtime.store.pool())
            .await
            .map_err(|_| ApiProblem::internal())?;
    let mut groups = Vec::with_capacity(ids.len());
    for id in ids {
        groups.push(
            load_group(
                runtime.store.pool(),
                &workspace_key,
                current.workspace_id,
                id,
            )
            .await?
            .ok_or_else(ApiProblem::internal)?,
        );
    }
    let mut response = Json(groups).into_response();
    prevent_caching(&mut response);
    Ok(response)
}

async fn get_group(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(group_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let group = load_group(
        runtime.store.pool(),
        &workspace_key,
        current.workspace_id,
        group_id,
    )
    .await?
    .ok_or_else(ApiProblem::not_found)?;
    let mut response = Json(group).into_response();
    prevent_caching(&mut response);
    Ok(response)
}

async fn update_group(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(group_id): Path<Uuid>,
    payload: Result<Json<UpdateGroupRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid group changes"))?;
    if input.name.is_none() && matches!(input.description, PatchField::Missing) {
        return Err(ApiProblem::validation("body", "no changes were supplied"));
    }
    let requested_name = input.name.as_deref().map(normalize_name).transpose()?;
    let requested_description = match input.description {
        PatchField::Missing => None,
        PatchField::Present(description) => Some(normalize_description(description)?),
    };
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    let row = group_envelopes_for_update(&mut transaction, current.workspace_id, group_id)
        .await?
        .ok_or_else(ApiProblem::not_found)?;
    let mut name = decrypt_text(
        &workspace_key,
        &format!("workspace:{}:group:{group_id}:name", current.workspace_id),
        row.0,
        row.1,
        row.2,
    )?;
    let mut description = decrypt_optional_text(
        &workspace_key,
        &format!(
            "workspace:{}:group:{group_id}:description",
            current.workspace_id
        ),
        row.3,
        row.4,
        row.5,
    )?;
    if let Some(new_name) = requested_name {
        name = new_name;
    }
    if let Some(new_description) = requested_description {
        description = new_description;
    }
    let name_envelope = encrypt_text(
        &workspace_key,
        &format!("workspace:{}:group:{group_id}:name", current.workspace_id),
        &name,
    )?;
    let description_envelope = description
        .as_deref()
        .map(|value| {
            encrypt_text(
                &workspace_key,
                &format!(
                    "workspace:{}:group:{group_id}:description",
                    current.workspace_id
                ),
                value,
            )
        })
        .transpose()?;
    sqlx::query(
        "UPDATE groups
            SET name_ciphertext = $3, name_nonce = $4, name_key_version = 1,
                description_ciphertext = $5, description_nonce = $6,
                description_key_version = $7, updated_at = now()
          WHERE workspace_id = $1 AND id = $2",
    )
    .bind(current.workspace_id)
    .bind(group_id)
    .bind(name_envelope.ciphertext)
    .bind(name_envelope.nonce.as_slice())
    .bind(
        description_envelope
            .as_ref()
            .map(|value| value.ciphertext.as_slice()),
    )
    .bind(
        description_envelope
            .as_ref()
            .map(|value| value.nonce.as_slice()),
    )
    .bind(description_envelope.as_ref().map(|_| 1_i32))
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    append_group_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "group.updated",
        group_id,
        json!({}),
    )
    .await?;
    let member_ids = group_member_ids(&mut *transaction, current.workspace_id, group_id).await?;
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    Ok(Json(GroupResponse {
        id: group_id,
        name,
        description,
        member_ids,
        archived_at: None,
    })
    .into_response())
}

async fn archive_group(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(group_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    let archived_at: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT archived_at FROM groups WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(current.workspace_id)
    .bind(group_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?
    .ok_or_else(ApiProblem::not_found)?;
    if archived_at.is_none() {
        sqlx::query("UPDATE groups SET archived_at = now(), updated_at = now() WHERE id = $1")
            .bind(group_id)
            .execute(&mut *transaction)
            .await
            .map_err(|_| ApiProblem::internal())?;
        append_group_audit(
            &mut transaction,
            &workspace_key,
            current,
            request_id,
            "group.archived",
            group_id,
            json!({}),
        )
        .await?;
    }
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn add_member(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path((group_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    ensure_active_group(&mut transaction, current.workspace_id, group_id).await?;
    let user_state: Option<(String, String)> = sqlx::query_as(
        "SELECT u.status, wm.status
           FROM users u
           JOIN workspace_members wm
             ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
          WHERE u.workspace_id = $1 AND u.id = $2 AND u.deleted_at IS NULL
          FOR UPDATE OF u, wm",
    )
    .bind(current.workspace_id)
    .bind(user_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    let Some((account_status, membership_status)) = user_state else {
        return Err(ApiProblem::not_found());
    };
    if account_status != "active" || membership_status != "active" {
        return Err(ApiProblem::conflict(
            "Only active workspace users can join a group",
        ));
    }
    let changed = sqlx::query(
        "INSERT INTO group_members (workspace_id, group_id, user_id, added_by)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (group_id, user_id) DO NOTHING",
    )
    .bind(current.workspace_id)
    .bind(group_id)
    .bind(user_id)
    .bind(current.user_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?
    .rows_affected()
        == 1;
    if changed {
        append_group_audit(
            &mut transaction,
            &workspace_key,
            current,
            request_id,
            "group.member_added",
            group_id,
            json!({"member_user_id": user_id}),
        )
        .await?;
    }
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn remove_member(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path((group_id, user_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| ApiProblem::internal())?;
    lock_and_revalidate_admin(&mut transaction, current).await?;
    ensure_active_group(&mut transaction, current.workspace_id, group_id).await?;
    let changed = sqlx::query(
        "DELETE FROM group_members
          WHERE workspace_id = $1 AND group_id = $2 AND user_id = $3",
    )
    .bind(current.workspace_id)
    .bind(group_id)
    .bind(user_id)
    .execute(&mut *transaction)
    .await
    .map_err(|_| ApiProblem::internal())?
    .rows_affected()
        == 1;
    if changed {
        append_group_audit(
            &mut transaction,
            &workspace_key,
            current,
            request_id,
            "group.member_removed",
            group_id,
            json!({"member_user_id": user_id}),
        )
        .await?;
    }
    transaction
        .commit()
        .await
        .map_err(|_| ApiProblem::internal())?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

type GroupEnvelopeRow = (
    Vec<u8>,
    Vec<u8>,
    i32,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<i32>,
);
type StoredGroupRow = (
    Vec<u8>,
    Vec<u8>,
    i32,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<i32>,
    Option<DateTime<Utc>>,
);

async fn group_envelopes_for_update(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    group_id: Uuid,
) -> Result<Option<GroupEnvelopeRow>, ApiProblem> {
    sqlx::query_as(
        "SELECT name_ciphertext, name_nonce, name_key_version,
                description_ciphertext, description_nonce, description_key_version
           FROM groups
          WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL
          FOR UPDATE",
    )
    .bind(workspace_id)
    .bind(group_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(|_| ApiProblem::internal())
}

async fn ensure_active_group(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_id: Uuid,
    group_id: Uuid,
) -> Result<(), ApiProblem> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM groups
             WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL
        )",
    )
    .bind(workspace_id)
    .bind(group_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(|_| ApiProblem::internal())?;
    if exists {
        Ok(())
    } else {
        Err(ApiProblem::not_found())
    }
}

async fn load_group(
    pool: &sqlx::PgPool,
    workspace_key: &WorkspaceKey,
    workspace_id: Uuid,
    group_id: Uuid,
) -> Result<Option<GroupResponse>, ApiProblem> {
    let row: Option<StoredGroupRow> = sqlx::query_as(
        "SELECT name_ciphertext, name_nonce, name_key_version,
                description_ciphertext, description_nonce, description_key_version, archived_at
           FROM groups WHERE workspace_id = $1 AND id = $2",
    )
    .bind(workspace_id)
    .bind(group_id)
    .fetch_optional(pool)
    .await
    .map_err(|_| ApiProblem::internal())?;
    let Some(row) = row else {
        return Ok(None);
    };
    let name = decrypt_text(
        workspace_key,
        &format!("workspace:{workspace_id}:group:{group_id}:name"),
        row.0,
        row.1,
        row.2,
    )?;
    let description = decrypt_optional_text(
        workspace_key,
        &format!("workspace:{workspace_id}:group:{group_id}:description"),
        row.3,
        row.4,
        row.5,
    )?;
    let member_ids = group_member_ids(pool, workspace_id, group_id).await?;
    Ok(Some(GroupResponse {
        id: group_id,
        name,
        description,
        member_ids,
        archived_at: row.6,
    }))
}

async fn group_member_ids<'e, E>(
    executor: E,
    workspace_id: Uuid,
    group_id: Uuid,
) -> Result<Vec<Uuid>, ApiProblem>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    sqlx::query_scalar(
        "SELECT user_id FROM group_members
          WHERE workspace_id = $1 AND group_id = $2 ORDER BY user_id",
    )
    .bind(workspace_id)
    .bind(group_id)
    .fetch_all(executor)
    .await
    .map_err(|_| ApiProblem::internal())
}

fn encrypt_text(
    workspace_key: &WorkspaceKey,
    aad: &str,
    value: &str,
) -> Result<EncryptedEnvelope, ApiProblem> {
    EnvelopeCipher::new(
        KeyVersion(1),
        workspace_key
            .derive(KeyPurpose::Content)
            .map_err(|_| ApiProblem::internal())?,
    )
    .encrypt(aad.as_bytes(), value.as_bytes())
    .map_err(|_| ApiProblem::internal())
}

fn decrypt_optional_text(
    workspace_key: &WorkspaceKey,
    aad: &str,
    ciphertext: Option<Vec<u8>>,
    nonce: Option<Vec<u8>>,
    key_version: Option<i32>,
) -> Result<Option<String>, ApiProblem> {
    match (ciphertext, nonce, key_version) {
        (Some(ciphertext), Some(nonce), Some(key_version)) => {
            decrypt_text(workspace_key, aad, ciphertext, nonce, key_version).map(Some)
        }
        (None, None, None) => Ok(None),
        _ => Err(ApiProblem::internal()),
    }
}

async fn append_group_audit(
    transaction: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    workspace_key: &WorkspaceKey,
    current: CurrentUser,
    request_id: RequestId,
    event_type: &'static str,
    group_id: Uuid,
    metadata: serde_json::Value,
) -> Result<(), ApiProblem> {
    append_audit_event(
        transaction,
        workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type,
            target_type: "group",
            target_id: Some(group_id),
            request_id: request_id.as_uuid(),
            metadata,
        },
    )
    .await
    .map_err(|_| ApiProblem::internal())
}

fn normalize_name(name: &str) -> Result<String, ApiProblem> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 200 {
        Err(ApiProblem::validation(
            "name",
            "must contain between 1 and 200 characters",
        ))
    } else {
        Ok(name.to_owned())
    }
}

fn normalize_description(description: Option<String>) -> Result<Option<String>, ApiProblem> {
    description
        .map(|description| {
            let description = description.trim();
            if description.chars().count() > 10_000 {
                Err(ApiProblem::validation(
                    "description",
                    "must not exceed 10000 characters",
                ))
            } else if description.is_empty() {
                Ok(None)
            } else {
                Ok(Some(description.to_owned()))
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn prevent_caching(response: &mut Response) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}
