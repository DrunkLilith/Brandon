//! Encrypted export job execution and operational cleanup.
use bytes::Bytes;
use knowledge_core::{
    ids::{SpaceId, UserId},
    permissions::{Capability, ResourceContext},
};
use knowledge_crypto::{
    EncryptedEnvelope, EnvelopeCipher, KeyPurpose, KeyVersion, WorkspaceKey,
    stream::{AttachmentStream, STREAM_CHUNK_SIZE},
};
use knowledge_store::{
    AccessResolver,
    attachments::{ObjectStore, ObjectUpload, lock_object, try_lock_object},
    jobs::{JobLease, JobRepository},
    pages::PageRepository,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{FromRow, Postgres, Transaction};
use std::{collections::HashMap, time::Duration};
use thiserror::Error;
use uuid::Uuid;

use crate::{
    CurrentUser,
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    export_archive::{
        ArchiveEncryptor, ArchiveInput, ArchiveLimits, ArchivePlan, AttachmentInput, PageInput,
        SourceId, TarPiece,
    },
    problem::ApiProblem,
    routes::{
        attachments::{attachment_aad, decrypt_text as decrypt_attachment_text},
        pages::page_aad,
        users::{decrypt_text, load_workspace_key},
    },
    state::AppState,
};

#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct ExportJobPayload {
    pub export_id: Uuid,
    pub space_id: Uuid,
    pub requester_id: Uuid,
    pub session_id: Uuid,
    pub request_id: Uuid,
}
#[derive(Serialize, Deserialize)]
struct SnapshotAccess {
    page_ids: Vec<Uuid>,
    attachment_ids: Vec<Uuid>,
}
#[derive(Debug, Error)]
#[error("operation failed")]
pub struct OperationError;
pub(crate) fn encrypt_job_payload(
    key: &WorkspaceKey,
    workspace: Uuid,
    job: Uuid,
    payload: &ExportJobPayload,
) -> Result<(Vec<u8>, [u8; 12]), OperationError> {
    let cipher = EnvelopeCipher::new(
        key.version(),
        key.derive(KeyPurpose::Content)
            .map_err(|_| OperationError)?,
    );
    let bytes = serde_json::to_vec(payload).map_err(|_| OperationError)?;
    let value = cipher
        .encrypt(
            format!("workspace:{workspace}:job:{job}:payload").as_bytes(),
            &bytes,
        )
        .map_err(|_| OperationError)?;
    Ok((value.ciphertext, value.nonce))
}
pub(crate) fn decrypt_job_payload(
    key: &WorkspaceKey,
    lease: &JobLease,
) -> Result<ExportJobPayload, OperationError> {
    let envelope = EncryptedEnvelope {
        algorithm: "AES-256-GCM".into(),
        key_version: KeyVersion(
            u32::try_from(lease.payload_key_version).map_err(|_| OperationError)?,
        ),
        nonce: lease
            .payload_nonce
            .as_slice()
            .try_into()
            .map_err(|_| OperationError)?,
        ciphertext: lease.payload_ciphertext.clone(),
    };
    let cipher = EnvelopeCipher::new(
        envelope.key_version,
        key.derive_for(KeyPurpose::Content, envelope.key_version)
            .map_err(|_| OperationError)?,
    );
    let clear = cipher
        .decrypt(
            format!("workspace:{}:job:{}:payload", lease.workspace_id, lease.id).as_bytes(),
            &envelope,
        )
        .map_err(|_| OperationError)?;
    serde_json::from_slice(clear.expose()).map_err(|_| OperationError)
}

