#!/usr/bin/env bash
set -euo pipefail

delete_volumes=false
timeout_seconds=420
while (($#)); do
  case "$1" in
    --delete-volumes) delete_volumes=true ;;
    --timeout-seconds) shift; timeout_seconds="${1:?missing timeout}" ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
compose_file="$repo_root/deploy/compose.yaml"
[[ -f "$compose_file" ]] || { echo "production Compose file is missing: $compose_file" >&2; exit 1; }

suffix="$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
project="kp-smoke-$suffix"
[[ "$project" =~ ^kp-smoke-[a-f0-9]{12}$ ]] || { echo 'generated Compose project name is unsafe' >&2; exit 1; }
secret_directory="${TMPDIR:-/tmp}/$project"
http_port="$((20000 + RANDOM % 20000))"

compose() { docker compose -f "$compose_file" -p "$project" "$@"; }
cleanup() {
  local args=(down --remove-orphans)
  $delete_volumes && args+=(--volumes)
  compose "${args[@]}" >/dev/null 2>&1 || true
  if [[ -d "$secret_directory" && "$(cd -- "$secret_directory/.." && pwd -P)/$(basename -- "$secret_directory")" == "${TMPDIR:-/tmp}/$project" ]]; then
    rm -rf -- "$secret_directory"
  fi
}
trap cleanup EXIT

umask 077
mkdir -- "$secret_directory"
openssl rand -base64 32 | tr -d '\r\n' >"$secret_directory/master_key"
printf 'db-%s' "$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')" >"$secret_directory/database_password"
printf 'kp%s' "$suffix" >"$secret_directory/object_access_key"
printf 'obj-%s' "$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')" >"$secret_directory/object_secret_key"
bootstrap_password="Bootstrap-$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')!"
printf '%s' "$bootstrap_password" >"$secret_directory/bootstrap_password"
chmod 700 -- "$secret_directory"
chmod 644 -- "$secret_directory"/*

export KNOWLEDGE_MASTER_KEY_FILE="$secret_directory/master_key"
export KNOWLEDGE_DATABASE_PASSWORD_FILE="$secret_directory/database_password"
export KNOWLEDGE_OBJECT_ACCESS_KEY_FILE="$secret_directory/object_access_key"
export KNOWLEDGE_OBJECT_SECRET_KEY_FILE="$secret_directory/object_secret_key"
export KNOWLEDGE_SITE_ADDRESS='http://localhost'
export KNOWLEDGE_PUBLIC_ORIGIN="http://localhost:$http_port"
export KNOWLEDGE_HTTP_PORT="$http_port"

compose config --quiet
compose build backend web minio minio-init
compose up -d postgres minio
compose up -d --no-deps backend
pre_migration_deadline=$((SECONDS + 60))
pre_migration_ready=''
while ((SECONDS < pre_migration_deadline)); do
  pre_migration_ready="$(compose exec -T backend curl --silent 'http://localhost:3000/api/v1/health/ready' 2>/dev/null || true)"
  [[ "$pre_migration_ready" == *'"migrations":false'* ]] && break
  sleep 2
done
[[ "$pre_migration_ready" == *'"migrations":false'* ]] || {
  echo 'backend did not report pending migrations before the explicit migrate step' >&2
  exit 1
}
compose stop backend
compose run --rm -T minio-init
compose run --rm -T migrate
if command -v cygpath >/dev/null 2>&1; then
  bootstrap_secret_windows="$(cygpath -w "$secret_directory/bootstrap_password")"
  compose_file_windows="$(cygpath -w "$compose_file")"
  MSYS2_ARG_CONV_EXCL='*' docker compose -f "$compose_file_windows" -p "$project" run --rm -T \
    --volume "$bootstrap_secret_windows:/run/secrets/bootstrap_password:ro" \
    --env KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/bootstrap_password \
    bootstrap --email admin@example.test --workspace-name 'Smoke Workspace' --display-name 'Smoke Admin'
else
  compose run --rm -T \
    --volume "$secret_directory/bootstrap_password:/run/secrets/bootstrap_password:ro" \
    --env KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/bootstrap_password \
    bootstrap --email admin@example.test --workspace-name 'Smoke Workspace' --display-name 'Smoke Admin'
fi
compose up -d --no-deps backend web caddy

deadline=$((SECONDS + timeout_seconds))
live=false ready=false login=false
while ((SECONDS < deadline)); do
  [[ "$(curl -fsS "http://localhost:$http_port/api/v1/health/live" 2>/dev/null || true)" == *'"status":"ok"'* ]] && live=true || live=false
  [[ "$(curl -fsS "http://localhost:$http_port/api/v1/health/ready" 2>/dev/null || true)" == *'"status":"ready"'* ]] && ready=true || ready=false
  [[ "$(curl -fsS "http://localhost:$http_port/" 2>/dev/null || true)" == *'<div id="root"></div>'* ]] && login=true || login=false
  $live && $ready && $login && break
  sleep 2
done
if ! $live || ! $ready || ! $login; then
  compose ps >&2 || true
  compose logs --no-color --tail 120 >&2 || true
  echo "production smoke timed out (live=$live ready=$ready login=$login)" >&2
  exit 1
fi
printf 'compose smoke passed: project=%s\n' "$project"
