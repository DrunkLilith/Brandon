use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::StoreError;

pub struct PageRepository;

impl PageRepository {
    pub async fn active_parent_exists(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        space_id: Uuid,
        parent_id: Uuid,
    ) -> Result<bool, StoreError> {
        Ok(sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM pages
                 WHERE workspace_id = $1 AND space_id = $2 AND id = $3 AND archived_at IS NULL
            )",
        )
        .bind(workspace_id)
        .bind(space_id)
        .bind(parent_id)
        .fetch_one(&mut **transaction)
        .await?)
    }
}
