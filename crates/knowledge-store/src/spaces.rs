use knowledge_core::{
    ids::{SpaceId, UserId},
    permissions::{AccountStatus, EffectiveAccess, PermissionContext, SpaceRole, WorkspaceRole},
};
use sqlx::{Postgres, Transaction};

use crate::StoreError;

pub struct AccessResolver;

impl AccessResolver {
    pub async fn for_space(
        user_id: UserId,
        space_id: SpaceId,
        transaction: &mut Transaction<'_, Postgres>,
    ) -> Result<EffectiveAccess, StoreError> {
        let identity: Option<(String, String, bool)> = sqlx::query_as(
            "SELECT u.status, wm.role, s.archived_at IS NULL
               FROM users u
               JOIN workspace_members wm
                 ON wm.workspace_id = u.workspace_id AND wm.user_id = u.id
               JOIN spaces s ON s.workspace_id = u.workspace_id
              WHERE u.id = $1 AND s.id = $2 AND u.deleted_at IS NULL
                AND wm.status = 'active'",
        )
        .bind(user_id.as_uuid())
        .bind(space_id.as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        let Some((account_status, workspace_role, active_space)) = identity else {
            return Ok(denied());
        };
        let account_status = parse_account_status(&account_status);
        let workspace_role = parse_workspace_role(&workspace_role);
        if !active_space {
            return Ok(denied());
        }
        let direct_role: Option<String> = sqlx::query_scalar(
            "SELECT role FROM space_grants WHERE space_id = $1 AND user_id = $2",
        )
        .bind(space_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        let group_roles: Vec<String> = sqlx::query_scalar(
            "SELECT sg.role
               FROM space_grants sg
               JOIN groups g ON g.id = sg.group_id AND g.workspace_id = sg.workspace_id
               JOIN group_members gm
                 ON gm.group_id = g.id AND gm.workspace_id = g.workspace_id
              WHERE sg.space_id = $1 AND gm.user_id = $2 AND g.archived_at IS NULL",
        )
        .bind(space_id.as_uuid())
        .bind(user_id.as_uuid())
        .fetch_all(&mut **transaction)
        .await?;
        Ok(PermissionContext::new(
            account_status,
            workspace_role,
            direct_role.as_deref().and_then(parse_space_role),
            group_roles.iter().filter_map(|role| parse_space_role(role)),
            None,
        ))
    }
}

fn denied() -> EffectiveAccess {
    PermissionContext::new(
        AccountStatus::Deleted,
        WorkspaceRole::Member,
        None,
        [],
        None,
    )
}

fn parse_account_status(value: &str) -> AccountStatus {
    match value {
        "active" => AccountStatus::Active,
        "suspended" => AccountStatus::Suspended,
        "invited" => AccountStatus::Invited,
        _ => AccountStatus::Deleted,
    }
}

fn parse_workspace_role(value: &str) -> WorkspaceRole {
    if value == "admin" {
        WorkspaceRole::Admin
    } else {
        WorkspaceRole::Member
    }
}

fn parse_space_role(value: &str) -> Option<SpaceRole> {
    match value {
        "owner" => Some(SpaceRole::Owner),
        "editor" => Some(SpaceRole::Editor),
        "viewer" => Some(SpaceRole::Viewer),
        _ => None,
    }
}
