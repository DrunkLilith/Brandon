#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
repo_root="$(cd -- "$script_dir/../.." && pwd -P)"
root="${TMPDIR:-/tmp}/kp-path-safety-$RANDOM-$RANDOM"
bundle="$root/bundle"
target="$root/target"
mkdir -p -- "$bundle/postgres" "$target"
trap 'rm -rf -- "$root"' EXIT
printf 'outside\n' >"$root/escape"
printf 'placeholder\n' >"$bundle/postgres/database.sql"
mkdir -p -- "$bundle/metadata"
: >"$bundle/metadata/object-index.jsonl"
printf 'knowledge-platform-backup-v1\n' >"$bundle/bundle-version.txt"
hash="$(sha256sum "$root/escape" | awk '{print $1}')"
printf '%s  ../escape\n' "$hash" >"$bundle/manifest.sha256"

assert_rejected() {
  local expected="$1" source="${2:-$bundle}" output status
  set +e
  output="$("$repo_root/scripts/restore.sh" --project kp-restore-0123456789ab --source "$source" --target "$target" 2>&1)"
  status=$?
  set -e
  [[ $status -ne 0 && "$output" == *"$expected"* ]] || {
    printf "unsafe restore input was not rejected with '%s': %s\n" "$expected" "$output" >&2
    exit 1
  }
}
assert_rejected 'backup manifest is invalid'

dump_hash="$(sha256sum "$bundle/postgres/database.sql" | awk '{print $1}')"
printf '%s  postgres/database.sql\n%s  postgres/database.sql\n' "$dump_hash" "$dump_hash" >"$bundle/manifest.sha256"
assert_rejected 'backup manifest is invalid'

printf '%s  postgres/database.sql\n' "$dump_hash" >"$bundle/manifest.sha256"
assert_rejected 'backup manifest is invalid'

version_hash="$(sha256sum "$bundle/bundle-version.txt" | awk '{print $1}')"
metadata_hash="$(sha256sum "$bundle/metadata/object-index.jsonl" | awk '{print $1}')"
printf '%s  postgres/database.sql\n%s  bundle-version.txt\n%s  metadata/object-index.jsonl\n' "$dump_hash" "$version_hash" "$metadata_hash" >"$bundle/manifest.sha256"
assert_rejected 'backup manifest is invalid'

# Git Bash cannot create a real Windows symlink without Developer Mode. This
# lexical alias still proves that canonicalization happens before the root
# comparison; the PowerShell companion covers a real NTFS junction.
assert_rejected 'unsafe root directory' "$repo_root/tests/.."
printf 'backup path safety passed\n'