pub struct JobRunner {
    owner: Uuid,
    lease_seconds: i32,
}
impl JobRunner {
    pub const fn new(owner: Uuid) -> Self {
        Self {
            owner,
            lease_seconds: 60,
        }
    }
    pub async fn run_once(&self, state: &AppState) -> Result<bool, OperationError> {
        let runtime = state.runtime().ok_or(OperationError)?;
        let Some(lease) =
            JobRepository::claim(runtime.store.pool(), self.owner, self.lease_seconds)
                .await
                .map_err(|_| OperationError)?
        else {
            return Ok(false);
        };
        let key = match load_workspace_key(state, lease.workspace_id).await {
            Ok(key) => key,
            Err(_) => {
                runtime.operation_failed("job_key", lease.id);
                self.fail_key_lease(state, &lease).await?;
                return Ok(true);
            }
        };
        let payload = match decrypt_job_payload(&key, &lease) {
            Ok(payload) => payload,
            Err(_) => {
                runtime.operation_failed("job_payload", lease.id);
                self.fail_opaque_lease(state, &key, &lease).await?;
                return Ok(true);
            }
        };
        if !lease.should_execute() {
            runtime.operation_failed("job_exhausted", lease.id);
            self.fail_lease(state, &key, &lease, &payload).await?;
            return Ok(true);
        }
        match self.execute_export(state, &key, &lease, &payload).await {
            Ok(()) => Ok(true),
            Err(_) => {
                runtime.operation_failed("job_execute", payload.request_id);
                self.fail_lease(state, &key, &lease, &payload).await?;
                Ok(true)
            }
        }
    }
    pub async fn run_maintenance_once(&self, state: &AppState) -> Result<u64, OperationError> {
        let runtime = state.runtime().ok_or(OperationError)?;
        let objects = runtime.objects.clone().ok_or(OperationError)?;
        let attachment_failed =
            knowledge_store::attachments::cleanup_orphans(runtime.store.pool(), objects.as_ref())
                .await
                .is_err();
        let export_orphan_failed = cleanup_export_orphans(state, objects.as_ref())
            .await
            .is_err();
        let expiry = expire_exports(state, objects.as_ref()).await;
        let purged = super::retention::purge(state, self.owner).await;
        let rotated = super::key_rotation::run(state, self.owner).await;
        for (phase, failed) in [
            ("attachment_orphans", attachment_failed),
            ("export_orphans", export_orphan_failed),
            ("export_expiry", expiry.is_err()),
            ("retention", purged.is_err()),
            ("key_rotation", rotated.is_err()),
        ] {
            if failed {
                runtime.operation_failed(phase, self.owner);
            }
        }
        if attachment_failed
            || export_orphan_failed
            || expiry.is_err()
            || purged.is_err()
            || rotated.is_err()
        {
            Err(OperationError)
        } else {
            Ok(expiry? + purged? + rotated?)
        }
    }
    async fn fail_key_lease(
        &self,
        state: &AppState,
        lease: &JobLease,
    ) -> Result<(), OperationError> {
        let runtime = state.runtime().ok_or(OperationError)?;
        let mut tx = runtime
            .store
            .pool()
            .begin()
            .await
            .map_err(|_| OperationError)?;
        lock_workspace_for_audit(&mut tx, lease.workspace_id)
            .await
            .map_err(|_| OperationError)?;
        if !JobRepository::retry(&mut tx, lease, backoff(lease.attempts))
            .await
            .map_err(|_| OperationError)?
        {
            return Ok(());
        }
        sqlx::query(
            "UPDATE exports
                SET status=$3,updated_at=now()
              WHERE workspace_id=$1 AND job_id=$2 AND status<>'ready'",
        )
        .bind(lease.workspace_id)
        .bind(lease.id)
        .bind(if lease.attempts >= 5 {
            "failed"
        } else {
            "queued"
        })
        .execute(&mut *tx)
        .await
        .map_err(|_| OperationError)?;
        tx.commit().await.map_err(|_| OperationError)
    }
    async fn fail_opaque_lease(
        &self,
        state: &AppState,
        key: &WorkspaceKey,
        lease: &JobLease,
    ) -> Result<(), OperationError> {
        let runtime = state.runtime().ok_or(OperationError)?;
        let mut tx = runtime
            .store
            .pool()
            .begin()
            .await
            .map_err(|_| OperationError)?;
        lock_workspace_for_audit(&mut tx, lease.workspace_id)
            .await
            .map_err(|_| OperationError)?;
        let export: Option<(Uuid, Uuid)> = sqlx::query_as(
            "SELECT id,requested_by FROM exports WHERE workspace_id=$1 AND job_id=$2",
        )
        .bind(lease.workspace_id)
        .bind(lease.id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| OperationError)?;
        if !JobRepository::retry(&mut tx, lease, backoff(lease.attempts))
            .await
            .map_err(|_| OperationError)?
        {
            return Ok(());
        }
        if let Some((export_id, actor)) = export {
            let terminal = lease.attempts >= 5;
            sqlx::query("UPDATE exports SET status=$2,updated_at=now() WHERE workspace_id=$1 AND id=$3 AND status<>'ready'")
                .bind(lease.workspace_id).bind(if terminal { "failed" } else { "queued" }).bind(export_id)
                .execute(&mut *tx).await.map_err(|_| OperationError)?;
            append_audit_event(
                &mut tx,
                key,
                AuditEvent {
                    workspace_id: lease.workspace_id,
                    actor_user_id: Some(actor),
                    session_id: None,
                    event_type: if terminal {
                        "export.failed"
                    } else {
                        "export.retry_scheduled"
                    },
                    target_type: "export",
                    target_id: Some(export_id),
                    request_id: Uuid::now_v7(),
                    metadata: json!({"attempt":lease.attempts}),
                },
            )
            .await
            .map_err(|_| OperationError)?;
        }
        tx.commit().await.map_err(|_| OperationError)?;
        Ok(())
    }
    async fn fail_lease(
        &self,
        state: &AppState,
        key: &WorkspaceKey,
        lease: &JobLease,
        payload: &ExportJobPayload,
    ) -> Result<(), OperationError> {
        let runtime = state.runtime().ok_or(OperationError)?;
        let mut tx = runtime
            .store
            .pool()
            .begin()
            .await
            .map_err(|_| OperationError)?;
        lock_workspace_for_audit(&mut tx, lease.workspace_id)
            .await
            .map_err(|_| OperationError)?;
        if !JobRepository::retry(&mut tx, lease, backoff(lease.attempts))
            .await
            .map_err(|_| OperationError)?
        {
            return Ok(());
        }
        let terminal = lease.attempts >= 5;
        sqlx::query("UPDATE exports SET status=$2,updated_at=now() WHERE workspace_id=$1 AND id=$3 AND status<>'ready'").bind(lease.workspace_id).bind(if terminal{"failed"}else{"queued"}).bind(payload.export_id).execute(&mut *tx).await.map_err(|_|OperationError)?;
        append_audit_event(
            &mut tx,
            key,
            AuditEvent {
                workspace_id: lease.workspace_id,
                actor_user_id: Some(payload.requester_id),
                session_id: Some(payload.session_id),
                event_type: if terminal {
                    "export.failed"
                } else {
                    "export.retry_scheduled"
                },
                target_type: "export",
                target_id: Some(payload.export_id),
                request_id: payload.request_id,
                metadata: json!({"attempt":lease.attempts}),
            },
        )
        .await
        .map_err(|_| OperationError)?;
        tx.commit().await.map_err(|_| OperationError)?;
        Ok(())
    }
    async fn execute_export(
        &self,
        state: &AppState,
        key: &WorkspaceKey,
        lease: &JobLease,
        payload: &ExportJobPayload,
    ) -> Result<(), OperationError> {
        let runtime = state.runtime().ok_or(OperationError)?;
        let objects = runtime.objects.clone().ok_or(OperationError)?;
        let (plan, pages, attachments) = snapshot(state, key, lease, payload).await?;
        let object = Uuid::now_v7();
        let snapshot_access = SnapshotAccess {
            page_ids: pages.keys().copied().collect(),
            attachment_ids: attachments.keys().copied().collect(),
        };
        let snapshot =
            encrypt_snapshot(key, lease.workspace_id, payload.export_id, &snapshot_access)?;
        let mut tx = runtime
            .store
            .pool()
            .begin()
            .await
            .map_err(|_| OperationError)?;
        lock_workspace_for_audit(&mut tx, lease.workspace_id)
            .await
            .map_err(|_| OperationError)?;
        ensure_fence(&mut tx, lease).await?;
        let generation: i32 =
            sqlx::query_scalar("SELECT current_key_version FROM workspaces WHERE id=$1")
                .bind(lease.workspace_id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|_| OperationError)?;
        if generation != key.version().0 as i32 {
            return Err(OperationError);
        }
        sqlx::query("UPDATE exports SET status='running',object_key=$3,snapshot_ciphertext=$4,snapshot_nonce=$5,snapshot_key_version=$7,plaintext_size=$6,key_version=$7,updated_at=now() WHERE workspace_id=$1 AND id=$2").bind(lease.workspace_id).bind(payload.export_id).bind(object).bind(snapshot.0).bind(snapshot.1.as_slice()).bind(i64::try_from(plan.archive_size()).map_err(|_|OperationError)?).bind(generation).execute(&mut *tx).await.map_err(|_|OperationError)?;
        sqlx::query(
            "INSERT INTO export_staging(object_key,workspace_id,export_id) VALUES($1,$2,$3)",
        )
        .bind(object)
        .bind(lease.workspace_id)
        .bind(payload.export_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| OperationError)?;
        tx.commit().await.map_err(|_| OperationError)?;
        let mut object_connection = tokio::time::timeout(
            Duration::from_secs(30),
            lock_object(runtime.store.pool(), object),
        )
        .await
        .map_err(|_| OperationError)?
        .map_err(|_| OperationError)?;
        let mut upload = tokio::time::timeout(Duration::from_secs(30), objects.start(object))
            .await
            .map_err(|_| OperationError)?
            .map_err(|_| OperationError)?;
        sqlx::query("UPDATE export_staging SET remote_upload_id=$2 WHERE object_key=$1")
            .bind(object)
            .bind(upload.upload_id())
            .execute(&mut *object_connection)
            .await
            .map_err(|_| OperationError)?;
        let mut encryptor = ArchiveEncryptor::with_version(
            key.derive(KeyPurpose::Attachment)
                .map_err(|_| OperationError)?,
            key.version(),
            lease.workspace_id,
            payload.export_id,
            plan.archive_size(),
        );
        write_remote(&mut upload, encryptor.header()).await?;
        let stream_context = ExportStreamContext {
            state,
            key,
            lease,
            payload,
            lease_seconds: self.lease_seconds,
        };
        for piece in plan.pieces() {
            if !JobRepository::renew(runtime.store.pool(), lease, self.lease_seconds)
                .await
                .map_err(|_| OperationError)?
            {
                let _ = upload.abort().await;
                return Err(OperationError);
            }
            match piece {
                TarPiece::Bytes(bytes) => {
                    push_clear(
                        runtime.store.pool(),
                        lease,
                        self.lease_seconds,
                        &mut upload,
                        &mut encryptor,
                        &bytes,
                    )
                    .await?
                }
                TarPiece::Source { id, size } => match id {
                    SourceId::PageMarkdown(id) => {
                        let source = pages.get(&id).ok_or(OperationError)?;
                        let body = load_page_body(&stream_context, source).await?;
                        if body.len() as u64 != size {
                            return Err(OperationError);
                        }
                        push_clear(
                            runtime.store.pool(),
                            lease,
                            self.lease_seconds,
                            &mut upload,
                            &mut encryptor,
                            body.as_bytes(),
                        )
                        .await?
                    }
                    SourceId::Attachment(id) => {
                        stream_attachment(
                            &stream_context,
                            id,
                            size,
                            attachments.get(&id).ok_or(OperationError)?,
                            &mut upload,
                            &mut encryptor,
                        )
                        .await?
                    }
                },
            }
        }
        let footer = encryptor.finish().map_err(|_| OperationError)?;
        write_remote(&mut upload, &footer).await?;
        let ciphertext = encryptor.ciphertext_size();
        tokio::time::timeout(Duration::from_secs(30), upload.finish())
            .await
            .map_err(|_| OperationError)?
            .map_err(|_| OperationError)?;
        let mut tx = runtime
            .store
            .pool()
            .begin()
            .await
            .map_err(|_| OperationError)?;
        lock_workspace_for_audit(&mut tx, lease.workspace_id)
            .await
            .map_err(|_| OperationError)?;
        authorize_payload(&mut tx, payload).await?;
        authorize_snapshot_rows(
            &mut tx,
            lease.workspace_id,
            payload.requester_id,
            &snapshot_access,
        )
        .await?;
        if !JobRepository::finish(&mut tx, lease)
            .await
            .map_err(|_| OperationError)?
        {
            return Err(OperationError);
        }
        sqlx::query("UPDATE exports SET status='ready',ciphertext_size=$3,stream_header=$4,ready_at=now(),expires_at=now()+make_interval(hours=>COALESCE((SELECT export_retention_hours FROM operational_settings WHERE workspace_id=$1),24)),updated_at=now() WHERE workspace_id=$1 AND id=$2 AND object_key=$5").bind(lease.workspace_id).bind(payload.export_id).bind(i64::try_from(ciphertext).map_err(|_|OperationError)?).bind(encryptor.header()).bind(object).execute(&mut *tx).await.map_err(|_|OperationError)?;
        sqlx::query("DELETE FROM export_staging WHERE object_key=$1")
            .bind(object)
            .execute(&mut *tx)
            .await
            .map_err(|_| OperationError)?;
        append_audit_event(
            &mut tx,
            key,
            AuditEvent {
                workspace_id: lease.workspace_id,
                actor_user_id: Some(payload.requester_id),
                session_id: Some(payload.session_id),
                event_type: "export.completed",
                target_type: "export",
                target_id: Some(payload.export_id),
                request_id: payload.request_id,
                metadata: json!({"spaceId":payload.space_id}),
            },
        )
        .await
        .map_err(|_| OperationError)?;
        tx.commit().await.map_err(|_| OperationError)?;
        drop(object_connection);
        Ok(())
    }
}

