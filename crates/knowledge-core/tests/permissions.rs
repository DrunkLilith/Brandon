use knowledge_core::permissions::{
    AccountStatus, Capability, PageRole, PermissionContext, ResourceContext, SpaceRole,
    WorkspaceRole,
};

fn member_with_space_role(role: SpaceRole) -> PermissionContext {
    PermissionContext::new(
        AccountStatus::Active,
        WorkspaceRole::Member,
        Some(role),
        [],
        None,
    )
}

#[test]
fn space_roles_allow_only_their_declared_capabilities() {
    let cases = [
        (SpaceRole::Viewer, Capability::ReadPage, true),
        (SpaceRole::Viewer, Capability::EditPage, false),
        (SpaceRole::Editor, Capability::EditPage, true),
        (SpaceRole::Editor, Capability::ManageSpace, false),
        (SpaceRole::Owner, Capability::ManageSpace, true),
    ];

    for (role, capability, expected) in cases {
        let context = member_with_space_role(role);
        assert_eq!(
            context.can(capability, ResourceContext::UnrestrictedPage),
            expected,
            "unexpected decision for {role:?} and {capability:?}"
        );
    }
}

#[test]
fn suspended_user_is_denied_even_with_owner_role() {
    let context = PermissionContext::new(
        AccountStatus::Suspended,
        WorkspaceRole::Member,
        Some(SpaceRole::Owner),
        [],
        None,
    );

    assert!(!context.can(Capability::ReadPage, ResourceContext::UnrestrictedPage));
}

#[test]
fn strongest_direct_or_group_space_role_wins() {
    let context = PermissionContext::new(
        AccountStatus::Active,
        WorkspaceRole::Member,
        Some(SpaceRole::Viewer),
        [SpaceRole::Editor, SpaceRole::Viewer],
        None,
    );

    assert_eq!(context.effective_space_role(), Some(SpaceRole::Editor));
    assert!(context.can(Capability::EditPage, ResourceContext::UnrestrictedPage));
}

#[test]
fn restricted_page_denies_space_owner_without_page_grant() {
    let context = member_with_space_role(SpaceRole::Owner);

    assert!(!context.can(Capability::ReadPage, ResourceContext::RestrictedPage));
}

#[test]
fn restricted_page_role_can_narrow_but_not_raise_space_role() {
    let owner_limited_to_viewer = PermissionContext::new(
        AccountStatus::Active,
        WorkspaceRole::Member,
        Some(SpaceRole::Owner),
        [],
        Some(PageRole::Viewer),
    );
    let viewer_granted_editor = PermissionContext::new(
        AccountStatus::Active,
        WorkspaceRole::Member,
        Some(SpaceRole::Viewer),
        [],
        Some(PageRole::Editor),
    );

    assert!(owner_limited_to_viewer.can(Capability::ReadPage, ResourceContext::RestrictedPage));
    assert!(!owner_limited_to_viewer.can(Capability::EditPage, ResourceContext::RestrictedPage));
    assert!(!viewer_granted_editor.can(Capability::EditPage, ResourceContext::RestrictedPage));
}

#[test]
fn page_grant_never_creates_space_access() {
    let context = PermissionContext::new(
        AccountStatus::Active,
        WorkspaceRole::Member,
        None,
        [],
        Some(PageRole::Editor),
    );

    assert!(!context.can(Capability::ReadPage, ResourceContext::RestrictedPage));
}

#[test]
fn workspace_admin_can_audit_and_reach_restricted_content() {
    let context =
        PermissionContext::new(AccountStatus::Active, WorkspaceRole::Admin, None, [], None);

    assert!(context.can(Capability::ManageWorkspace, ResourceContext::Workspace));
    assert!(context.can(Capability::ReadPage, ResourceContext::RestrictedPage));
}
