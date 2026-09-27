#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

project=''
source_dir=''
target=''
compose_file=''
storage_timeout_seconds=120
while (($#)); do
  case "$1" in
    --project) shift; project="${1:?missing project}" ;;
    --source) shift; source_dir="${1:?missing source}" ;;
    --target) shift; target="${1:?missing target}" ;;
    --compose-file) shift; compose_file="${1:?missing compose file}" ;;
    --storage-timeout-seconds) shift; storage_timeout_seconds="${1:?missing storage timeout}" ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/.." && pwd -P)"
compose_file="${compose_file:-$repo_root/deploy/compose.yaml}"
safe_absolute_dir() {
  local label="$1" value="$2"
  case "$value" in *[\*\?\[\]]*) echo "$label must be an existing absolute directory without glob characters" >&2; exit 2 ;; esac
  [[ -n "$value" && "$value" == /* && -d "$value" ]] || { echo "$label must be an existing absolute directory without glob characters" >&2; exit 2; }
  local resolved
  resolved="$(cd -- "$value" && pwd -P)"
  [[ "$resolved" != / && "$resolved" != "${HOME:-}" && "$resolved" != "$repo_root" ]] || { echo "$label is an unsafe root directory" >&2; exit 2; }
  printf '%s' "$resolved"
}
[[ "$project" =~ ^kp-restore-[a-f0-9]{12}$ ]] || { echo 'restore project must match kp-restore-<12 hex>' >&2; exit 2; }
[[ "$storage_timeout_seconds" =~ ^[1-9][0-9]*$ ]] || { echo 'storage timeout must be a positive integer' >&2; exit 2; }
source_dir="$(safe_absolute_dir 'backup source' "$source_dir")"
target="$(safe_absolute_dir 'restore target' "$target")"
umask 077
chmod 700 -- "$target"
[[ "$source_dir" != "$target" ]] || { echo 'backup source and restore target must differ' >&2; exit 2; }
[[ -z "$(find "$target" -mindepth 1 -maxdepth 1 -print -quit)" ]] || { echo 'restore target must be empty' >&2; exit 2; }
[[ -f "$source_dir/manifest.sha256" ]] || { echo 'backup bundle is incomplete' >&2; exit 2; }

declare -A listed=()
manifest_pattern='^([a-f0-9]{64})  (.+)$'
previous_relative=''
while IFS= read -r line || [[ -n "$line" ]]; do
  [[ "$line" =~ $manifest_pattern ]] || { echo 'backup manifest is invalid' >&2; exit 2; }
  expected_hash="${BASH_REMATCH[1]}"
  relative="${BASH_REMATCH[2]}"
  [[ "$relative" != /* && "$relative" != *\\* && "$relative" != *:* && ! "$relative" =~ [[:cntrl:]] ]] || { echo 'backup manifest is invalid' >&2; exit 2; }
  case "$relative" in *[\*\?\[\]]*) echo 'backup manifest is invalid' >&2; exit 2 ;; esac
  IFS='/' read -r -a segments <<<"$relative"
  ((${#segments[@]} > 0)) || { echo 'backup manifest is invalid' >&2; exit 2; }
  for segment in "${segments[@]}"; do
    [[ -n "$segment" && "$segment" != . && "$segment" != .. ]] || { echo 'backup manifest is invalid' >&2; exit 2; }
  done
  [[ -z "$previous_relative" || "$relative" > "$previous_relative" ]] || { echo 'backup manifest is invalid' >&2; exit 2; }
  previous_relative="$relative"
  [[ "$relative" != manifest.sha256 && -z "${listed[$relative]+present}" ]] || { echo 'backup manifest is invalid' >&2; exit 2; }
  candidate="$source_dir/$relative"
  [[ -f "$candidate" && ! -L "$candidate" ]] || { echo 'backup manifest is invalid' >&2; exit 2; }
  actual_hash="$(sha256sum -- "$candidate" | awk '{print tolower($1)}')"
  [[ "$actual_hash" == "$expected_hash" ]] || { echo 'backup manifest verification failed' >&2; exit 1; }
  listed["$relative"]=1
done <"$source_dir/manifest.sha256"

declare -A actual=()
while IFS= read -r -d '' entry; do
  [[ ! -L "$entry" ]] || { echo 'backup manifest is invalid' >&2; exit 2; }
  if [[ -f "$entry" ]]; then
    relative="${entry#"$source_dir"/}"
    [[ "$relative" == manifest.sha256 ]] || actual["$relative"]=1
  elif [[ ! -d "$entry" ]]; then
    echo 'backup manifest is invalid' >&2
    exit 2
  fi
done < <(find "$source_dir" -mindepth 1 -print0)
for required in bundle-version.txt postgres/database.sql metadata/object-index.jsonl; do
  [[ -n "${actual[$required]+present}" ]] || { echo 'backup bundle is incomplete' >&2; exit 2; }
done
((${#actual[@]} == ${#listed[@]})) || { echo 'backup manifest is invalid' >&2; exit 2; }
for relative in "${!actual[@]}"; do
  [[ -n "${listed[$relative]+present}" ]] || { echo 'backup manifest is invalid' >&2; exit 2; }
done
cmp -s -- "$source_dir/bundle-version.txt" <(printf 'knowledge-platform-backup-v1\n') || { echo 'unsupported backup bundle version' >&2; exit 2; }

compose() { docker compose -f "$compose_file" -p "$project" "$@"; }
[[ -z "$(compose ps -aq)" ]] || { echo 'restore project already has containers' >&2; exit 1; }
parse_compose_volumes() {
  local line key='' name='' in_volumes=false count=0
  declare -A seen_keys=() seen_names=()
  while IFS= read -r line; do
    if ! $in_volumes; then
      [[ "$line" != '  "volumes": {' ]] || in_volumes=true
      continue
    fi
    if [[ -z "$key" ]]; then
      if [[ "$line" == '  }' || "$line" == '  },' ]]; then
        ((count > 0)) || return 1
        return 0
      fi
      [[ "$line" =~ ^[[:space:]]{4}\"([a-zA-Z0-9][a-zA-Z0-9_.-]*)\":\ \{$ ]] || return 1
      key="${BASH_REMATCH[1]}"
      [[ -z "${seen_keys[$key]+present}" ]] || return 1
      seen_keys["$key"]=1
      continue
    fi
    if [[ -z "$name" ]]; then
      [[ "$line" =~ ^[[:space:]]{6}\"name\":\ \"([a-zA-Z0-9][a-zA-Z0-9_.-]*)\",?$ ]] || return 1
      name="${BASH_REMATCH[1]}"
      [[ -z "${seen_names[$name]+present}" ]] || return 1
      seen_names["$name"]=1
      continue
    fi
    [[ "$line" == '    },' || "$line" == '    }' ]] || return 1
    printf '%s\t%s\n' "$key" "$name"
    key=''
    name=''
    ((count += 1))
  done
  return 1
}
if ! compose_config_json="$(compose config --format json 2>/dev/null)"; then
  echo 'restore Compose volume configuration is invalid' >&2
  exit 1
fi
if ! resolved_volumes="$(parse_compose_volumes <<<"$compose_config_json")"; then
  echo 'restore Compose volume configuration is invalid' >&2
  exit 1
fi
expected_volumes=()
while IFS=$'\t' read -r key volume; do
  [[ -n "$key" && -n "$volume" ]] || { echo 'restore Compose volume configuration is invalid' >&2; exit 1; }
  expected_volumes+=("$volume|$key")
  if docker volume inspect "$volume" >/dev/null 2>&1; then
    echo "restore project already has volume: $volume" >&2
    exit 1
  fi
done <<<"$resolved_volumes"
((${#expected_volumes[@]} > 0)) || { echo 'restore Compose file declares no volumes' >&2; exit 1; }
printf '{"project":"%s","source":"%s"}\n' "$project" "$source_dir" >"$target/restore-state.json"

owned_volumes=()
refresh_owned_volumes() {
  local entry volume key labels
  owned_volumes=()
  for entry in "${expected_volumes[@]}"; do
    volume="${entry%%|*}"
    key="${entry#*|}"
    if docker volume inspect "$volume" >/dev/null 2>&1; then
      labels="$(docker volume inspect --format '{{ index .Labels "com.docker.compose.project" }}|{{ index .Labels "com.docker.compose.volume" }}' "$volume")"
      [[ "$labels" == "$project|$key" ]] || return 1
      owned_volumes+=("$volume")
    fi
  done
}
cleanup_on_error() {
  local code=$?
  if ((code)); then
    compose down --remove-orphans >/dev/null 2>&1 || true
    if refresh_owned_volumes && ((${#owned_volumes[@]} > 0)); then
      docker volume rm "${owned_volumes[@]}" >/dev/null 2>&1 || true
    fi
  fi
  exit "$code"
}
trap cleanup_on_error EXIT
compose up --detach postgres minio
refresh_owned_volumes || { echo 'restore volume ownership could not be established' >&2; exit 1; }
deadline=$((SECONDS + storage_timeout_seconds))
while ((SECONDS < deadline)); do
  postgres_id="$(compose ps -q postgres)"
  minio_id="$(compose ps -q minio)"
  [[ -n "$postgres_id" && -n "$minio_id" ]] || { sleep 2; continue; }
  [[ "$(docker inspect -f '{{.State.Health.Status}}' "$postgres_id")" == healthy && "$(docker inspect -f '{{.State.Health.Status}}' "$minio_id")" == healthy ]] && break
  sleep 2
done
postgres_id="$(compose ps -q postgres)"
minio_id="$(compose ps -q minio)"
[[ -n "$postgres_id" && -n "$minio_id" ]] &&
  [[ "$(docker inspect -f '{{.State.Health.Status}}' "$postgres_id")" == healthy ]] &&
  [[ "$(docker inspect -f '{{.State.Health.Status}}' "$minio_id")" == healthy ]] || {
    echo 'restore storage did not become healthy' >&2
    exit 1
  }
compose run --rm -T minio-init

run_with_mount() {
  local host_path="$1" container_path="$2"
  shift 2
  if command -v cygpath >/dev/null 2>&1; then
    local host_windows compose_windows
    host_windows="$(cygpath -w "$host_path")"
    compose_windows="$(cygpath -w "$compose_file")"
    MSYS2_ARG_CONV_EXCL='*' docker compose -f "$compose_windows" -p "$project" run --rm -T --volume "$host_windows:$container_path" "$@"
  else
    local host_uid host_gid
    host_uid="$(id -u)"; host_gid="$(id -g)"
    compose run --rm -T --user "$host_uid:$host_gid" \
      --tmpfs "/tmp:rw,noexec,nosuid,size=8m,mode=0700,uid=$host_uid,gid=$host_gid" \
      --volume "$host_path:$container_path" "$@"
  fi
}
run_with_mount "$source_dir/postgres" /backup --entrypoint /bin/sh postgres -ec \
  'export PGPASSWORD="$(cat /run/secrets/database_password)"; exec psql --no-psqlrc --set=ON_ERROR_STOP=1 --host=postgres --username=knowledge --dbname=knowledge --file=/backup/database.sql'
run_with_mount "$source_dir/objects" /backup-objects --entrypoint /bin/sh minio-init -ec '
  mc-credentials http://minio:9000 | mc alias import local >/dev/null
  mc mirror --overwrite /backup-objects local/knowledge
'
trap - EXIT
printf 'restore completed: project=%s target=%s\n' "$project" "$target"