pub async fn run_scheduler(state: AppState) {
    let runner = JobRunner::new(Uuid::now_v7());
    loop {
        for _ in 0..100 {
            match runner.run_once(&state).await {
                Ok(true) => {}
                Ok(false) => break,
                Err(_) => {
                    if let Some(runtime) = state.runtime() {
                        runtime.operation_failed("scheduler_claim_or_retry", runner.owner);
                    }
                    break;
                }
            }
        }
        let _ = runner.run_maintenance_once(&state).await;
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

async fn cleanup_export_orphans(
    state: &AppState,
    objects: &dyn ObjectStore,
) -> Result<(), OperationError> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let keys:Vec<Uuid>=sqlx::query_scalar("SELECT object_key FROM export_staging WHERE expires_at<now() ORDER BY expires_at LIMIT 100").fetch_all(runtime.store.pool()).await.map_err(|_|OperationError)?;
    for object in keys {
        let Some(mut connection) = try_lock_object(runtime.store.pool(), object)
            .await
            .map_err(|_| OperationError)?
        else {
            continue;
        };
        let row:Option<(Option<String>,bool)>=sqlx::query_as("SELECT s.remote_upload_id,EXISTS(SELECT 1 FROM exports e WHERE e.object_key=s.object_key AND e.status='ready') FROM export_staging s WHERE s.object_key=$1 AND s.expires_at<now()").bind(object).fetch_optional(&mut *connection).await.map_err(|_|OperationError)?;
        let Some((remote, live)) = row else { continue };
        if live {
            continue;
        }
        if let Some(id) = remote {
            tokio::time::timeout(Duration::from_secs(30), objects.abort_upload(object, &id))
                .await
                .map_err(|_| OperationError)?
                .map_err(|_| OperationError)?;
        }
        tokio::time::timeout(Duration::from_secs(30), objects.delete(object))
            .await
            .map_err(|_| OperationError)?
            .map_err(|_| OperationError)?;
        sqlx::query("UPDATE exports SET object_key=NULL WHERE object_key=$1 AND status<>'ready'")
            .bind(object)
            .execute(&mut *connection)
            .await
            .map_err(|_| OperationError)?;
        sqlx::query("DELETE FROM export_staging WHERE object_key=$1")
            .bind(object)
            .execute(&mut *connection)
            .await
            .map_err(|_| OperationError)?;
    }
    Ok(())
}

async fn expire_exports(
    state: &AppState,
    objects: &dyn ObjectStore,
) -> Result<u64, OperationError> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let rows:Vec<(Uuid,Uuid,Uuid,Uuid)>=sqlx::query_as("SELECT e.id,e.workspace_id,e.requested_by,e.object_key FROM exports e JOIN spaces s ON s.workspace_id=e.workspace_id AND s.id=e.space_id WHERE e.status='ready' AND (e.expires_at<=clock_timestamp() OR retention_expired(s.workspace_id,s.archived_at)) ORDER BY e.expires_at LIMIT 16").fetch_all(runtime.store.pool()).await.map_err(|_|OperationError)?;
    let mut count = 0;
    for (export, workspace, actor, object) in rows {
        let Some(_connection) = try_lock_object(runtime.store.pool(), object)
            .await
            .map_err(|_| OperationError)?
        else {
            continue;
        };
        let mut tx = runtime
            .store
            .pool()
            .begin()
            .await
            .map_err(|_| OperationError)?;
        lock_workspace_for_audit(&mut tx, workspace)
            .await
            .map_err(|_| OperationError)?;
        let key = load_workspace_key(state, workspace)
            .await
            .map_err(|_| OperationError)?;
        let eligible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM exports e JOIN spaces s ON s.workspace_id=e.workspace_id AND s.id=e.space_id WHERE e.id=$1 AND e.workspace_id=$2 AND e.object_key=$3 AND e.status='ready' AND (e.expires_at<=clock_timestamp() OR retention_expired(s.workspace_id,s.archived_at)))").bind(export).bind(workspace).bind(object).fetch_one(&mut *tx).await.map_err(|_|OperationError)?;
        if !eligible {
            continue;
        }
        tokio::time::timeout(Duration::from_secs(30), objects.delete(object))
            .await
            .map_err(|_| OperationError)?
            .map_err(|_| OperationError)?;
        let changed=sqlx::query("UPDATE exports SET status='expired',object_key=NULL,updated_at=now() WHERE id=$1 AND workspace_id=$2 AND object_key=$3 AND status='ready'").bind(export).bind(workspace).bind(object).execute(&mut *tx).await.map_err(|_|OperationError)?.rows_affected();
        if changed == 1 {
            append_audit_event(
                &mut tx,
                &key,
                AuditEvent {
                    workspace_id: workspace,
                    actor_user_id: Some(actor),
                    session_id: None,
                    event_type: "export.expired",
                    target_type: "export",
                    target_id: Some(export),
                    request_id: Uuid::now_v7(),
                    metadata: json!({"operation":"retention"}),
                },
            )
            .await
            .map_err(|_| OperationError)?;
            tx.commit().await.map_err(|_| OperationError)?;
            count += 1
        } else {
            tx.rollback().await.map_err(|_| OperationError)?
        }
    }
    Ok(count)
}

