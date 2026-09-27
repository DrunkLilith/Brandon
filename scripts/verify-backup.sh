#!/usr/bin/env bash
set -euo pipefail

test_mode=false
timeout_seconds=420
while (($#)); do
  case "$1" in
    --test-mode) test_mode=true ;;
    --timeout-seconds) shift; timeout_seconds="${1:?missing timeout}" ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done
$test_mode || { echo 'verify-backup requires --test-mode; production restore targets must be operated explicitly' >&2; exit 2; }

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
compose_file="$repo_root/deploy/compose.yaml"
suffix="$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
source_project="kp-verify-$suffix"
restore_project="kp-restore-$suffix"
test_root="${TMPDIR:-/tmp}/$source_project"
secret_directory="$test_root/secrets"
backup_powershell="$test_root/backup-powershell"
backup_bash="$test_root/backup-bash"
restore_target="$test_root/restore-target"
marker_file="$test_root/marker.txt"
http_port="$((20000 + RANDOM % 18000))"
public_origin="http://localhost:$http_port"

compose() { local project="$1"; shift; docker compose -f "$compose_file" -p "$project" "$@"; }
cleanup() {
  compose "$source_project" down --remove-orphans --volumes >/dev/null 2>&1 || true
  compose "$restore_project" down --remove-orphans --volumes >/dev/null 2>&1 || true
  [[ ! -d "$test_root" ]] || rm -rf -- "$test_root"
}
trap cleanup EXIT

wait_storage() {
  local project="$1" deadline=$((SECONDS + timeout_seconds)) postgres_id minio_id
  while ((SECONDS < deadline)); do
    postgres_id="$(compose "$project" ps -q postgres)"
    minio_id="$(compose "$project" ps -q minio)"
    if [[ -n "$postgres_id" && -n "$minio_id" ]] &&
       [[ "$(docker inspect -f '{{.State.Health.Status}}' "$postgres_id")" == healthy ]] &&
       [[ "$(docker inspect -f '{{.State.Health.Status}}' "$minio_id")" == healthy ]]; then
      return
    fi
    sleep 2
  done
  echo "storage did not become healthy for $project" >&2
  return 1
}

wait_public_ready() {
  local deadline=$((SECONDS + timeout_seconds)) body
  while ((SECONDS < deadline)); do
    body="$(curl -fsS "$public_origin/api/v1/health/ready" 2>/dev/null || true)"
    [[ "$body" == *'"status":"ready"'* ]] && return
    sleep 2
  done
  echo 'public API did not become ready' >&2
  return 1
}

json_field() {
  local field="$1"
  sed -n "s/.*\"$field\":\"\([^\"]*\)\".*/\1/p"
}

run_powershell_backup() {
  if command -v cygpath >/dev/null 2>&1; then
    pwsh.exe -NoProfile -File "$(cygpath -w "$repo_root/scripts/backup.ps1")" \
      -ProjectName "$source_project" -TargetDirectory "$(cygpath -w "$backup_powershell")"
  else
    pwsh -NoProfile -File "$repo_root/scripts/backup.ps1" -ProjectName "$source_project" -TargetDirectory "$backup_powershell"
  fi
}

umask 077
mkdir -p -- "$secret_directory" "$backup_powershell" "$backup_bash" "$restore_target"
openssl rand -base64 32 | tr -d '\r\n' >"$secret_directory/master_key"
openssl rand -base64 32 | tr -d '\r\n' >"$secret_directory/wrong_master_key"
printf 'db-%s' "$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')" >"$secret_directory/database_password"
printf 'kp%s' "$suffix" >"$secret_directory/object_access_key"
printf 'obj-%s' "$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')" >"$secret_directory/object_secret_key"
bootstrap_password="Bootstrap-$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')!"
printf '%s' "$bootstrap_password" >"$secret_directory/bootstrap_password"
marker="backup-marker-$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
attachment_marker="attachment-$marker"
printf '%s\n' "$attachment_marker" >"$marker_file"

