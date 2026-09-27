# Operations guide

This is the supported single-host Docker Compose deployment for Knowledge Platform v1. It runs Caddy, the web application, the Rust backend, PostgreSQL 18.6, and MinIO. Only Caddy publishes host ports; PostgreSQL and MinIO remain on internal Docker networks.

## Prerequisites

- A Linux host with Docker Engine and Docker Compose v2.
- A public DNS name whose A/AAAA records point at the host.
- Inbound TCP 80 and 443 only. Do not publish PostgreSQL 5432 or MinIO 9000.
- Encrypted, monitored storage for Docker volumes and a separate secret store.
- Enough capacity for the database, object data, one complete backup, and restore drills.

The production MinIO server is built from the official, archived MinIO CE source tag `RELEASE.2025-10-15T17-29-55Z` at commit `9e49d5e7a648f00e26f2246f4dc28e6b07f8c84a`. The official `mc` source is pinned to `RELEASE.2025-08-13T08-35-41Z` at commit `7394ce0dd2a80935aded936b09fa12cbb3cb8096`. Docker builds verify the recorded source archive SHA-256 values. MinIO CE is source-only and archived upstream; review AGPLv3/commercial licensing and the stale security-maintenance risk before production use. No third-party binary mirror is used.

## First installation

Create secret files outside the repository. Files may have no newline or exactly one terminal LF/CRLF; other line breaks, NUL, empty values, unreadable files, and simultaneous direct plus `_FILE` values are rejected.

```sh
sudo install -d -m 0700 /srv/knowledge/secrets
sudo sh -c 'umask 077; openssl rand -base64 32 | tr -d "\r\n" > /srv/knowledge/secrets/master_key'
sudo sh -c 'umask 077; openssl rand -hex 32 > /srv/knowledge/secrets/database_password'
sudo sh -c 'umask 077; printf "kpadmin%s" "$(openssl rand -hex 8)" > /srv/knowledge/secrets/object_access_key'
sudo sh -c 'umask 077; openssl rand -hex 32 > /srv/knowledge/secrets/object_secret_key'
sudo chmod 0644 /srv/knowledge/secrets/*
cp deploy/.env.example deploy/.env
```

Keep the directory `0700`; the files are deliberately `0644` so Docker can bind each file directly into the non-root service that needs it. The private parent directory prevents other host users from traversing to those files. Do not place the files in a shared or traversable directory.

Edit `deploy/.env` to set the real HTTPS hostname and absolute secret-file paths. The `.env` file contains paths and public configuration only, never secret values. Then validate and build:

```sh
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge config --quiet
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge build
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge up -d postgres minio
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge run --rm -T minio-init
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge run --rm -T migrate
```

Bootstrap exactly once without putting the password in shell history or process arguments:

```sh
bootstrap_dir=$(mktemp -d /dev/shm/knowledge-bootstrap.XXXXXX)
chmod 0700 "$bootstrap_dir"
bootstrap_secret="$bootstrap_dir/password"
trap 'rm -rf -- "$bootstrap_dir"' EXIT
read -r -s -p 'Initial administrator password: ' bootstrap_password; printf '\n'
printf '%s' "$bootstrap_password" >"$bootstrap_secret"
chmod 0644 "$bootstrap_secret"
unset bootstrap_password
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge --profile tools run --rm -T \
  --volume "$bootstrap_secret:/run/secrets/bootstrap_password:ro" \
  --env KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/bootstrap_password \
  bootstrap --email admin@example.com --workspace-name 'Knowledge' --display-name 'Administrator'
rm -rf -- "$bootstrap_dir"
```

Start the application only after both explicit one-shot commands succeed:

```sh
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge up -d backend web caddy
curl --fail --silent https://knowledge.example.com/api/v1/health/live
curl --fail --silent https://knowledge.example.com/api/v1/health/ready
```

Normal backend and bootstrap startup never runs migrations. `/live` only proves that a process responds; route traffic only when `/ready` reports `status: ready`, including `database`, `migrations`, `masterKey`, and `objectStorage` all true.

## Upgrade and rollback decision

1. Read release notes and verify there is enough backup capacity.
2. Create and verify a backup with the current release.
3. Build the new pinned images.
4. Stop Caddy and backend, leaving PostgreSQL and MinIO running.
5. Run the new `migrate` service explicitly.
6. Start backend and verify `/ready`; only then start web and Caddy.

```sh
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge stop caddy backend
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge build
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge run --rm -T migrate
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge up -d backend web caddy
```

If migration fails, keep the application stopped, retain logs, and restore the verified pre-upgrade backup into a separate installation. Reverting an image is safe only when release notes explicitly declare the migrated schema backward-compatible. There is no general destructive/down-migration command; never use `down --volumes` as rollback.