#[derive(FromRow)]
struct PageCandidate {
    id: Uuid,
    markdown_size: i64,
    metadata_size: i64,
}
#[derive(FromRow)]
struct PageSource {
    id: Uuid,
    stable_key: Uuid,
    parent_id: Option<Uuid>,
    version: i64,
    revision_id: Uuid,
    title_ciphertext: Vec<u8>,
    title_nonce: Vec<u8>,
    title_key_version: i32,
    properties_ciphertext: Vec<u8>,
    properties_nonce: Vec<u8>,
    properties_key_version: i32,
    markdown_size: i64,
}
#[derive(FromRow)]
struct AttachmentSource {
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
}

fn preflight_page_sizes(
    sizes: &[(u64, usize)],
    limits: ArchiveLimits,
) -> Result<(), OperationError> {
    if sizes.len() > limits.max_pages
        || sizes
            .iter()
            .any(|(_, metadata)| *metadata > limits.max_page_metadata_bytes)
    {
        return Err(OperationError);
    }
    let total = sizes.iter().try_fold(0_u64, |total, (body, metadata)| {
        total
            .checked_add(*body)
            .and_then(|value| value.checked_add(*metadata as u64))
    });
    if total.is_none_or(|total| total > limits.max_archive_bytes) {
        return Err(OperationError);
    }
    Ok(())
}

