use axum::{
    Json,
    extract::{Extension, Path, Query, State, rejection::JsonRejection},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
};
use knowledge_core::{
    pages::SavePage,
    permissions::{Capability, EffectiveAccess, ResourceContext, SpaceRole},
};
use knowledge_crypto::{BlindIndexer, EncryptedEnvelope, EnvelopeCipher, KeyPurpose, WorkspaceKey};
use knowledge_store::{pages::PageRepository, search::BlindSearchRepository};
use pulldown_cmark::{Event, Parser, Tag};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    CurrentUser,
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    middleware::RequestId,
    problem::ApiProblem,
    routes::{
        spaces::{authorize, resolve_access, revalidate_session},
        users::{decrypt_text, load_workspace_key},
    },
    state::AppState,
};

api_router! {
    pub(crate) fn router;
    "/api/v1/pages" => [
        post => create_page => ("createPage", Mutation, 201),
        get => list_pages => ("listPages", Session, 200),
    ],
    "/api/v1/pages/{id}" => [
        get => get_page => ("getPage", Session, 200),
        patch => save_page => ("updatePage", Mutation, 200),
        delete => archive_page => ("archivePage", Mutation, 204),
    ],
    "/api/v1/pages/{id}/move" => [post => move_page => ("movePage", Mutation, 204)],
    "/api/v1/pages/{id}/unarchive" => [post => unarchive_page => ("unarchivePage", Mutation, 204)],
    "/api/v1/pages/{id}/revisions" => [get => list_revisions => ("listPageRevisions", Session, 200)],
    "/api/v1/pages/{id}/revisions/{version}" => [get => get_revision => ("getPageRevision", Session, 200)],
    "/api/v1/pages/{id}/restore" => [post => restore_revision => ("restorePageRevision", Mutation, 200)],
    "/api/v1/pages/{id}/backlinks" => [get => backlinks => ("listBacklinks", Session, 200)],
    "/api/v1/pages/{id}/permissions" => [get => list_page_permissions => ("listPagePermissions", Session, 200)],
    "/api/v1/pages/{id}/permissions/{subject}" => [
        put => upsert_page_permission => ("upsertPagePermission", Mutation, 200),
        delete => delete_page_permission => ("deletePagePermission", Mutation, 204),
    ],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SpaceQuery {
    space_id: Uuid,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct MovePageRequest {
    space_id: Uuid,
    parent_id: Option<Uuid>,
    #[serde(default)]
    before_id: Option<Uuid>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RestoreRevisionRequest {
    version: i64,
    expected_version: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreatePageRequest {
    space_id: Uuid,
    parent_id: Option<Uuid>,
    #[serde(default)]
    before_id: Option<Uuid>,
    title: String,
    markdown: String,
    properties: Value,
    #[serde(default)]
    restricted: bool,
}

#[derive(Clone, Copy)]
enum PermissionSubject {
    User(Uuid),
    Group(Uuid),
}

impl PermissionSubject {
    fn parse(value: &str) -> Result<Self, ApiProblem> {
        let (kind, id) = value
            .split_once(':')
            .ok_or_else(|| ApiProblem::validation("subject", "expected user:<id> or group:<id>"))?;
        let id = id
            .parse()
            .map_err(|_| ApiProblem::validation("subject", "contains an invalid id"))?;
        match kind {
            "user" => Ok(Self::User(id)),
            "group" => Ok(Self::Group(id)),
            _ => Err(ApiProblem::validation(
                "subject",
                "expected user:<id> or group:<id>",
            )),
        }
    }

    const fn kind(self) -> &'static str {
        match self {
            Self::User(_) => "user",
            Self::Group(_) => "group",
        }
    }

    const fn id(self) -> Uuid {
        match self {
            Self::User(id) | Self::Group(id) => id,
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum PageRoleInput {
    Viewer,
    Editor,
}

impl PageRoleInput {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Editor => "editor",
        }
    }
}

#[derive(Deserialize)]
struct UpsertPagePermissionRequest {
    role: PageRoleInput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageResponse {
    id: Uuid,
    space_id: Uuid,
    parent_id: Option<Uuid>,
    stable_key: Uuid,
    title: String,
    markdown: String,
    properties: Value,
    version: i64,
    restricted: bool,
    capabilities: PageCapabilities,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PageCapabilities {
    edit: bool,
    manage_permissions: bool,
}

impl PageCapabilities {
    fn from_context(context: &knowledge_core::permissions::PageAccessContext) -> Self {
        Self {
            edit: context.can(Capability::EditPage),
            manage_permissions: context.access().is_workspace_admin()
                || context.access().effective_space_role() == Some(SpaceRole::Owner),
        }
    }
}

type RevisionPayloadRow = (
    Uuid,
    Vec<u8>,
    Vec<u8>,
    i32,
    Vec<u8>,
    Vec<u8>,
    i32,
    Vec<u8>,
    Vec<u8>,
    i32,
    Uuid,
    chrono::DateTime<chrono::Utc>,
);

async fn create_page(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    payload: Result<Json<CreatePageRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid page details"))?;
    validate_page(&input.title, &input.markdown, &input.properties)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let access = resolve_access(&mut transaction, current.user_id, input.space_id).await?;
    authorize(&access, Capability::CreatePage)?;
    if let Some(parent_id) = input.parent_id
        && !PageRepository::active_parent_exists(
            &mut transaction,
            current.workspace_id,
            input.space_id,
            parent_id,
        )
        .await
        .map_err(internal)?
    {
        return Err(ApiProblem::not_found());
    }
    if input.restricted && !access.is_workspace_admin() {
        return Err(ApiProblem::forbidden());
    }
    if let Some(parent_id) = input.parent_id {
        authorize_page(&mut transaction, current, parent_id, Capability::CreatePage).await?;
    }
    if let Some(before_id) = input.before_id {
        authorize_page(&mut transaction, current, before_id, Capability::CreatePage).await?;
    }

    let page_id = Uuid::now_v7();
    let stable_key = Uuid::now_v7();
    let properties = serde_json::to_vec(&input.properties).map_err(internal)?;
    let current_envelopes = encrypt_page_fields(
        &workspace_key,
        current.workspace_id,
        page_id,
        None,
        &input.title,
        &input.markdown,
        &properties,
    )?;
    let sort_key = next_sort_key(
        &mut transaction,
        current,
        &access,
        input.space_id,
        input.parent_id,
        input.before_id,
        None,
    )
    .await?;
    sqlx::query(
        "INSERT INTO pages (
            id, workspace_id, space_id, parent_id, stable_key, sort_key,
            title_ciphertext, title_nonce, title_key_version,
            body_ciphertext, body_nonce, body_key_version,
            properties_ciphertext, properties_nonce, properties_key_version,
            version, restricted, created_by, updated_by
         ) VALUES (
            $1, $2, $3, $4, $5, $6,
            $7, $8, workspace_current_key_version($2), $9, $10, workspace_current_key_version($2), $11, $12, workspace_current_key_version($2), 1, $13, $14, $14
         )",
    )
    .bind(page_id)
    .bind(current.workspace_id)
    .bind(input.space_id)
    .bind(input.parent_id)
    .bind(stable_key)
    .bind(sort_key)
    .bind(current_envelopes.title.ciphertext)
    .bind(current_envelopes.title.nonce.as_slice())
    .bind(current_envelopes.body.ciphertext)
    .bind(current_envelopes.body.nonce.as_slice())
    .bind(current_envelopes.properties.ciphertext)
    .bind(current_envelopes.properties.nonce.as_slice())
    .bind(input.restricted)
    .bind(current.user_id)
    .execute(&mut *transaction)
    .await
    .map_err(internal)?;
    insert_revision(
        &mut transaction,
        &workspace_key,
        RevisionIdentity {
            workspace_id: current.workspace_id,
            page_id,
            version: 1,
            author_id: current.user_id,
        },
        PagePlaintext {
            title: &input.title,
            markdown: &input.markdown,
            properties: &properties,
        },
    )
    .await?;
    reindex_links(
        &mut transaction,
        current.workspace_id,
        page_id,
        &input.markdown,
    )
    .await?;
    reindex_search(
        &mut transaction,
        &workspace_key,
        current.workspace_id,
        page_id,
        &input.title,
        &input.markdown,
    )
    .await?;
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "page.created",
            target_type: "page",
            target_id: Some(page_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"space_id": input.space_id}),
        },
    )
    .await
    .map_err(internal)?;
    let capabilities = PageCapabilities::from_context(
        &authorize_page(&mut transaction, current, page_id, Capability::ReadPage).await?,
    );
    transaction.commit().await.map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        Json(PageResponse {
            id: page_id,
            space_id: input.space_id,
            parent_id: input.parent_id,
            stable_key,
            title: input.title,
            markdown: input.markdown,
            properties: input.properties,
            version: 1,
            restricted: input.restricted,
            capabilities,
        }),
    )
        .into_response())
}

