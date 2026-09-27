#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
operational_files=(
  deploy/compose.yaml
  scripts/backup.sh
  scripts/backup.ps1
  scripts/restore.sh
  scripts/restore.ps1
)
for relative in "${operational_files[@]}"; do
  file="$repo_root/$relative"
  if grep -F -- 'mc alias set' "$file" >/dev/null; then
    printf '%s expands MinIO credentials into mc argv\n' "$relative" >&2
    exit 1
  fi
  grep -F -- 'mc-credentials http://minio:9000' "$file" >/dev/null || {
    printf '%s does not source MinIO credentials from the mounted secret files\n' "$relative" >&2
    exit 1
  }
  grep -F -- 'mc alias import local' "$file" >/dev/null || {
    printf '%s does not import MinIO credentials through standard input\n' "$relative" >&2
    exit 1
  }
done

docker build --quiet -f "$repo_root/deploy/mc.Dockerfile" -t knowledge-platform/mc:task19-secret-test "$repo_root" >/dev/null
suffix="$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
container="kp-mc-secret-$suffix"
root="${TMPDIR:-/tmp}/$container"
mkdir -p -- "$root"
access='access-"quoted"'
secret='secret-\backslash'
printf '%s' "$access" >"$root/access"
printf '%s' "$secret" >"$root/secret"
cleanup() {
  docker rm -f "$container" >/dev/null 2>&1 || true
  rm -rf -- "$root"
}
trap cleanup EXIT

access_mount="$root/access"
secret_mount="$root/secret"
if command -v cygpath >/dev/null 2>&1; then
  access_mount="$(cygpath -w "$access_mount")"
  secret_mount="$(cygpath -w "$secret_mount")"
fi
MSYS2_ARG_CONV_EXCL='*' docker run --detach --name "$container" \
  --tmpfs /tmp:rw,noexec,nosuid,size=8m,mode=0700,uid=10002,gid=10002 \
  --env MC_CONFIG_DIR=/tmp/.mc \
  --volume "$access_mount:/run/secrets/object_access_key:ro" \
  --volume "$secret_mount:/run/secrets/object_secret_key:ro" \
  --entrypoint /bin/sh knowledge-platform/mc:task19-secret-test -ec \
  'mc-credentials http://minio:9000 | mc alias import local >/dev/null
   mc alias export local >/dev/null
   sleep 30' >/dev/null
sleep 1
[[ "$(docker inspect --format '{{.State.Running}}' "$container")" == true ]] || {
  docker logs "$container" >&2 || true
  echo 'pinned mc image did not import the secret-file credentials' >&2
  exit 1
}
metadata="$(docker inspect --format '{{json .Config.Cmd}} {{json .Config.Env}}' "$container")"
[[ "$metadata" != *"$access"* && "$metadata" != *"$secret"* ]] || {
  echo 'MinIO credential leaked into container argv or environment' >&2
  exit 1
}

printf 'mc secret transport passed\n'
