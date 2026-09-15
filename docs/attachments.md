# Encrypted attachments (v1)

All routes use `/api/v1`, session authentication, and the existing CSRF/same-origin checks for mutations. The parent page is the final ACL authority; inactive pages, inactive ancestor pages/spaces, and inaccessible restricted boundaries return enumeration-safe `404`. A visible viewer's mutations return `403`.

## Upload and list

`POST /pages/{id}/attachments?size=<plaintext bytes>` accepts one multipart `file` field with filename and Content-Type. Declared size must equal the streamed bytes and be between 1 and 104857600 (100 MiB). Empty files and additional fields are rejected. First multipart headers including initial boundary are capped at 8192 bytes; the entire wire body is capped at declared size + 8192, including headers and closing boundary. Normal browser FormData boundaries (up to 70 ASCII bytes) and quoted Content-Type boundaries are supported; preambles are rejected. Files are streamed in 64 KiB authenticated chunks; neither handler nor production adapter collects the whole file. Malformed queries/multipart return redacted Problem Details.

Allowed MIME types: PNG, JPEG, GIF, PDF (checked against byte signatures), UTF-8 `text/plain`/`text/markdown` (validated across the entire stream, including split Unicode sequences), and `application/octet-stream` (explicitly unknown/untrusted binary). MIME signatures are not malware detection or full format validation. All downloads are forced attachments, never executable inline content.

Returns `201` JSON `{id,pageId,name,mime,size,archived}` after object finalize, fresh authorization/session validation under the workspace audit lock, and atomic attachment/audit/request-ledger SQL commit. Names and MIME use attachment-purpose AEAD with workspace/entity/field authentication. The object key is a separate opaque UUID. The stored checksum is SHA-256 of **ciphertext**, not a plaintext digest.

`GET /pages/{id}/attachments` lists active records, at most 100, in ascending ID order. Pass `after=<last UUID>` for cursor pagination. `?archived=true` lists soft-deleted records for an authorized admin/space owner; the same page-read/restricted checks still apply. Metadata responses use `Cache-Control: no-store`.

Uploads have a separate actor-domain SQL body-processing bucket, 60 attempts per 15 minutes, including successful-key replay and body-mismatch attempts. Each attempt is charged once before receiving the file body; saturation returns `429` without replay body processing or another object/mutation. Requests rejected for authorization, an already pending key, or different declared page/size do not enter body processing. Login/invitation thresholds remain five per 15 minutes. At most two long uploads/replays run per process, further reduced to leave three shared pool connections available; pools with three or fewer connections disable uploads. Concurrency saturation returns `503`. Body processing is limited to five minutes, individual body/object operations to 30 seconds. There is no SQL transaction held while streaming.

## Idempotency

Optional `Idempotency-Key` must be a UUID. Its scope is workspace + actor + attachment-upload operation. Successful replay validates the entire multipart body with a domain-separated streaming HMAC binding request/page/size/name/MIME/content, without writing another object or creating another attachment/upload mutation event. Current page permissions and attachment activity are rechecked; a saved response never bypasses revocation or archive.

Administrative list/download/successful replay uses record a separate `attachment.admin_access` audit event committed before metadata or body disclosure. Authorization and current-session revalidation run under the workspace audit lock; audit failure fails closed. Events contain actor/session/request IDs, page or attachment target ID, and encrypted metadata with only a fixed `operation` value (`list`, `download`, `upload_replay`), never filenames/MIME/content. Ordinary non-admin access does not create this administrative event. Replay still retains exactly one original `attachment.uploaded` mutation.

Different page, size, supported metadata or content for the same key returns redacted `409`. Pending/concurrent requests also return `409`; retry after the first request completes. Failed/cancelled/crashed reservations remain pending until their one-hour staging expiry and safe orphan cleanup. Use a fresh key for an immediate retry, or reuse the original after cleanup. A committed request remains associated with its attachment; replay of an archived attachment does not resurrect it.

## Download ranges and stream integrity

