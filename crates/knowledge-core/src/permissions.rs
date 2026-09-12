use serde::{Deserialize, Serialize};

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
