use axum::{
    Json,
    extract::{Extension, Path, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use knowledge_core::{
    ids::{SpaceId, UserId},
    permissions::{Capability, EffectiveAccess, ResourceContext, SpaceRole},
};
use knowledge_crypto::{EncryptedEnvelope, EnvelopeCipher, KeyPurpose, KeyVersion, WorkspaceKey};
use knowledge_store::AccessResolver;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    CurrentUser,
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    middleware::RequestId,
    problem::ApiProblem,
    routes::users::{decrypt_text, load_workspace_key, require_admin},
    state::AppState,
};

api_router! {
    pub(crate) fn router;
    "/api/v1/spaces" => [
        post => create_space => ("createSpace", Mutation, 201),
        get => list_spaces => ("listSpaces", Session, 200),
    ],
    "/api/v1/spaces/{id}" => [
        get => get_space => ("getSpace", Session, 200),
        patch => update_space => ("updateSpace", Mutation, 200),
        delete => archive_space => ("archiveSpace", Mutation, 204),
    ],
    "/api/v1/spaces/{space_id}/grants" => [
        post => create_grant => ("createSpaceGrant", Mutation, 201),
        get => list_grants => ("listSpaceGrants", Session, 200),
    ],
    "/api/v1/spaces/{space_id}/grants/{grant_id}" => [
        patch => update_grant => ("updateSpaceGrant", Mutation, 200),
        delete => delete_grant => ("deleteSpaceGrant", Mutation, 204),
    ],
}

#[derive(Deserialize)]
struct CreateSpaceRequest {
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
struct UpdateSpaceRequest {
    name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_patch_field")]
    description: PatchField<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceResponse {
    id: Uuid,
    name: String,
    description: Option<String>,
    effective_role: String,
    archived_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SubjectType {
    User,
    Group,
}

impl SubjectType {
    const fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Group => "group",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SpaceRoleInput {
    Owner,
    Editor,
    Viewer,
}

impl SpaceRoleInput {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Editor => "editor",
            Self::Viewer => "viewer",
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateGrantRequest {
    subject_type: SubjectType,
    subject_id: Uuid,
    role: SpaceRoleInput,
}

#[derive(Deserialize)]
struct UpdateGrantRequest {
    role: SpaceRoleInput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct GrantResponse {
    id: Uuid,
    subject_type: &'static str,
    subject_id: Uuid,
    role: &'static str,
}

async fn create_space(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    payload: Result<Json<CreateSpaceRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    require_admin(current)?;
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid space details"))?;
    let name = normalize_name(&input.name)?;
    let description = normalize_description(input.description)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let space_id = Uuid::now_v7();
    let name_envelope = encrypt_text(
        &workspace_key,
        &format!("workspace:{}:space:{space_id}:name", current.workspace_id),
        &name,
    )?;
    let description_envelope = encrypt_optional(
        &workspace_key,
        &format!(
            "workspace:{}:space:{space_id}:description",
            current.workspace_id
        ),
        description.as_deref(),
    )?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_admin(&mut transaction, current).await?;
    sqlx::query(
        "INSERT INTO spaces (
            id, workspace_id, name_ciphertext, name_nonce, name_key_version,
            description_ciphertext, description_nonce, description_key_version, created_by
         ) VALUES ($1, $2, $3, $4, 1, $5, $6, $7, $8)",
    )
    .bind(space_id)
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
    .map_err(internal)?;
    sqlx::query(
        "INSERT INTO space_grants (id, workspace_id, space_id, user_id, role, granted_by)
         VALUES ($1, $2, $3, $4, 'owner', $4)",
    )
    .bind(Uuid::now_v7())
    .bind(current.workspace_id)
    .bind(space_id)
    .bind(current.user_id)
    .execute(&mut *transaction)
    .await
    .map_err(internal)?;
    append_space_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "space.created",
        space_id,
        json!({}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(SpaceResponse {
            id: space_id,
            name,
            description,
            effective_role: "admin".to_owned(),
            archived_at: None,
        }),
    )
        .into_response())
}

async fn list_spaces(
    State(state): State<AppState>,
    current: CurrentUser,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    let ids: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM spaces WHERE workspace_id = $1 AND archived_at IS NULL
         ORDER BY created_at, id",
    )
    .bind(current.workspace_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(internal)?;
    let mut spaces = Vec::new();
    for id in ids {
        let access = resolve_access(&mut transaction, current.user_id, id).await?;
        if access.can(Capability::ReadPage, ResourceContext::UnrestrictedPage) {
            spaces.push(
                load_space(
                    &mut transaction,
                    &workspace_key,
                    current.workspace_id,
                    id,
                    effective_role(current, &access),
                )
                .await?
                .ok_or_else(ApiProblem::internal)?,
            );
        }
    }
    transaction.commit().await.map_err(internal)?;
    let mut response = Json(spaces).into_response();
    prevent_caching(&mut response);
    Ok(response)
}

async fn get_space(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(space_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    let access = resolve_access(&mut transaction, current.user_id, space_id).await?;
    authorize(&access, Capability::ReadPage)?;
    let space = load_space(
        &mut transaction,
        &workspace_key,
        current.workspace_id,
        space_id,
        effective_role(current, &access),
    )
    .await?
    .ok_or_else(ApiProblem::not_found)?;
    transaction.commit().await.map_err(internal)?;
    let mut response = Json(space).into_response();
    prevent_caching(&mut response);
    Ok(response)
}

async fn update_space(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(space_id): Path<Uuid>,
    payload: Result<Json<UpdateSpaceRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid space changes"))?;
    if input.name.is_none() && matches!(input.description, PatchField::Missing) {
        return Err(ApiProblem::validation("body", "no changes were supplied"));
    }
    let requested_name = input.name.as_deref().map(normalize_name).transpose()?;
    let requested_description = match input.description {
        PatchField::Missing => None,
        PatchField::Present(value) => Some(normalize_description(value)?),
    };
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let access = resolve_access(&mut transaction, current.user_id, space_id).await?;
    authorize(&access, Capability::ManageSpace)?;
    let row = space_envelopes_for_update(&mut transaction, current.workspace_id, space_id)
        .await?
        .ok_or_else(ApiProblem::not_found)?;
    let mut name = decrypt_text(
        &workspace_key,
        &format!("workspace:{}:space:{space_id}:name", current.workspace_id),
        row.0,
        row.1,
        row.2,
    )?;
    let mut description = decrypt_optional(
        &workspace_key,
        &format!(
            "workspace:{}:space:{space_id}:description",
            current.workspace_id
        ),
        row.3,
        row.4,
        row.5,
    )?;
    if let Some(value) = requested_name {
        name = value;
    }
    if let Some(value) = requested_description {
        description = value;
    }
    let name_envelope = encrypt_text(
        &workspace_key,
        &format!("workspace:{}:space:{space_id}:name", current.workspace_id),
        &name,
    )?;
    let description_envelope = encrypt_optional(
        &workspace_key,
        &format!(
            "workspace:{}:space:{space_id}:description",
            current.workspace_id
        ),
        description.as_deref(),
    )?;
    sqlx::query(
        "UPDATE spaces SET name_ciphertext = $3, name_nonce = $4, name_key_version = 1,
            description_ciphertext = $5, description_nonce = $6, description_key_version = $7,
            updated_at = now() WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL",
    )
    .bind(current.workspace_id)
    .bind(space_id)
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
    .map_err(internal)?;
    append_space_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "space.updated",
        space_id,
        json!({}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(SpaceResponse {
        id: space_id,
        name,
        description,
        effective_role: effective_role(current, &access),
        archived_at: None,
    })
    .into_response())
}

async fn archive_space(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(space_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let access = resolve_access(&mut transaction, current.user_id, space_id).await?;
    authorize(&access, Capability::ManageSpace)?;
    let changed = sqlx::query(
        "UPDATE spaces SET archived_at = now(), updated_at = now()
          WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL",
    )
    .bind(current.workspace_id)
    .bind(space_id)
    .execute(&mut *transaction)
    .await
    .map_err(internal)?
    .rows_affected();
    if changed != 1 {
        return Err(ApiProblem::not_found());
    }
    append_space_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "space.archived",
        space_id,
        json!({}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn create_grant(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(space_id): Path<Uuid>,
    payload: Result<Json<CreateGrantRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid grant details"))?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let access = resolve_access(&mut transaction, current.user_id, space_id).await?;
    authorize(&access, Capability::ManageSpace)?;
    validate_subject(
        &mut transaction,
        current.workspace_id,
        input.subject_type,
        input.subject_id,
    )
    .await?;
    let grant_id = Uuid::now_v7();
    let (user_id, group_id) = match input.subject_type {
        SubjectType::User => (Some(input.subject_id), None),
        SubjectType::Group => (None, Some(input.subject_id)),
    };
    let inserted = sqlx::query(
        "INSERT INTO space_grants (
            id, workspace_id, space_id, user_id, group_id, role, granted_by
         ) VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(grant_id)
    .bind(current.workspace_id)
    .bind(space_id)
    .bind(user_id)
    .bind(group_id)
    .bind(input.role.as_str())
    .bind(current.user_id)
    .execute(&mut *transaction)
    .await;
    if let Err(error) = inserted {
        if error.as_database_error().is_some_and(|database| {
            matches!(
                database.constraint(),
                Some("space_grants_user_unique" | "space_grants_group_unique")
            )
        }) {
            return Err(ApiProblem::conflict(
                "A grant for this subject already exists",
            ));
        }
        return Err(ApiProblem::internal());
    }
    append_space_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "space.grant_created",
        space_id,
        json!({"grant_id": grant_id, "subject_type": input.subject_type.as_str()}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(GrantResponse {
            id: grant_id,
            subject_type: input.subject_type.as_str(),
            subject_id: input.subject_id,
            role: input.role.as_str(),
        }),
    )
        .into_response())
}

async fn list_grants(
    State(state): State<AppState>,
    current: CurrentUser,
    Path(space_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    let access = resolve_access(&mut transaction, current.user_id, space_id).await?;
    authorize(&access, Capability::ManageSpace)?;
    let rows: Vec<(Uuid, Option<Uuid>, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT id, user_id, group_id, role FROM space_grants
          WHERE workspace_id = $1 AND space_id = $2 ORDER BY created_at, id",
    )
    .bind(current.workspace_id)
    .bind(space_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(internal)?;
    transaction.commit().await.map_err(internal)?;
    let grants: Vec<GrantResponse> = rows
        .into_iter()
        .filter_map(|(id, user_id, group_id, role)| {
            let (subject_type, subject_id) = match (user_id, group_id) {
                (Some(id), None) => ("user", id),
                (None, Some(id)) => ("group", id),
                _ => return None,
            };
            Some(GrantResponse {
                id,
                subject_type,
                subject_id,
                role: role_label(&role)?,
            })
        })
        .collect();
    let mut response = Json(grants).into_response();
    prevent_caching(&mut response);
    Ok(response)
}

async fn update_grant(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path((space_id, grant_id)): Path<(Uuid, Uuid)>,
    payload: Result<Json<UpdateGrantRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected a valid grant role"))?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let access = resolve_access(&mut transaction, current.user_id, space_id).await?;
    authorize(&access, Capability::ManageSpace)?;
    let changed = sqlx::query(
        "UPDATE space_grants SET role = $4, updated_at = now()
          WHERE workspace_id = $1 AND space_id = $2 AND id = $3",
    )
    .bind(current.workspace_id)
    .bind(space_id)
    .bind(grant_id)
    .bind(input.role.as_str())
    .execute(&mut *transaction)
    .await
    .map_err(internal)?
    .rows_affected();
    if changed != 1 {
        return Err(ApiProblem::not_found());
    }
    append_space_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "space.grant_updated",
        space_id,
        json!({"grant_id": grant_id, "role": input.role.as_str()}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(json!({"id": grant_id, "role": input.role.as_str()})).into_response())
}

async fn delete_grant(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path((space_id, grant_id)): Path<(Uuid, Uuid)>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let access = resolve_access(&mut transaction, current.user_id, space_id).await?;
    authorize(&access, Capability::ManageSpace)?;
    let changed = sqlx::query(
        "DELETE FROM space_grants WHERE workspace_id = $1 AND space_id = $2 AND id = $3",
    )
    .bind(current.workspace_id)
    .bind(space_id)
    .bind(grant_id)
    .execute(&mut *transaction)
    .await
    .map_err(internal)?
    .rows_affected();
    if changed != 1 {
        return Err(ApiProblem::not_found());
    }
    append_space_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "space.grant_deleted",
        space_id,
        json!({"grant_id": grant_id}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

type SpaceEnvelopeRow = (
    Vec<u8>,
    Vec<u8>,
    i32,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<i32>,
);
type StoredSpaceRow = (
    Vec<u8>,
    Vec<u8>,
    i32,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    Option<i32>,
    Option<DateTime<Utc>>,
);

async fn space_envelopes_for_update(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    space_id: Uuid,
) -> Result<Option<SpaceEnvelopeRow>, ApiProblem> {
    sqlx::query_as(
        "SELECT name_ciphertext, name_nonce, name_key_version,
                description_ciphertext, description_nonce, description_key_version
           FROM spaces WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL FOR UPDATE",
    )
    .bind(workspace_id)
    .bind(space_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(internal)
}

async fn load_space(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_key: &WorkspaceKey,
    workspace_id: Uuid,
    space_id: Uuid,
    effective_role: String,
) -> Result<Option<SpaceResponse>, ApiProblem> {
    let row: Option<StoredSpaceRow> = sqlx::query_as(
        "SELECT name_ciphertext, name_nonce, name_key_version,
                description_ciphertext, description_nonce, description_key_version, archived_at
           FROM spaces WHERE workspace_id = $1 AND id = $2",
    )
    .bind(workspace_id)
    .bind(space_id)
    .fetch_optional(&mut **transaction)
    .await
    .map_err(internal)?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(SpaceResponse {
        id: space_id,
        name: decrypt_text(
            workspace_key,
            &format!("workspace:{workspace_id}:space:{space_id}:name"),
            row.0,
            row.1,
            row.2,
        )?,
        description: decrypt_optional(
            workspace_key,
            &format!("workspace:{workspace_id}:space:{space_id}:description"),
            row.3,
            row.4,
            row.5,
        )?,
        effective_role,
        archived_at: row.6,
    }))
}

pub(crate) async fn resolve_access(
    transaction: &mut Transaction<'_, Postgres>,
    user_id: Uuid,
    space_id: Uuid,
) -> Result<EffectiveAccess, ApiProblem> {
    AccessResolver::for_space(
        UserId::from_uuid(user_id),
        SpaceId::from_uuid(space_id),
        transaction,
    )
    .await
    .map_err(internal)
}

pub(crate) fn authorize(
    access: &EffectiveAccess,
    capability: Capability,
) -> Result<(), ApiProblem> {
    if access.can(capability, ResourceContext::UnrestrictedPage) {
        Ok(())
    } else if access.effective_space_role().is_none() {
        Err(ApiProblem::not_found())
    } else {
        Err(ApiProblem::forbidden())
    }
}

fn effective_role(current: CurrentUser, access: &EffectiveAccess) -> String {
    if current.workspace_admin {
        "admin".to_owned()
    } else {
        access
            .effective_space_role()
            .map(|role| match role {
                SpaceRole::Owner => "owner",
                SpaceRole::Editor => "editor",
                SpaceRole::Viewer => "viewer",
            })
            .unwrap_or("none")
            .to_owned()
    }
}

async fn validate_subject(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    subject_type: SubjectType,
    subject_id: Uuid,
) -> Result<(), ApiProblem> {
    let valid: bool = match subject_type {
        SubjectType::User => sqlx::query_scalar(
            "SELECT EXISTS (
                    SELECT 1 FROM users u
                    JOIN workspace_members wm
                      ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
                    WHERE u.workspace_id = $1 AND u.id = $2 AND u.status = 'active'
                      AND u.deleted_at IS NULL AND wm.status = 'active'
                )",
        )
        .bind(workspace_id)
        .bind(subject_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(internal)?,
        SubjectType::Group => sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM groups WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL
            )",
        )
        .bind(workspace_id)
        .bind(subject_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(internal)?,
    };
    if valid {
        Ok(())
    } else {
        Err(ApiProblem::not_found())
    }
}

async fn revalidate_admin(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
) -> Result<(), ApiProblem> {
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM users u
            JOIN workspace_members wm ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
            JOIN sessions s ON s.workspace_id = u.workspace_id AND s.user_id = u.id
            WHERE u.workspace_id = $1 AND u.id = $2 AND s.id = $3
              AND u.status = 'active' AND u.deleted_at IS NULL
              AND wm.status = 'active' AND wm.role = 'admin'
              AND s.revoked_at IS NULL AND s.expires_at > now()
        )",
    )
    .bind(current.workspace_id)
    .bind(current.user_id)
    .bind(current.session_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(internal)?;
    if valid {
        Ok(())
    } else {
        Err(ApiProblem::forbidden())
    }
}

pub(crate) async fn revalidate_session(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
) -> Result<(), ApiProblem> {
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM sessions
             WHERE workspace_id = $1 AND user_id = $2 AND id = $3
               AND revoked_at IS NULL AND expires_at > now()
        )",
    )
    .bind(current.workspace_id)
    .bind(current.user_id)
    .bind(current.session_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(internal)?;
    if valid {
        Ok(())
    } else {
        Err(ApiProblem::unauthorized())
    }
}

async fn append_space_audit(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_key: &WorkspaceKey,
    current: CurrentUser,
    request_id: RequestId,
    event_type: &'static str,
    space_id: Uuid,
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
            target_type: "space",
            target_id: Some(space_id),
            request_id: request_id.as_uuid(),
            metadata,
        },
    )
    .await
    .map_err(internal)
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
            .map_err(internal)?,
    )
    .encrypt(aad.as_bytes(), value.as_bytes())
    .map_err(internal)
}

fn encrypt_optional(
    workspace_key: &WorkspaceKey,
    aad: &str,
    value: Option<&str>,
) -> Result<Option<EncryptedEnvelope>, ApiProblem> {
    value
        .map(|value| encrypt_text(workspace_key, aad, value))
        .transpose()
}

fn decrypt_optional(
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

fn normalize_name(value: &str) -> Result<String, ApiProblem> {
    let value = value.trim();
    if value.is_empty() || value.chars().count() > 200 {
        Err(ApiProblem::validation(
            "name",
            "must contain between 1 and 200 characters",
        ))
    } else {
        Ok(value.to_owned())
    }
}

fn normalize_description(value: Option<String>) -> Result<Option<String>, ApiProblem> {
    value
        .map(|value| {
            let value = value.trim();
            if value.chars().count() > 10_000 {
                Err(ApiProblem::validation(
                    "description",
                    "must not exceed 10000 characters",
                ))
            } else if value.is_empty() {
                Ok(None)
            } else {
                Ok(Some(value.to_owned()))
            }
        })
        .transpose()
        .map(Option::flatten)
}

fn role_label(value: &str) -> Option<&'static str> {
    match value {
        "owner" => Some("owner"),
        "editor" => Some("editor"),
        "viewer" => Some("viewer"),
        _ => None,
    }
}

fn prevent_caching(response: &mut Response) {
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

fn internal<E>(_error: E) -> ApiProblem {
    ApiProblem::internal()
}
