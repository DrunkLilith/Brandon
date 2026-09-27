#!/usr/bin/env bash
set -euo pipefail

project=''
target=''
compose_file=''
restart_timeout_seconds=120
while (($#)); do
  case "$1" in
    --project) shift; project="${1:?missing project}" ;;
    --target) shift; target="${1:?missing target}" ;;
    --compose-file) shift; compose_file="${1:?missing compose file}" ;;
    --restart-timeout-seconds) shift; restart_timeout_seconds="${1:?missing restart timeout}" ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
compose_file="${compose_file:-$repo_root/deploy/compose.yaml}"

[[ "$project" =~ ^[a-z0-9][a-z0-9_-]{2,62}$ ]] || { echo 'project must be an explicit safe Compose project name' >&2; exit 2; }
[[ "$restart_timeout_seconds" =~ ^[1-9][0-9]*$ ]] || { echo 'restart timeout must be a positive integer' >&2; exit 2; }
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
if ! backend_container="$(compose ps -q backend 2>/dev/null)"; then
  echo 'backend state could not be determined' >&2
  exit 1
fi
[[ -n "$backend_container" ]] && backend_was_running=true
restart_backend() {
  $backend_was_running || return 0
  if ! compose start backend >/dev/null; then
    echo 'backend failed to restart after backup' >&2
    return 1
  fi
  local deadline=$((SECONDS + restart_timeout_seconds)) container_id status
  while ((SECONDS < deadline)); do
    container_id="$(compose ps -q backend)"
    if [[ -n "$container_id" ]]; then
      status="$(docker inspect -f '{{.State.Health.Status}}' "$container_id" 2>/dev/null || true)"
      [[ "$status" != healthy ]] || return 0
    fi
    sleep 2
  done
  echo 'backend failed to return ready after backup' >&2
  return 1
}
restart_on_exit() {
  local code=$?
  trap - EXIT
  if ! restart_backend && ((code == 0)); then
    code=1
  fi
  exit "$code"
}
trap restart_on_exit EXIT
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
  mc-credentials http://minio:9000 | mc alias import local >/dev/null
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
restart_backend || exit 1
printf 'backup completed: project=%s target=%s\n' "$project" "$target"
