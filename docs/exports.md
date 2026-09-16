# Audit, jobs and encrypted exports (v1)

## Audit query

`GET /api/v1/audit-events` requires an active workspace administrator and returns `Cache-Control: no-store`. Optional filters are `actor`, `type`, `target`, `targetType`, `from`, `to`, `after`, and `limit` (1–100). The UUIDv7 cursor is exclusive and filters are applied before the limit. Administrative disclosure appends and commits an `audit.admin_access` event before metadata decryption/response.

The historical v1 verifier checks every previous-hash link, recomputes the documented SHA-256 digest, authenticates encrypted safe metadata, and requires a caller-held expected count/head checkpoint. The historical digest does not authenticate `created_at`, metadata nonce, or metadata key version. A checkpoint read only from the same mutable database is not an external anchor; this does not claim protection against a database superuser who also has the workspace key.

## Export API

`POST /api/v1/exports` accepts JSON `{ "spaceId": "UUID" }` and requires a UUID `Idempotency-Key`, CSRF, an active session, and current workspace-admin or space-owner authority. Keys are scoped by workspace + actor + export-request operation. Replay returns the same logical export/job without a second mutation or object. New requests have an independent actor limit of 10 per 15 minutes. Returns `202` with `{id,spaceId,status,expiresAt,createdAt}`.

`GET /api/v1/exports` returns up to 100 exports after current space-owner/admin filtering, never object keys or payloads. `GET /api/v1/exports/{id}/download` requires a ready, unexpired export, revalidates the current session, current space authority, every snapshotted page ACL/active ancestor and attachment activity, commits the disclosure audit, then takes the per-object lock and streams authenticated plaintext. It returns `application/x-tar`, forced attachment disposition, `Cache-Control: no-store`, and no object URL.

The uncompressed USTAR root is `space-{space UUID}` with UUID-only `pages/{page UUID}.md` and `attachments/{attachment UUID}` paths plus `manifest.json`. Original names/titles remain encrypted archive data. Markdown has JSON/YAML-1.2 frontmatter with page/stable/parent/revision IDs, title and properties. The manifest maps generated paths to original names and IDs. Input is rejected, never truncated, above 1,000 pages, 1,000 attachments, 64 KiB serialized frontmatter per page, 4 MiB manifest or 1 GiB archive. Empty accessible spaces produce a manifest-only archive.

Objects use the Task13 64 KiB AES-256-GCM stream framing with a distinct authenticated export domain and a separately generated opaque object UUID. The declared tar size is calculated before the first byte. Durable `export_staging` exists before remote multipart creation; remote upload ID is persisted before the authenticated header write. No SQL transaction spans remote streaming, no plaintext temporary archive is created, and each source length plus total authenticated length must agree.

## Runner and retention

The production binary starts one in-process scheduler with a random runner UUID. Every five seconds it drains at most 100 runnable jobs, then runs attachment-orphan, export-orphan and export-expiry maintenance. Claims use `FOR UPDATE SKIP LOCKED`; owner + monotonically increasing attempt + unexpired lease fence renewal, retry and completion. Leases are 60 seconds and renew before every archive piece and each 64 KiB remote write. Object operations have 30-second timeouts. Attempts use fixed backoff of 1, 5, 30 and then 60 seconds; five executions maximum. An expired fifth execution is reclaimed only as attempt six for fenced terminalization and never invokes the remote handler. Stored errors are only `operation_failed`.

Snapshot selection and final commit each revalidate the requester's original active session, membership, space authority, page ACLs/restricted boundaries and object activity. A stale runner cannot commit a newer result. Completion, job success, export ready state, staging removal and `export.completed` audit commit atomically after remote finalize. Failures/revocation retain a safely reclaimable staging row; orphan cleanup takes the object lock, checks that no ready export owns the key, aborts a known multipart ID, deletes the object and clears only non-ready references.

Ready exports expire after 24 hours. Expiry takes the same object lock (downloads retain it through their response body), rechecks eligible ready state, deletes ciphertext idempotently, then atomically records expired state and `export.expired`. Cleanup never treats a ready committed object or an active locked stream as orphaned. Provider-side abort-incomplete-multipart lifecycle remains required for the known empty create-before-ID crash gap at the deployment checkpoint.

Task15 still owns persisted operational/security settings, recovery prerequisites and expanded readiness/status contracts before client generation. Task19 owns selected-provider integration and production bucket lifecycle verification; no provider was chosen or configured here.
