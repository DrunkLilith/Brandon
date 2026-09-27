#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
root="${TMPDIR:-/tmp}/kp-backup-restart-$RANDOM-$RANDOM"
bin_dir="$root/bin"
compose_file="$root/compose.yaml"
mkdir -p -- "$bin_dir"
printf 'services: {}\n' >"$compose_file"
cat >"$bin_dir/cygpath" <<'SH'
#!/usr/bin/env bash
[[ "${1:-}" == -w && $# -eq 2 ]] || exit 2
printf '%s' "$2"
SH
cat >"$bin_dir/docker" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
joined=" $* "
case "$joined" in
  *' ps -q postgres '*) printf 'postgres-id\n' ;;
  *' ps -q minio '*) printf 'minio-id\n' ;;
  *' ps -q backend '*)
    [[ "${FAKE_DOCKER_MODE:-}" != state-query-fail ]] || { echo 'BACKEND_STATE_QUERY_FAILURE' >&2; exit 42; }
    printf 'backend-id\n'
    ;;
  *' stop backend '*) ;;
  *' start backend '*)
    [[ "${FAKE_DOCKER_MODE:-}" != start-fail ]] || { echo 'simulated backend start failure' >&2; exit 1; }
    [[ "${FAKE_DOCKER_MODE:-}" != primary-and-restart-fail ]] || { echo 'SECONDARY_RESTART_FAILURE' >&2; exit 1; }
    ;;
  *' run '*)
    [[ "${FAKE_DOCKER_MODE:-}" != primary-and-restart-fail || "$joined" != *' postgres '* ]] || { echo 'PRIMARY_BACKUP_FAILURE' >&2; exit 17; }
    mount=''
    previous=''
    for argument in "$@"; do
      [[ "$previous" != --volume ]] || mount="$argument"
      previous="$argument"
    done
    host="${mount%:/backup}"
    if [[ "$joined" == *' postgres '* ]]; then
      mkdir -p -- "$host"
      printf 'database dump\n' >"$host/database.sql"
    else
      mkdir -p -- "$host/objects" "$host/metadata"
      printf 'ciphertext\n' >"$host/objects/object"
      printf '{}\n' >"$host/metadata/object-index.jsonl"
    fi
    ;;
  *)
    if [[ "${1:-}" == inspect ]]; then
      printf 'unhealthy\n'
    fi
    ;;
esac
SH
chmod +x "$bin_dir/cygpath" "$bin_dir/docker"
cleanup() { rm -rf -- "$root"; }
trap cleanup EXIT

run_failure_case() {
  local mode="$1" expected="$2" output status
  local target="$root/$mode"
  local timeout_arguments=()
  [[ "$mode" == start-fail ]] || timeout_arguments=(--restart-timeout-seconds 1)
  mkdir -- "$target"
  set +e
  output="$(PATH="$bin_dir:$PATH" FAKE_DOCKER_MODE="$mode" \
    "$repo_root/scripts/backup.sh" --project kp-restart-test --target "$target" \
      --compose-file "$compose_file" "${timeout_arguments[@]}" 2>&1)"
  status=$?
  set -e
  [[ $status -ne 0 && "$output" == *"$expected"* && "$output" != *'backup completed:'* ]] || {
    printf 'backup did not report %s truthfully: status=%s output=%s\n' "$mode" "$status" "$output" >&2
    exit 1
  }
}

run_failure_case start-fail 'backend failed to restart after backup'
run_failure_case readiness-fail 'backend failed to return ready after backup'

target="$root/primary-and-restart-fail"
mkdir -- "$target"
set +e
output="$(PATH="$bin_dir:$PATH" FAKE_DOCKER_MODE=primary-and-restart-fail \
  "$repo_root/scripts/backup.sh" --project kp-restart-test --target "$target" --compose-file "$compose_file" 2>&1)"
status=$?
set -e
[[ $status -ne 0 && "$output" == *PRIMARY_BACKUP_FAILURE* && "$output" == *SECONDARY_RESTART_FAILURE* && "$output" != *'backup completed:'* ]] || {
  printf 'backup did not preserve primary and recovery failures: status=%s output=%s\n' "$status" "$output" >&2
  exit 1
}

target="$root/state-query-fail"
mkdir -- "$target"
set +e
output="$(PATH="$bin_dir:$PATH" FAKE_DOCKER_MODE=state-query-fail \
  "$repo_root/scripts/backup.sh" --project kp-restart-test --target "$target" --compose-file "$compose_file" 2>&1)"
status=$?
set -e
[[ $status -ne 0 && "$output" == *'backend state could not be determined'* && "$output" != *BACKEND_STATE_QUERY_FAILURE* && "$output" != *'backup completed:'* && ! -e "$target/postgres/database.sql" && ! -e "$target/objects/object" ]] || {
  printf 'backup did not fail closed on backend state query failure: status=%s output=%s\n' "$status" "$output" >&2
  exit 1
}
printf 'backup restart safety passed\n'