async fn snapshot(
    state: &AppState,
    key: &WorkspaceKey,
    lease: &JobLease,
    payload: &ExportJobPayload,
) -> Result<
    (
        ArchivePlan,
        HashMap<Uuid, PageSource>,
        HashMap<Uuid, AttachmentSource>,
    ),
    OperationError,
> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let mut tx = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| OperationError)?;
    lock_workspace_for_audit(&mut tx, lease.workspace_id)
        .await
        .map_err(|_| OperationError)?;
    ensure_fence(&mut tx, lease).await?;
    authorize_payload(&mut tx, payload).await?;
    let space:(Vec<u8>,Vec<u8>,i32)=sqlx::query_as("SELECT name_ciphertext,name_nonce,name_key_version FROM spaces WHERE workspace_id=$1 AND id=$2 AND archived_at IS NULL").bind(lease.workspace_id).bind(payload.space_id).fetch_one(&mut *tx).await.map_err(|_|OperationError)?;
    let space_name = decrypt_text(
        key,
        &format!(
            "workspace:{}:space:{}:name",
            lease.workspace_id, payload.space_id
        ),
        space.0,
        space.1,
        space.2,
    )
    .map_err(|_| OperationError)?;
    let candidates: Vec<PageCandidate> = sqlx::query_as(
        "SELECT p.id,(octet_length(r.body_ciphertext)-16)::bigint markdown_size,
                (octet_length(r.title_ciphertext)+octet_length(r.properties_ciphertext)-32)::bigint metadata_size
           FROM pages p
           JOIN page_revisions r
             ON r.workspace_id=p.workspace_id AND r.page_id=p.id AND r.version=p.version
          WHERE p.workspace_id=$1 AND p.space_id=$2 AND p.archived_at IS NULL
          ORDER BY p.id LIMIT 1001",
    )
    .bind(lease.workspace_id)
    .bind(payload.space_id)
    .fetch_all(&mut *tx)
    .await
    .map_err(|_| OperationError)?;
    if candidates.len() > 1000 {
        return Err(OperationError);
    }
    let mut accessible = Vec::new();
    for page in candidates {
        let access = PageRepository::access_context(
            &mut tx,
            lease.workspace_id,
            payload.requester_id,
            page.id,
        )
        .await
        .map_err(|_| OperationError)?;
        if !access.is_some_and(|a| a.can(Capability::ReadPage)) {
            continue;
        }
        let active:bool=sqlx::query_scalar("WITH RECURSIVE a(id,parent_id,archived_at) AS (SELECT id,parent_id,archived_at FROM pages WHERE workspace_id=$1 AND id=$2 UNION ALL SELECT p.id,p.parent_id,p.archived_at FROM pages p JOIN a ON a.parent_id=p.id WHERE p.workspace_id=$1) SELECT NOT EXISTS(SELECT 1 FROM a WHERE archived_at IS NOT NULL)").bind(lease.workspace_id).bind(page.id).fetch_one(&mut *tx).await.map_err(|_|OperationError)?;
        if !active {
            continue;
        }
        accessible.push(page);
    }
    let sizes = accessible
        .iter()
        .map(|page| {
            Ok((
                u64::try_from(page.markdown_size).map_err(|_| OperationError)?,
                usize::try_from(page.metadata_size).map_err(|_| OperationError)?,
            ))
        })
        .collect::<Result<Vec<_>, OperationError>>()?;
    preflight_page_sizes(&sizes, ArchiveLimits::default())?;
    let accessible_ids: Vec<_> = accessible.iter().map(|page| page.id).collect();
    let metadata: Vec<PageSource> = if accessible_ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as(
            "SELECT p.id,p.stable_key,p.parent_id,p.version,r.id revision_id,
                    r.title_ciphertext,r.title_nonce,r.title_key_version,
                    r.properties_ciphertext,r.properties_nonce,r.properties_key_version,
                    (octet_length(r.body_ciphertext)-16)::bigint markdown_size
               FROM pages p
               JOIN page_revisions r
                 ON r.workspace_id=p.workspace_id AND r.page_id=p.id AND r.version=p.version
              WHERE p.workspace_id=$1 AND p.space_id=$2 AND p.id=ANY($3)
              ORDER BY p.id",
        )
        .bind(lease.workspace_id)
        .bind(payload.space_id)
        .bind(&accessible_ids)
        .fetch_all(&mut *tx)
        .await
        .map_err(|_| OperationError)?
    };
    let mut pages = HashMap::new();
    let mut page_inputs = Vec::new();
    for page in metadata {
        let title = decrypt_text(
            key,
            &page_aad(lease.workspace_id, page.id, Some(page.version), "title"),
            page.title_ciphertext.clone(),
            page.title_nonce.clone(),
            page.title_key_version,
        )
        .map_err(|_| OperationError)?;
        let props = decrypt_text(
            key,
            &page_aad(
                lease.workspace_id,
                page.id,
                Some(page.version),
                "properties",
            ),
            page.properties_ciphertext.clone(),
            page.properties_nonce.clone(),
            page.properties_key_version,
        )
        .map_err(|_| OperationError)?;
        let properties = serde_json::from_str(&props).map_err(|_| OperationError)?;
        page_inputs.push(PageInput {
            page_id: page.id,
            stable_key: page.stable_key,
            parent_page_id: page.parent_id.filter(|id| pages.contains_key(id)),
            revision_id: page.revision_id,
            revision: page.version,
            title,
            properties,
            markdown_size: u64::try_from(page.markdown_size).map_err(|_| OperationError)?,
        });
        pages.insert(page.id, page);
    }
    for input in &mut page_inputs {
        let page = pages.get(&input.page_id).ok_or(OperationError)?;
        input.parent_page_id = page.parent_id.filter(|parent| pages.contains_key(parent));
    }
    let ids: Vec<Uuid> = pages.keys().copied().collect();
    let rows: Vec<AttachmentSource> = if ids.is_empty() {
        Vec::new()
    } else {
        sqlx::query_as("SELECT id,page_id,object_key,plaintext_size,ciphertext_size,original_name_ciphertext,original_name_nonce,original_name_key_version,mime_ciphertext,mime_nonce,mime_key_version,key_version FROM attachments WHERE workspace_id=$1 AND page_id=ANY($2) AND archived_at IS NULL ORDER BY id LIMIT 1001").bind(lease.workspace_id).bind(&ids).fetch_all(&mut *tx).await.map_err(|_|OperationError)?
    };
    if rows.len() > 1000 {
        return Err(OperationError);
    }
    let mut attachment_inputs = Vec::new();
    let mut attachments = HashMap::new();
    for row in rows {
        let name = decrypt_attachment_text(
            key,
            &attachment_aad(lease.workspace_id, row.id, "name"),
            row.original_name_ciphertext.clone(),
            row.original_name_nonce.clone(),
            row.original_name_key_version,
        )
        .map_err(|_| OperationError)?;
        let mime = decrypt_attachment_text(
            key,
            &attachment_aad(lease.workspace_id, row.id, "mime"),
            row.mime_ciphertext.clone(),
            row.mime_nonce.clone(),
            row.mime_key_version,
        )
        .map_err(|_| OperationError)?;
        attachment_inputs.push(AttachmentInput {
            attachment_id: row.id,
            page_id: row.page_id,
            original_name: name,
            media_type: mime,
            size: u64::try_from(row.plaintext_size).map_err(|_| OperationError)?,
        });
        attachments.insert(row.id, row);
    }
    tx.commit().await.map_err(|_| OperationError)?;
    let plan = ArchivePlan::new(
        ArchiveInput {
            space_id: payload.space_id,
            space_name,
            pages: page_inputs,
            attachments: attachment_inputs,
        },
        ArchiveLimits::default(),
    )
    .map_err(|_| OperationError)?;
    Ok((plan, pages, attachments))
}

