//! Versioned DEK rotation. Mutable envelopes and object frames migrate in
//! bounded fenced batches; immutable audit ciphertext retains read-only keys.
use super::OperationError;
use crate::{
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    routes::{
        pages::page_aad,
        users::{decrypt_text, load_workspace_key, load_workspace_key_in_transaction},
    },
    state::AppState,
};
use bytes::Bytes;
use knowledge_crypto::{
    BlindIndexer, EncryptedEnvelope, EnvelopeCipher, KeyPurpose, KeyVersion, Keyring, WorkspaceKey,
    stream::AttachmentStream,
};
use knowledge_store::{attachments::try_lock_object, search::BlindSearchRepository};
use serde_json::json;
use sqlx::{Postgres, Transaction};
use std::time::Duration;
use uuid::Uuid;
use zeroize::Zeroizing;
type Result<T> = std::result::Result<T, OperationError>;
fn fail<T>(_: T) -> OperationError {
    OperationError
}
const LIMIT: i64 = 16;
const OP: Duration = Duration::from_secs(30);

#[derive(Clone, Copy)]
struct Lease {
    workspace: Uuid,
    owner: Uuid,
    generation: i64,
}

async fn fence(tx: &mut Transaction<'_, Postgres>, lease: Lease) -> Result<()> {
    lock_workspace_for_audit(tx, lease.workspace)
        .await
        .map_err(fail)?;
    let changed=sqlx::query("UPDATE key_rotations SET lease_expires_at=clock_timestamp()+interval '10 minutes' WHERE workspace_id=$1 AND lease_owner=$2 AND lease_generation=$3 AND lease_expires_at>clock_timestamp() AND state<>'complete'")
        .bind(lease.workspace).bind(lease.owner).bind(lease.generation).execute(&mut **tx).await.map_err(fail)?.rows_affected();
    if changed != 1 {
        return Err(OperationError);
    }
    Ok(())
}
pub(crate) async fn run(state: &AppState, owner: Uuid) -> Result<u64> {
    let runtime = state.runtime().ok_or(OperationError)?;
    cleanup_objects(state).await?;
    let claimed:Option<(Uuid,i64)>=sqlx::query_as("WITH candidate AS (SELECT workspace_id FROM key_rotations WHERE state<>'complete' AND (lease_expires_at IS NULL OR lease_expires_at<=clock_timestamp()) ORDER BY workspace_id FOR UPDATE SKIP LOCKED LIMIT 1) UPDATE key_rotations r SET lease_owner=$1,lease_generation=lease_generation+1,lease_expires_at=clock_timestamp()+interval '10 minutes' FROM candidate c WHERE r.workspace_id=c.workspace_id RETURNING r.workspace_id,r.lease_generation")
        .bind(owner).fetch_optional(runtime.store.pool()).await.map_err(fail)?;
    let Some((workspace, generation)) = claimed else {
        return Ok(0);
    };
    let lease = Lease {
        workspace,
        owner,
        generation,
    };
    let result = step(state, lease).await;
    sqlx::query("UPDATE key_rotations SET lease_owner=NULL,lease_expires_at=NULL WHERE workspace_id=$1 AND lease_owner=$2 AND lease_generation=$3")
        .bind(workspace).bind(owner).bind(generation).execute(runtime.store.pool()).await.map_err(fail)?;
    result
}

