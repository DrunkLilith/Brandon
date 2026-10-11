//! Bounded, restartable final retention. Every phase is independently retryable.
use super::OperationError;
use crate::{
    audit::{AuditEvent, append_audit_event, lock_workspace_for_audit},
    routes::users::load_workspace_key,
    state::AppState,
};
use knowledge_store::attachments::try_lock_object;
use serde_json::json;
use sqlx::{Postgres, Transaction};
use std::time::Duration;
use uuid::Uuid;

pub(super) async fn purge(state: &AppState, owner: Uuid) -> Result<u64, OperationError> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let workspaces: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM workspaces ORDER BY id LIMIT 16")
            .fetch_all(runtime.store.pool())
            .await
            .map_err(|_| OperationError)?;
    let mut count = 0;
    for workspace in workspaces {
        let claimed = sqlx::query("INSERT INTO maintenance_leases(workspace_id,owner,expires_at) VALUES($1,$2,clock_timestamp()+interval '60 seconds') ON CONFLICT(workspace_id) DO UPDATE SET owner=$2,expires_at=clock_timestamp()+interval '60 seconds' WHERE maintenance_leases.expires_at<=clock_timestamp()")
            .bind(workspace).bind(owner).execute(runtime.store.pool()).await.map_err(|_|OperationError)?.rows_affected()==1;
        if !claimed {
            continue;
        }
        let result = purge_workspace(state, workspace, owner).await;
        // Failed/crashed workers leave a reclaimable lease. Never release a
        // successor's lease if ownership changed after a long external call.
        sqlx::query("DELETE FROM maintenance_leases WHERE workspace_id=$1 AND owner=$2")
            .bind(workspace)
            .bind(owner)
            .execute(runtime.store.pool())
            .await
            .map_err(|_| OperationError)?;
        count += result?;
    }
    Ok(count)
}

async fn fence(
    tx: &mut Transaction<'_, Postgres>,
    workspace: Uuid,
    owner: Uuid,
) -> Result<(), OperationError> {
    lock_workspace_for_audit(tx, workspace)
        .await
        .map_err(|_| OperationError)?;
    let renewed=sqlx::query("UPDATE maintenance_leases SET expires_at=clock_timestamp()+interval '60 seconds' WHERE workspace_id=$1 AND owner=$2 AND expires_at>clock_timestamp()")
        .bind(workspace).bind(owner).execute(&mut **tx).await.map_err(|_|OperationError)?.rows_affected();
    if renewed == 1 {
        Ok(())
    } else {
        Err(OperationError)
    }
}