async fn authorize_payload(
    tx: &mut Transaction<'_, Postgres>,
    payload: &ExportJobPayload,
) -> Result<(), OperationError> {
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM sessions s JOIN users u ON u.workspace_id=s.workspace_id AND u.id=s.user_id JOIN workspace_members m ON m.workspace_id=u.workspace_id AND m.user_id=u.id WHERE s.id=$1 AND s.user_id=$2 AND s.revoked_at IS NULL AND s.expires_at>now() AND u.status='active' AND u.deleted_at IS NULL AND m.status='active')").bind(payload.session_id).bind(payload.requester_id).fetch_one(&mut **tx).await.map_err(|_|OperationError)?;
    if !valid {
        return Err(OperationError);
    }
    let access = AccessResolver::for_space(
        UserId::from_uuid(payload.requester_id),
        SpaceId::from_uuid(payload.space_id),
        tx,
    )
    .await
    .map_err(|_| OperationError)?;
    if !access.can(Capability::ManageSpace, ResourceContext::Workspace) {
        return Err(OperationError);
    }
    Ok(())
}
async fn ensure_fence(
    tx: &mut Transaction<'_, Postgres>,
    lease: &JobLease,
) -> Result<(), OperationError> {
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM jobs WHERE id=$1 AND lease_owner=$2 AND attempts=$3 AND status='running' AND lease_expires_at>clock_timestamp())").bind(lease.id).bind(lease.lease_owner).bind(lease.attempts).fetch_one(&mut **tx).await.map_err(|_|OperationError)?;
    if valid { Ok(()) } else { Err(OperationError) }
}
fn encrypt_snapshot(
    key: &WorkspaceKey,
    workspace: Uuid,
    export: Uuid,
    value: &SnapshotAccess,
) -> Result<(Vec<u8>, [u8; 12]), OperationError> {
    let cipher = EnvelopeCipher::new(
        key.version(),
        key.derive(KeyPurpose::Content)
            .map_err(|_| OperationError)?,
    );
    let bytes = serde_json::to_vec(value).map_err(|_| OperationError)?;
    let result = cipher
        .encrypt(
            format!("workspace:{workspace}:export:{export}:snapshot").as_bytes(),
            &bytes,
        )
        .map_err(|_| OperationError)?;
    Ok((result.ciphertext, result.nonce))
}
fn decrypt_snapshot(
    key: &WorkspaceKey,
    workspace: Uuid,
    export: Uuid,
    ciphertext: Vec<u8>,
    nonce: Vec<u8>,
    version: i32,
) -> Result<SnapshotAccess, OperationError> {
    let version = KeyVersion(u32::try_from(version).map_err(|_| OperationError)?);
    let cipher = EnvelopeCipher::new(
        version,
        key.derive_for(KeyPurpose::Content, version)
            .map_err(|_| OperationError)?,
    );
    let value = EncryptedEnvelope {
        algorithm: "AES-256-GCM".into(),
        key_version: version,
        nonce: nonce.as_slice().try_into().map_err(|_| OperationError)?,
        ciphertext,
    };
    let clear = cipher
        .decrypt(
            format!("workspace:{workspace}:export:{export}:snapshot").as_bytes(),
            &value,
        )
        .map_err(|_| OperationError)?;
    serde_json::from_slice(clear.expose()).map_err(|_| OperationError)
}
async fn authorize_snapshot_rows(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    user: Uuid,
    snapshot: &SnapshotAccess,
) -> Result<(), OperationError> {
    for page in &snapshot.page_ids {
        let active:bool=sqlx::query_scalar("WITH RECURSIVE a(id,parent_id,archived_at) AS (SELECT id,parent_id,archived_at FROM pages WHERE workspace_id=$1 AND id=$2 UNION ALL SELECT p.id,p.parent_id,p.archived_at FROM pages p JOIN a ON a.parent_id=p.id WHERE p.workspace_id=$1) SELECT EXISTS(SELECT 1 FROM a) AND NOT EXISTS(SELECT 1 FROM a WHERE archived_at IS NOT NULL)").bind(workspace).bind(page).fetch_one(&mut **tx).await.map_err(|_|OperationError)?;
        let access = PageRepository::access_context(tx, workspace, user, *page)
            .await
            .map_err(|_| OperationError)?;
        if !active || !access.is_some_and(|a| a.can(Capability::ReadPage)) {
            return Err(OperationError);
        }
    }
    for attachment in &snapshot.attachment_ids {
        let page:Option<Uuid>=sqlx::query_scalar("SELECT page_id FROM attachments WHERE workspace_id=$1 AND id=$2 AND archived_at IS NULL").bind(workspace).bind(attachment).fetch_optional(&mut **tx).await.map_err(|_|OperationError)?;
        if page.is_none() {
            return Err(OperationError);
        }
    }
    Ok(())
}
pub(crate) async fn authorize_export_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    key: &WorkspaceKey,
    current: CurrentUser,
    export: Uuid,
    space: Uuid,
) -> Result<(), ApiProblem> {
    let access = AccessResolver::for_space(
        UserId::from_uuid(current.user_id),
        SpaceId::from_uuid(space),
        tx,
    )
    .await
    .map_err(|_| ApiProblem::internal())?;
    if !access.can(Capability::ManageSpace, ResourceContext::Workspace) {
        return Err(ApiProblem::not_found());
    }
    let row:Option<(Vec<u8>,Vec<u8>,i32)>=sqlx::query_as("SELECT snapshot_ciphertext,snapshot_nonce,snapshot_key_version FROM exports WHERE workspace_id=$1 AND id=$2").bind(current.workspace_id).bind(export).fetch_optional(&mut **tx).await.map_err(|_|ApiProblem::internal())?;
    let (cipher, nonce, version) = row.ok_or_else(ApiProblem::not_found)?;
    let snapshot = decrypt_snapshot(key, current.workspace_id, export, cipher, nonce, version)
        .map_err(|_| ApiProblem::internal())?;
    authorize_snapshot_rows(tx, current.workspace_id, current.user_id, &snapshot)
        .await
        .map_err(|_| ApiProblem::not_found())
}