async fn step(state: &AppState, lease: Lease) -> Result<u64> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let key = load_workspace_key(state, lease.workspace)
        .await
        .map_err(fail)?;
    let mut tx = runtime.store.pool().begin().await.map_err(fail)?;
    fence(&mut tx, lease).await?;
    let phase: String = sqlx::query_scalar("SELECT state FROM key_rotations WHERE workspace_id=$1")
        .bind(lease.workspace)
        .fetch_one(&mut *tx)
        .await
        .map_err(fail)?;
    if phase == "migrating" {
        sqlx::query("SELECT set_config('knowledge.rotation_workspace',$1,true)")
            .bind(lease.workspace.to_string())
            .execute(&mut *tx)
            .await
            .map_err(fail)?;
        let changed = migrate_fields(&mut tx, lease.workspace, &key).await?;
        if changed > 0 {
            sqlx::query("UPDATE key_rotations SET processed=processed+$2,updated_at=now() WHERE workspace_id=$1").bind(lease.workspace).bind(changed as i64).execute(&mut *tx).await.map_err(fail)?;
            tx.commit().await.map_err(fail)?;
            return Ok(changed);
        }
        tx.commit().await.map_err(fail)?;
        if migrate_object(state, lease, &key).await? {
            return Ok(1);
        }
        let mut tx = runtime.store.pool().begin().await.map_err(fail)?;
        fence(&mut tx, lease).await?;
        sqlx::query(
            "UPDATE key_rotations SET state='rebuilding',after_page=NULL WHERE workspace_id=$1",
        )
        .bind(lease.workspace)
        .execute(&mut *tx)
        .await
        .map_err(fail)?;
        tx.commit().await.map_err(fail)?;
        return Ok(0);
    }
    let cursor: Option<Uuid> =
        sqlx::query_scalar("SELECT after_page FROM key_rotations WHERE workspace_id=$1")
            .bind(lease.workspace)
            .fetch_one(&mut *tx)
            .await
            .map_err(fail)?;
    type Row = (Uuid, Vec<u8>, Vec<u8>, i32, Vec<u8>, Vec<u8>, i32, bool);
    let rows:Vec<Row>=sqlx::query_as("SELECT id,title_ciphertext,title_nonce,title_key_version,body_ciphertext,body_nonce,body_key_version,archived_at IS NOT NULL FROM pages WHERE workspace_id=$1 AND ($2::uuid IS NULL OR id>$2) ORDER BY id LIMIT 16")
        .bind(lease.workspace).bind(cursor).fetch_all(&mut *tx).await.map_err(fail)?;
    let indexer = BlindIndexer::new(key.derive(KeyPurpose::Search).map_err(fail)?);
    let last = rows.last().map(|r| r.0);
    let count = rows.len() as u64;
    for (id, title, tn, tv, body, bn, bv, archived) in rows {
        if archived {
            BlindSearchRepository::remove_page_tokens(&mut tx, lease.workspace, &[id])
                .await
                .map_err(fail)?;
            continue;
        }
        let title = decrypt_text(
            &key,
            &page_aad(lease.workspace, id, None, "title"),
            title,
            tn,
            tv,
        )
        .map_err(fail)?;
        let body = decrypt_text(
            &key,
            &page_aad(lease.workspace, id, None, "body"),
            body,
            bn,
            bv,
        )
        .map_err(fail)?;
        BlindSearchRepository::replace_page_tokens(
            &mut tx,
            lease.workspace,
            id,
            &indexer.tokens("und", &title),
            &indexer.tokens("und", &body),
        )
        .await
        .map_err(fail)?;
    }
    if let Some(last) = last {
        sqlx::query("UPDATE key_rotations SET after_page=$2,processed=processed+$3,updated_at=now() WHERE workspace_id=$1").bind(lease.workspace).bind(last).bind(count as i64).execute(&mut *tx).await.map_err(fail)?;
    } else {
        sqlx::query(
            "UPDATE key_rotations SET state='complete',updated_at=now() WHERE workspace_id=$1",
        )
        .bind(lease.workspace)
        .execute(&mut *tx)
        .await
        .map_err(fail)?;
        audit(
            &mut tx,
            &key,
            lease.workspace,
            lease.owner,
            "key.rotation_completed",
        )
        .await?;
    }
    tx.commit().await.map_err(fail)?;
    Ok(count)
}

