# Brandon desktop

Local-first Markdown workspace built with Tauri 2, Rust, React and CodeMirror.
Notes and boards remain ordinary `.md` and JSON Canvas `.canvas` files. Managed
task state is authoritative in the vault's `.brandon/tasks.sqlite3`; back up that
database together with notes and attachments. Do not treat it as a disposable
search cache.

## Develop on Windows

Install Rust 1.98 (see the root toolchain file), Node.js 24, pnpm 11.19, Microsoft
C++ Build Tools/Windows SDK and WebView2. Follow the official
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

Run from the repository root:

```powershell
pnpm install --frozen-lockfile
pnpm --dir apps/desktop desktop:dev
```

No server, PostgreSQL, Docker, model download or AI account is needed. Open a
new test folder as a vault; no personal demo vault is shipped with these sources.

## Build

```powershell
pnpm --dir apps/desktop desktop:build
```

On Windows this produces `target/release/knowledge-desktop.exe` and an NSIS
installer under `target/release/bundle/nsis/`. Native GPU Canvas is automatic on
Windows, with recovery to the ordinary renderer if unavailable. macOS currently
uses DOM/2D; see [the Mac guide](../../docs/build-macos.md). Actual Mac execution,
mixed-DPI monitor transitions and sleep/resume remain unverified.

## Verify

```powershell
pnpm --dir apps/desktop build
pnpm --dir apps/desktop lint
pnpm --dir apps/desktop test:canvas
cargo fmt --all -- --check
cargo test -p knowledge-vault -p knowledge-markdown -p knowledge-desktop
cargo clippy -p knowledge-vault -p knowledge-desktop --all-targets -- -D warnings
```

Live desktop scenarios are in `tests/desktop`. On Windows, launch the standalone
build with a local WebView2 debug endpoint only for development:

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9222'
& ./target/release/knowledge-desktop.exe
```

Use an isolated test workspace and save any work first. Run one live suite at a
time: they create generated fixture vaults, temporarily switch the application
to those vaults and restore the original vault afterwards. Examples:

```powershell
node tests/desktop/canvas-interaction-ui-smoke.mjs
node tests/desktop/canvas-attachments-smoke.mjs
pnpm --dir apps/desktop test:render
pnpm --dir apps/desktop test:smoke
```

Restart without the debug environment variable for normal use. Tests and GPU
measurements are development checks, not a universal frame-rate guarantee.

## Current boundaries

- Image thumbnails and arbitrary file references are implemented. Embedded
  PDF/audio/video viewers are not complete; unsupported files get a detail peek.
- The ordinary UI search remains Markdown search. Experimental FTS code is not
  promoted into that UI path; vector search and scoped agent integrations remain
  planned work.
- `scripts/local-ai` contains optional, isolated OCR/ASR experiments, not bundled
  product workers. Model caches, sample downloads and reports stay outside Git.
- Personal notification automation and its credentials are local-only, not part
  of this application's source distribution.

See [the roadmap](../../docs/canvas-planner-product.md) for verified checkpoints
and remaining work. Keep real vaults, tokens, keys and local state out of commits.
