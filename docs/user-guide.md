# User guide

Knowledge Platform is a shared Markdown knowledge base. Your administrator supplies the site URL and account. Sign in at `/login`; sessions use secure same-site cookies, so use the configured HTTPS hostname rather than a direct backend address.

## Roles and access

- **Workspace admin** manages users, invitations, groups, spaces, settings, jobs/exports, and audit views.
- **Space owner** manages a space, its grants, and content.
- **Editor** reads and changes accessible pages and attachments.
- **Viewer** reads accessible pages, history, search results, and attachments.

Group and direct space grants combine to the strongest applicable space role. A restricted page/subtree is a narrower boundary: it remains hidden unless you are explicitly allowed under that boundary. A link, page ID, revision ID, attachment ID, search query, or export does not bypass permissions. Access can change between requests; reload and contact an owner/admin if previously visible content disappears.

## Pages and revisions

Choose a space, then create a root page or child page. Titles, Markdown, properties, hierarchy, and attachment metadata are encrypted by the server before storage. Publishing a change creates a new immutable revision. If another editor saved first, the server returns a conflict instead of silently overwriting text; copy or compare your local draft and retry against the current version.

History is available only while you can read the current page. Editors can restore an older revision as a new revision, preserving the original history. Archive is a recoverable soft deletion during the configured retention period; archiving a space or ancestor hides its descendants. Ask an owner/admin to restore an item before retention cleanup.

Drafts are local to your account and browser, with independent copies per tab. The recovery button discovers your drafts even after the server version advances. Compare and continue a draft, then resolve the normal version-conflict dialog before publishing; opening a recovery does not erase the original copy. Sign-out/page-leave flush pending editor text. Unowned drafts from older releases are not shown to another account. Browser storage is not a backup; use a trusted device and copy important unpublished work before clearing site data.

Owners/admins see **Manage space** for settings, grants and exports without entering workspace administration. **Page archive** lists recoverable pages in that space; **Space archive** on Overview restores an archived space first. Expired or newly unauthorized items cannot be recovered, and failures are shown without discarding other draft work. The page tree remains available at mobile widths. **Backlinks** opens currently accessible referring pages.

## Attachments

Upload attachments from an editable page. Upload/download goes through the authorized backend; MinIO is not a public file server. Large uploads are bounded and encrypted as a stream. Attachment names and MIME metadata are encrypted at rest. If a transfer is interrupted, retry from the page; do not ask an operator to copy raw MinIO objects into the application.

Use **Load more attachments** to traverse beyond the first 100. Editors may archive a file; owners/admins with page-edit permission use **Attachment archive → Restore attachment** within the recovery window. Restoring a parent page/space is required before its attachments become accessible.

## Search

Search covers only pages you currently may read. Filters can narrow by space, author, and date. V1 search uses a keyed blind index, so expect exact-word/prefix-oriented behavior rather than Elasticsearch-style morphology, fuzzy matching, or globally tuned relevance. Phrase matches and snippets are verified after decryption. Recently changed content may briefly wait for its background indexing job.

## Exports

Owners/admins can request an encrypted-at-rest background export of an accessible space. Restricted unreadable pages are not included merely because a user can request a space export. Status is visible in the exports view. Download links are short-lived, recheck current permission, and are audited; revoked access fails closed. Exports expire according to the configured retention window.

## Administration

Workspace admins can invite or suspend users, revoke sessions, manage groups and space grants, inspect system readiness, change supported retention settings, and review audit events. Administrative access does not make the system zero-knowledge: the trusted backend handles decryption. Audit metadata avoids plaintext content, and the audit hash chain still requires an external expected head/count checkpoint for strong deletion detection.

**Delete member** requires confirmation, permanently deactivates the identity and revokes all sessions while preserving authorship/audit attribution. The last active administrator cannot be removed. Settings also offers a confirmed workspace-key rotation and its progress; search is temporarily unavailable during rebuild. The status page displays redacted maintenance health; operators should follow the operations guide for retries and master-key rewrap.

## Troubleshooting

- **Cannot sign in:** verify the exact HTTPS URL, account status, and credentials. Repeated failures are rate-limited.
- **403/404 on known content:** permissions, restricted ancestors, archive state, or account status may have changed. The API may return 404 to avoid revealing inaccessible resources.
- **Conflict while saving:** preserve your local draft, reload the current revision, then reconcile manually.
- **Search misses content:** check access and allow indexing time; fuzzy/morphological matching is intentionally limited.
- **Upload/export unavailable:** check the admin status page; object storage or the background job runner may not be ready.
- **System reports not ready:** an operator must inspect database migrations, the separately held master key, and object storage. Users should not retry destructive actions.

Operators should use [operations.md](operations.md); the trust and key boundaries are in [security.md](security.md).