// All identifiers/AAD expressions are static allowlisted literals, never input.
async fn migrate_fields(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    key: &WorkspaceKey,
) -> Result<u64> {
    let fields = [
        (
            "workspaces",
            "name",
            "'workspace:'||id||':name'",
            KeyPurpose::Content,
        ),
        (
            "users",
            "display_name",
            "'workspace:'||workspace_id||':user:'||id||':display-name'",
            KeyPurpose::Content,
        ),
        (
            "users",
            "email",
            "'workspace:'||workspace_id||':user:'||id||':email'",
            KeyPurpose::Content,
        ),
        (
            "sessions",
            "client_metadata",
            "'workspace:'||workspace_id||':session:'||id||':client-metadata'",
            KeyPurpose::Content,
        ),
        (
            "invitations",
            "email",
            "'workspace:'||workspace_id||':invitation:'||id||':email'",
            KeyPurpose::Content,
        ),
        (
            "invitation_idempotency",
            "token",
            "'workspace:'||workspace_id||':invitation-idempotency:'||actor_user_id||':'||idempotency_key||':token'",
            KeyPurpose::Content,
        ),
        (
            "groups",
            "name",
            "'workspace:'||workspace_id||':group:'||id||':name'",
            KeyPurpose::Content,
        ),
        (
            "groups",
            "description",
            "'workspace:'||workspace_id||':group:'||id||':description'",
            KeyPurpose::Content,
        ),
        (
            "spaces",
            "name",
            "'workspace:'||workspace_id||':space:'||id||':name'",
            KeyPurpose::Content,
        ),
        (
            "spaces",
            "description",
            "'workspace:'||workspace_id||':space:'||id||':description'",
            KeyPurpose::Content,
        ),
        (
            "pages",
            "title",
            "'workspace:'||workspace_id||':page:'||id||':current:title'",
            KeyPurpose::Content,
        ),
        (
            "pages",
            "body",
            "'workspace:'||workspace_id||':page:'||id||':current:body'",
            KeyPurpose::Content,
        ),
        (
            "pages",
            "properties",
            "'workspace:'||workspace_id||':page:'||id||':current:properties'",
            KeyPurpose::Content,
        ),
        (
            "page_revisions",
            "title",
            "'workspace:'||workspace_id||':page:'||page_id||':revision:'||version||':title'",
            KeyPurpose::Content,
        ),
        (
            "page_revisions",
            "body",
            "'workspace:'||workspace_id||':page:'||page_id||':revision:'||version||':body'",
            KeyPurpose::Content,
        ),
        (
            "page_revisions",
            "properties",
            "'workspace:'||workspace_id||':page:'||page_id||':revision:'||version||':properties'",
            KeyPurpose::Content,
        ),
        (
            "page_revisions",
            "change_reason",
            "'workspace:'||workspace_id||':page:'||page_id||':revision:'||version||':change-reason'",
            KeyPurpose::Content,
        ),
        (
            "attachments",
            "original_name",
            "'knowledge/attachment/v1/'||workspace_id||'/'||id||'/name'",
            KeyPurpose::Attachment,
        ),
        (
            "attachments",
            "mime",
            "'knowledge/attachment/v1/'||workspace_id||'/'||id||'/mime'",
            KeyPurpose::Attachment,
        ),
        (
            "jobs",
            "payload",
            "'workspace:'||workspace_id||':job:'||id||':payload'",
            KeyPurpose::Content,
        ),
        (
            "exports",
            "snapshot",
            "'workspace:'||workspace_id||':export:'||id||':snapshot'",
            KeyPurpose::Content,
        ),
    ];
    let mut count = 0;
    for (table, field, aad, purpose) in fields {
        let workspace_column = if table == "workspaces" {
            "id"
        } else {
            "workspace_id"
        };
        let query = format!(
            "SELECT ctid::text,{field}_ciphertext,{field}_nonce,{field}_key_version,{aad} FROM {table} WHERE {workspace_column}=$1 AND {field}_key_version<>$2 ORDER BY ctid LIMIT $3 FOR UPDATE"
        );
        type FieldEnvelope = (String, Vec<u8>, Vec<u8>, i32, String);
        let rows: Vec<FieldEnvelope> = sqlx::query_as(sqlx::AssertSqlSafe(query))
            .bind(workspace)
            .bind(key.version().0 as i32)
            .bind(LIMIT - count)
            .fetch_all(&mut **tx)
            .await
            .map_err(fail)?;
        for (row, ciphertext, nonce, version, aad) in rows {
            let old = EncryptedEnvelope {
                algorithm: "AES-256-GCM".into(),
                key_version: KeyVersion(version as u32),
                nonce: nonce.as_slice().try_into().map_err(fail)?,
                ciphertext,
            };
            let clear = EnvelopeCipher::new(
                old.key_version,
                key.derive_for(purpose, old.key_version).map_err(fail)?,
            )
            .decrypt(aad.as_bytes(), &old)
            .map_err(fail)?;
            let encrypted = EnvelopeCipher::new(key.version(), key.derive(purpose).map_err(fail)?)
                .encrypt(aad.as_bytes(), clear.expose())
                .map_err(fail)?;
            let update = format!(
                "UPDATE {table} SET {field}_ciphertext=$2,{field}_nonce=$3,{field}_key_version=$4 WHERE ctid=$1::text::tid"
            );
            sqlx::query(sqlx::AssertSqlSafe(update))
                .bind(row)
                .bind(encrypted.ciphertext)
                .bind(encrypted.nonce.as_slice())
                .bind(key.version().0 as i32)
                .execute(&mut **tx)
                .await
                .map_err(fail)?;
            count += 1;
        }
        if count == LIMIT {
            break;
        }
    }
    Ok(count as u64)
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    key: &WorkspaceKey,
    workspace: Uuid,
    request: Uuid,
    event: &str,
) -> Result<()> {
    append_audit_event(
        tx,
        key,
        AuditEvent {
            workspace_id: workspace,
            actor_user_id: None,
            session_id: None,
            event_type: event,
            target_type: "workspace",
            target_id: Some(workspace),
            request_id: request,
            metadata: json!({"operation":"key_rotation"}),
        },
    )
    .await
    .map_err(fail)
}

