#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"

for relative in tests/operations/compose-smoke.sh scripts/verify-backup.sh; do
  script="$repo_root/$relative"
  if grep -E -- '--data[^[:space:]]*[[:space:]].*\$bootstrap_password|--data[[:space:]].*\$bootstrap_password' "$script" >/dev/null; then
    printf '%s passes the bootstrap password through process arguments\n' "$relative" >&2
    exit 1
  fi
done

grep -F -- '--data-binary @-' "$repo_root/scripts/verify-backup.sh" >/dev/null || {
  printf '%s\n' 'scripts/verify-backup.sh does not send the login body through standard input' >&2
  exit 1
}

printf 'script secret argv safety passed\n'
