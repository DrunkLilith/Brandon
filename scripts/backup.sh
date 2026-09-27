#!/usr/bin/env bash
set -euo pipefail

project=''
target=''
compose_file=''
while (($#)); do
  case "$1" in
    --project) shift; project="${1:?missing project}" ;;
    --target) shift; target="${1:?missing target}" ;;
    --compose-file) shift; compose_file="${1:?missing compose file}" ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
compose_file="${compose_file:-$repo_root/deploy/compose.yaml}"

[[ "$project" =~ ^[a-z0-9][a-z0-9_-]{2,62}$ ]] || { echo 'project must be an explicit safe Compose project name' >&2; exit 2; }
case "$target" in *[\*\?\[\]]*) echo 'backup target must be an absolute path without glob characters' >&2; exit 2 ;; esac
[[ -n "$target" && "$target" == /* ]] || { echo 'backup target must be an absolute path without glob characters' >&2; exit 2; }
[[ -d "$target" ]] || { echo 'backup target must be an existing empty directory' >&2; exit 2; }
target="$(cd -- "$target" && pwd -P)"
[[ "$target" != / && "$target" != "${HOME:-}" && "$target" != "$repo_root" ]] || { echo 'backup target is an unsafe root directory' >&2; exit 2; }
[[ -z "$(find "$target" -mindepth 1 -maxdepth 1 -print -quit)" ]] || { echo 'backup target must be empty' >&2; exit 2; }
[[ -f "$compose_file" ]] || { echo "Compose file not found: $compose_file" >&2; exit 2; }

compose() { docker compose -f "$compose_file" -p "$project" "$@"; }
[[ -n "$(compose ps -q postgres)" && -n "$(compose ps -q minio)" ]] || { echo 'source project PostgreSQL and MinIO must be running' >&2; exit 1; }

mkdir -- "$target/postgres" "$target/objects" "$target/metadata"
printf 'knowledge-platform-backup-v1\n' >"$target/bundle-version.txt"

backend_was_running=false
[[ -n "$(compose ps -q backend)" ]] && backend_was_running=true
restart_backend() { $backend_was_running && compose start backend >/dev/null 2>&1 || true; }
trap restart_backend EXIT
$backend_was_running && compose stop backend >/dev/null

run_with_mount() {
  local host_path="$1" container_path="$2"
  shift 2
  if command -v cygpath >/dev/null 2>&1; then
    local host_windows compose_windows
    host_windows="$(cygpath -w "$host_path")"
    compose_windows="$(cygpath -w "$compose_file")"
    MSYS2_ARG_CONV_EXCL='*' docker compose -f "$compose_windows" -p "$project" run --rm -T \
      --volume "$host_windows:$container_path" "$@"
  else
    compose run --rm -T --volume "$host_path:$container_path" "$@"
  fi
}

run_with_mount "$target/postgres" /backup --entrypoint /bin/sh postgres -ec \
  'export PGPASSWORD="$(cat /run/secrets/database_password)"; exec pg_dump --host=postgres --username=knowledge --dbname=knowledge --format=plain --no-owner --no-privileges --no-comments --restrict-key=KnowledgePlatformBackupV1 --file=/backup/database.sql'
run_with_mount "$target" /backup --entrypoint /bin/sh minio-init -ec '
  access=$(cat /run/secrets/object_access_key)
  secret=$(cat /run/secrets/object_secret_key)
  mc alias set --quiet local http://minio:9000 "$access" "$secret"
  mc mirror --overwrite local/knowledge /backup/objects
  mc ls --recursive --json local/knowledge | LC_ALL=C sort > /backup/metadata/object-index.jsonl
'

(
  cd -- "$target"
  while IFS= read -r -d '' file; do
    relative="${file#./}"
    hash="$(sha256sum "$relative" | awk '{print tolower($1)}')"
    printf '%s  %s\n' "$hash" "$relative"
  done < <(find . -type f ! -name manifest.sha256 -print0 | LC_ALL=C sort -z)
) >"$target/manifest.sha256"

trap - EXIT
restart_backend
printf 'backup completed: project=%s target=%s\n' "$project" "$target"