## Workspace and master-key rotation

Before either rotation, make and verify a backup and preserve the matching master-key version separately. Allow free object-storage capacity for one additional encrypted object while migration is active. A compromised key also requires incident response: old backups and immutable audit history cannot be made secret retroactively.

For workspace rotation, open **Administration → Settings → Rotate workspace key**, read the interruption/search notice and confirm. Only a freshly authorized workspace administrator can start or inspect it. The status reports current/previous generations, migration/rebuild phase and processed units. New writes use a fresh random DEK immediately. The scheduler migrates at most 16 encrypted fields or one object (64 KiB authenticated frames, five-minute copy deadline) per batch, then indexes 16 pages per batch. Search is temporarily unavailable until completion; page and attachment reads continue with versioned decryption. One fenced ten-minute lease prevents competing workers from publishing twice. Process interruption leaves durable checkpoints and multipart/old-object cleanup records; restarting the same configured backend resumes automatically. Do not manually delete keys, objects or lease rows. A second start while busy returns conflict.

Audit events are append-only: old audit ciphertext and hashes remain unchanged and their encrypted key generations are retained. Revision migration may change only encryption envelope fields under a fenced migration transaction; identity, authorship, version, content hashes and semantic fields cannot change. The reserved optional session-client and revision-reason envelope domains are `workspace:<workspace>:session:<session>:client-metadata` and `workspace:<workspace>:page:<page>:revision:<version>:change-reason`; normal v1 writes leave them null. Mutable content, attachments, exports and the search key rotate; this is not wrapper-only workspace rotation.

Master rewrap is a separate offline operator workflow. Stop backend/public traffic, keep storage running, generate a fresh protected file alongside the old one, and set the non-secret `.env` values `KNOWLEDGE_MASTER_KEY_FILE=/srv/knowledge/secrets/master_key_v2` and `KNOWLEDGE_MASTER_KEY_VERSION=2` (use your actual monotonically increasing versions). Keep the old file as `master_key_v1`; never put either value in arguments or environment variables.

```sh
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge stop caddy backend
sudo sh -c 'umask 077; openssl rand -base64 32 | tr -d "\r\n" > /srv/knowledge/secrets/master_key_v2'
sudo chmod 0644 /srv/knowledge/secrets/master_key_v2
# Update only the paths/version in deploy/.env before the following command.
docker compose --env-file deploy/.env -f deploy/compose.yaml -p knowledge run --rm -T --no-deps \
  --entrypoint /usr/local/bin/rewrap-keys \
  --env KNOWLEDGE_PREVIOUS_MASTER_KEY_VERSION=1 \
  --env KNOWLEDGE_PREVIOUS_MASTER_KEY_FILE=/run/secrets/previous_master_key \
  --volume /srv/knowledge/secrets/master_key_v1:/run/secrets/previous_master_key:ro backend
```

The command commits at most 16 retained generations and prints only `{"rewrapped":N}`. Repeat the same command until it succeeds with zero. Each generation and its audit event commit atomically, including the legacy current-wrapper pointer. After a crash or partial failure, keep both files/versions and repeat; do not run an old-only backend against partially rewrapped data. Once zero is verified, start backend using the new primary secret/version, check `/ready` and authorized historical content/audit reads, then restore public traffic. The previous mount is not needed for the final backend, but retain the previous key separately for historical backups. A missing/wrong retained master generation fails readiness. No content, secret, raw provider error or SQL appears in command diagnostics.

## Retention and maintenance health

Archive recovery ends exactly when `archivedAt + recoveryDays <= database clock_timestamp()`. Listing and restore share that cutoff; expiration never waits for the purge worker. Purge uses reclaimable leases and bounded batches (16 parent/object rows, 100 dependent rows per parent). It removes objects before final metadata, drains child/revision/grant/link rows safely, and appends a `retention.purged` event per final entity. Active object locks are skipped and failures retry idempotently. Export objects in finally expired spaces are deleted even if their own download deadline is later.

Administration status shows process-local maintenance failure count and the last fixed phase/code, timestamp and correlation UUID; structured stderr events use the same fields, never raw errors. Counters reset on process restart; correlate with the retained structured log stream and immutable operation audit. Check database/key/object-store readiness and storage capacity, then allow the scheduler to retry; do not erase staging records to hide failures. Shutdown cancels scheduler claims and drains active HTTP requests for at most 15 seconds (Compose allows 20); interrupted work resumes via leases/checkpoints after restart.

## Backup procedure

Create an existing, empty, absolute target directory on separate protected storage. The scripts canonicalize the path and reject roots, home, the repository, glob-like paths, and Windows reparse points. They stop a running backend during the logical PostgreSQL dump and MinIO mirror, then restore its prior running state.