async fn get_page(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let row: Option<StoredPageRow> = sqlx::query_as(
        "SELECT space_id, parent_id, stable_key,
                title_ciphertext, title_nonce, title_key_version,
                body_ciphertext, body_nonce, body_key_version,
                properties_ciphertext, properties_nonce, properties_key_version,
                version, restricted
           FROM pages
          WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal)?;
    let Some(row) = row else {
        return Err(ApiProblem::not_found());
    };
    let context = authorize_page(&mut transaction, current, page_id, Capability::ReadPage).await?;
    let title = decrypt_text(
        &workspace_key,
        &page_aad(current.workspace_id, page_id, None, "title"),
        row.3,
        row.4,
        row.5,
    )?;
    let markdown = decrypt_text(
        &workspace_key,
        &page_aad(current.workspace_id, page_id, None, "body"),
        row.6,
        row.7,
        row.8,
    )?;
    let properties = decrypt_text(
        &workspace_key,
        &page_aad(current.workspace_id, page_id, None, "properties"),
        row.9,
        row.10,
        row.11,
    )?;
    let properties = serde_json::from_str(&properties).map_err(internal)?;
    let parent_id = visible_parent_id(&mut transaction, current, row.1).await?;
    if context.access().is_workspace_admin() {
        append_page_audit(
            &mut transaction,
            &workspace_key,
            current,
            request_id,
            "page.admin_access",
            page_id,
            json!({"operation":"read"}),
        )
        .await?;
    }
    transaction.commit().await.map_err(internal)?;
    let mut response = Json(PageResponse {
        id: page_id,
        space_id: row.0,
        parent_id,
        stable_key: row.2,
        title,
        markdown,
        properties,
        version: row.12,
        restricted: row.13,
        capabilities: PageCapabilities::from_context(&context),
    })
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn save_page(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
    payload: Result<Json<SavePage>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected valid page content"))?;
    validate_page(&input.title, &input.markdown, &input.properties)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let row: Option<(Uuid, i64)> = sqlx::query_as(
        "SELECT space_id, version FROM pages
          WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL FOR UPDATE",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal)?;
    let Some((_space_id, current_version)) = row else {
        return Err(ApiProblem::not_found());
    };
    authorize_page(&mut transaction, current, page_id, Capability::EditPage).await?;
    if input.expected_version != current_version {
        let mut response = (
            StatusCode::CONFLICT,
            Json(json!({
                "type": "https://knowledge.local/problems/version-conflict",
                "title": "Version conflict",
                "status": 409,
                "code": "version_conflict",
                "currentVersion": current_version
            })),
        )
            .into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        return Ok(response);
    }
    let new_version = current_version + 1;
    let properties = serde_json::to_vec(&input.properties).map_err(internal)?;
    let envelopes = encrypt_page_fields(
        &workspace_key,
        current.workspace_id,
        page_id,
        None,
        &input.title,
        &input.markdown,
        &properties,
    )?;
    sqlx::query(
        "UPDATE pages SET title_ciphertext = $3, title_nonce = $4, title_key_version = workspace_current_key_version($1),
            body_ciphertext = $5, body_nonce = $6, body_key_version = workspace_current_key_version($1),
            properties_ciphertext = $7, properties_nonce = $8, properties_key_version = workspace_current_key_version($1),
            version = $9, updated_by = $10, updated_at = now()
          WHERE workspace_id = $1 AND id = $2",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .bind(envelopes.title.ciphertext)
    .bind(envelopes.title.nonce.as_slice())
    .bind(envelopes.body.ciphertext)
    .bind(envelopes.body.nonce.as_slice())
    .bind(envelopes.properties.ciphertext)
    .bind(envelopes.properties.nonce.as_slice())
    .bind(new_version)
    .bind(current.user_id)
    .execute(&mut *transaction)
    .await
    .map_err(internal)?;
    insert_revision(
        &mut transaction,
        &workspace_key,
        RevisionIdentity {
            workspace_id: current.workspace_id,
            page_id,
            version: new_version,
            author_id: current.user_id,
        },
        PagePlaintext {
            title: &input.title,
            markdown: &input.markdown,
            properties: &properties,
        },
    )
    .await?;
    reindex_links(
        &mut transaction,
        current.workspace_id,
        page_id,
        &input.markdown,
    )
    .await?;
    reindex_search(
        &mut transaction,
        &workspace_key,
        current.workspace_id,
        page_id,
        &input.title,
        &input.markdown,
    )
    .await?;
    append_audit_event(
        &mut transaction,
        &workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "page.updated",
            target_type: "page",
            target_id: Some(page_id),
            request_id: request_id.as_uuid(),
            metadata: json!({"version": new_version}),
        },
    )
    .await
    .map_err(internal)?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(json!({"id": page_id, "version": new_version})).into_response())
}

async fn list_pages(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Query(query): Query<SpaceQuery>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let access = resolve_access(&mut transaction, current.user_id, query.space_id).await?;
    authorize(&access, Capability::ReadPage)?;
    let rows: Vec<TreeRow> = sqlx::query_as(
        "SELECT id, parent_id, sort_key, version, restricted, title_ciphertext, title_nonce, title_key_version FROM pages
          WHERE workspace_id = $1 AND space_id = $2 AND archived_at IS NULL
          ORDER BY parent_id NULLS FIRST, sort_key, id",
    )
    .bind(current.workspace_id)
    .bind(query.space_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(internal)?;
    let mut visible = Vec::new();
    for (id, parent_id, sort_key, version, restricted, ciphertext, nonce, key_version) in rows {
        let context = PageRepository::access_context(
            &mut transaction,
            current.workspace_id,
            current.user_id,
            id,
        )
        .await
        .map_err(internal)?;
        if let Some(context) = context.filter(|context| context.can(Capability::ReadPage)) {
            let parent_id = visible_parent_id(&mut transaction, current, parent_id).await?;
            let title = decrypt_text(
                &workspace_key,
                &page_aad(current.workspace_id, id, None, "title"),
                ciphertext,
                nonce,
                key_version,
            )?;
            visible.push(json!({"id": id, "parentId": parent_id, "sortKey": sort_key, "version": version, "restricted": restricted, "title":title, "capabilities":PageCapabilities::from_context(&context)}));
        }
    }
    if access.is_workspace_admin() {
        append_audit_event(
            &mut transaction,
            &workspace_key,
            AuditEvent {
                workspace_id: current.workspace_id,
                actor_user_id: Some(current.user_id),
                session_id: Some(current.session_id),
                event_type: "page.admin_access",
                target_type: "space",
                target_id: Some(query.space_id),
                request_id: request_id.as_uuid(),
                metadata: json!({"operation":"list"}),
            },
        )
        .await
        .map_err(internal)?;
    }
    transaction.commit().await.map_err(internal)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(visible)).into_response())
}