export KNOWLEDGE_MASTER_KEY_FILE="$secret_directory/master_key"
export KNOWLEDGE_DATABASE_PASSWORD_FILE="$secret_directory/database_password"
export KNOWLEDGE_OBJECT_ACCESS_KEY_FILE="$secret_directory/object_access_key"
export KNOWLEDGE_OBJECT_SECRET_KEY_FILE="$secret_directory/object_secret_key"
export KNOWLEDGE_SITE_ADDRESS='http://localhost'
export KNOWLEDGE_PUBLIC_ORIGIN="$public_origin"
export KNOWLEDGE_HTTP_PORT="$http_port"
export KNOWLEDGE_HTTPS_PORT="$((http_port + 1))"

compose "$source_project" config --quiet
compose "$source_project" build backend web
compose "$source_project" up -d postgres minio
wait_storage "$source_project"
compose "$source_project" run --rm -T minio-init
compose "$source_project" run --rm -T migrate
if command -v cygpath >/dev/null 2>&1; then
  compose_file_windows="$(cygpath -w "$compose_file")"
  bootstrap_windows="$(cygpath -w "$secret_directory/bootstrap_password")"
  MSYS2_ARG_CONV_EXCL='*' docker compose -f "$compose_file_windows" -p "$source_project" run --rm -T \
    --volume "$bootstrap_windows:/run/secrets/bootstrap_password:ro" \
    --env KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/bootstrap_password \
    bootstrap --email admin@example.test --workspace-name 'Backup Verification' --display-name 'Backup Admin'
else
  compose "$source_project" run --rm -T \
    --volume "$secret_directory/bootstrap_password:/run/secrets/bootstrap_password:ro" \
    --env KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/bootstrap_password \
    bootstrap --email admin@example.test --workspace-name 'Backup Verification' --display-name 'Backup Admin'
fi
compose "$source_project" up -d --no-deps backend web caddy
wait_public_ready

headers="$test_root/login-headers"
login_body="$test_root/login-body"
printf '{"email":"admin@example.test","password":"%s"}' "$bootstrap_password" |
  curl -fsS -D "$headers" -o "$login_body" -X POST "$public_origin/api/v1/auth/login" \
    -H 'Content-Type: application/json' -H "Origin: $public_origin" -H 'Sec-Fetch-Site: same-origin' \
    --data-binary @-
cookie="$(awk 'BEGIN { IGNORECASE=1 } /^set-cookie:/ { sub(/\r$/, ""); split($2, value, ";"); print value[1]; exit }' "$headers")"
[[ "$cookie" == knowledge_session=* ]] || { echo 'login did not return a session cookie' >&2; exit 1; }
csrf="$(curl -fsS "$public_origin/api/v1/auth/csrf" -H "Cookie: $cookie" | json_field token)"
[[ -n "$csrf" ]] || { echo 'CSRF endpoint did not return a token' >&2; exit 1; }
mutation_headers=(-H "Cookie: $cookie" -H "X-CSRF-Token: $csrf" -H "Origin: $public_origin" -H 'Sec-Fetch-Site: same-origin')
space_json="$(curl -fsS -X POST "$public_origin/api/v1/spaces" -H 'Content-Type: application/json' "${mutation_headers[@]}" --data '{"name":"Backup Space","description":"restore drill"}')"
space_id="$(printf '%s' "$space_json" | json_field id)"
[[ -n "$space_id" ]] || { echo "space creation failed: $space_json" >&2; exit 1; }
page_json="$(curl -fsS -X POST "$public_origin/api/v1/pages" -H 'Content-Type: application/json' "${mutation_headers[@]}" \
  --data "{\"spaceId\":\"$space_id\",\"parentId\":null,\"title\":\"Backup Marker\",\"markdown\":\"$marker\",\"properties\":{},\"restricted\":false}")"
page_id="$(printf '%s' "$page_json" | json_field id)"
[[ -n "$page_id" ]] || { echo "page creation failed: $page_json" >&2; exit 1; }
attachment_size="$(wc -c <"$marker_file" | tr -d ' ')"
curl_marker_file="$marker_file"
command -v cygpath >/dev/null 2>&1 && curl_marker_file="$(cygpath -w "$marker_file")"
attachment_json="$(curl -fsS -X POST "$public_origin/api/v1/pages/$page_id/attachments?size=$attachment_size" "${mutation_headers[@]}" \
  -F "file=@$curl_marker_file;type=text/plain;filename=marker.txt")"