async fn migrate_object(state: &AppState, lease: Lease, key: &WorkspaceKey) -> Result<bool> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let objects = runtime.objects.as_ref().ok_or(OperationError)?;
    let row:Option<(Uuid,Uuid,i64,i32,String)>=sqlx::query_as("SELECT id,object_key,plaintext_size,key_version,'attachment'::text FROM attachments WHERE workspace_id=$1 AND key_version<>$2 UNION ALL SELECT id,object_key,plaintext_size,key_version,'export'::text FROM exports WHERE workspace_id=$1 AND key_version<>$2 AND status='ready' AND object_key IS NOT NULL LIMIT 1")
        .bind(lease.workspace).bind(key.version().0 as i32).fetch_optional(runtime.store.pool()).await.map_err(fail)?;
    let Some((id, old_object, size, version, kind)) = row else {
        return Ok(false);
    };
    let Some(_old_lock) = try_lock_object(runtime.store.pool(), old_object)
        .await
        .map_err(fail)?
    else {
        return Ok(true);
    };
    let new_object = Uuid::now_v7();
    let Some(_new_lock) = try_lock_object(runtime.store.pool(), new_object)
        .await
        .map_err(fail)?
    else {
        return Err(OperationError);
    };
    let mut tx = runtime.store.pool().begin().await.map_err(fail)?;
    fence(&mut tx, lease).await?;
    sqlx::query("INSERT INTO key_rotation_objects(object_key,workspace_id,entity_id,kind,old_object_key) VALUES($1,$2,$3,$4,$5)")
        .bind(new_object).bind(lease.workspace).bind(id).bind(&kind).bind(old_object).execute(&mut *tx).await.map_err(fail)?;
    tx.commit().await.map_err(fail)?;
    let mut upload = tokio::time::timeout(OP, objects.start(new_object))
        .await
        .map_err(fail)?
        .map_err(fail)?;
    sqlx::query("UPDATE key_rotation_objects SET remote_upload_id=$2 WHERE object_key=$1")
        .bind(new_object)
        .bind(upload.upload_id())
        .execute(runtime.store.pool())
        .await
        .map_err(fail)?;
    let version = KeyVersion(u32::try_from(version).map_err(fail)?);
    let size = u64::try_from(size).map_err(fail)?;
    let constructor = if kind == "attachment" {
        AttachmentStream::new
    } else {
        AttachmentStream::new_export
    };
    let old = constructor(
        key.derive_for(KeyPurpose::Attachment, version)
            .map_err(fail)?,
        version,
        lease.workspace.as_bytes(),
        id.as_bytes(),
        size,
    );
    let new = constructor(
        key.derive(KeyPurpose::Attachment).map_err(fail)?,
        key.version(),
        lease.workspace.as_bytes(),
        id.as_bytes(),
        size,
    );
    let copied = tokio::time::timeout(Duration::from_secs(300), async {
        if objects.size(old_object).await.map_err(fail)? != old.ciphertext_size() {
            return Err(OperationError);
        }
        if objects
            .read_range(old_object, 0..old.header().len() as u64)
            .await
            .map_err(fail)?
            .as_ref()
            != old.header()
        {
            return Err(OperationError);
        }
        old.verify_footer(
            &objects
                .read_range(old_object, old.footer_range())
                .await
                .map_err(fail)?,
        )
        .map_err(fail)?;
        upload
            .write(Bytes::copy_from_slice(new.header()))
            .await
            .map_err(fail)?;
        for index in 0..old.chunk_count() {
            let frame = objects
                .read_range(old_object, old.frame_range(index).map_err(fail)?)
                .await
                .map_err(fail)?;
            let clear = Zeroizing::new(old.decrypt_chunk(index, &frame).map_err(fail)?);
            upload
                .write(Bytes::from(new.encrypt_chunk(index, &clear).map_err(fail)?))
                .await
                .map_err(fail)?;
        }
        upload
            .write(Bytes::from(new.encrypt_footer().map_err(fail)?))
            .await
            .map_err(fail)?;
        Ok::<(), OperationError>(())
    })
    .await;
    if !matches!(copied, Ok(Ok(()))) {
        let _ = tokio::time::timeout(OP, upload.abort()).await;
        return Err(OperationError);
    }
    tokio::time::timeout(OP, upload.finish())
        .await
        .map_err(fail)?
        .map_err(fail)?;
    let mut tx = runtime.store.pool().begin().await.map_err(fail)?;
    fence(&mut tx, lease).await?;
    let table = if kind == "attachment" {
        "attachments"
    } else {
        "exports"
    };
    let update = format!(
        "UPDATE {table} SET object_key=$3,key_version=$4 WHERE workspace_id=$1 AND id=$2 AND object_key=$5"
    );
    let changed = sqlx::query(sqlx::AssertSqlSafe(update))
        .bind(lease.workspace)
        .bind(id)
        .bind(new_object)
        .bind(key.version().0 as i32)
        .bind(old_object)
        .execute(&mut *tx)
        .await
        .map_err(fail)?
        .rows_affected();
    if changed != 1 {
        return Err(OperationError);
    }
    if kind == "attachment" {
        sqlx::query("UPDATE attachment_upload_requests SET object_key=$3 WHERE workspace_id=$1 AND attachment_id=$2").bind(lease.workspace).bind(id).bind(new_object).execute(&mut *tx).await.map_err(fail)?;
    }
    sqlx::query(
        "UPDATE key_rotation_objects SET published=true,expires_at=now() WHERE object_key=$1",
    )
    .bind(new_object)
    .execute(&mut *tx)
    .await
    .map_err(fail)?;
    tx.commit().await.map_err(fail)?;
    tokio::time::timeout(OP, objects.delete(old_object))
        .await
        .map_err(fail)?
        .map_err(fail)?;
    sqlx::query("DELETE FROM key_rotation_objects WHERE object_key=$1")
        .bind(new_object)
        .execute(runtime.store.pool())
        .await
        .map_err(fail)?;
    Ok(true)
}

