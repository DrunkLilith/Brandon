use knowledge_core::ids::{GroupId, UserId};
use sqlx::PgPool;

use crate::StoreError;

pub struct GroupRepository;

impl GroupRepository {
    pub async fn subject_groups(
        pool: &PgPool,
        user_id: UserId,
    ) -> Result<Vec<GroupId>, StoreError> {
        let ids: Vec<uuid::Uuid> = sqlx::query_scalar(
            "SELECT gm.group_id
               FROM group_members gm
               JOIN groups g
                 ON g.workspace_id = gm.workspace_id AND g.id = gm.group_id
              WHERE gm.user_id = $1 AND g.archived_at IS NULL
              ORDER BY gm.group_id",
        )
        .bind(user_id.as_uuid())
        .fetch_all(pool)
        .await?;
        Ok(ids.into_iter().map(GroupId::from_uuid).collect())
    }
}