attachment_id="$(printf '%s' "$attachment_json" | json_field id)"
[[ -n "$attachment_id" ]] || { echo "attachment upload failed: $attachment_json" >&2; exit 1; }
curl -fsS "$public_origin/api/v1/attachments/$attachment_id" -H "Cookie: $cookie" -o "$test_root/seed-download.txt"
cmp -s -- "$marker_file" "$test_root/seed-download.txt" || { echo 'seed attachment round trip failed before backup' >&2; exit 1; }

compose "$source_project" stop backend
run_powershell_backup
"$repo_root/scripts/backup.sh" --project "$source_project" --target "$backup_bash"
cmp -s -- "$backup_powershell/manifest.sha256" "$backup_bash/manifest.sha256" || { echo 'PowerShell and Bash backup manifests differ' >&2; exit 1; }
[[ -z "$(find "$backup_powershell" "$backup_bash" -iname '*master*key*' -print -quit)" ]] || { echo 'backup bundle contains master-key material' >&2; exit 1; }

compose "$source_project" down --remove-orphans
"$repo_root/scripts/restore.sh" --project "$restore_project" --source "$backup_bash" --target "$restore_target"

compose "$restore_project" rm -sf backend >/dev/null 2>&1 || true
export KNOWLEDGE_MASTER_KEY_FILE="$secret_directory/master_key"
set +e
missing_output="$(compose "$restore_project" run --rm -T --no-deps --env KNOWLEDGE_MASTER_KEY_FILE=/run/secrets/not_supplied backend 2>&1)"
missing_status=$?
set -e
[[ $missing_status -ne 0 && "$missing_output" == *'KNOWLEDGE_MASTER_KEY_FILE'* ]] || { echo 'backend did not fail safely without the master-key file' >&2; exit 1; }
[[ "$missing_output" != *"$(cat "$secret_directory/master_key")"* ]] || { echo 'missing-key failure leaked the master key' >&2; exit 1; }

export KNOWLEDGE_MASTER_KEY_FILE="$secret_directory/wrong_master_key"
compose "$restore_project" up -d --no-deps backend
wrong_deadline=$((SECONDS + 90))
wrong_key_rejected=false
while ((SECONDS < wrong_deadline)); do
  ready_body="$(compose "$restore_project" exec -T backend curl --silent 'http://127.0.0.1:3000/api/v1/health/ready' 2>/dev/null || true)"
  [[ "$ready_body" == *'"masterKey":false'* ]] && { wrong_key_rejected=true; break; }
  sleep 2
done
$wrong_key_rejected || { echo 'restored backend did not fail readiness with the wrong master key' >&2; exit 1; }
compose "$restore_project" rm -sf backend

export KNOWLEDGE_MASTER_KEY_FILE="$secret_directory/master_key"
compose "$restore_project" up -d --no-deps backend web caddy
wait_public_ready
restored_headers="$test_root/restored-login-headers"
printf '{"email":"admin@example.test","password":"%s"}' "$bootstrap_password" |
  curl -fsS -D "$restored_headers" -o /dev/null -X POST "$public_origin/api/v1/auth/login" \
    -H 'Content-Type: application/json' -H "Origin: $public_origin" -H 'Sec-Fetch-Site: same-origin' \
    --data-binary @-
restored_cookie="$(awk 'BEGIN { IGNORECASE=1 } /^set-cookie:/ { sub(/\r$/, ""); split($2, value, ";"); print value[1]; exit }' "$restored_headers")"
restored_page="$(curl -fsS "$public_origin/api/v1/pages/$page_id" -H "Cookie: $restored_cookie")"
[[ "$(printf '%s' "$restored_page" | json_field markdown)" == "$marker" ]] || { echo 'restored page marker does not match' >&2; exit 1; }
curl -fsS "$public_origin/api/v1/attachments/$attachment_id" -H "Cookie: $restored_cookie" -o "$test_root/restored-download.txt"
cmp -s -- "$marker_file" "$test_root/restored-download.txt" || { echo 'restored attachment marker does not match' >&2; exit 1; }
printf 'backup verification passed: source=%s restore=%s\n' "$source_project" "$restore_project"