type TreeRow = (Uuid, Option<Uuid>, String, i64, bool, Vec<u8>, Vec<u8>, i32);

async fn move_page(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
    payload: Result<Json<MovePageRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected a move target"))?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let source: Option<(Uuid, bool)> = sqlx::query_as(
        "SELECT space_id, restricted FROM pages WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL FOR UPDATE"
    ).bind(current.workspace_id).bind(page_id).fetch_optional(&mut *transaction).await.map_err(internal)?;
    let (source_space, source_restricted) = source.ok_or_else(ApiProblem::not_found)?;
    let source_context =
        authorize_page(&mut transaction, current, page_id, Capability::EditPage).await?;
    authorize_subtree(&mut transaction, current, page_id, Capability::EditPage).await?;
    let source_access = resolve_access(&mut transaction, current.user_id, source_space).await?;
    let destination_access =
        resolve_access(&mut transaction, current.user_id, input.space_id).await?;
    authorize(&destination_access, Capability::EditPage)?;
    let mut destination_boundary = None;
    if let Some(parent_id) = input.parent_id {
        if !PageRepository::active_parent_exists(
            &mut transaction,
            current.workspace_id,
            input.space_id,
            parent_id,
        )
        .await
        .map_err(internal)?
        {
            return Err(ApiProblem::not_found());
        }
        let creates_cycle: bool = sqlx::query_scalar(
            "WITH RECURSIVE descendants(id) AS (
                SELECT id FROM pages WHERE workspace_id = $1 AND id = $2
                UNION ALL SELECT p.id FROM pages p JOIN descendants d ON p.parent_id = d.id
                 WHERE p.workspace_id = $1
             ) SELECT EXISTS (SELECT 1 FROM descendants WHERE id = $3)",
        )
        .bind(current.workspace_id)
        .bind(page_id)
        .bind(parent_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(internal)?;
        if creates_cycle {
            return Err(ApiProblem::conflict("A page cannot be moved below itself"));
        }
        destination_boundary =
            authorize_page(&mut transaction, current, parent_id, Capability::EditPage)
                .await?
                .boundary_page_id();
    }
    if let Some(before_id) = input.before_id {
        authorize_page(&mut transaction, current, before_id, Capability::EditPage).await?;
    }
    let resulting_boundary = if source_restricted {
        Some(page_id)
    } else {
        destination_boundary
    };
    let restricted_audience_changes = source_space != input.space_id
        && subtree_contains_restricted_boundary(&mut transaction, current.workspace_id, page_id)
            .await?;
    if (source_context.boundary_page_id() != resulting_boundary || restricted_audience_changes)
        && !can_change_restricted_boundary(&source_access, &destination_access)
    {
        return Err(ApiProblem::forbidden());
    }
    let sort_key = next_sort_key(
        &mut transaction,
        current,
        &destination_access,
        input.space_id,
        input.parent_id,
        input.before_id,
        Some(page_id),
    )
    .await?;
    sqlx::query(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM pages WHERE workspace_id = $1 AND id = $2
            UNION ALL SELECT p.id FROM pages p JOIN subtree s ON p.parent_id = s.id
             WHERE p.workspace_id = $1
         ) UPDATE pages SET space_id = $3,
             parent_id = CASE WHEN id = $2 THEN $4 ELSE parent_id END,
             sort_key = CASE WHEN id = $2 THEN $5 ELSE sort_key END,
             updated_by = $6, updated_at = now()
           WHERE id IN (SELECT id FROM subtree)",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .bind(input.space_id)
    .bind(input.parent_id)
    .bind(sort_key)
    .bind(current.user_id)
    .execute(&mut *transaction)
    .await
    .map_err(internal)?;
    append_page_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "page.moved",
        page_id,
        json!({
            "space_id": input.space_id,
            "source_boundary_id": source_context.boundary_page_id(),
            "destination_boundary_id": resulting_boundary
        }),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn archive_page(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    set_subtree_archive(state, request_id, current, page_id, true).await
}

async fn unarchive_page(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    set_subtree_archive(state, request_id, current, page_id, false).await
}

