#!/bin/bash
# Build on a Mac. No signing account, server, Docker, or Windows SDK is needed.
set -euo pipefail

project_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$project_root"

if [[ "$(uname -s)" != Darwin ]]; then
  printf '%s\n' 'This script must run on macOS. See START-HERE-MACOS.md (or docs/build-macos.md).'
  exit 1
fi
if [[ "${1:-}" != '' && "${1:-}" != '--universal' ]] || [[ "$#" -gt 1 ]]; then
  printf '%s\n' 'Usage: bash scripts/build-macos.sh [--universal]'
  exit 1
fi
if ! xcrun --find clang >/dev/null 2>&1 || ! xcrun --show-sdk-path >/dev/null 2>&1; then
  printf '%s\n' 'Install Apple Command Line Tools first: xcode-select --install'
  exit 1
fi
# rustup's installer normally adds this directory after reopening Terminal.
if [[ -f "$HOME/.cargo/env" ]]; then source "$HOME/.cargo/env"; fi
for executable in node npm rustup ditto codesign; do
  if ! command -v "$executable" >/dev/null 2>&1; then
    printf 'Missing %s. Follow the prerequisites in docs/build-macos.md.\n' "$executable"
    exit 1
  fi
done
node -e 'if (Number(process.versions.node.split(".")[0]) < 24) { console.error("Node.js 24 or newer is required"); process.exit(1) }'

case "$(uname -m)" in
  arm64) build_target=aarch64-apple-darwin ;;
  x86_64) build_target=x86_64-apple-darwin ;;
  *) printf '%s\n' 'Unsupported Mac architecture'; exit 1 ;;
esac
if [[ "${1:-}" == '--universal' ]]; then build_target=universal-apple-darwin; fi

printf '\nPreparing Rust 1.98.0 for %s…\n' "$build_target"
rustup toolchain install 1.98.0 --profile minimal --component clippy,rustfmt
if [[ "$build_target" == universal-apple-darwin ]]; then
  rustup target add --toolchain 1.98.0 aarch64-apple-darwin x86_64-apple-darwin
else
  rustup target add --toolchain 1.98.0 "$build_target"
fi

# npm exec runs this exact pnpm without changing a global pnpm installation.
pnpm_run() { npm exec --yes --package=pnpm@11.19.0 -- pnpm "$@"; }
pnpm_run install --frozen-lockfile
pnpm_run --dir apps/desktop typecheck
pnpm_run --dir apps/desktop lint
cargo +1.98.0 test --locked -p knowledge-markdown -p knowledge-vault

# Keep output paths predictable even if the caller has CARGO_TARGET_DIR configured.
export CARGO_TARGET_DIR="$project_root/target"
export MACOSX_DEPLOYMENT_TARGET=12.0
export APPLE_SIGNING_IDENTITY=-
# This is a local ad-hoc demo build, not an Apple-notarized public release.
unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_PASSWORD APPLE_TEAM_ID
unset APPLE_API_KEY APPLE_API_KEY_PATH APPLE_API_ISSUER
pnpm_run --dir apps/desktop tauri build --target "$build_target" --bundles app --ci -- --locked

app_path="$project_root/target/$build_target/release/bundle/macos/Brandon.app"
if [[ ! -d "$app_path" ]]; then printf 'Missing expected app: %s\n' "$app_path"; exit 1; fi
codesign --verify --deep --strict "$app_path"
version="$(node -p 'JSON.parse(require("fs").readFileSync("apps/desktop/package.json", "utf8")).version')"
output_dir="$project_root/dist-macos"
mkdir -p "$output_dir"
release_zip="$output_dir/Brandon-$version-$build_target-$(date '+%Y%m%d-%H%M%S').zip"
if [[ -e "$release_zip" ]]; then printf 'Output already exists: %s\n' "$release_zip"; exit 1; fi
ditto -c -k --sequesterRsrc --keepParent "$app_path" "$release_zip"
printf '\nApp: %s\nShareable ZIP: %s\n' "$app_path" "$release_zip"
printf '%s\n' 'Launch the app with:'
printf 'open "%s"\n' "$app_path"
