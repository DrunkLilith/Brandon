//! Streaming multipart uploads with optional UUID Idempotency-Key.
//! Keys are scoped to actor/workspace/upload operation. Replays validate the
//! body fingerprint without storage writes and recheck current ACLs. Every
//! streamed attempt, including replay, consumes the actor body-processing cap.
//! Administrative disclosures commit a separate access audit first. Pending
//! attempts return 409; failure/crash reservations are reclaimed only by expired
//! orphan cleanup after the upload's session lock has gone away. Retry with a
//! new key immediately, or the same key after cleanup. No plaintext fingerprint.
use crate::{
    CurrentUser,
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    middleware::RequestId,
    problem::ApiProblem,
    routes::{spaces::revalidate_session, users::load_workspace_key},
    state::AppState,
};
use axum::{
    Json, Router,
    body::Body,
    extract::{
        DefaultBodyLimit, Extension, FromRequest, Multipart, Path, Query, Request, State,
        rejection::QueryRejection,
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use bytes::Bytes;
use futures_util::StreamExt;
use hmac::{Hmac, KeyInit, Mac};
use knowledge_core::permissions::{Capability, SpaceRole};
use knowledge_crypto::{
    EncryptedEnvelope, EnvelopeCipher, KeyPurpose, KeyVersion, WorkspaceKey,
    stream::{AttachmentStream, STREAM_CHUNK_SIZE},
};
use knowledge_store::{
    attachments::{ObjectStore, ObjectUpload, lock_object},
    pages::PageRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{sync::Arc, time::Duration};
use uuid::Uuid;
use zeroize::Zeroizing;

pub const MAX_UPLOAD_SIZE: u64 = 100 * 1024 * 1024;
const OP_TIMEOUT: Duration = Duration::from_secs(30);
const MULTIPART_OVERHEAD: usize = 8192;
pub(crate) fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/pages/{id}/attachments", post(upload).get(list))
        .route("/api/v1/attachments/{id}", get(download).delete(archive))
        .route("/api/v1/attachments/{id}/unarchive", post(unarchive))
        .layer(DefaultBodyLimit::max(
            MAX_UPLOAD_SIZE as usize + MULTIPART_OVERHEAD,
        ))
}
async fn bounded_multipart(
    request: Request,
    state: &AppState,
    size: u64,
) -> Result<Multipart, ApiProblem> {
    let boundary = multer::parse_boundary(
        request
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .ok_or_else(|| ApiProblem::validation("file", "expected multipart content"))?,
    )
    .map_err(|_| ApiProblem::validation("file", "invalid multipart boundary"))?;
    if boundary.is_empty()
        || boundary.len() > 70
        || !boundary.is_ascii()
        || boundary.chars().any(char::is_control)
    {
        return Err(ApiProblem::validation("file", "invalid multipart boundary"));
    }
    let prefix = format!("--{boundary}\r\n").into_bytes();
    let (parts, body) = request.into_parts();
    let stream = futures_util::stream::try_unfold(
        (body.into_data_stream(), Vec::new(), false, 0u64),
        move |(mut stream, mut first_header, mut complete, mut count)| {
            let prefix = prefix.clone();
            async move {
                let Some(part) = tokio::time::timeout(OP_TIMEOUT, stream.next())
                    .await
                    .map_err(|_| std::io::Error::other("upload interrupted"))?
                else {
                    return Ok::<_, std::io::Error>(None);
                };
                let part = part.map_err(|_| std::io::Error::other("upload interrupted"))?;
                count = count
                    .checked_add(part.len() as u64)
                    .ok_or_else(|| std::io::Error::other("upload exceeds limit"))?;
                if count > size + MULTIPART_OVERHEAD as u64 {
                    return Err(std::io::Error::other("upload exceeds limit"));
                }
                if !complete {
                    for byte in &part {
                        first_header.push(*byte);
                        let len = first_header.len();
                        if len > MULTIPART_OVERHEAD
                            || len <= prefix.len() && *byte != prefix[len - 1]
                        {
                            return Err(std::io::Error::other("invalid multipart header"));
                        }
                        if len >= prefix.len() && first_header.ends_with(b"\r\n\r\n") {
                            complete = true;
                            first_header.clear();
                            break;
                        }
                    }
                }
                Ok(Some((part, (stream, first_header, complete, count))))
            }
        },
    );
    Multipart::from_request(Request::from_parts(parts, Body::from_stream(stream)), state)
        .await
        .map_err(|_| ApiProblem::validation("file", "expected valid multipart upload"))
}
#[derive(Deserialize)]
struct UploadQuery {
    size: u64,
}
#[derive(sqlx::FromRow)]
struct UploadReservation {
    page_id: Uuid,
    declared_size: i64,
    attachment_id: Option<Uuid>,
    fingerprint: Option<Vec<u8>>,
}
struct ReceivedUpload {
    name: String,
    mime: String,
    checksum: [u8; 32],
    fingerprint: Hmac<Sha256>,
}
fn upload_fingerprint(
    key: &WorkspaceKey,
    current: CurrentUser,
    page: Uuid,
    size: u64,
    request: Option<Uuid>,
) -> Result<Hmac<Sha256>, ApiProblem> {
    let child_key = Zeroizing::new(
        key.derive(KeyPurpose::Attachment)
            .map_err(internal)?
            .authenticate(b"knowledge/upload-fingerprint-key/v1"),
    );
    let mut mac = <Hmac<Sha256> as KeyInit>::new_from_slice(&child_key[..]).map_err(internal)?;
    mac.update(b"knowledge/upload-fingerprint/v1");
    mac.update(current.workspace_id.as_bytes());
    mac.update(current.user_id.as_bytes());
    mac.update(page.as_bytes());
    mac.update(&size.to_be_bytes());
    mac.update(request.unwrap_or(Uuid::nil()).as_bytes());
    Ok(mac)
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct ListQuery {
    #[serde(default)]
    archived: bool,
    after: Option<Uuid>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AttachmentResponse {
    id: Uuid,
    page_id: Uuid,
    name: String,
    mime: String,
    size: i64,
    archived: bool,
}
#[derive(sqlx::FromRow)]
struct StoredAttachment {
    id: Uuid,
    page_id: Uuid,
    object_key: Uuid,
    plaintext_size: i64,
    ciphertext_size: i64,
    original_name_ciphertext: Vec<u8>,
    original_name_nonce: Vec<u8>,
    original_name_key_version: i32,
    mime_ciphertext: Vec<u8>,
    mime_nonce: Vec<u8>,
    mime_key_version: i32,
    key_version: i32,
    stream_header: Vec<u8>,
    archived: bool,
}
macro_rules! select_attachment { ($suffix:literal) => {concat!("SELECT id,page_id,object_key,plaintext_size,ciphertext_size,original_name_ciphertext,original_name_nonce,original_name_key_version,mime_ciphertext,mime_nonce,mime_key_version,key_version,stream_header,(archived_at IS NOT NULL) AS archived FROM attachments ",$suffix)}; }
fn internal<E>(_: E) -> ApiProblem {
    ApiProblem::internal()
}
pub fn attachment_aad(workspace: Uuid, id: Uuid, field: &str) -> Vec<u8> {
    format!("knowledge/attachment/v1/{workspace}/{id}/{field}").into_bytes()
}
async fn authorize(
    transaction: &mut Transaction<'_, Postgres>,
    current: CurrentUser,
    page: Uuid,
    capability: Capability,
    owner: bool,
) -> Result<bool, ApiProblem> {
    let active:bool=sqlx::query_scalar("WITH RECURSIVE ancestors AS (SELECT id,parent_id,space_id,archived_at FROM pages WHERE workspace_id=$1 AND id=$2 UNION ALL SELECT p.id,p.parent_id,p.space_id,p.archived_at FROM pages p JOIN ancestors a ON p.id=a.parent_id WHERE p.workspace_id=$1) SELECT EXISTS(SELECT 1 FROM ancestors) AND NOT EXISTS(SELECT 1 FROM ancestors a JOIN spaces s ON s.id=a.space_id WHERE a.archived_at IS NOT NULL OR s.archived_at IS NOT NULL)").bind(current.workspace_id).bind(page).fetch_one(&mut **transaction).await.map_err(internal)?;
    if !active {
        return Err(ApiProblem::not_found());
    }
    let access =
        PageRepository::access_context(transaction, current.workspace_id, current.user_id, page)
            .await
            .map_err(internal)?
            .ok_or_else(ApiProblem::not_found)?;
    if !access.can(capability) {
        return Err(if access.can(Capability::ReadPage) {
            ApiProblem::forbidden()
        } else {
            ApiProblem::not_found()
        });
    }
    if owner
        && !access.access().is_workspace_admin()
        && access.access().effective_space_role() != Some(SpaceRole::Owner)
    {
        return Err(ApiProblem::forbidden());
    }
    Ok(access.access().is_workspace_admin())
}
fn response(
    key: &WorkspaceKey,
    workspace: Uuid,
    row: StoredAttachment,
) -> Result<AttachmentResponse, ApiProblem> {
    Ok(AttachmentResponse {
        id: row.id,
        page_id: row.page_id,
        name: decrypt_text(
            key,
            &attachment_aad(workspace, row.id, "name"),
            row.original_name_ciphertext,
            row.original_name_nonce,
            row.original_name_key_version,
        )?,
        mime: decrypt_text(
            key,
            &attachment_aad(workspace, row.id, "mime"),
            row.mime_ciphertext,
            row.mime_nonce,
            row.mime_key_version,
        )?,
        size: row.plaintext_size,
        archived: row.archived,
    })
}
pub(crate) fn decrypt_text(
    key: &WorkspaceKey,
    aad: &[u8],
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    version: i32,
) -> Result<String, ApiProblem> {
    let version = KeyVersion(u32::try_from(version).map_err(internal)?);
    let envelope = EncryptedEnvelope {
        algorithm: "AES-256-GCM".into(),
        key_version: version,
        nonce: nonce.try_into().map_err(internal)?,
        ciphertext,
    };
    let cipher = EnvelopeCipher::new(
        version,
        key.derive(KeyPurpose::Attachment).map_err(internal)?,
    );
    String::from_utf8(
        cipher
            .decrypt(aad, &envelope)
            .map_err(internal)?
            .expose()
            .to_vec(),
    )
    .map_err(internal)
}
async fn write(
    objects: &mut Option<&mut Box<dyn ObjectUpload>>,
    checksum: &mut Sha256,
    bytes: Vec<u8>,
) -> Result<(), ApiProblem> {
    checksum.update(&bytes);
    let Some(objects) = objects.as_mut() else {
        return Ok(());
    };
    tokio::time::timeout(OP_TIMEOUT, objects.write(Bytes::from(bytes)))
        .await
        .map_err(internal)?
        .map_err(internal)
}
async fn upload(
    State(state): State<AppState>,
    Extension(request): Extension<RequestId>,
    current: CurrentUser,
    Path(page): Path<Uuid>,
    input: Result<Query<UploadQuery>, QueryRejection>,
    headers: HeaderMap,
    request_body: Request,
) -> Result<Response, ApiProblem> {
    let Query(input) =
        input.map_err(|_| ApiProblem::validation("size", "expected declared byte length"))?;
    if input.size == 0 || input.size > MAX_UPLOAD_SIZE {
        return Err(ApiProblem::validation(
            "size",
            "outside the allowed upload limit",
        ));
    }
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let objects = runtime
        .objects
        .as_ref()
        .ok_or_else(ApiProblem::unavailable)?;
    let _permit = runtime
        .object_streams
        .clone()
        .try_acquire_owned()
        .map_err(|_| ApiProblem::unavailable())?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut multipart = bounded_multipart(request_body, &state, input.size).await?;
    let request_key = headers
        .get("idempotency-key")
        .map(|value| {
            value
                .to_str()
                .ok()
                .and_then(|value| value.parse::<Uuid>().ok())
                .ok_or_else(|| ApiProblem::validation("idempotency-key", "expected a UUID"))
        })
        .transpose()?;
    {
        let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
        revalidate_session(&mut tx, current).await?;
        authorize(&mut tx, current, page, Capability::UploadAttachment, false).await?;
        if let Some(request_key) = request_key {
            let reservation:Option<UploadReservation>=sqlx::query_as("SELECT page_id,declared_size,attachment_id,fingerprint FROM attachment_upload_requests WHERE workspace_id=$1 AND user_id=$2 AND request_key=$3").bind(current.workspace_id).bind(current.user_id).bind(request_key).fetch_optional(&mut *tx).await.map_err(internal)?;
            if let Some(reservation) = reservation {
                tx.commit().await.map_err(internal)?;
                if reservation.page_id != page || reservation.declared_size != input.size as i64 {
                    return Err(ApiProblem::conflict(
                        "Upload request conflicts with an earlier attempt",
                    ));
                }
                let attachment = reservation.attachment_id.ok_or_else(|| {
                    ApiProblem::conflict(
                        "Upload is pending; retry after completion or orphan cleanup",
                    )
                })?;
                let fingerprint = reservation.fingerprint.ok_or_else(ApiProblem::internal)?;
                charge_body_attempt(&state, current).await?;
                let stream = AttachmentStream::new(
                    key.derive(KeyPurpose::Attachment).map_err(internal)?,
                    KeyVersion(1),
                    current.workspace_id.as_bytes(),
                    attachment.as_bytes(),
                    input.size,
                );
                let received = tokio::time::timeout(
                    Duration::from_secs(300),
                    receive(
                        &mut multipart,
                        None,
                        &stream,
                        input.size,
                        upload_fingerprint(&key, current, page, input.size, Some(request_key))?,
                    ),
                )
                .await
                .map_err(|_| {
                    ApiProblem::conflict("Upload request conflicts with an earlier attempt")
                })?
                .map_err(|_| {
                    ApiProblem::conflict("Upload request conflicts with an earlier attempt")
                })?;
                received
                    .fingerprint
                    .verify_slice(&fingerprint)
                    .map_err(|_| {
                        ApiProblem::conflict("Upload request conflicts with an earlier attempt")
                    })?;
                let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
                lock_workspace_for_audit(&mut tx, current.workspace_id)
                    .await
                    .map_err(internal)?;
                revalidate_session(&mut tx, current).await?;
                let administrative =
                    authorize(&mut tx, current, page, Capability::UploadAttachment, false).await?;
                let stored = load(&mut tx, current.workspace_id, attachment, false).await?;
                if stored.page_id != page {
                    return Err(ApiProblem::internal());
                }
                if administrative {
                    audit_access(
                        &mut tx,
                        &key,
                        current,
                        request,
                        ("attachment", attachment),
                        "upload_replay",
                    )
                    .await?;
                }
                tx.commit().await.map_err(internal)?;
                return Ok((
                    StatusCode::CREATED,
                    [(header::CACHE_CONTROL, "no-store")],
                    Json(response(&key, current.workspace_id, stored)?),
                )
                    .into_response());
            }
        }
        tx.commit().await.map_err(internal)?;
    }
    charge_body_attempt(&state, current).await?;
    let id = Uuid::now_v7();
    let object = Uuid::now_v7();
    let _guard = tokio::time::timeout(OP_TIMEOUT, lock_object(runtime.store.pool(), object))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    let mut reserve = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut reserve, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut reserve, current).await?;
    authorize(
        &mut reserve,
        current,
        page,
        Capability::UploadAttachment,
        false,
    )
    .await?;
    if let Some(request_key) = request_key {
        let added=sqlx::query("INSERT INTO attachment_upload_requests(workspace_id,user_id,request_key,page_id,declared_size,object_key) VALUES($1,$2,$3,$4,$5,$6) ON CONFLICT DO NOTHING").bind(current.workspace_id).bind(current.user_id).bind(request_key).bind(page).bind(input.size as i64).bind(object).execute(&mut *reserve).await.map_err(internal)?;
        if added.rows_affected() != 1 {
            return Err(ApiProblem::conflict(
                "Upload is pending; retry after completion or orphan cleanup",
            ));
        }
    }
    sqlx::query(
        "INSERT INTO attachment_staging(object_key,workspace_id,attachment_id) VALUES($1,$2,$3)",
    )
    .bind(object)
    .bind(current.workspace_id)
    .bind(id)
    .execute(&mut *reserve)
    .await
    .map_err(internal)?;
    reserve.commit().await.map_err(internal)?;
    let stream = AttachmentStream::new(
        key.derive(KeyPurpose::Attachment).map_err(internal)?,
        KeyVersion(1),
        current.workspace_id.as_bytes(),
        id.as_bytes(),
        input.size,
    );
    let mut writer = tokio::time::timeout(OP_TIMEOUT, objects.start(object))
        .await
        .map_err(internal)?
        .map_err(internal)?;
    // The only unavoidable cross-system crash gap leaves an EMPTY remote
    // multipart. Deployment must configure AbortIncompleteMultipartUpload.
    // Never accept any file bytes before the remote identity is durable.
    if let Err(error) =
        sqlx::query("UPDATE attachment_staging SET remote_upload_id=$2 WHERE object_key=$1")
            .bind(object)
            .bind(writer.upload_id())
            .execute(runtime.store.pool())
            .await
    {
        let _ = tokio::time::timeout(OP_TIMEOUT, writer.abort()).await;
        return Err(internal(error));
    }
    let result = tokio::time::timeout(
        Duration::from_secs(300),
        receive(
            &mut multipart,
            Some(&mut writer),
            &stream,
            input.size,
            upload_fingerprint(&key, current, page, input.size, request_key)?,
        ),
    )
    .await;
    let ReceivedUpload {
        name,
        mime,
        checksum,
        fingerprint,
    } = match result {
        Ok(Ok(details)) => details,
        other => {
            let _ = tokio::time::timeout(OP_TIMEOUT, writer.abort()).await;
            return Err(match other {
                Ok(Err(error)) => error,
                _ => ApiProblem::validation("file", "upload interrupted"),
            });
        }
    };
    tokio::time::timeout(OP_TIMEOUT, writer.finish())
        .await
        .map_err(internal)?
        .map_err(internal)?;
    let cipher = EnvelopeCipher::new(
        KeyVersion(1),
        key.derive(KeyPurpose::Attachment).map_err(internal)?,
    );
    let name_envelope = cipher
        .encrypt(
            &attachment_aad(current.workspace_id, id, "name"),
            name.as_bytes(),
        )
        .map_err(internal)?;
    let mime_envelope = cipher
        .encrypt(
            &attachment_aad(current.workspace_id, id, "mime"),
            mime.as_bytes(),
        )
        .map_err(internal)?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut tx, current).await?;
    authorize(&mut tx, current, page, Capability::UploadAttachment, false).await?;
    sqlx::query("INSERT INTO attachments(id,workspace_id,page_id,original_name_ciphertext,original_name_nonce,original_name_key_version,mime_ciphertext,mime_nonce,mime_key_version,object_key,plaintext_size,ciphertext_size,checksum,key_version,stream_header,created_by) VALUES($1,$2,$3,$4,$5,1,$6,$7,1,$8,$9,$10,$11,1,$12,$13)").bind(id).bind(current.workspace_id).bind(page).bind(name_envelope.ciphertext).bind(name_envelope.nonce.as_slice()).bind(mime_envelope.ciphertext).bind(mime_envelope.nonce.as_slice()).bind(object).bind(input.size as i64).bind(stream.ciphertext_size() as i64).bind(checksum.as_slice()).bind(stream.header()).bind(current.user_id).execute(&mut *tx).await.map_err(internal)?;
    audit(&mut tx, &key, current, request, id, "attachment.uploaded").await?;
    if let Some(request_key) = request_key {
        sqlx::query("UPDATE attachment_upload_requests SET attachment_id=$4,fingerprint=$5 WHERE workspace_id=$1 AND user_id=$2 AND request_key=$3").bind(current.workspace_id).bind(current.user_id).bind(request_key).bind(id).bind(fingerprint.finalize().into_bytes().as_slice()).execute(&mut *tx).await.map_err(internal)?;
    }
    sqlx::query("DELETE FROM attachment_staging WHERE object_key=$1")
        .bind(object)
        .execute(&mut *tx)
        .await
        .map_err(internal)?;
    tx.commit().await.map_err(internal)?;
    Ok((
        StatusCode::CREATED,
        [(header::CACHE_CONTROL, "no-store")],
        Json(AttachmentResponse {
            id,
            page_id: page,
            name,
            mime,
            size: input.size as i64,
            archived: false,
        }),
    )
        .into_response())
}
async fn receive(
    multipart: &mut Multipart,
    mut writer: Option<&mut Box<dyn ObjectUpload>>,
    stream: &AttachmentStream,
    size: u64,
    mut fingerprint: Hmac<Sha256>,
) -> Result<ReceivedUpload, ApiProblem> {
    let mut field = multipart
        .next_field()
        .await
        .map_err(|_| ApiProblem::validation("file", "malformed multipart upload"))?
        .ok_or_else(|| ApiProblem::validation("file", "required"))?;
    if field.name() != Some("file") {
        return Err(ApiProblem::validation("file", "expected one file field"));
    }
    let name = field
        .file_name()
        .filter(|name| !name.is_empty() && name.len() <= 255 && !name.chars().any(char::is_control))
        .ok_or_else(|| ApiProblem::validation("name", "invalid filename"))?
        .to_owned();
    let mime = field
        .content_type()
        .ok_or_else(|| ApiProblem::validation("mime", "required"))?
        .to_owned();
    let mut checksum = Sha256::new();
    fingerprint.update(&(name.len() as u64).to_be_bytes());
    fingerprint.update(name.as_bytes());
    fingerprint.update(&(mime.len() as u64).to_be_bytes());
    fingerprint.update(mime.as_bytes());
    write(&mut writer, &mut checksum, stream.header().to_vec()).await?;
    let mut pending = Zeroizing::new(Vec::with_capacity(STREAM_CHUNK_SIZE));
    let mut count = 0u64;
    let mut index = 0u64;
    let text = matches!(mime.as_str(), "text/plain" | "text/markdown");
    let mut utf8_tail = Zeroizing::new(Vec::new());
    while let Some(bytes) = tokio::time::timeout(OP_TIMEOUT, field.chunk())
        .await
        .map_err(|_| ApiProblem::validation("file", "upload interrupted"))?
        .map_err(|_| ApiProblem::validation("file", "upload interrupted"))?
    {
        count = count
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| ApiProblem::validation("size", "invalid file size"))?;
        if count > size {
            return Err(ApiProblem::validation("size", "declared size mismatch"));
        }
        let mut remaining = &bytes[..];
        while !remaining.is_empty() {
            let take = (STREAM_CHUNK_SIZE - pending.len()).min(remaining.len());
            pending.extend_from_slice(&remaining[..take]);
            remaining = &remaining[take..];
            if pending.len() == STREAM_CHUNK_SIZE {
                if text {
                    validate_utf8(&pending, &mut utf8_tail)?;
                } else if index == 0 {
                    verify_mime(&pending, &mime)?;
                }
                write(
                    &mut writer,
                    &mut checksum,
                    stream.encrypt_chunk(index, &pending).map_err(internal)?,
                )
                .await?;
                fingerprint.update(&pending);
                pending.clear();
                index += 1;
            }
        }
    }
    if count != size {
        return Err(ApiProblem::validation("size", "declared size mismatch"));
    }
    if !pending.is_empty() {
        if text {
            validate_utf8(&pending, &mut utf8_tail)?;
        } else if index == 0 {
            verify_mime(&pending, &mime)?;
        }
        write(
            &mut writer,
            &mut checksum,
            stream.encrypt_chunk(index, &pending).map_err(internal)?,
        )
        .await?;
        fingerprint.update(&pending);
    }
    if !utf8_tail.is_empty() {
        return Err(ApiProblem::validation("mime", "invalid UTF-8 file"));
    }
    drop(field);
    if multipart
        .next_field()
        .await
        .map_err(|_| ApiProblem::validation("file", "malformed multipart upload"))?
        .is_some()
    {
        return Err(ApiProblem::validation("file", "expected one file field"));
    }
    write(
        &mut writer,
        &mut checksum,
        stream.encrypt_footer().map_err(internal)?,
    )
    .await?;
    Ok(ReceivedUpload {
        name,
        mime,
        checksum: checksum.finalize().into(),
        fingerprint,
    })
}
fn validate_utf8(bytes: &[u8], tail: &mut Zeroizing<Vec<u8>>) -> Result<(), ApiProblem> {
    if bytes.contains(&0) {
        return Err(ApiProblem::validation("mime", "binary bytes in text file"));
    }
    let mut joined = Zeroizing::new(Vec::with_capacity(tail.len() + bytes.len()));
    joined.extend_from_slice(tail);
    joined.extend_from_slice(bytes);
    tail.clear();
    if let Err(error) = std::str::from_utf8(&joined) {
        if error.error_len().is_some() {
            return Err(ApiProblem::validation("mime", "invalid UTF-8 file"));
        }
        tail.extend_from_slice(&joined[error.valid_up_to()..]);
    }
    Ok(())
}
fn verify_mime(bytes: &[u8], mime: &str) -> Result<(), ApiProblem> {
    let valid = match mime {
        "image/png" => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        "image/jpeg" => bytes.starts_with(b"\xff\xd8\xff"),
        "image/gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "application/pdf" => bytes.starts_with(b"%PDF-"),
        "text/plain" | "text/markdown" => std::str::from_utf8(bytes).is_ok() && !bytes.contains(&0),
        "application/octet-stream" => true,
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(ApiProblem::validation(
            "mime",
            "file bytes do not match an allowed type",
        ))
    }
}
async fn list(
    State(state): State<AppState>,
    Extension(request): Extension<RequestId>,
    current: CurrentUser,
    Path(page): Path<Uuid>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Response, ApiProblem> {
    let Query(query) = query.map_err(|_| {
        ApiProblem::validation("query", "expected valid attachment cursor and archive flag")
    })?;
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut tx, current).await?;
    let administrative = authorize(
        &mut tx,
        current,
        page,
        Capability::DownloadAttachment,
        query.archived,
    )
    .await?;
    let rows:Vec<StoredAttachment>=sqlx::query_as(select_attachment!("WHERE workspace_id=$1 AND page_id=$2 AND (archived_at IS NOT NULL)=$3 AND ($4::uuid IS NULL OR id>$4) ORDER BY id LIMIT 100")).bind(current.workspace_id).bind(page).bind(query.archived).bind(query.after).fetch_all(&mut *tx).await.map_err(internal)?;
    if administrative {
        audit_access(&mut tx, &key, current, request, ("page", page), "list").await?;
    }
    tx.commit().await.map_err(internal)?;
    let items = rows
        .into_iter()
        .map(|row| response(&key, current.workspace_id, row))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(items)).into_response())
}
async fn load(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    id: Uuid,
    archived: bool,
) -> Result<StoredAttachment, ApiProblem> {
    sqlx::query_as(select_attachment!(
        "WHERE workspace_id=$1 AND id=$2 AND (archived_at IS NOT NULL)=$3"
    ))
    .bind(workspace)
    .bind(id)
    .bind(archived)
    .fetch_optional(&mut **tx)
    .await
    .map_err(internal)?
    .ok_or_else(ApiProblem::not_found)
}
async fn download(
    State(state): State<AppState>,
    Extension(request): Extension<RequestId>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
    headers: HeaderMap,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let objects = runtime
        .objects
        .clone()
        .ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut tx, current).await?;
    let row = load(&mut tx, current.workspace_id, id, false).await?;
    let administrative = authorize(
        &mut tx,
        current,
        row.page_id,
        Capability::DownloadAttachment,
        false,
    )
    .await?;
    if administrative {
        audit_access(
            &mut tx,
            &key,
            current,
            request,
            ("attachment", id),
            "download",
        )
        .await?;
    }
    tx.commit().await.map_err(internal)?;
    let version = u32::try_from(row.key_version).map_err(internal)?;
    let size = u64::try_from(row.plaintext_size).map_err(internal)?;
    let stream = AttachmentStream::new(
        key.derive(KeyPurpose::Attachment).map_err(internal)?,
        KeyVersion(version),
        current.workspace_id.as_bytes(),
        id.as_bytes(),
        size,
    );
    if row.stream_header != stream.header()
        || row.ciphertext_size as u64 != stream.ciphertext_size()
    {
        return Err(ApiProblem::internal());
    }
    let object = row.object_key;
    if tokio::time::timeout(OP_TIMEOUT, objects.size(object))
        .await
        .map_err(internal)?
        .map_err(internal)?
        != stream.ciphertext_size()
    {
        return Err(ApiProblem::internal());
    }
    if read(&objects, object, 0..stream.header().len() as u64).await?[..] != stream.header()[..] {
        return Err(ApiProblem::internal());
    }
    stream
        .verify_footer(&read(&objects, object, stream.footer_range()).await?)
        .map_err(internal)?;
    let range = if headers.get_all(header::RANGE).iter().count() > 1 {
        Err(())
    } else {
        headers
            .get(header::RANGE)
            .map(|value| value.to_str().map_err(|_| ()))
            .transpose()
    };
    let (start, end, partial) = match range.and_then(|range| parse_range(range, size)) {
        Ok(range) => range,
        Err(()) => {
            return Ok((
                StatusCode::RANGE_NOT_SATISFIABLE,
                [(header::CONTENT_RANGE, format!("bytes */{size}"))],
                Body::empty(),
            )
                .into_response());
        }
    };
    let details = response(&key, current.workspace_id, row)?;
    let first = start / STREAM_CHUNK_SIZE as u64;
    let last = end.div_ceil(STREAM_CHUNK_SIZE as u64);
    let body = Body::from_stream(futures_util::stream::try_unfold(
        (objects, stream, first),
        move |(objects, stream, index)| async move {
            if index >= last {
                return Ok::<_, std::io::Error>(None);
            }
            let range = stream
                .frame_range(index)
                .map_err(|_| std::io::Error::other("authenticated stream failed"))?;
            let frame = read(&objects, object, range)
                .await
                .map_err(|_| std::io::Error::other("object stream failed"))?;
            let bytes = stream
                .decrypt_chunk(index, &frame)
                .map_err(|_| std::io::Error::other("authenticated stream failed"))?;
            Ok(Some((Bytes::from(bytes), (objects, stream, index + 1))))
        },
    ));
    // ASCII fallback is deliberate: no header injection, path components or
    // executable inline rendering, including for HTML-like text/plain files.
    let safe: String = details
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let mut response = (
        if partial {
            StatusCode::PARTIAL_CONTENT
        } else {
            StatusCode::OK
        },
        body,
    )
        .into_response();
    for (name, value) in [
        (header::CONTENT_TYPE, details.mime),
        (
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{safe}\""),
        ),
        (header::CACHE_CONTROL, "no-store".into()),
        (header::CONTENT_LENGTH, (end - start).to_string()),
        (header::ACCEPT_RANGES, "bytes".into()),
    ] {
        response
            .headers_mut()
            .insert(name, value.parse().map_err(internal)?);
    }
    response.headers_mut().insert(
        "x-content-type-options",
        "nosniff".parse().expect("static header"),
    );
    if partial {
        response.headers_mut().insert(
            header::CONTENT_RANGE,
            format!("bytes {start}-{}/{size}", end - 1)
                .parse()
                .map_err(internal)?,
        );
    }
    Ok(response)
}
async fn read(
    objects: &Arc<dyn ObjectStore>,
    key: Uuid,
    range: std::ops::Range<u64>,
) -> Result<Bytes, ApiProblem> {
    tokio::time::timeout(OP_TIMEOUT, objects.read_range(key, range))
        .await
        .map_err(internal)?
        .map_err(internal)
}
fn parse_range(range: Option<&str>, size: u64) -> Result<(u64, u64, bool), ()> {
    let Some(range) = range else {
        return Ok((0, size, false));
    };
    let (start, end) = range
        .strip_prefix("bytes=")
        .ok_or(())?
        .split_once('-')
        .ok_or(())?;
    let start = start.parse::<u64>().map_err(|_| ())?;
    let end = if end.is_empty() {
        size
    } else {
        end.parse::<u64>()
            .map_err(|_| ())?
            .checked_add(1)
            .ok_or(())?
    };
    if start >= end
        || end > size
        || !start.is_multiple_of(STREAM_CHUNK_SIZE as u64)
        || (end != size && !end.is_multiple_of(STREAM_CHUNK_SIZE as u64))
    {
        return Err(());
    }
    Ok((start, end, true))
}
async fn archive(
    State(state): State<AppState>,
    Extension(request): Extension<RequestId>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    mutate(state, request, current, id, false).await
}
async fn unarchive(
    State(state): State<AppState>,
    Extension(request): Extension<RequestId>,
    current: CurrentUser,
    Path(id): Path<Uuid>,
) -> Result<Response, ApiProblem> {
    mutate(state, request, current, id, true).await
}
async fn mutate(
    state: AppState,
    request: RequestId,
    current: CurrentUser,
    id: Uuid,
    restore: bool,
) -> Result<Response, ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let key = load_workspace_key(&state, current.workspace_id).await?;
    let mut tx = runtime.store.pool().begin().await.map_err(internal)?;
    lock_workspace_for_audit(&mut tx, current.workspace_id)
        .await
        .map_err(internal)?;
    revalidate_session(&mut tx, current).await?;
    let row = load(&mut tx, current.workspace_id, id, restore).await?;
    authorize(
        &mut tx,
        current,
        row.page_id,
        Capability::UploadAttachment,
        restore,
    )
    .await?;
    if restore {
        let persisted_retention: Option<(i32, bool)> = sqlx::query_as(
            "SELECT attachment_recovery_days, updated_by IS NOT NULL
               FROM operational_settings WHERE workspace_id=$1",
        )
        .bind(current.workspace_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal)?;
        let retention_days = match persisted_retention {
            Some((days, true)) => days,
            _ => runtime.attachment_retention_days as i32,
        };
        let retained: bool = sqlx::query_scalar(
            "SELECT archived_at > now()-make_interval(days => $2) FROM attachments WHERE id=$1",
        )
        .bind(id)
        .bind(retention_days)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal)?;
        if !retained {
            return Err(ApiProblem::not_found());
        }
    }
    sqlx::query("UPDATE attachments SET archived_at=CASE WHEN $3 THEN NULL ELSE now() END WHERE workspace_id=$1 AND id=$2").bind(current.workspace_id).bind(id).bind(restore).execute(&mut *tx).await.map_err(internal)?;
    audit(
        &mut tx,
        &key,
        current,
        request,
        id,
        if restore {
            "attachment.restored"
        } else {
            "attachment.archived"
        },
    )
    .await?;
    tx.commit().await.map_err(internal)?;
    Ok(StatusCode::NO_CONTENT.into_response())
}
async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    key: &WorkspaceKey,
    current: CurrentUser,
    request: RequestId,
    id: Uuid,
    event: &str,
) -> Result<(), ApiProblem> {
    append_audit_event(
        tx,
        key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: event,
            target_type: "attachment",
            target_id: Some(id),
            request_id: request.as_uuid(),
            metadata: json!({}),
        },
    )
    .await
    .map_err(internal)
}

