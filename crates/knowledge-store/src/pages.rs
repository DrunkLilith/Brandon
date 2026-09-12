use knowledge_core::{
    ids::{SpaceId, UserId},
    permissions::{PageAccessContext, PageRole},
};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{AccessResolver, StoreError};

pub struct PageRepository;

impl PageRepository {
    pub async fn access_context(
        transaction: &mut Transaction<'_, Postgres>,
        workspace_id: Uuid,
        user_id: Uuid,
        page_id: Uuid,
    ) -> Result<Option<PageAccessContext>, StoreError> {
        let page: Option<(Uuid, Option<Uuid>)> = sqlx::query_as(
            "WITH RECURSIVE ancestors(id, parent_id, restricted, depth, space_id) AS (
                SELECT id, parent_id, restricted, 0, space_id FROM pages
                 WHERE workspace_id = $1 AND id = $2
                UNION ALL
                SELECT p.id, p.parent_id, p.restricted, a.depth + 1, p.space_id
                  FROM pages p JOIN ancestors a ON a.parent_id = p.id
                 WHERE p.workspace_id = $1
             )
             SELECT space_id,
                    (SELECT id FROM ancestors WHERE restricted ORDER BY depth LIMIT 1)
               FROM ancestors WHERE depth = 0",
        )
        .bind(workspace_id)
        .bind(page_id)
        .fetch_optional(&mut **transaction)
        .await?;
        let Some((space_id, boundary_page_id)) = page else {
            return Ok(None);
        };
        let mut access = AccessResolver::for_space(
            UserId::from_uuid(user_id),
            SpaceId::from_uuid(space_id),
            transaction,
        )
        .await?;
        if let Some(boundary_page_id) = boundary_page_id {
            let roles: Vec<String> = sqlx::query_scalar(
                "SELECT role FROM page_permissions
                  WHERE workspace_id = $1 AND page_id = $2 AND user_id = $3
                 UNION ALL
                 SELECT pp.role FROM page_permissions pp
                 JOIN groups g ON g.workspace_id = pp.workspace_id AND g.id = pp.group_id
                 JOIN group_members gm
                   ON gm.workspace_id = g.workspace_id AND gm.group_id = g.id
                  WHERE pp.workspace_id = $1 AND pp.page_id = $2 AND gm.user_id = $3
                    AND g.archived_at IS NULL",
            )
            .bind(workspace_id)
            .bind(boundary_page_id)
            .bind(user_id)
            .fetch_all(&mut **transaction)
            .await?;
            let page_role = roles.iter().filter_map(|role| parse_page_role(role)).max();
            access = access.with_page_role(page_role);
        }
        Ok(Some(PageAccessContext::new(boundary_page_id, access)))
    }

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

fn parse_page_role(value: &str) -> Option<PageRole> {
    match value {
        "viewer" => Some(PageRole::Viewer),
        "editor" => Some(PageRole::Editor),
        _ => None,
    }
}