async fn set_subtree_archive(
    state: AppState,
    request_id: RequestId,
    current: CurrentUser,
    page_id: Uuid,
    archive: bool,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let space_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT space_id FROM pages WHERE workspace_id = $1 AND id = $2 FOR UPDATE",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal)?;
    let _space_id = space_id.ok_or_else(ApiProblem::not_found)?;
    authorize_subtree(
        &mut transaction,
        current,
        page_id,
        if archive {
            Capability::EditPage
        } else {
            Capability::ManageSpace
        },
    )
    .await?;
    if !archive {
        let expired: bool = sqlx::query_scalar(
            "WITH RECURSIVE subtree AS (SELECT id,archived_at FROM pages WHERE workspace_id=$1 AND id=$2 UNION ALL SELECT p.id,p.archived_at FROM pages p JOIN subtree s ON p.parent_id=s.id WHERE p.workspace_id=$1) SELECT EXISTS(SELECT 1 FROM subtree WHERE retention_expired($1,archived_at) OR EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=$1 AND m.object_type='page' AND m.object_id=subtree.id))",
        ).bind(current.workspace_id).bind(page_id).fetch_one(&mut *transaction).await.map_err(internal)?;
        if expired {
            return Err(ApiProblem::not_found());
        }
        sqlx::query(
            "UPDATE pages child SET parent_id = NULL
              WHERE child.workspace_id = $1 AND child.id = $2
                AND child.parent_id IS NOT NULL
                AND EXISTS (SELECT 1 FROM pages parent
                             WHERE parent.id = child.parent_id AND parent.archived_at IS NOT NULL)",
        )
        .bind(current.workspace_id)
        .bind(page_id)
        .execute(&mut *transaction)
        .await
        .map_err(internal)?;
    }
    let subtree_ids: Vec<Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM pages WHERE workspace_id = $1 AND id = $2
            UNION ALL
            SELECT p.id FROM pages p JOIN subtree s ON p.parent_id = s.id
             WHERE p.workspace_id = $1
         ) SELECT id FROM subtree",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_all(&mut *transaction)
    .await
    .map_err(internal)?;
    sqlx::query(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM pages WHERE workspace_id = $1 AND id = $2
            UNION ALL SELECT p.id FROM pages p JOIN subtree s ON p.parent_id = s.id WHERE p.workspace_id = $1
         ) UPDATE pages SET archived_at = CASE WHEN $3 THEN now() ELSE NULL END,
             archived_by = CASE WHEN $3 THEN $4 ELSE NULL END, updated_at = now()
           WHERE id IN (SELECT id FROM subtree)"
    ).bind(current.workspace_id).bind(page_id).bind(archive).bind(current.user_id)
    .execute(&mut *transaction).await.map_err(internal)?;
    if archive {
        BlindSearchRepository::remove_page_tokens(
            &mut transaction,
            current.workspace_id,
            &subtree_ids,
        )
        .await
        .map_err(internal)?;
    } else {
        let rows: Vec<SearchIndexPageRow> = sqlx::query_as(
            "SELECT id, title_ciphertext, title_nonce, title_key_version,
                    body_ciphertext, body_nonce, body_key_version
               FROM pages WHERE workspace_id = $1 AND id = ANY($2) AND archived_at IS NULL",
        )
        .bind(current.workspace_id)
        .bind(&subtree_ids)
        .fetch_all(&mut *transaction)
        .await
        .map_err(internal)?;
        for (
            indexed_page_id,
            title_ciphertext,
            title_nonce,
            title_key_version,
            body_ciphertext,
            body_nonce,
            body_key_version,
        ) in rows
        {
            let title = decrypt_text(
                &workspace_key,
                &page_aad(current.workspace_id, indexed_page_id, None, "title"),
                title_ciphertext,
                title_nonce,
                title_key_version,
            )?;
            let markdown = decrypt_text(
                &workspace_key,
                &page_aad(current.workspace_id, indexed_page_id, None, "body"),
                body_ciphertext,
                body_nonce,
                body_key_version,
            )?;
            reindex_search(
                &mut transaction,
                &workspace_key,
                current.workspace_id,
                indexed_page_id,
                &title,
                &markdown,
            )
            .await?;
        }
    }
    append_page_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        if archive {
            "page.archived"
        } else {
            "page.unarchived"
        },
        page_id,
        json!({}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn list_revisions(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    let current = super::users::lock_and_refresh_current(&mut transaction, current).await?;
    let space_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT space_id FROM pages WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal)?;
    let _space_id = space_id.ok_or_else(ApiProblem::not_found)?;
    authorize_page(&mut transaction, current, page_id, Capability::ViewHistory).await?;
    let rows: Vec<(i64, Uuid, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        "SELECT version, author_id, created_at FROM page_revisions WHERE workspace_id = $1 AND page_id = $2 ORDER BY version"
    ).bind(current.workspace_id).bind(page_id).fetch_all(&mut *transaction).await.map_err(internal)?;
    super::users::audit_disclosure(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "revision.admin_access",
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(rows.into_iter().map(|(version, author_id, created_at)| json!({"version": version, "authorId": author_id, "createdAt": created_at})).collect::<Vec<_>>()).into_response())
}

