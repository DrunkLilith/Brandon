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

## Commands, navigation and preferences

The command registry is shared by the palette, application hotkeys, editor menu
and primary toolbar actions. Open Settings with the gear on the left rail, from
the palette, or with `Ctrl+Shift+,`. Editor preferences are stored locally as
`brandon.settings.v1`; the established visual/source preference is migrated.
They do not modify the vault or require new native capabilities.

- Right-click in prose or a fenced code editor, or press `Shift+F10` / the menu
  key. Menus support arrow keys, Enter and Escape. Code disables prose-only
  formatting; native table fields keep their native text menu.
- Outline follows the current buffer and navigates in visual, source, reading
  and split modes. Wiki/Markdown links retain heading/block anchors, including
  same-note and parent-relative targets. Missing/ambiguous targets do not create
  notes. `Ctrl+click` or `Ctrl+Enter` opens a link in the editor.
- Local hover previews can require Ctrl or work without a modifier. They do not
  fetch external URLs. Note tags open the common local search.
- Settings include editor mode, default document view, readable width, line
  numbers, spellcheck and previews. Hotkeys use physical keys across RU/EN,
  detect collisions, support unassign/reset, and preserve native editing keys.

Undo remains scoped to the mounted document editor. R1B additionally provides
two independent document panes, pinned tabs, vault-scoped session restoration,
bookmarks, crash drafts and a bounded source-version history.

## Workspace and recovery

Use the pane buttons beside the document actions (or the command palette) to
split right/down. Each pane has its own documents, tabs, view mode and position;
the separator supports dragging and arrow keys. The primary Canvas retains its
native renderer; a Canvas in the second pane uses the ordinary renderer because
the current native surface has one owner. This release supports two panes, not
an arbitrary nested layout tree.

Pins protect a tab from closing until unpinned. The star sidebar stores document,
heading and search bookmarks; bookmarks can be renamed, reordered and removed.
The document-toolbar star toggles its plain document bookmark and shows a filled
star while bookmarked; removing it leaves heading, block and search bookmarks
intact. Browser context menus are suppressed on non-editable shell surfaces,
while editor/Canvas custom menus and native text-field menus remain available.
Physical hotkeys route editor operations, save, tab cycling and read/edit
switching to the focused pane. Settings also configure Daily folder/date/template
and the template folder. Reading view supports bounded local note embeds
(up to 36 source lines / 4,000 characters, without recursive execution).

Workspace state is machine-local and scoped by the canonical vault root. On
Windows it is stored in `%LOCALAPPDATA%/dev.brandon.knowledge/workspace-v1.sqlite3`.
This database contains bookmarks, session state, crash drafts and source history:
it is **not** a disposable search cache. Back it up separately if those local
records are needed; copying only the vault does not transfer them. Notes,
boards, attachments and canonical task state remain in the vault.

Draft writes use SQLite transactions with full synchronization; recovery never
silently overwrites a changed source. The recovery center offers the retained
buffer; conflicts show both versions, and a missing source can be saved as a
separate copy. Drafts are limited to 8 MiB each, 100 records / 64 MiB per vault.
Source history retains up to 20 versions per file within a 128 MiB per-vault
budget. History records the source before edits, not every keystroke or every
external version. A forced crash can still precede an unacknowledged journal
write; this is not a claim of zero-loss storage under every hardware failure.

## Canvas cards and images

A single click opens raster attachments on the board or in the file tree.
The image viewer supports zoom and Escape, with normalized previews up to
2560 px (never upscaled), existing decoder/size limits, and no source-byte changes.
Card/background context menus work with the mouse or keyboard. On native boards
of at most 200 nodes, visible text cards use formatted Markdown overlays; larger
boards keep the GPU-oriented representation. This is not a full rich-text Canvas
format or a PDF/video viewer.

## Verify

```powershell
pnpm --dir apps/desktop build
pnpm --dir apps/desktop lint
pnpm --dir apps/desktop test:canvas
pnpm --dir apps/desktop test:search
pnpm --dir apps/desktop test:r1a
pnpm --dir apps/desktop test:r1b
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
pnpm --dir apps/desktop test:r1a:ui
pnpm --dir apps/desktop test:cards:ui
```

`pnpm --dir apps/desktop test:r1b:ui` owns and restarts its own child process,
including forced-crash checks in generated fixture vaults. Close a clean QA app
first; do not run another desktop suite concurrently with this lifecycle test.

Restart without the debug environment variable for normal use. Tests and GPU
measurements are development checks, not a universal frame-rate guarantee.

## Current boundaries

- Image thumbnails and arbitrary file references are implemented. Embedded
  PDF/audio/video viewers are not complete; unsupported files get a detail peek.
- The ordinary UI uses local FTS5 Words/Literal search, incremental source
  updates and read-time validation. Shared incremental metadata for all other
  views, vector search and scoped agent integrations remain planned work.
- `scripts/local-ai` contains optional, isolated OCR/ASR experiments, not bundled
  product workers. Model caches, sample downloads and reports stay outside Git.
- Personal notification automation and its credentials are local-only, not part
  of this application's source distribution.

See the [master plan](../../docs/brandon-master-plan.md) for delivery order and
the [Canvas/Planner history](../../docs/canvas-planner-product.md) for earlier
checkpoints. Keep real vaults, tokens, keys and local state out of commits.
