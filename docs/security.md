# Security model and operator checklist

Knowledge Platform v1 uses trusted-server application encryption. It is not zero-knowledge or end-to-end encryption: the running backend and anyone controlling its host or master key can decrypt authorized content.

## Protected threats and trust boundaries

The design protects plaintext content against disclosure of a PostgreSQL dump, MinIO object data, or a backup bundle when the separately held master key is absent. It also enforces authenticated sessions, CSRF checks, workspace/space/page ACLs, restricted-page boundaries, encrypted attachment streaming, and audited administrative/content operations.

It does not protect against host root, backend process-memory inspection, a malicious host administrator, a compromised master key, or a malicious replacement of both a backup and its unsigned checksum manifest. Caddy terminates external TLS; traffic on Docker's internal networks and the backend process are trusted. Use host/volume encryption as a second layer.

## Secret custody

- Generate the 256-bit master key independently and store it in an operator-controlled secret system, never in Git, `.env`, PostgreSQL, MinIO, logs, an image layer, or a backup bundle.
- Keep secrets in a private mode-0700 parent directory. The tested direct-file Compose mounts use mode-0644 files so non-root containers can read their individual mounts; host users cannot traverse the parent. Never put these files in a shared/traversable directory. Compose receives only absolute paths. A different secret manager may enforce its own container-readable ownership/modes.
- MinIO client jobs read those mounted files through the bundled credential encoder, import JSON over standard input, and keep the resulting client configuration only on the container tmpfs. Credentials are not placed in `mc` arguments or persistent environment variables.
- Do not set both a direct secret variable and its `_FILE` form. Startup rejects ambiguous, empty, multiline, NUL-containing, or unreadable secrets without logging values.
- Bootstrap accepts an interactive or temporary file secret only. Never pass the password as an argument.
- Restrict backup operators, restore operators, and key custodians separately where staffing permits. A backup becomes plaintext-readable only when combined with the key and a trusted backend.

Workspace rotation generates a fresh data-encryption key and migrates mutable ciphertext, revisions and objects in bounded background batches. New writes use the current generation immediately; authenticated reads retain previous generations during migration. Search returns a temporary unavailable response until the deterministic blind-index rebuild completes. Immutable audit ciphertext/hash-chain entries are never rewritten: their old generations remain wrapped solely for historical reads and existing identity/idempotency HMAC compatibility. Rotating content does not retroactively protect old backups or already-disclosed plaintext, audit metadata, or stable identity HMACs.

Master rotation separately rewraps every retained generation, without content re-encryption. Use the resumable `rewrap-keys` procedure in [operations.md](operations.md); never overwrite the old secret in place or discard it during a partially completed rewrap. Database and MinIO credential rotation still requires coordinated downtime and service-specific validation; changing only the application file is not a complete rotation.

## Network and host hardening

- Publish only Caddy ports 80/443. Firewall PostgreSQL 5432, MinIO 9000, backend 3000, and web 8080 from the host and external networks.
- Use a real DNS name and valid HTTPS configuration. The local HTTP override is for smoke/drill only.
- Retain HSTS, CSP, frame denial, content-type protection, and no-referrer policy from `deploy/Caddyfile` unless a reviewed change replaces them.
- Keep both proxy access and HTTP error logs suppressed. URI paths, query strings and referrers may contain confidential data even on failed requests; backend diagnostics expose only explicitly redacted fields.
- Run Docker root access as privileged access to all application data. Patch the host and Docker promptly.
- Keep containers non-root/read-only where configured, preserve capability drops and `no-new-privileges`, and enforce the declared resource limits at the runtime/orchestrator level.
- Encrypt persistent volumes and backup media; restrict filesystem ACLs for secret and backup directories.

## Permissions and disclosure

Workspace admins can administer the workspace, but sensitive administrative reads are audited. Space roles are owner, editor, and viewer; direct and group grants combine to the strongest role. Restricted page boundaries then narrow access, and page grants cannot bypass absence of space access. Revisions, search results, attachments, and exports inherit the current page/ancestor authorization. Archiving an ancestor makes descendants unavailable.

Authorization is re-evaluated by the backend. Hiding a UI control is not a security boundary. Do not expose MinIO directly or issue plaintext public object URLs.

Search uses keyed blind tokens, not a plaintext full-text index. This reduces database disclosure but leaks bounded equality/frequency information and does not provide morphological, fuzzy, or global-quality ranking. Search results are ACL-filtered and then verified after decryption.

Audit events form a hash chain and carry encrypted metadata, but the chain is not a database-superuser-proof external ledger. Detecting suffix deletion requires an expected head/count checkpoint held outside the mutable database. Export/archive access is also audited.

## Backup and dependency risks

Backup manifests are deterministic SHA-256 inventories, not signatures. Accept bundles only from controlled storage, retain an external manifest hash/signature, and run the separate restore drill. The deterministic PostgreSQL dump uses a fixed alphanumeric `psql` restrict key solely to make cross-shell manifests comparable; restore only from the trusted application database and validated bundle because a predictable restrict key is not suitable for a hostile database server.

MinIO CE is built from the official archived source rather than an unofficial mirror. The pinned server source is AGPLv3 (commercial terms may also be available); deployment requires legal review. Archived/source-only distribution creates a stale security-maintenance risk. Track upstream advisories and make an explicit provider migration decision if the pin can no longer meet policy. The same first-party pin/checksum rule applies to `mc`.

## Incident response minimums

1. Remove public traffic and stop backend writes; do not delete volumes.
2. Preserve container logs, image/source pins, external audit checkpoint, and affected backup manifests without secret values.
3. Revoke user sessions or suspend accounts through the admin interface when applicable.
4. Restore into a distinct project with the separately held key and validate through the API before cutover.
5. Treat a lost master key as permanent data loss and a disclosed key as full plaintext compromise; copying ciphertext elsewhere does not remediate key compromise.

See [operations.md](operations.md) for exact installation, backup, restore, and upgrade commands.