async fn get_revision(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path((page_id, version)): Path<(Uuid, i64)>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let current = super::users::refresh_workspace_admin(&mut transaction, current).await?;
    let active: bool = sqlx::query_scalar(
        "SELECT EXISTS (
            SELECT 1 FROM pages
             WHERE workspace_id=$1 AND id=$2 AND archived_at IS NULL
        )",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_one(&mut *transaction)
    .await
    .map_err(internal)?;
    if !active {
        return Err(ApiProblem::not_found());
    }
    authorize_page(&mut transaction, current, page_id, Capability::ViewHistory).await?;
    let row: Option<RevisionPayloadRow> = sqlx::query_as(
        "SELECT id,title_ciphertext,title_nonce,title_key_version,
                body_ciphertext,body_nonce,body_key_version,
                properties_ciphertext,properties_nonce,properties_key_version,
                author_id,created_at
           FROM page_revisions
          WHERE workspace_id=$1 AND page_id=$2 AND version=$3",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .bind(version)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal)?;
    let row = row.ok_or_else(ApiProblem::not_found)?;
    super::users::audit_disclosure(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "revision.admin_access",
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    let title = decrypt_text(
        &workspace_key,
        &page_aad(current.workspace_id, page_id, Some(version), "title"),
        row.1,
        row.2,
        row.3,
    )?;
    let markdown = decrypt_text(
        &workspace_key,
        &page_aad(current.workspace_id, page_id, Some(version), "body"),
        row.4,
        row.5,
        row.6,
    )?;
    let properties: Value = serde_json::from_str(&decrypt_text(
        &workspace_key,
        &page_aad(current.workspace_id, page_id, Some(version), "properties"),
        row.7,
        row.8,
        row.9,
    )?)
    .map_err(internal)?;
    let mut response = Json(json!({
        "id":row.0,"pageId":page_id,"version":version,"title":title,
        "markdown":markdown,"properties":properties,"authorId":row.10,"createdAt":row.11
    }))
    .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

async fn restore_revision(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
    payload: Result<Json<RestoreRevisionRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected restore details"))?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    let current_row: Option<(Uuid, i64)> = sqlx::query_as("SELECT space_id, version FROM pages WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL FOR UPDATE")
        .bind(current.workspace_id).bind(page_id).fetch_optional(&mut *transaction).await.map_err(internal)?;
    let (_space_id, current_version) = current_row.ok_or_else(ApiProblem::not_found)?;
    authorize_page(&mut transaction, current, page_id, Capability::EditPage).await?;
    if current_version != input.expected_version {
        return Ok(version_conflict(current_version));
    }
    let revision: Option<RevisionRow> = sqlx::query_as(
        "SELECT title_ciphertext, title_nonce, title_key_version, body_ciphertext, body_nonce, body_key_version,
                properties_ciphertext, properties_nonce, properties_key_version
           FROM page_revisions WHERE workspace_id = $1 AND page_id = $2 AND version = $3"
    ).bind(current.workspace_id).bind(page_id).bind(input.version)
    .fetch_optional(&mut *transaction).await.map_err(internal)?;
    let revision = revision.ok_or_else(ApiProblem::not_found)?;
    let title = decrypt_text(
        &workspace_key,
        &page_aad(current.workspace_id, page_id, Some(input.version), "title"),
        revision.0,
        revision.1,
        revision.2,
    )?;
    let markdown = decrypt_text(
        &workspace_key,
        &page_aad(current.workspace_id, page_id, Some(input.version), "body"),
        revision.3,
        revision.4,
        revision.5,
    )?;
    let properties_text = decrypt_text(
        &workspace_key,
        &page_aad(
            current.workspace_id,
            page_id,
            Some(input.version),
            "properties",
        ),
        revision.6,
        revision.7,
        revision.8,
    )?;
    let new_version = current_version + 1;
    let current_envelopes = encrypt_page_fields(
        &workspace_key,
        current.workspace_id,
        page_id,
        None,
        &title,
        &markdown,
        properties_text.as_bytes(),
    )?;
    sqlx::query("UPDATE pages SET title_ciphertext=$3,title_nonce=$4,title_key_version=workspace_current_key_version($1),body_ciphertext=$5,body_nonce=$6,body_key_version=workspace_current_key_version($1),properties_ciphertext=$7,properties_nonce=$8,properties_key_version=workspace_current_key_version($1),version=$9,updated_by=$10,updated_at=now() WHERE workspace_id=$1 AND id=$2")
        .bind(current.workspace_id).bind(page_id).bind(current_envelopes.title.ciphertext).bind(current_envelopes.title.nonce.as_slice())
        .bind(current_envelopes.body.ciphertext).bind(current_envelopes.body.nonce.as_slice()).bind(current_envelopes.properties.ciphertext)
        .bind(current_envelopes.properties.nonce.as_slice()).bind(new_version).bind(current.user_id)
        .execute(&mut *transaction).await.map_err(internal)?;
    insert_revision(
        &mut transaction,
        &workspace_key,
        RevisionIdentity {
            workspace_id: current.workspace_id,
            page_id,
            version: new_version,
            author_id: current.user_id,
        },
        PagePlaintext {
            title: &title,
            markdown: &markdown,
            properties: properties_text.as_bytes(),
        },
    )
    .await?;
    reindex_links(&mut transaction, current.workspace_id, page_id, &markdown).await?;
    reindex_search(
        &mut transaction,
        &workspace_key,
        current.workspace_id,
        page_id,
        &title,
        &markdown,
    )
    .await?;
    append_page_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "page.restored",
        page_id,
        json!({"source_version": input.version, "version": new_version}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(json!({"id": page_id, "version": new_version})).into_response())
}

async fn backlinks(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    let current = super::users::lock_and_refresh_current(&mut transaction, current).await?;
    let target_space: Option<Uuid> = sqlx::query_scalar(
        "SELECT space_id FROM pages WHERE workspace_id=$1 AND id=$2 AND archived_at IS NULL",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal)?;
    let _target_space = target_space.ok_or_else(ApiProblem::not_found)?;
    authorize_page(&mut transaction, current, page_id, Capability::ReadPage).await?;
    let candidates: Vec<(Uuid, Uuid)> = sqlx::query_as("SELECT p.id,p.space_id FROM page_links l JOIN pages p ON p.id=l.source_page_id AND p.workspace_id=l.workspace_id WHERE l.workspace_id=$1 AND l.target_page_id=$2 AND p.archived_at IS NULL")
        .bind(current.workspace_id).bind(page_id).fetch_all(&mut *transaction).await.map_err(internal)?;
    let mut visible = Vec::new();
    for (id, space_id) in candidates {
        let access = resolve_access(&mut transaction, current.user_id, space_id).await?;
        if access.can(Capability::ReadPage, ResourceContext::UnrestrictedPage)
            && can_access_page(&mut transaction, current, id, Capability::ReadPage).await?
        {
            visible.push(json!({"id": id}));
        }
    }
    super::users::audit_disclosure(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "backlink.admin_access",
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(visible).into_response())
}

async fn list_page_permissions(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path(page_id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    authorize_page_permission_management(&mut transaction, current, page_id).await?;
    let rows: Vec<(Option<Uuid>, Option<Uuid>, String)> = sqlx::query_as(
        "SELECT pp.user_id, pp.group_id, pp.role FROM page_permissions pp
         WHERE pp.workspace_id=$1 AND pp.page_id=$2 AND (
           EXISTS (SELECT 1 FROM users u JOIN workspace_members wm ON wm.workspace_id=u.workspace_id AND wm.user_id=u.id
             WHERE u.id=pp.user_id AND u.workspace_id=pp.workspace_id AND u.status='active' AND u.deleted_at IS NULL AND wm.status='active')
           OR EXISTS (SELECT 1 FROM groups g WHERE g.id=pp.group_id AND g.workspace_id=pp.workspace_id AND g.archived_at IS NULL))
         ORDER BY pp.user_id NULLS LAST, pp.group_id"
    ).bind(current.workspace_id).bind(page_id).fetch_all(&mut *transaction).await.map_err(internal)?;
    append_page_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "page.permissions_read",
        page_id,
        json!({}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(rows.into_iter().map(|(user, group, role)| json!({"subjectType":if user.is_some(){"user"}else{"group"},"subjectId":user.or(group),"role":role})).collect::<Vec<_>>())).into_response())
}

async fn upsert_page_permission(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path((page_id, subject)): Path<(Uuid, String)>,
    payload: Result<Json<UpsertPagePermissionRequest>, JsonRejection>,
) -> Result<Response, ApiProblem> {
    let subject = PermissionSubject::parse(&subject)?;
    let Json(input) =
        payload.map_err(|_| ApiProblem::validation("body", "expected a valid page role"))?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    authorize_page_permission_management(&mut transaction, current, page_id).await?;
    let restricted: Option<bool> = sqlx::query_scalar(
        "SELECT restricted FROM pages
          WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL FOR UPDATE",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_optional(&mut *transaction)
    .await
    .map_err(internal)?;
    if restricted != Some(true) {
        return Err(if restricted.is_some() {
            ApiProblem::conflict("Permissions can be assigned only to a restricted page")
        } else {
            ApiProblem::not_found()
        });
    }
    validate_permission_subject(&mut transaction, current.workspace_id, subject).await?;
    let permission_id = Uuid::now_v7();
    match subject {
        PermissionSubject::User(user_id) => {
            sqlx::query(
                "INSERT INTO page_permissions (
                    id, workspace_id, page_id, user_id, role, granted_by
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (page_id, user_id) WHERE user_id IS NOT NULL
                 DO UPDATE SET role = EXCLUDED.role, granted_by = EXCLUDED.granted_by,
                               updated_at = now()",
            )
            .bind(permission_id)
            .bind(current.workspace_id)
            .bind(page_id)
            .bind(user_id)
            .bind(input.role.as_str())
            .bind(current.user_id)
            .execute(&mut *transaction)
            .await
            .map_err(internal)?;
        }
        PermissionSubject::Group(group_id) => {
            sqlx::query(
                "INSERT INTO page_permissions (
                    id, workspace_id, page_id, group_id, role, granted_by
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (page_id, group_id) WHERE group_id IS NOT NULL
                 DO UPDATE SET role = EXCLUDED.role, granted_by = EXCLUDED.granted_by,
                               updated_at = now()",
            )
            .bind(permission_id)
            .bind(current.workspace_id)
            .bind(page_id)
            .bind(group_id)
            .bind(input.role.as_str())
            .bind(current.user_id)
            .execute(&mut *transaction)
            .await
            .map_err(internal)?;
        }
    }
    append_page_audit(
        &mut transaction,
        &workspace_key,
        current,
        request_id,
        "page.permission_upserted",
        page_id,
        json!({"subject_type": subject.kind(), "subject_id": subject.id(), "role": input.role.as_str()}),
    )
    .await?;
    transaction.commit().await.map_err(internal)?;
    Ok(Json(json!({
        "subjectType": subject.kind(),
        "subjectId": subject.id(),
        "role": input.role.as_str()
    }))
    .into_response())
}

async fn delete_page_permission(
    State(state): State<AppState>,
    Extension(request_id): Extension<RequestId>,
    current: CurrentUser,
    Path((page_id, subject)): Path<(Uuid, String)>,
) -> Result<Response, ApiProblem> {
    let subject = PermissionSubject::parse(&subject)?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let workspace_key = load_workspace_key(&state, current.workspace_id).await?;
    let mut transaction = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut transaction, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut transaction, current).await?;
    authorize_page_permission_management(&mut transaction, current, page_id).await?;
    let removed = match subject {
        PermissionSubject::User(user_id) => sqlx::query(
            "DELETE FROM page_permissions
              WHERE workspace_id = $1 AND page_id = $2 AND user_id = $3",
        )
        .bind(current.workspace_id)
        .bind(page_id)
        .bind(user_id)
        .execute(&mut *transaction)
        .await
        .map_err(internal)?
        .rows_affected(),
        PermissionSubject::Group(group_id) => sqlx::query(
            "DELETE FROM page_permissions
              WHERE workspace_id = $1 AND page_id = $2 AND group_id = $3",
        )
        .bind(current.workspace_id)
        .bind(page_id)
        .bind(group_id)
        .execute(&mut *transaction)
        .await
        .map_err(internal)?
        .rows_affected(),
    };
    if removed == 1 {
        append_page_audit(
            &mut transaction,
            &workspace_key,
            current,
            request_id,
            "page.permission_deleted",
            page_id,
            json!({"subject_type": subject.kind(), "subject_id": subject.id()}),
        )
        .await?;
    }
    transaction.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

struct PageEnvelopes {
    title: EncryptedEnvelope,
    body: EncryptedEnvelope,
    properties: EncryptedEnvelope,
}

#[derive(Clone, Copy)]
struct RevisionIdentity {
    workspace_id: Uuid,
    page_id: Uuid,
    version: i64,
    author_id: Uuid,
}

#[derive(Clone, Copy)]
struct PagePlaintext<'a> {
    title: &'a str,
    markdown: &'a str,
    properties: &'a [u8],
}

type StoredPageRow = (
    Uuid,
    Option<Uuid>,
    Uuid,
    Vec<u8>,
    Vec<u8>,
    i32,
    Vec<u8>,
    Vec<u8>,
    i32,
    Vec<u8>,
    Vec<u8>,
    i32,
    i64,
    bool,
);
type RevisionRow = (
    Vec<u8>,
    Vec<u8>,
    i32,
    Vec<u8>,
    Vec<u8>,
    i32,
    Vec<u8>,
    Vec<u8>,
    i32,
);
type SearchIndexPageRow = (Uuid, Vec<u8>, Vec<u8>, i32, Vec<u8>, Vec<u8>, i32);

fn version_conflict(current_version: i64) -> Response {
    let mut response = (
        StatusCode::CONFLICT,
        Json(json!({
            "type": "https://knowledge.local/problems/version-conflict",
            "title": "Version conflict", "status": 409, "code": "version_conflict",
            "currentVersion": current_version
        })),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/problem+json"),
    );
    response
}

async fn append_page_audit(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_key: &WorkspaceKey,
    current: CurrentUser,
    request_id: RequestId,
    event_type: &'static str,
    page_id: Uuid,
    metadata: Value,
) -> Result<(), ApiProblem> {
    append_audit_event(
        transaction,
        workspace_key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type,
            target_type: "page",
            target_id: Some(page_id),
            request_id: request_id.as_uuid(),
            metadata,
        },
    )
    .await
    .map_err(internal)
}

async fn reindex_links(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    source_page_id: Uuid,
    markdown: &str,
) -> Result<(), ApiProblem> {
    let stable_keys: Vec<Uuid> = Parser::new(markdown)
        .filter_map(|event| match event {
            Event::Start(Tag::Link { dest_url, .. }) => dest_url
                .strip_prefix("/pages/")
                .and_then(|value| value.split(['#', '?']).next())
                .and_then(|value| value.parse().ok()),
            _ => None,
        })
        .collect();
    sqlx::query("DELETE FROM page_links WHERE workspace_id=$1 AND source_page_id=$2")
        .bind(workspace_id)
        .bind(source_page_id)
        .execute(&mut **transaction)
        .await
        .map_err(internal)?;
    for stable_key in stable_keys {
        let target_id: Option<Uuid> = sqlx::query_scalar(
            "SELECT id FROM pages WHERE workspace_id=$1 AND stable_key=$2 AND archived_at IS NULL",
        )
        .bind(workspace_id)
        .bind(stable_key)
        .fetch_optional(&mut **transaction)
        .await
        .map_err(internal)?;
        if let Some(target_id) = target_id.filter(|target_id| *target_id != source_page_id) {
            sqlx::query("INSERT INTO page_links(workspace_id,source_page_id,target_page_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(workspace_id).bind(source_page_id).bind(target_id).execute(&mut **transaction).await.map_err(internal)?;
        }
    }
    Ok(())
}

async fn reindex_search(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_key: &WorkspaceKey,
    workspace_id: Uuid,
    page_id: Uuid,
    title: &str,
    markdown: &str,
) -> Result<(), ApiProblem> {
    let indexer = BlindIndexer::new(workspace_key.derive(KeyPurpose::Search).map_err(internal)?);
    let title_tokens = indexer.tokens("und", title);
    let body_tokens = indexer.tokens("und", markdown);
    BlindSearchRepository::replace_page_tokens(
        transaction,
        workspace_id,
        page_id,
        &title_tokens,
        &body_tokens,
    )
    .await
    .map_err(internal)
}

fn encrypt_page_fields(
    workspace_key: &WorkspaceKey,
    workspace_id: Uuid,
    page_id: Uuid,
    revision: Option<i64>,
    title: &str,
    markdown: &str,
    properties: &[u8],
) -> Result<PageEnvelopes, ApiProblem> {
    let cipher = EnvelopeCipher::new(
        workspace_key.version(),
        workspace_key
            .derive(KeyPurpose::Content)
            .map_err(internal)?,
    );
    Ok(PageEnvelopes {
        title: cipher
            .encrypt(
                page_aad(workspace_id, page_id, revision, "title").as_bytes(),
                title.as_bytes(),
            )
            .map_err(internal)?,
        body: cipher
            .encrypt(
                page_aad(workspace_id, page_id, revision, "body").as_bytes(),
                markdown.as_bytes(),
            )
            .map_err(internal)?,
        properties: cipher
            .encrypt(
                page_aad(workspace_id, page_id, revision, "properties").as_bytes(),
                properties,
            )
            .map_err(internal)?,
    })
}

async fn insert_revision(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_key: &WorkspaceKey,
    identity: RevisionIdentity,
    plaintext: PagePlaintext<'_>,
) -> Result<(), ApiProblem> {
    let envelopes = encrypt_page_fields(
        workspace_key,
        identity.workspace_id,
        identity.page_id,
        Some(identity.version),
        plaintext.title,
        plaintext.markdown,
        plaintext.properties,
    )?;
    let content_hash = content_hash(plaintext.title, plaintext.markdown, plaintext.properties);
    sqlx::query(
        "INSERT INTO page_revisions (
            id, workspace_id, page_id, version,
            title_ciphertext, title_nonce, title_key_version,
            body_ciphertext, body_nonce, body_key_version,
            properties_ciphertext, properties_nonce, properties_key_version,
            content_hash, author_id
         ) VALUES ($1, $2, $3, $4, $5, $6, workspace_current_key_version($2), $7, $8, workspace_current_key_version($2), $9, $10, workspace_current_key_version($2), $11, $12)",
    )
    .bind(Uuid::now_v7())
    .bind(identity.workspace_id)
    .bind(identity.page_id)
    .bind(identity.version)
    .bind(envelopes.title.ciphertext)
    .bind(envelopes.title.nonce.as_slice())
    .bind(envelopes.body.ciphertext)
    .bind(envelopes.body.nonce.as_slice())
    .bind(envelopes.properties.ciphertext)
    .bind(envelopes.properties.nonce.as_slice())
    .bind(content_hash.as_slice())
    .bind(identity.author_id)
    .execute(&mut **transaction)
    .await
    .map_err(internal)?;
    Ok(())
}

pub(crate) fn page_aad(
    workspace_id: Uuid,
    page_id: Uuid,
    revision: Option<i64>,
    field: &str,
) -> String {
    match revision {
        Some(version) => {
            format!("workspace:{workspace_id}:page:{page_id}:revision:{version}:{field}")
        }
        None => format!("workspace:{workspace_id}:page:{page_id}:current:{field}"),
    }
}

fn content_hash(title: &str, markdown: &str, properties: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(b"knowledge-platform/page-content/v1\0");
    digest.update((title.len() as u64).to_be_bytes());
    digest.update(title.as_bytes());
    digest.update((markdown.len() as u64).to_be_bytes());
    digest.update(markdown.as_bytes());
    digest.update(properties);
    digest.finalize().into()
}

fn validate_page(title: &str, markdown: &str, properties: &Value) -> Result<(), ApiProblem> {
    if title.trim().is_empty() || title.chars().count() > 500 {
        return Err(ApiProblem::validation(
            "title",
            "must contain between 1 and 500 characters",
        ));
    }
    if markdown.len() > 5 * 1024 * 1024 {
        return Err(ApiProblem::validation("markdown", "is too large"));
    }
    if !properties.is_object() {
        return Err(ApiProblem::validation("properties", "must be an object"));
    }
    Ok(())
}

async fn authorize_page(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
    page_id: Uuid,
    capability: Capability,
) -> Result<knowledge_core::permissions::PageAccessContext, ApiProblem> {
    let context =
        PageRepository::access_context(transaction, current.workspace_id, current.user_id, page_id)
            .await
            .map_err(internal)?
            .ok_or_else(ApiProblem::not_found)?;
    if context.can(capability) {
        Ok(context)
    } else if context.can(Capability::ReadPage) {
        Err(ApiProblem::forbidden())
    } else {
        Err(ApiProblem::not_found())
    }
}

async fn authorize_page_permission_management(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
    page_id: Uuid,
) -> Result<(), ApiProblem> {
    let context =
        PageRepository::access_context(transaction, current.workspace_id, current.user_id, page_id)
            .await
            .map_err(internal)?
            .ok_or_else(ApiProblem::not_found)?;
    if context.access().is_workspace_admin()
        || context.access().effective_space_role() == Some(SpaceRole::Owner)
    {
        Ok(())
    } else {
        Err(ApiProblem::not_found())
    }
}

async fn visible_parent_id(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
    parent_id: Option<Uuid>,
) -> Result<Option<Uuid>, ApiProblem> {
    let Some(parent_id) = parent_id else {
        return Ok(None);
    };
    if can_access_page(transaction, current, parent_id, Capability::ReadPage).await? {
        Ok(Some(parent_id))
    } else {
        Ok(None)
    }
}

fn can_change_restricted_boundary(
    source_access: &EffectiveAccess,
    destination_access: &EffectiveAccess,
) -> bool {
    source_access.is_workspace_admin()
        || (source_access.effective_space_role() == Some(SpaceRole::Owner)
            && destination_access.effective_space_role() == Some(SpaceRole::Owner))
}

async fn can_access_page(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
    page_id: Uuid,
    capability: Capability,
) -> Result<bool, ApiProblem> {
    Ok(
        PageRepository::access_context(transaction, current.workspace_id, current.user_id, page_id)
            .await
            .map_err(internal)?
            .is_some_and(|context| context.can(capability)),
    )
}

async fn authorize_subtree(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
    page_id: Uuid,
    capability: Capability,
) -> Result<(), ApiProblem> {
    let page_ids: Vec<Uuid> = sqlx::query_scalar(
        "WITH RECURSIVE subtree(id) AS (
            SELECT id FROM pages WHERE workspace_id = $1 AND id = $2
            UNION ALL
            SELECT p.id FROM pages p JOIN subtree s ON p.parent_id = s.id
             WHERE p.workspace_id = $1
         ) SELECT id FROM subtree",
    )
    .bind(current.workspace_id)
    .bind(page_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal)?;
    for descendant_id in page_ids {
        authorize_page(transaction, current, descendant_id, capability).await?;
    }
    Ok(())
}

async fn subtree_contains_restricted_boundary(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    page_id: Uuid,
) -> Result<bool, ApiProblem> {
    sqlx::query_scalar(
        "WITH RECURSIVE subtree(id, restricted) AS (
            SELECT id, restricted FROM pages WHERE workspace_id = $1 AND id = $2
            UNION ALL
            SELECT p.id, p.restricted FROM pages p JOIN subtree s ON p.parent_id = s.id
             WHERE p.workspace_id = $1
         ) SELECT COALESCE(bool_or(restricted), false) FROM subtree",
    )
    .bind(workspace_id)
    .bind(page_id)
    .fetch_one(&mut **transaction)
    .await
    .map_err(internal)
}