`GET /attachments/{id}` streams authenticated plaintext. Each frame binds workspace, attachment ID, key version, header, declared size/count, frame index and data/end domain. Object length, header, and authenticated footer are checked before the response begins. Every data frame is authenticated before emitting bytes. A later corrupted frame terminates the body without exposing that frame or any later bytes; clients must discard partial downloads on body/Content-Length failure. The authenticated footer plus exact expected object length rejects truncation and appends; indices reject reordering/substitution.

Downloads use `Content-Disposition: attachment` with a sanitized ASCII filename, `X-Content-Type-Options: nosniff`, `Cache-Control: no-store`, and never expose object URLs.

Only a single `Range: bytes=start-end` (inclusive end) or `bytes=start-` is supported. Start must be a multiple of 65536, and exclusive end must be a multiple of 65536 **or the exact plaintext size**. Bounds must satisfy `0 <= start < end-exclusive <= size`. Successful ranges return `206` with Content-Range/Content-Length. Suffix ranges, multipart ranges, unaligned, malformed, reversed or out-of-bounds ranges return `416`, empty body, `Content-Range: bytes */<plaintext size>`. A full ordinary GET returns `200`. The storage adapter reads at most 128 KiB per operation and rejects oversized/truncated provider streams.

## Deletion, recovery, and orphan cleanup

`DELETE /attachments/{id}` requires page-upload permission and soft-deletes metadata with its audit event. It does not remove the object. `POST /attachments/{id}/unarchive` requires current page-upload permission and admin/space-owner status; it restores with an audit event only inside the configured retention window. Default: 30 days; `KNOWLEDGE_ATTACHMENT_RETENTION_DAYS` accepts 1–3650. Parent page/space must first be active and accessible. Final retention purge must be an audited job (Task 14/17); orphan cleanup never treats even an archived committed attachment as an orphan.

Durable staging records precede remote multipart creation. The remote upload ID is persisted before any object bytes are written. A dedicated PostgreSQL connection holds a per-object session advisory lock across remote work and SQL commit. It is marked `close_on_drop` **before** the lock await, so cancellation/crash closes the session and cannot return a locked session to the pool. No open SQL transaction spans the body.

`knowledge_store::attachments::cleanup_orphans` is a bounded, idempotent job primitive for Task 14: expired staging records only, per-object try-lock, authoritative live-attachment recheck (including archived rows), abort known remote multipart ID, delete finalized orphan, remove pending idempotency reservation and staging ledger. Active uploads are skipped even if expiry has elapsed. Failed cleanup retains the ledger for retry. It never deletes a committed/live object.

Cross-system atomicity is deliberately not claimed: a crash between remote multipart creation and SQL upload-ID persistence can leave an **empty** multipart reservation. No file/ciphertext parts can be written before persistence succeeds. Production bucket configuration **must** include an AbortIncompleteMultipartUpload lifecycle rule, verified with the selected provider. Cleanup of known IDs handles completed or unfinished uploads; missing IDs/objects must be idempotent on the provider.

## Production S3-compatible configuration

The binary requires `KNOWLEDGE_S3_ENDPOINT`, `KNOWLEDGE_S3_BUCKET`, `KNOWLEDGE_S3_REGION`, `KNOWLEDGE_S3_ACCESS_KEY_ID`, and `KNOWLEDGE_S3_SECRET_ACCESS_KEY`. TLS is default; internal HTTP requires explicit `KNOWLEDGE_S3_ALLOW_HTTP=true`. Credentials have no Debug representation and configuration/storage errors are redacted. Use least-privilege credentials for a private application-owned bucket.

`S3ObjectStore` uses generic path-style S3 multipart operations and 5 MiB parts. It awaits `WriteMultipart::wait_for_capacity(2)` before each bounded write (roughly two in-flight parts plus one 5 MiB buffer). `MemoryObjectStore` is explicitly injected only in tests; an unconfigured AppState fails closed, never silently storing production files in RAM.

No production object-storage provider is selected/deployed by this task. MinIO Community is archived and is not a supported production recommendation. Selected-provider multipart/range/abort/lifecycle compatibility and production Compose/readiness are required at the later deployment checkpoint.