async fn charge_body_attempt(state: &AppState, current: CurrentUser) -> Result<(), ApiProblem> {
    let runtime = state.runtime().ok_or_else(ApiProblem::unavailable)?;
    let bucket =
        super::auth::rate_limit_bucket(b"attachment-upload/v1", current.user_id.as_bytes());
    if knowledge_store::users::UserRepository::consume_rate_limit_attempt(
        runtime.store.pool(),
        &[bucket],
        60,
    )
    .await
    .map_err(internal)?
    {
        Ok(())
    } else {
        Err(ApiProblem::upload_rate_limited())
    }
}

async fn audit_access(
    tx: &mut Transaction<'_, Postgres>,
    key: &WorkspaceKey,
    current: CurrentUser,
    request: RequestId,
    target: (&str, Uuid),
    operation: &str,
) -> Result<(), ApiProblem> {
    append_audit_event(
        tx,
        key,
        AuditEvent {
            workspace_id: current.workspace_id,
            actor_user_id: Some(current.user_id),
            session_id: Some(current.session_id),
            event_type: "attachment.admin_access",
            target_type: target.0,
            target_id: Some(target.1),
            request_id: request.as_uuid(),
            metadata: json!({"operation": operation}),
        },
    )
    .await
    .map_err(internal)
}