async fn purge_workspace(
    state: &AppState,
    workspace: Uuid,
    owner: Uuid,
) -> Result<u64, OperationError> {
    let runtime = state.runtime().ok_or(OperationError)?;
    let objects = runtime.objects.as_ref().ok_or(OperationError)?;
    let key = load_workspace_key(state, workspace)
        .await
        .map_err(|_| OperationError)?;
    let mut count = 0;
    let attachments:Vec<(Uuid,Uuid)>=sqlx::query_as("SELECT a.id,a.object_key FROM attachments a JOIN pages p ON p.workspace_id=a.workspace_id AND p.id=a.page_id JOIN spaces s ON s.workspace_id=p.workspace_id AND s.id=p.space_id WHERE a.workspace_id=$1 AND (retention_expired(a.workspace_id,a.archived_at) OR retention_expired(p.workspace_id,p.archived_at) OR retention_expired(s.workspace_id,s.archived_at) OR EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=p.workspace_id AND m.object_type='page' AND m.object_id=p.id)) ORDER BY a.id LIMIT 16")
        .bind(workspace).fetch_all(runtime.store.pool()).await.map_err(|_|OperationError)?;
    for (id, object) in attachments {
        let Some(_object_lock) = try_lock_object(runtime.store.pool(), object)
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
        fence(&mut tx, workspace, owner).await?;
        let eligible:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM attachments a JOIN pages p ON p.workspace_id=a.workspace_id AND p.id=a.page_id JOIN spaces s ON s.workspace_id=p.workspace_id AND s.id=p.space_id WHERE a.workspace_id=$1 AND a.id=$2 AND a.object_key=$3 AND (retention_expired(a.workspace_id,a.archived_at) OR retention_expired(p.workspace_id,p.archived_at) OR retention_expired(s.workspace_id,s.archived_at) OR EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=p.workspace_id AND m.object_type='page' AND m.object_id=p.id)))")
            .bind(workspace).bind(id).bind(object).fetch_one(&mut *tx).await.map_err(|_|OperationError)?;
        if !eligible {
            continue;
        }
        sqlx::query("INSERT INTO retention_purge_marks(workspace_id,object_type,object_id,archived_at) SELECT p.workspace_id,'page',p.id,COALESCE(p.archived_at,s.archived_at) FROM attachments a JOIN pages p ON p.workspace_id=a.workspace_id AND p.id=a.page_id JOIN spaces s ON s.workspace_id=p.workspace_id AND s.id=p.space_id WHERE a.workspace_id=$1 AND a.id=$2 AND (retention_expired(p.workspace_id,p.archived_at) OR retention_expired(s.workspace_id,s.archived_at) OR EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=p.workspace_id AND m.object_type='page' AND m.object_id=p.id)) ON CONFLICT DO NOTHING")
            .bind(workspace).bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
        tokio::time::timeout(Duration::from_secs(30), objects.delete(object))
            .await
            .map_err(|_| OperationError)?
            .map_err(|_| OperationError)?;
        sqlx::query("DELETE FROM attachment_upload_requests WHERE attachment_id=$1")
            .bind(id)
            .execute(&mut *tx)
            .await
            .map_err(|_| OperationError)?;
        sqlx::query("DELETE FROM attachments WHERE id=$1 AND workspace_id=$2")
            .bind(id)
            .bind(workspace)
            .execute(&mut *tx)
            .await
            .map_err(|_| OperationError)?;
        audit(&mut tx, &key, workspace, owner, "attachment", id).await?;
        tx.commit().await.map_err(|_| OperationError)?;
        count += 1;
    }
    let mut tx = runtime
        .store
        .pool()
        .begin()
        .await
        .map_err(|_| OperationError)?;
    fence(&mut tx, workspace, owner).await?;
    let pages:Vec<Uuid>=sqlx::query_scalar("SELECT p.id FROM pages p JOIN spaces s ON s.workspace_id=p.workspace_id AND s.id=p.space_id WHERE p.workspace_id=$1 AND (retention_expired(p.workspace_id,p.archived_at) OR retention_expired(s.workspace_id,s.archived_at) OR EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=p.workspace_id AND m.object_type='page' AND m.object_id=p.id)) AND NOT EXISTS(SELECT 1 FROM pages child WHERE child.parent_id=p.id) AND NOT EXISTS(SELECT 1 FROM attachments a WHERE a.page_id=p.id) AND NOT EXISTS(SELECT 1 FROM attachment_upload_requests r WHERE r.page_id=p.id) ORDER BY p.id LIMIT 16")
        .bind(workspace).fetch_all(&mut *tx).await.map_err(|_|OperationError)?;
    for id in pages {
        sqlx::query("INSERT INTO retention_purge_marks(workspace_id,object_type,object_id,archived_at) SELECT p.workspace_id,'page',p.id,COALESCE(p.archived_at,s.archived_at) FROM pages p JOIN spaces s ON s.workspace_id=p.workspace_id AND s.id=p.space_id WHERE p.workspace_id=$1 AND p.id=$2 ON CONFLICT DO NOTHING")
            .bind(workspace).bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
        for query in [
            "DELETE FROM page_revisions WHERE id IN (SELECT id FROM page_revisions WHERE page_id=$1 LIMIT 100)",
            "DELETE FROM page_permissions WHERE id IN (SELECT id FROM page_permissions WHERE page_id=$1 LIMIT 100)",
            "DELETE FROM blind_index_tokens WHERE ctid IN (SELECT ctid FROM blind_index_tokens WHERE page_id=$1 LIMIT 100)",
            "DELETE FROM page_links WHERE ctid IN (SELECT ctid FROM page_links WHERE source_page_id=$1 OR target_page_id=$1 LIMIT 100)",
        ] {
            sqlx::query(query)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|_| OperationError)?;
        }
        let removed=sqlx::query("DELETE FROM pages p WHERE id=$1 AND NOT EXISTS(SELECT 1 FROM page_revisions r WHERE r.page_id=p.id) AND NOT EXISTS(SELECT 1 FROM page_permissions r WHERE r.page_id=p.id) AND NOT EXISTS(SELECT 1 FROM blind_index_tokens r WHERE r.page_id=p.id) AND NOT EXISTS(SELECT 1 FROM page_links r WHERE r.source_page_id=p.id OR r.target_page_id=p.id)")
            .bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?.rows_affected();
        if removed == 1 {
            audit(&mut tx, &key, workspace, owner, "page", id).await?;
            sqlx::query("DELETE FROM retention_purge_marks WHERE workspace_id=$1 AND object_type='page' AND object_id=$2")
                .bind(workspace).bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
            count += 1;
        }
    }
    let groups:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM groups WHERE workspace_id=$1 AND (retention_expired(workspace_id,archived_at) OR EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=groups.workspace_id AND m.object_type='group' AND m.object_id=groups.id)) ORDER BY id LIMIT 16")
        .bind(workspace).fetch_all(&mut *tx).await.map_err(|_|OperationError)?;
    for id in groups {
        sqlx::query("INSERT INTO retention_purge_marks(workspace_id,object_type,object_id,archived_at) SELECT workspace_id,'group',id,archived_at FROM groups WHERE workspace_id=$1 AND id=$2 ON CONFLICT DO NOTHING")
            .bind(workspace).bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
        for query in [
            "DELETE FROM group_members WHERE ctid IN(SELECT ctid FROM group_members WHERE group_id=$1 LIMIT 100)",
            "DELETE FROM space_grants WHERE id IN(SELECT id FROM space_grants WHERE group_id=$1 LIMIT 100)",
            "DELETE FROM page_permissions WHERE id IN(SELECT id FROM page_permissions WHERE group_id=$1 LIMIT 100)",
        ] {
            sqlx::query(query)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|_| OperationError)?;
        }
        let removed=sqlx::query("DELETE FROM groups g WHERE id=$1 AND NOT EXISTS(SELECT 1 FROM group_members m WHERE m.group_id=g.id) AND NOT EXISTS(SELECT 1 FROM space_grants m WHERE m.group_id=g.id) AND NOT EXISTS(SELECT 1 FROM page_permissions m WHERE m.group_id=g.id)")
            .bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?.rows_affected();
        if removed == 1 {
            audit(&mut tx, &key, workspace, owner, "group", id).await?;
            sqlx::query("DELETE FROM retention_purge_marks WHERE workspace_id=$1 AND object_type='group' AND object_id=$2")
                .bind(workspace).bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
            count += 1;
        }
    }
    let spaces:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM spaces s WHERE workspace_id=$1 AND (retention_expired(workspace_id,archived_at) OR EXISTS(SELECT 1 FROM retention_purge_marks m WHERE m.workspace_id=s.workspace_id AND m.object_type='space' AND m.object_id=s.id)) AND NOT EXISTS(SELECT 1 FROM pages p WHERE p.space_id=s.id) ORDER BY id LIMIT 16")
        .bind(workspace).fetch_all(&mut *tx).await.map_err(|_|OperationError)?;
    for id in spaces {
        sqlx::query("INSERT INTO retention_purge_marks(workspace_id,object_type,object_id,archived_at) SELECT workspace_id,'space',id,archived_at FROM spaces WHERE workspace_id=$1 AND id=$2 ON CONFLICT DO NOTHING")
            .bind(workspace).bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
        sqlx::query("DELETE FROM space_grants WHERE id IN(SELECT id FROM space_grants WHERE space_id=$1 LIMIT 100)").bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
        sqlx::query("DELETE FROM exports e WHERE id IN(SELECT id FROM exports e2 WHERE e2.space_id=$1 AND e2.object_key IS NULL AND NOT EXISTS(SELECT 1 FROM export_staging st WHERE st.export_id=e2.id) LIMIT 100)").bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
        let removed=sqlx::query("DELETE FROM spaces s WHERE id=$1 AND NOT EXISTS(SELECT 1 FROM space_grants g WHERE g.space_id=s.id) AND NOT EXISTS(SELECT 1 FROM exports e WHERE e.space_id=s.id)")
            .bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?.rows_affected();
        if removed == 1 {
            audit(&mut tx, &key, workspace, owner, "space", id).await?;
            sqlx::query("DELETE FROM retention_purge_marks WHERE workspace_id=$1 AND object_type='space' AND object_id=$2")
                .bind(workspace).bind(id).execute(&mut *tx).await.map_err(|_|OperationError)?;
            count += 1;
        }
    }
    tx.commit().await.map_err(|_| OperationError)?;
    Ok(count)
}

async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    key: &knowledge_crypto::WorkspaceKey,
    workspace: Uuid,
    owner: Uuid,
    kind: &str,
    id: Uuid,
) -> Result<(), OperationError> {
    append_audit_event(
        tx,
        key,
        AuditEvent {
            workspace_id: workspace,
            actor_user_id: None,
            session_id: None,
            event_type: "retention.purged",
            target_type: kind,
            target_id: Some(id),
            request_id: owner,
            metadata: json!({"operation":"retention"}),
        },
    )
    .await
    .map_err(|_| OperationError)
}
