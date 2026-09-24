export type paths = {
    readonly "/api/v1/admin/settings": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getSettings"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch: operations["updateSettings"];
        readonly trace?: never;
    };
    readonly "/api/v1/admin/status": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getAdminStatus"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/attachments/{id}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["downloadAttachment"];
        readonly put?: never;
        readonly post?: never;
        readonly delete: operations["archiveAttachment"];
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/attachments/{id}/unarchive": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["unarchiveAttachment"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/audit-events": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listAuditEvents"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/auth/csrf": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getCsrf"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/auth/login": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["login"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/auth/logout": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["logout"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/auth/session": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getSession"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/exports": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listExports"];
        readonly put?: never;
        readonly post: operations["createExport"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/exports/{id}/download": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["downloadExport"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/groups": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listGroups"];
        readonly put?: never;
        readonly post: operations["createGroup"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/groups/{group_id}/members/{user_id}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put: operations["addGroupMember"];
        readonly post?: never;
        readonly delete: operations["removeGroupMember"];
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/groups/{id}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getGroup"];
        readonly put?: never;
        readonly post?: never;
        readonly delete: operations["archiveGroup"];
        readonly options?: never;
        readonly head?: never;
        readonly patch: operations["updateGroup"];
        readonly trace?: never;
    };
    readonly "/api/v1/groups/{id}/restore": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["restoreGroup"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/groups/archived": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listArchivedGroups"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/health/live": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getLiveness"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/health/ready": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getReadiness"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/invitations": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listInvitations"];
        readonly put?: never;
        readonly post: operations["createInvitation"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/invitations/{id}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post?: never;
        readonly delete: operations["revokeInvitation"];
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/invitations/{token}/accept": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["acceptInvitation"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/openapi.json": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getOpenApi"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listPages"];
        readonly put?: never;
        readonly post: operations["createPage"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getPage"];
        readonly put?: never;
        readonly post?: never;
        readonly delete: operations["archivePage"];
        readonly options?: never;
        readonly head?: never;
        readonly patch: operations["updatePage"];
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/attachments": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listAttachments"];
        readonly put?: never;
        readonly post: operations["uploadAttachment"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/backlinks": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listBacklinks"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/move": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["movePage"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/permissions": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listPagePermissions"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/permissions/{subject}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put: operations["upsertPagePermission"];
        readonly post?: never;
        readonly delete: operations["deletePagePermission"];
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/restore": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["restorePageRevision"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/revisions": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listPageRevisions"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/revisions/{version}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getPageRevision"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/pages/{id}/unarchive": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["unarchivePage"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/search": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["search"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/spaces": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listSpaces"];
        readonly put?: never;
        readonly post: operations["createSpace"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/spaces/{id}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["getSpace"];
        readonly put?: never;
        readonly post?: never;
        readonly delete: operations["archiveSpace"];
        readonly options?: never;
        readonly head?: never;
        readonly patch: operations["updateSpace"];
        readonly trace?: never;
    };
    readonly "/api/v1/spaces/{id}/restore": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["restoreSpace"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/spaces/{id}/subjects": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listSubjects"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/spaces/{space_id}/grants": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listSpaceGrants"];
        readonly put?: never;
        readonly post: operations["createSpaceGrant"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/spaces/{space_id}/grants/{grant_id}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post?: never;
        readonly delete: operations["deleteSpaceGrant"];
        readonly options?: never;
        readonly head?: never;
        readonly patch: operations["updateSpaceGrant"];
        readonly trace?: never;
    };
    readonly "/api/v1/spaces/archived": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listArchivedSpaces"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/users": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get: operations["listUsers"];
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
    readonly "/api/v1/users/{id}": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post?: never;
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch: operations["updateUser"];
        readonly trace?: never;
    };
    readonly "/api/v1/users/{id}/revoke-sessions": {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly get?: never;
        readonly put?: never;
        readonly post: operations["revokeUserSessions"];
        readonly delete?: never;
        readonly options?: never;
        readonly head?: never;
        readonly patch?: never;
        readonly trace?: never;
    };
};
export type webhooks = Record<string, never>;
export type components = {
    schemas: {
        readonly AdminStatus: {
            readonly activeExports: number;
            readonly components: {
                readonly database: boolean;
                readonly masterKey: boolean;
                readonly migrations: boolean;
                readonly objectStorage: boolean;
            };
            /** @enum {string} */
            readonly migrationState: "current" | "pending";
            readonly queuedJobs: number;
            readonly version: string;
        };
        readonly ArchivedItem: {
            /** Format: date-time */
            readonly archivedAt: string;
            /** Format: uuid */
            readonly id: string;
            readonly name: string;
        };
        readonly ArchivedList: {
            readonly items: readonly components["schemas"]["ArchivedItem"][];
            readonly nextCursor: string | null;
        };
        readonly Attachment: {
            readonly archived: boolean;
            /** Format: uuid */
            readonly id: string;
            readonly mime: string;
            readonly name: string;
            /** Format: uuid */
            readonly pageId: string;
            /** Format: int64 */
            readonly size: number;
        };
        readonly AttachmentUpload: {
            /** Format: binary */
            readonly file: string;
        };
        readonly AuditEvent: {
            readonly actorUserId: string | null;
            /** Format: date-time */
            readonly createdAt: string;
            /** Format: uuid */
            readonly id: string;
            readonly metadata: {
                readonly [key: string]: unknown;
            };
            /** Format: uuid */
            readonly requestId: string;
            readonly sessionId: string | null;
            readonly targetId: string | null;
            readonly targetType: string;
            readonly type: string;
        };
        readonly AuditEventList: {
            readonly events: readonly components["schemas"]["AuditEvent"][];
            readonly nextCursor: string | null;
        };
        readonly Csrf: {
            readonly token: string;
        };
        readonly EmptyResponse: Record<string, never>;
        readonly Export: {
            /** Format: date-time */
            readonly createdAt: string;
            readonly expiresAt: string | null;
            /** Format: uuid */
            readonly id: string;
            /** Format: uuid */
            readonly spaceId: string;
            /** @enum {string} */
            readonly status: "queued" | "running" | "ready" | "failed" | "expired";
        };
        readonly ExportCreateRequest: {
            /** Format: uuid */
            readonly spaceId: string;
        };
        readonly ExportList: {
            readonly exports: readonly components["schemas"]["Export"][];
            readonly nextCursor: string | null;
        };
        readonly Group: {
            readonly archivedAt: string | null;
            readonly description: string | null;
            /** Format: uuid */
            readonly id: string;
            readonly memberIds: readonly string[];
            readonly name: string;
        };
        readonly GroupCreateRequest: {
            readonly description?: string | null;
            readonly name: string;
        };
        readonly GroupUpdateRequest: {
            readonly description?: string | null;
            readonly name?: string;
        };
        readonly Invitation: {
            /** Format: email */
            readonly email: string | null;
            /** Format: date-time */
            readonly expiresAt: string;
            /** Format: uuid */
            readonly id: string;
            readonly status: string;
            /** @enum {string} */
            readonly workspaceRole: "admin" | "member";
        };
        readonly InvitationAcceptRequest: {
            readonly displayName: string;
            /** Format: password */
            readonly password: string;
        };
        readonly InvitationCreated: {
            /** Format: date-time */
            readonly expiresAt: string;
            /** Format: uuid */
            readonly id: string;
            readonly token: string;
        };
        readonly InvitationCreateRequest: {
            /** Format: email */
            readonly email: string;
            /** @default 168 */
            readonly expiresInHours: number;
            /** @enum {string} */
            readonly workspaceRole: "admin" | "member";
        };
        readonly Liveness: {
            /** @enum {string} */
            readonly status: "ok";
        };
        readonly LoginRequest: {
            /** Format: email */
            readonly email: string;
            /** Format: password */
            readonly password: string;
        };
        readonly OpenApiDocument: {
            readonly components: Record<string, never>;
            readonly info: Record<string, never>;
            readonly openapi: string;
            readonly paths: Record<string, never>;
        } & {
            readonly [key: string]: unknown;
        };
        readonly Page: {
            readonly capabilities: components["schemas"]["PageCapabilities"];
            /** Format: uuid */
            readonly id: string;
            readonly markdown: string;
            readonly parentId: string | null;
            readonly properties: {
                readonly [key: string]: unknown;
            };
            readonly restricted: boolean;
            /** Format: uuid */
            readonly spaceId: string;
            /** Format: uuid */
            readonly stableKey: string;
            readonly title: string;
            readonly version: number;
        };
        readonly PageCapabilities: {
            readonly edit: boolean;
            readonly managePermissions: boolean;
        };
        readonly PageCreateRequest: {
            readonly beforeId?: string | null;
            readonly markdown: string;
            readonly parentId?: string | null;
            readonly properties: {
                readonly [key: string]: unknown;
            };
            /** @default false */
            readonly restricted: boolean;
            /** Format: uuid */
            readonly spaceId: string;
            readonly title: string;
        };
        readonly PageMoveRequest: {
            readonly beforeId?: string | null;
            readonly parentId?: string | null;
            /** Format: uuid */
            readonly spaceId: string;
        };
        readonly PagePermission: {
            /** @enum {string} */
            readonly role: "viewer" | "editor";
            /** Format: uuid */
            readonly subjectId: string;
            /** @enum {string} */
            readonly subjectType: "user" | "group";
        };
        readonly PagePermissionRequest: {
            /** @enum {string} */
            readonly role: "viewer" | "editor";
        };
        readonly PageReference: {
            /** Format: uuid */
            readonly id: string;
        };
        readonly PageRevision: {
            /** Format: uuid */
            readonly authorId: string;
            /** Format: date-time */
            readonly createdAt: string;
            /** Format: uuid */
            readonly id: string;
            readonly markdown: string;
            /** Format: uuid */
            readonly pageId: string;
            readonly properties: {
                readonly [key: string]: unknown;
            };
            readonly title: string;
            readonly version: number;
        };
        readonly PageTreeItem: {
            readonly capabilities: components["schemas"]["PageCapabilities"];
            /** Format: uuid */
            readonly id: string;
            readonly parentId: string | null;
            readonly restricted: boolean;
            readonly sortKey: string;
            readonly title: string;
            readonly version: number;
        };
        readonly PageUpdateRequest: {
            readonly expectedVersion: number;
            readonly markdown: string;
            readonly properties: {
                readonly [key: string]: unknown;
            };
            readonly restricted: boolean;
            readonly title: string;
        };
        readonly PageVersion: {
            /** Format: uuid */
            readonly id: string;
            readonly version: number;
        };
        readonly Problem: {
            readonly code: string;
            readonly detail: string;
            readonly status: number;
            readonly title: string;
            readonly type: string;
        };
        readonly Readiness: {
            readonly database: boolean;
            readonly masterKey: boolean;
            readonly migrations: boolean;
            readonly objectStorage: boolean;
            /** @enum {string} */
            readonly status: "ready" | "not_ready";
        };
        readonly RevisionRestoreRequest: {
            readonly expectedVersion: number;
            readonly version: number;
        };
        readonly RevisionSummary: {
            /** Format: uuid */
            readonly authorId: string;
            /** Format: date-time */
            readonly createdAt: string;
            readonly version: number;
        };
        readonly SearchHit: {
            /** Format: uuid */
            readonly pageId: string;
            readonly snippet: string;
            readonly title: string;
            /** Format: date-time */
            readonly updatedAt: string;
        };
        readonly Session: {
            /** Format: uuid */
            readonly sessionId: string;
            /** Format: uuid */
            readonly userId: string;
            readonly workspaceAdmin: boolean;
            /** Format: uuid */
            readonly workspaceId: string;
        };
        readonly Settings: {
            readonly attachmentRecoveryDays: number;
            readonly exportRetentionHours: number;
        };
        readonly Space: {
            readonly archivedAt: string | null;
            readonly description: string | null;
            /** @enum {string} */
            readonly effectiveRole: "admin" | "owner" | "editor" | "viewer" | "none";
            /** Format: uuid */
            readonly id: string;
            readonly name: string;
        };
        readonly SpaceCreateRequest: {
            readonly description?: string | null;
            readonly name: string;
        };
        readonly SpaceGrant: {
            /** Format: uuid */
            readonly id: string;
            /** @enum {string} */
            readonly role: "owner" | "editor" | "viewer";
            /** Format: uuid */
            readonly subjectId: string;
            /** @enum {string} */
            readonly subjectType: "user" | "group";
        };
        readonly SpaceGrantCreateRequest: {
            /** @enum {string} */
            readonly role: "owner" | "editor" | "viewer";
            /** Format: uuid */
            readonly subjectId: string;
            /** @enum {string} */
            readonly subjectType: "user" | "group";
        };
        readonly SpaceGrantUpdate: {
            /** Format: uuid */
            readonly id: string;
            /** @enum {string} */
            readonly role: "owner" | "editor" | "viewer";
        };
        readonly SpaceGrantUpdateRequest: {
            /** @enum {string} */
            readonly role: "owner" | "editor" | "viewer";
        };
        readonly SpaceUpdateRequest: {
            readonly description?: string | null;
            readonly name?: string;
        };
        readonly Subject: {
            /** Format: uuid */
            readonly id: string;
            readonly label: string;
            /** @enum {string} */
            readonly status: "active";
            /** @enum {string} */
            readonly type: "user" | "group";
        };
        readonly SubjectList: {
            readonly items: readonly components["schemas"]["Subject"][];
            readonly nextCursor: string | null;
        };
        readonly User: {
            readonly displayName: string;
            /** Format: email */
            readonly email: string | null;
            /** Format: uuid */
            readonly id: string;
            /** @enum {string} */
            readonly status: "active" | "suspended" | "deleted" | "invited";
            /** @enum {string} */
            readonly workspaceRole: "admin" | "member";
        };
        readonly UserCreated: {
            /** Format: uuid */
            readonly id: string;
            /** Format: uuid */
            readonly workspaceId: string;
        };
        readonly UserUpdate: {
            /** Format: uuid */
            readonly id: string;
            /** @enum {string} */
            readonly status: "active" | "suspended";
            /** @enum {string} */
            readonly workspaceRole: "admin" | "member";
        };
        readonly UserUpdateRequest: {
            /** @enum {string} */
            readonly status?: "active" | "suspended";
            /** @enum {string} */
            readonly workspaceRole?: "admin" | "member";
        };
        readonly VersionConflictProblem: {
            /** @enum {string} */
            readonly code: "version_conflict";
            readonly currentVersion: number;
            /** @enum {integer} */
            readonly status: 409;
            readonly title: string;
            readonly type: string;
        };
    };
    responses: never;
    parameters: never;
    requestBodies: never;
    headers: never;
    pathItems: never;
};
export type $defs = Record<string, never>;
export interface operations {
    readonly getSettings: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Settings"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly updateSettings: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["Settings"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Settings"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getAdminStatus: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["AdminStatus"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly downloadAttachment: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: {
                readonly Range?: string;
            };
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "Accept-Ranges": "bytes";
                    readonly "Cache-Control": "no-store";
                    readonly "Content-Disposition": string;
                    readonly "Content-Length": number;
                    readonly "X-Content-Type-Options": "nosniff";
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "*/*": string;
                };
            };
            /** @description Partial attachment */
            readonly 206: {
                headers: {
                    readonly "Accept-Ranges": "bytes";
                    readonly "Cache-Control": "no-store";
                    readonly "Content-Disposition": string;
                    readonly "Content-Length": number;
                    readonly "Content-Range": string;
                    readonly "X-Content-Type-Options": "nosniff";
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "*/*": string;
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Range not satisfiable */
            readonly 416: {
                headers: {
                    readonly "Content-Range": string;
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly archiveAttachment: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly unarchiveAttachment: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listAuditEvents: {
        readonly parameters: {
            readonly query?: {
                readonly actor?: string;
                readonly after?: string;
                readonly from?: string;
                readonly limit?: number;
                readonly target?: string;
                readonly targetType?: string;
                readonly to?: string;
                readonly type?: string;
            };
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["AuditEventList"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getCsrf: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Csrf"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly login: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["LoginRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "Set-Cookie": string;
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Session"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Rate limited */
            readonly 429: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly logout: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "Set-Cookie": string;
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getSession: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Session"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listExports: {
        readonly parameters: {
            readonly query?: {
                readonly after?: string;
                readonly limit?: number;
            };
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["ExportList"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly createExport: {
        readonly parameters: {
            readonly query?: never;
            readonly header: {
                readonly "Idempotency-Key": string;
            };
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["ExportCreateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 202: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Export"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Rate limited */
            readonly 429: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly downloadExport: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "Cache-Control": "no-store";
                    readonly "Content-Disposition": string;
                    readonly "Content-Length": number;
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/x-tar": string;
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listGroups: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["Group"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly createGroup: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["GroupCreateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 201: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Group"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly addGroupMember: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly group_id: string;
                readonly user_id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly removeGroupMember: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly group_id: string;
                readonly user_id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getGroup: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Group"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly archiveGroup: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly updateGroup: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["GroupUpdateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Group"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly restoreGroup: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listArchivedGroups: {
        readonly parameters: {
            readonly query?: {
                readonly after?: string;
                readonly limit?: number;
            };
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["ArchivedList"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getLiveness: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Liveness"];
                };
            };
        };
    };
    readonly getReadiness: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Readiness"];
                };
            };
            /** @description Not ready */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Readiness"];
                };
            };
        };
    };
    readonly listInvitations: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["Invitation"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly createInvitation: {
        readonly parameters: {
            readonly query?: never;
            readonly header: {
                readonly "Idempotency-Key": string;
            };
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["InvitationCreateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 201: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["InvitationCreated"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Rate limited */
            readonly 429: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly revokeInvitation: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly acceptInvitation: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly token: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["InvitationAcceptRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 201: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["UserCreated"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Rate limited */
            readonly 429: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getOpenApi: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["OpenApiDocument"];
                };
            };
        };
    };
    readonly listPages: {
        readonly parameters: {
            readonly query: {
                readonly spaceId: string;
            };
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["PageTreeItem"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly createPage: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["PageCreateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 201: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Page"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getPage: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Page"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly archivePage: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly updatePage: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["PageUpdateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["PageVersion"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Version conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["VersionConflictProblem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listAttachments: {
        readonly parameters: {
            readonly query?: {
                readonly after?: string;
                readonly archived?: boolean;
            };
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["Attachment"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly uploadAttachment: {
        readonly parameters: {
            readonly query: {
                readonly size: number;
            };
            readonly header?: {
                readonly "Idempotency-Key"?: string;
            };
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "multipart/form-data": components["schemas"]["AttachmentUpload"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 201: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Attachment"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Rate limited */
            readonly 429: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listBacklinks: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["PageReference"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly movePage: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["PageMoveRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listPagePermissions: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["PagePermission"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly upsertPagePermission: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
                readonly subject: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["PagePermissionRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["PagePermission"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly deletePagePermission: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
                readonly subject: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly restorePageRevision: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["RevisionRestoreRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["PageVersion"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Version conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["VersionConflictProblem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listPageRevisions: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["RevisionSummary"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getPageRevision: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
                readonly version: number;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["PageRevision"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly unarchivePage: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly search: {
        readonly parameters: {
            readonly query: {
                readonly author?: string;
                readonly from?: string;
                readonly q: string;
                readonly space?: string;
                readonly to?: string;
            };
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["SearchHit"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listSpaces: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["Space"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly createSpace: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["SpaceCreateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 201: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Space"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly getSpace: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Space"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly archiveSpace: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly updateSpace: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["SpaceUpdateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["Space"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly restoreSpace: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listSubjects: {
        readonly parameters: {
            readonly query?: {
                readonly after?: string;
                readonly limit?: number;
            };
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["SubjectList"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listSpaceGrants: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly space_id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["SpaceGrant"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly createSpaceGrant: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly space_id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["SpaceGrantCreateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 201: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["SpaceGrant"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly deleteSpaceGrant: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly grant_id: string;
                readonly space_id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly updateSpaceGrant: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly grant_id: string;
                readonly space_id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["SpaceGrantUpdateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["SpaceGrantUpdate"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listArchivedSpaces: {
        readonly parameters: {
            readonly query?: {
                readonly after?: string;
                readonly limit?: number;
            };
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["ArchivedList"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly listUsers: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path?: never;
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": readonly components["schemas"]["User"][];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly updateUser: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody: {
            readonly content: {
                readonly "application/json": components["schemas"]["UserUpdateRequest"];
            };
        };
        readonly responses: {
            /** @description Success */
            readonly 200: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/json": components["schemas"]["UserUpdate"];
                };
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Conflict */
            readonly 409: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Validation failed */
            readonly 422: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
    readonly revokeUserSessions: {
        readonly parameters: {
            readonly query?: never;
            readonly header?: never;
            readonly path: {
                readonly id: string;
            };
            readonly cookie?: never;
        };
        readonly requestBody?: never;
        readonly responses: {
            /** @description Success */
            readonly 204: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content?: never;
            };
            /** @description Authentication required */
            readonly 401: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Forbidden */
            readonly 403: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Not found */
            readonly 404: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Internal error */
            readonly 500: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
            /** @description Service unavailable */
            readonly 503: {
                headers: {
                    readonly "X-Request-ID": string;
                    readonly [name: string]: unknown;
                };
                content: {
                    readonly "application/problem+json": components["schemas"]["Problem"];
                };
            };
        };
    };
}
