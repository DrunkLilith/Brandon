#!/usr/bin/env bash
set -euo pipefail
command -v cygpath >/dev/null 2>&1 && export MSYS2_ARG_CONV_EXCL='*'

image='alpine:3.22.1@sha256:4bcff63911fcb4448bd4fdacec207030997caf25e9bea4045fa6c8c44de311d1'
suffix="$(od -An -N6 -tx1 /dev/urandom | tr -d ' \n')"
volume="kp-permissions-$suffix"
[[ "$volume" =~ ^kp-permissions-[a-f0-9]{12}$ ]] || { echo 'unsafe probe volume name' >&2; exit 1; }

cleanup() {
  local owner
  owner="$(docker volume inspect --format '{{ index .Labels "knowledge-platform.release-owned" }}' "$volume" 2>/dev/null || true)"
  [[ "$owner" != true ]] || docker volume rm --force "$volume" >/dev/null
}
trap cleanup EXIT

docker volume create --label knowledge-platform.release-owned=true "$volume" >/dev/null
docker run --rm --volume "$volume:/case" "$image" sh -ec '
  mkdir -p /case/secrets /case/backup
  chmod 700 /case/secrets /case/backup
  printf test-only >/case/secrets/key
  chmod 644 /case/secrets/key
  chown 1000:1000 /case/backup
'

docker run --rm --user 10001:10001 \
  --mount "type=volume,src=$volume,dst=/run/secrets/key,volume-subpath=secrets/key,readonly" \
  "$image" sh -ec '[[ "$(cat /run/secrets/key)" == test-only ]]'

if docker run --rm --user 10001:10001 --volume "$volume:/case:ro" "$image" cat /case/secrets/key >/dev/null 2>&1; then
  echo 'private host secret directory was traversable by the service UID' >&2
  exit 1
fi

docker run --rm --user 1000:1000 --volume "$volume:/case" "$image" sh -ec \
  'umask 077; printf backup-test >/case/backup/database.sql'
permissions="$(docker run --rm --volume "$volume:/case:ro" "$image" stat -c '%a:%u:%g' /case/backup /case/backup/database.sql)"
[[ "$permissions" == $'700:1000:1000\n600:1000:1000' ]] || { echo "unsafe backup permissions: $permissions" >&2; exit 1; }
docker run --rm --user 1000:1000 --volume "$volume:/case:ro" "$image" sh -ec \
  '[[ "$(cat /case/backup/database.sql)" == backup-test ]]'

printf 'native Linux permission probe PASS\n'
