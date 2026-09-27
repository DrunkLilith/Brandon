#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
suffix="$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
collision_project="kp-restore-$suffix"
cleanup_suffix="$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
cleanup_project="kp-restore-$cleanup_suffix"
root="${TMPDIR:-/tmp}/kp-volume-ownership-$suffix"
bundle="$root/bundle"
collision_target="$root/collision-target"
cleanup_target="$root/cleanup-target"
collision_compose="$root/collision-compose.yaml"
cleanup_compose="$root/cleanup-compose.yaml"
collision_volume="${collision_project}_postgres_data"
preserved_volume="kp-preserved-$suffix"

cleanup() {
  docker compose -f "$collision_compose" -p "$collision_project" down --remove-orphans >/dev/null 2>&1 || true
  docker compose -f "$cleanup_compose" -p "$cleanup_project" down --remove-orphans >/dev/null 2>&1 || true
  docker volume rm "$collision_volume" "$preserved_volume" >/dev/null 2>&1 || true
  for volume in "${cleanup_project}_postgres_data" "${cleanup_project}_minio_data"; do
    docker volume rm "$volume" >/dev/null 2>&1 || true
  done
  [[ ! -d "$root" ]] || rm -rf -- "$root"
}
trap cleanup EXIT

mkdir -p -- "$bundle/postgres" "$bundle/objects" "$bundle/metadata" "$collision_target" "$cleanup_target"
printf 'placeholder\n' >"$bundle/postgres/database.sql"
: >"$bundle/metadata/object-index.jsonl"
printf 'knowledge-platform-backup-v1\n' >"$bundle/bundle-version.txt"
(
  cd -- "$bundle"
  while IFS= read -r file; do
    printf '%s  %s\n' "$(sha256sum "$file" | awk '{print $1}')" "$file"
  done < <(printf '%s\n' bundle-version.txt metadata/object-index.jsonl postgres/database.sql)
) >"$bundle/manifest.sha256"

cat >"$collision_compose" <<'YAML'
services:
  postgres:
    image: alpine:3.22.1
    network_mode: "container:kp-volume-owner-missing"
    volumes:
      - postgres_data:/data
  minio:
    image: alpine:3.22.1
    network_mode: "container:kp-volume-owner-missing"
    volumes:
      - minio_data:/data
volumes:
  postgres_data:
  minio_data:
YAML
cat >"$cleanup_compose" <<'YAML'
services:
  postgres:
    image: alpine:3.22.1
    command: ["sh", "-c", "sleep 300"]
    healthcheck:
      test: ["CMD", "false"]
      interval: 1s
      timeout: 1s
      retries: 1
    volumes:
      - postgres_data:/data
  minio:
    image: alpine:3.22.1
    command: ["sh", "-c", "sleep 300"]
    healthcheck:
      test: ["CMD", "false"]
      interval: 1s
      timeout: 1s
      retries: 1
    volumes:
      - minio_data:/data
volumes:
  postgres_data:
  minio_data:
YAML

docker volume create "$collision_volume" >/dev/null
set +e
collision_output="$("$repo_root/scripts/restore.sh" --project "$collision_project" --source "$bundle" --target "$collision_target" --compose-file "$collision_compose" 2>&1)"
collision_status=$?
set -e
[[ $collision_status -ne 0 && "$collision_output" == *'restore project already has volume'* ]] || {
  printf 'exact-name collision was not rejected before restore: %s\n' "$collision_output" >&2
  exit 1
}
docker volume inspect "$collision_volume" >/dev/null 2>&1 || {
  echo 'restore deleted a pre-existing exact-name volume' >&2
  exit 1
}

docker volume create "$preserved_volume" >/dev/null
set +e
cleanup_output="$("$repo_root/scripts/restore.sh" --project "$cleanup_project" --source "$bundle" --target "$cleanup_target" --compose-file "$cleanup_compose" --storage-timeout-seconds 1 2>&1)"
cleanup_status=$?
set -e
[[ $cleanup_status -ne 0 && "$cleanup_output" == *'restore storage did not become healthy'* ]] || {
  printf 'controlled restore failure did not occur: %s\n' "$cleanup_output" >&2
  exit 1
}
docker volume inspect "$preserved_volume" >/dev/null 2>&1 || {
  echo 'restore cleanup deleted an unrelated volume' >&2
  exit 1
}
for volume in "${cleanup_project}_postgres_data" "${cleanup_project}_minio_data"; do
  if docker volume inspect "$volume" >/dev/null 2>&1; then
    printf 'restore cleanup retained its owned volume: %s\n' "$volume" >&2
    exit 1
  fi
done

printf 'restore volume ownership passed\n'