PowerShell:

```powershell
New-Item -ItemType Directory -Path 'D:\knowledge-backups\2026-09-27'
pwsh -File scripts/backup.ps1 -ProjectName knowledge -TargetDirectory 'D:\knowledge-backups\2026-09-27'
```

Bash:

```sh
install -d -m 0700 /srv/knowledge-backups/2026-09-27
./scripts/backup.sh --project knowledge --target /srv/knowledge-backups/2026-09-27
```

The bundle contains `postgres/database.sql`, encrypted objects under `objects/`, `metadata/object-index.jsonl`, `bundle-version.txt`, and an ordinal path-sorted `manifest.sha256`. It never contains or derives the master key. Store the master key separately. The manifest detects corruption and accidental replacement, but it is not a signature; protect or sign the bundle through the backup system and keep off-site copies.

On native Linux, run backup and restore as the account that owns the private `0700` bundle directory. The scripts run bind-mounted helper containers with that account's UID/GID and enforce no group/other permissions on the resulting plaintext database dump and metadata. Do not relax the bundle directory to make a container write to it.

If the backend was running, backup stops it for the database/object snapshot, restarts it, and waits boundedly for `/ready` through the container health check. A restart failure or readiness timeout makes the backup command fail even when the bundle files were written; investigate the backend before treating that bundle as an accepted backup.

## Restore

Restore only from a trusted, verified bundle. Supply the same database/object credentials expected by the Compose environment and the separately held original master key. The restore target must be an existing empty absolute directory, and the Compose project must be a new name matching `kp-restore-` plus 12 lowercase hexadecimal characters.

```sh
install -d -m 0700 /srv/knowledge-restore/drill-state
./scripts/restore.sh \
  --project kp-restore-0123456789ab \
  --source /srv/knowledge-backups/2026-09-27 \
  --target /srv/knowledge-restore/drill-state
docker compose --env-file deploy/.env -f deploy/compose.yaml -p kp-restore-0123456789ab up -d backend web caddy
```

PowerShell uses the corresponding parameters:

```powershell
pwsh -File scripts/restore.ps1 -ProjectName kp-restore-0123456789ab `
  -SourceDirectory 'D:\knowledge-backups\2026-09-27' `
  -TargetDirectory 'D:\knowledge-restore\drill-state'
```

Restore validates every manifest path and checksum, rejects duplicates, traversal, links, missing files, unlisted files, and unsupported bundle versions, then loads a fresh PostgreSQL volume and MinIO bucket. It refuses any project that already has containers or any exact Compose-resolved volume name, even if a pre-existing volume has no Compose labels. Failure cleanup removes only volumes whose project and logical-volume ownership labels match this restore invocation. It does not start the backend or copy a key.

## Restore drill

Run both self-contained drills after storage, database, Docker, or backup-script changes. They create unique source and restore projects, seed a page and attachment through the public API, compare Bash and PowerShell manifests, verify missing/wrong keys fail closed, verify the correct key, and delete only their exact test volumes.

```powershell
pwsh -File scripts/verify-backup.ps1 -TestMode
```

```sh
./scripts/verify-backup.sh --test-mode
```

The Windows Bash command must use Git Bash explicitly, for example `C:\Program Files\Git\bin\bash.exe`; the Docker Desktop WSL shim named `bash` is not Linux verification. Run the Bash script on a clean Linux host before release as well.

## Routine checklist and recovery

- Monitor `/api/v1/health/ready`, container restarts, disk space, database connections, job failures, upload/export failures, and backup age.
- Test restore regularly and record the bundle identifier, manifest hash, release version, and result without recording secrets.
- Patch Docker, Debian, PostgreSQL, Caddy, Rust/Node base images, and review the pinned archived MinIO source risk on every release.
- Keep Docker volumes on encrypted storage and back up the key in a different security domain.
- Use `docker compose logs --no-color --tail 200 SERVICE`; configuration errors name variables but do not print their values.
- Proxy request logging is intentionally disabled, including nginx request errors and Caddy's HTTP logger namespace: even a 502 can otherwise record confidential URI/query/referrer data. Use health probes, container state and redacted backend operational events for diagnosis. Do not enable verbose/request logging on production traffic.
- A lost master key makes encrypted database and object backups unrecoverable. Stop writes and escalate to the key custodian.
- Suspected database/object corruption: stop backend, preserve volumes, restore the latest verified bundle into a new project, and compare through the API before cutover.
- Do not manually delete objects from MinIO. Application retention/jobs own attachment and export cleanup.

V1 has no HA database/object-store topology, automatic off-site schedule, point-in-time recovery, or zero-downtime migration guarantee. One backend instance is the supported deployment.