async fn validate_permission_subject(
    transaction: &mut Transaction<'_, Postgres>,
    workspace_id: Uuid,
    subject: PermissionSubject,
) -> Result<(), ApiProblem> {
    let valid = match subject {
        PermissionSubject::User(user_id) => sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM users u
                JOIN workspace_members wm
                  ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
               WHERE u.workspace_id = $1 AND u.id = $2 AND u.status = 'active'
                 AND u.deleted_at IS NULL AND wm.status = 'active'
            )",
        )
        .bind(workspace_id)
        .bind(user_id)
        .fetch_one(&mut **transaction)
        .await
        .map_err(internal)?,
        PermissionSubject::Group(group_id) => sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM groups
                 WHERE workspace_id = $1 AND id = $2 AND archived_at IS NULL
            )",
        )
        .bind(workspace_id)
        .bind(group_id)
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

async fn next_sort_key(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
    access: &EffectiveAccess,
    space_id: Uuid,
    parent_id: Option<Uuid>,
    before_id: Option<Uuid>,
    excluded_id: Option<Uuid>,
) -> Result<String, ApiProblem> {
    const GAP: u64 = 1_000_000_000_000;
    let mut siblings: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, sort_key FROM pages
          WHERE workspace_id = $1 AND space_id = $2
            AND parent_id IS NOT DISTINCT FROM $3 AND archived_at IS NULL
          ORDER BY sort_key, id FOR UPDATE",
    )
    .bind(current.workspace_id)
    .bind(space_id)
    .bind(parent_id)
    .fetch_all(&mut **transaction)
    .await
    .map_err(internal)?;
    siblings.retain(|(id, _)| Some(*id) != excluded_id);
    let insertion_index = match before_id {
        Some(before_id) => siblings
            .iter()
            .position(|(id, _)| *id == before_id)
            .ok_or_else(ApiProblem::not_found)?,
        None => siblings.len(),
    };
    let ranks = siblings
        .iter()
        .map(|(_, key)| key.parse::<u64>())
        .collect::<Result<Vec<_>, _>>();
    let sparse_rank = if siblings.is_empty() {
        Some(GAP)
    } else if insertion_index == siblings.len() {
        ranks
            .as_ref()
            .ok()
            .and_then(|ranks| ranks.last().copied())
            .and_then(|rank| rank.checked_add(GAP))
    } else {
        ranks.as_ref().ok().and_then(|ranks| {
            let upper = ranks[insertion_index];
            let lower = if insertion_index == 0 {
                0
            } else {
                ranks[insertion_index - 1]
            };
            upper.checked_sub(lower).and_then(|distance| {
                let candidate = lower + distance / 2;
                (candidate > lower && candidate < upper).then_some(candidate)
            })
        })
    };
    if let Some(rank) = sparse_rank {
        return Ok(format!("{rank:020}"));
    }

    if !access.is_workspace_admin() {
        for (id, _) in &siblings {
            if !can_access_page(transaction, current, *id, Capability::EditPage).await? {
                return Err(ApiProblem::conflict("Sibling order needs compaction"));
            }
        }
    }
    for (index, (id, _)) in siblings.iter().enumerate() {
        let position = if index < insertion_index {
            index + 1
        } else {
            index + 2
        };
        let rank = (position as u64)
            .checked_mul(GAP)
            .ok_or_else(|| ApiProblem::conflict("Sibling order needs compaction"))?;
        sqlx::query("UPDATE pages SET sort_key = $2 WHERE id = $1")
            .bind(id)
            .bind(format!("{rank:020}"))
            .execute(&mut **transaction)
            .await
            .map_err(internal)?;
    }
    let rank = ((insertion_index + 1) as u64)
        .checked_mul(GAP)
        .ok_or_else(|| ApiProblem::conflict("Sibling order needs compaction"))?;
    Ok(format!("{rank:020}"))
}

fn internal<E>(_error: E) -> ApiProblem {
    ApiProblem::internal()
}