async fn cleanup_objects(state: &AppState) -> Result<()> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let objects = runtime.objects.as_ref().ok_or(OperationError)?;
    let rows:Vec<(Uuid,Uuid,Option<String>,bool)>=sqlx::query_as("SELECT object_key,old_object_key,remote_upload_id,published FROM key_rotation_objects WHERE expires_at<clock_timestamp() ORDER BY object_key LIMIT 4").fetch_all(runtime.store.pool()).await.map_err(fail)?;
    for (object, old, remote, published) in rows {
        let target = if published { old } else { object };
        let Some(_lock) = try_lock_object(runtime.store.pool(), target)
            .await
            .map_err(fail)?
        else {
            continue;
        };
        let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachments WHERE object_key=$1 UNION ALL SELECT 1 FROM exports WHERE object_key=$1)").bind(target).fetch_one(runtime.store.pool()).await.map_err(fail)?;
        if live {
            continue;
        }
        if !published && let Some(remote) = remote {
            tokio::time::timeout(OP, objects.abort_upload(object, &remote))
                .await
                .map_err(fail)?
                .map_err(fail)?;
        }
        tokio::time::timeout(OP, objects.delete(target))
            .await
            .map_err(fail)?
            .map_err(fail)?;
        sqlx::query("DELETE FROM key_rotation_objects WHERE object_key=$1")
            .bind(object)
            .execute(runtime.store.pool())
            .await
            .map_err(fail)?;
    }
    Ok(())
}