async fn push_clear(
    pool: &sqlx::PgPool,
    lease: &JobLease,
    lease_seconds: i32,
    upload: &mut Box<dyn ObjectUpload>,
    encryptor: &mut ArchiveEncryptor,
    clear: &[u8],
) -> Result<(), OperationError> {
    for part in clear.chunks(STREAM_CHUNK_SIZE) {
        if !JobRepository::renew(pool, lease, lease_seconds)
            .await
            .map_err(|_| OperationError)?
        {
            return Err(OperationError);
        }
        for frame in encryptor.push(part).map_err(|_| OperationError)? {
            write_remote(upload, &frame).await?
        }
    }
    Ok(())
}
async fn write_remote(
    upload: &mut Box<dyn ObjectUpload>,
    bytes: &[u8],
) -> Result<(), OperationError> {
    tokio::time::timeout(
        Duration::from_secs(30),
        upload.write(Bytes::copy_from_slice(bytes)),
    )
    .await
    .map_err(|_| OperationError)?
    .map_err(|_| OperationError)
}
struct ExportStreamContext<'a> {
    state: &'a AppState,
    key: &'a WorkspaceKey,
    lease: &'a JobLease,
    payload: &'a ExportJobPayload,
    lease_seconds: i32,
}
async fn load_page_body(
    context: &ExportStreamContext<'_>,
    source: &PageSource,
) -> Result<String, OperationError> {
    let runtime = context.state.runtime().ok_or(OperationError)?;
    let mut tx = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| OperationError)?;
    authorize_payload(&mut tx, context.payload).await?;
    let access = PageRepository::access_context(
        &mut tx,
        context.lease.workspace_id,
        context.payload.requester_id,
        source.id,
    )
    .await
    .map_err(|_| OperationError)?;
    let active: bool = sqlx::query_scalar(
        "WITH RECURSIVE a(id,parent_id,archived_at) AS (
             SELECT id,parent_id,archived_at FROM pages WHERE workspace_id=$1 AND id=$2
             UNION ALL
             SELECT p.id,p.parent_id,p.archived_at FROM pages p JOIN a ON a.parent_id=p.id
              WHERE p.workspace_id=$1
         )
         SELECT EXISTS(SELECT 1 FROM a)
            AND NOT EXISTS(SELECT 1 FROM a WHERE archived_at IS NOT NULL)",
    )
    .bind(context.lease.workspace_id)
    .bind(source.id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| OperationError)?;
    if !active || !access.is_some_and(|value| value.can(Capability::ReadPage)) {
        return Err(OperationError);
    }
    let row: (Vec<u8>, Vec<u8>, i32) = sqlx::query_as(
        "SELECT body_ciphertext,body_nonce,body_key_version
           FROM page_revisions
          WHERE workspace_id=$1 AND page_id=$2 AND id=$3 AND version=$4",
    )
    .bind(context.lease.workspace_id)
    .bind(source.id)
    .bind(source.revision_id)
    .bind(source.version)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| OperationError)?;
    let body = decrypt_text(
        context.key,
        &page_aad(
            context.lease.workspace_id,
            source.id,
            Some(source.version),
            "body",
        ),
        row.0,
        row.1,
        row.2,
    )
    .map_err(|_| OperationError)?;
    tx.commit().await.map_err(|_| OperationError)?;
    Ok(body)
}
async fn stream_attachment(
    context: &ExportStreamContext<'_>,
    id: Uuid,
    size: u64,
    row: &AttachmentSource,
    upload: &mut Box<dyn ObjectUpload>,
    encryptor: &mut ArchiveEncryptor,
) -> Result<(), OperationError> {
    let runtime = context.state.runtime().ok_or(OperationError)?;
    let objects = runtime.objects.clone().ok_or(OperationError)?;
    let mut tx = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| OperationError)?;
    authorize_payload(&mut tx, context.payload).await?;
    let access = PageRepository::access_context(
        &mut tx,
        context.lease.workspace_id,
        context.payload.requester_id,
        row.page_id,
    )
    .await
    .map_err(|_| OperationError)?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachments WHERE workspace_id=$1 AND id=$2 AND archived_at IS NULL)").bind(context.lease.workspace_id).bind(id).fetch_one(&mut *tx).await.map_err(|_|OperationError)?;
    if !active || !access.is_some_and(|a| a.can(Capability::DownloadAttachment)) {
        return Err(OperationError);
    }
    tx.commit().await.map_err(|_| OperationError)?;
    if u64::try_from(row.plaintext_size).map_err(|_| OperationError)? != size {
        return Err(OperationError);
    }
    let stream = AttachmentStream::new(
        context
            .key
            .derive_for(
                KeyPurpose::Attachment,
                KeyVersion(u32::try_from(row.key_version).map_err(|_| OperationError)?),
            )
            .map_err(|_| OperationError)?,
        KeyVersion(u32::try_from(row.key_version).map_err(|_| OperationError)?),
        context.lease.workspace_id.as_bytes(),
        id.as_bytes(),
        size,
    );
    if tokio::time::timeout(Duration::from_secs(30), objects.size(row.object_key))
        .await
        .map_err(|_| OperationError)?
        .map_err(|_| OperationError)?
        != u64::try_from(row.ciphertext_size).map_err(|_| OperationError)?
    {
        return Err(OperationError);
    }
    let footer = tokio::time::timeout(
        Duration::from_secs(30),
        objects.read_range(row.object_key, stream.footer_range()),
    )
    .await
    .map_err(|_| OperationError)?
    .map_err(|_| OperationError)?;
    stream.verify_footer(&footer).map_err(|_| OperationError)?;
    for index in 0..stream.chunk_count() {
        let frame = tokio::time::timeout(
            Duration::from_secs(30),
            objects.read_range(
                row.object_key,
                stream.frame_range(index).map_err(|_| OperationError)?,
            ),
        )
        .await
        .map_err(|_| OperationError)?
        .map_err(|_| OperationError)?;
        let clear = stream
            .decrypt_chunk(index, &frame)
            .map_err(|_| OperationError)?;
        push_clear(
            runtime.store.pool(),
            context.lease,
            context.lease_seconds,
            upload,
            encryptor,
            &clear,
        )
        .await?;
    }
    Ok(())
}
const fn backoff(attempt: i32) -> i32 {
    match attempt {
        ..=1 => 1,
        2 => 5,
        3 => 30,
        _ => 60,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_size_preflight_rejects_oversized_sources_without_allocating_them() {
        let limits = ArchiveLimits::default();
        assert!(
            preflight_page_sizes(&[(limits.max_archive_bytes + 1, 1)], limits).is_err(),
            "oversized bodies must be rejected from lengths alone"
        );
        assert!(
            preflight_page_sizes(&[(1, limits.max_page_metadata_bytes + 1)], limits).is_err(),
            "oversized metadata must be rejected before ciphertext retention or decryption"
        );
    }
}
