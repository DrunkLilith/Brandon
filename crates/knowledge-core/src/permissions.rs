use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AccountStatus {
    Invited,
    Active,
    Suspended,
    Deleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum WorkspaceRole {
    Admin,
    Member,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum SpaceRole {
    Viewer,
    Editor,
    Owner,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum PageRole {
    Viewer,
    Editor,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capability {
    ReadPage,
    ViewHistory,
    DownloadAttachment,
    Search,
    CreatePage,
    EditPage,
    UploadAttachment,
    ManagePagePermissions,
    ManageSpace,
    ManageWorkspace,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceContext {
    Workspace,
    UnrestrictedPage,
    RestrictedPage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionContext {
    account_status: AccountStatus,
    workspace_role: WorkspaceRole,
    direct_space_role: Option<SpaceRole>,
    group_space_roles: Vec<SpaceRole>,
    page_role: Option<PageRole>,
}

/// The effective access snapshot returned by a storage-backed resolver.
pub type EffectiveAccess = PermissionContext;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PageAccessContext {
    boundary_page_id: Option<Uuid>,
    access: EffectiveAccess,
}

impl PageAccessContext {
    #[must_use]
    pub fn new(boundary_page_id: Option<Uuid>, access: EffectiveAccess) -> Self {
        Self {
            boundary_page_id,
            access,
        }
    }

    #[must_use]
    pub fn boundary_page_id(&self) -> Option<Uuid> {
        self.boundary_page_id
    }

    #[must_use]
    pub fn access(&self) -> &EffectiveAccess {
        &self.access
    }

    #[must_use]
    pub fn resource_context(&self) -> ResourceContext {
        if self.boundary_page_id.is_some() {
            ResourceContext::RestrictedPage
        } else {
            ResourceContext::UnrestrictedPage
        }
    }

    #[must_use]
    pub fn can(&self, capability: Capability) -> bool {
        self.access.can(capability, self.resource_context())
    }
}

impl PermissionContext {
    #[must_use]
    pub fn new<I>(
        account_status: AccountStatus,
        workspace_role: WorkspaceRole,
        direct_space_role: Option<SpaceRole>,
        group_space_roles: I,
        page_role: Option<PageRole>,
    ) -> Self
    where
        I: IntoIterator<Item = SpaceRole>,
    {
        Self {
            account_status,
            workspace_role,
            direct_space_role,
            group_space_roles: group_space_roles.into_iter().collect(),
            page_role,
        }
    }

    #[must_use]
    pub fn effective_space_role(&self) -> Option<SpaceRole> {
        self.direct_space_role
            .into_iter()
            .chain(self.group_space_roles.iter().copied())
            .max()
    }

    #[must_use]
    pub fn is_workspace_admin(&self) -> bool {
        self.account_status == AccountStatus::Active && self.workspace_role == WorkspaceRole::Admin
    }

    #[must_use]
    pub fn with_page_role(mut self, page_role: Option<PageRole>) -> Self {
        self.page_role = page_role;
        self
    }

    #[must_use]
    pub fn can(&self, capability: Capability, resource: ResourceContext) -> bool {
        if self.account_status != AccountStatus::Active {
            return false;
        }

        if self.workspace_role == WorkspaceRole::Admin {
            return true;
        }

        if capability == Capability::ManageWorkspace {
            return false;
        }

        let Some(space_role) = self.effective_space_role() else {
            return false;
        };

        let effective_role = match resource {
            ResourceContext::Workspace | ResourceContext::UnrestrictedPage => space_role,
            ResourceContext::RestrictedPage => {
                let Some(page_role) = self.page_role else {
                    return false;
                };
                space_role.min(page_role.into())
            }
        };

        effective_role.allows(capability)
    }
}

impl SpaceRole {
    fn allows(self, capability: Capability) -> bool {
        match capability {
            Capability::ReadPage
            | Capability::ViewHistory
            | Capability::DownloadAttachment
            | Capability::Search => true,
            Capability::CreatePage | Capability::EditPage | Capability::UploadAttachment => {
                self >= Self::Editor
            }
            Capability::ManagePagePermissions | Capability::ManageSpace => self == Self::Owner,
            Capability::ManageWorkspace => false,
        }
    }
}

impl From<PageRole> for SpaceRole {
    fn from(value: PageRole) -> Self {
        match value {
            PageRole::Viewer => Self::Viewer,
            PageRole::Editor => Self::Editor,
        }
    }
}