/// Each committed row is its own durable rewrap checkpoint. Both configured
/// master generations are required until this returns zero remaining records.
pub async fn rewrap_master_once(state: &AppState) -> Result<u64> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let rows:Vec<(Uuid,i32)>=sqlx::query_as("SELECT workspace_id,version FROM workspace_key_generations WHERE master_version<>$1 ORDER BY workspace_id,version LIMIT 16")
        .bind(runtime.master_key.version().0 as i32).fetch_all(runtime.store.pool()).await.map_err(fail)?;
    let mut count = 0;
    for (workspace, version) in rows {
        let mut tx = runtime.store.pool().begin().await.map_err(fail)?;
        lock_workspace_for_audit(&mut tx, workspace)
            .await
            .map_err(fail)?;
        let row:(Vec<u8>,Vec<u8>,i32)=sqlx::query_as("SELECT ciphertext,nonce,master_version FROM workspace_key_generations WHERE workspace_id=$1 AND version=$2 FOR UPDATE").bind(workspace).bind(version).fetch_one(&mut *tx).await.map_err(fail)?;
        if row.2 == runtime.master_key.version().0 as i32 {
            continue;
        }
        let wrapped = EncryptedEnvelope {
            algorithm: "AES-256-GCM".into(),
            key_version: KeyVersion(row.2 as u32),
            nonce: row.1.as_slice().try_into().map_err(fail)?,
            ciphertext: row.0,
        };
        let clear = Keyring::unwrap(&runtime.master_key, &wrapped).map_err(fail)?;
        let wrapped = Keyring::wrap(&runtime.master_key, &clear, runtime.master_key.version())
            .map_err(fail)?;
        sqlx::query("UPDATE workspace_key_generations SET ciphertext=$3,nonce=$4,master_version=$5 WHERE workspace_id=$1 AND version=$2").bind(workspace).bind(version).bind(&wrapped.ciphertext).bind(wrapped.nonce.as_slice()).bind(wrapped.key_version.0 as i32).execute(&mut *tx).await.map_err(fail)?;
        sqlx::query("UPDATE workspaces SET wrapped_key_ciphertext=$3,wrapped_key_nonce=$4,wrapped_key_version=$5 WHERE id=$1 AND current_key_version=$2").bind(workspace).bind(version).bind(&wrapped.ciphertext).bind(wrapped.nonce.as_slice()).bind(wrapped.key_version.0 as i32).execute(&mut *tx).await.map_err(fail)?;
        let key = load_workspace_key_in_transaction(state, &mut tx, workspace)
            .await
            .map_err(fail)?;
        audit(
            &mut tx,
            &key,
            workspace,
            Uuid::now_v7(),
            "key.master_rewrapped",
        )
        .await?;
        tx.commit().await.map_err(fail)?;
        count += 1;
    }
    Ok(count)
}
