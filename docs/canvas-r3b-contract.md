# R3B: frames and working formats

Implementation starts over `40b3c0f965148ad4112579b649b87f89b1918d78`.
This contract fixes the writer before enabling UI writes. It is not an R3B
completion claim; templates, linked formats and export follow separate gates.

## Upstream audit (19 October 2026)

Read-only HEAD lookup resolved Advanced Canvas to
[`4b8630c641bec6f6329adb8436a4d35752280d74`](https://github.com/Developer-Mike/obsidian-advanced-canvas/tree/4b8630c641bec6f6329adb8436a4d35752280d74),
manifest **7.1.0**, min Obsidian **1.13.0**. The pinned
[LICENSE](https://github.com/Developer-Mike/obsidian-advanced-canvas/blob/4b8630c641bec6f6329adb8436a4d35752280d74/LICENSE)
is GPL-3.0. No upstream code, package, CSS, runtime or install script is copied,
installed or executed. This is an independently implemented native feature,
not an unreviewed GPL source port or a claim of full plugin compatibility.

Inspected the pinned
[Advanced JSON Canvas specification](https://github.com/Developer-Mike/obsidian-advanced-canvas/blob/4b8630c641bec6f6329adb8436a4d35752280d74/assets/formats/advanced-json-canvas/spec/1.0-1.0.md)
and selected
[presentation implementation](https://github.com/Developer-Mike/obsidian-advanced-canvas/blob/4b8630c641bec6f6329adb8436a4d35752280d74/src/canvas-extensions/presentation-canvas-extension.ts)
to distinguish their conventions from ours. Its presentation can start from a
metadata node and navigate edges. Brandon's initial presentation is an explicit
ordered list of group-backed frames, not that edge-driven graph. Existing
foreign metadata/startNode/ratio/styleAttributes remain untouched and are not
silently reinterpreted as a Brandon presentation. There is no Advanced Canvas
import adapter in this slice. Actual source reuse would need a separate license
and transitive-dependency decision; this audit does not provide one.

## Frame writer (v1)

A frame is a standard JSON Canvas `group` with label/bounds and
`brandon:{version:1,frame:{version:1,preset,notes}}`. `notes` is optional plain
text up to 16384 UTF-16 code units; it is never executable HTML or a second copy
of the group's contents. Presets: custom, A4, Letter, 16:9, 4:3, 1:1, mobile,
tablet, desktop, with fixed initial dimensions. Preset records creation intent,
not a promise of locked aspect ratio or physical print size after free resize.
Non-group frames and frame/primitive/task/ink ambiguity fail closed.

Document extension: `brandon:{version:1,presentation:{version:1,frames:[id,...]}}`.
Order is separate from node z-order. Up to 512 actual frames and 4096 unique
nonempty ordered IDs (1..512 UTF-16 code units each). Missing/deleted IDs are
retained but not shown; this preserves the slot if Undo or an external edit
brings the same frame back. Frames absent from the list append in source node
order, so paste/remapped IDs cannot vanish from navigation. Unknown fields in
both extension levels survive. Unknown known versions/invalid known data open
read-only; no silent pruning, data trimming or rewrites on read.

Creating a frame around a selection expands selected geometric groups, checks
the same descendant locks, adds margin, and enlarges only the new frame to fit
the selected aspect ratio. Existing objects/sources are not moved or cloned.
A new frame inserts before its members to preserve their z-order. Custom frames
use the padded selection bounds. Empty creation uses the selected preset at the
camera centre. Duplicate ID, over-budget geometry and creation inside a locked
group refuse the entire operation. New sequence entries preserve foreign data.

Moving/locking/copying uses shared group semantics. Delete removes a frame and
its geometric children; **remove frame only** uses ungroup semantics and leaves
contents and canonical sources intact. Reordering the presentation cannot move
objects or reorder renderer layers. Frame metadata edits are CAS/one Undo and
respect group/descendant locks, like the existing shared style command.

## Presentation contract

One active frame, explicit previous/next/Home/End/Escape controls, plain speaker
notes, camera fit plus clipping to its bounds in native and DOM/overview paths.
Initial presentation budget is 2000 intersecting objects per frame on either
renderer. Over-budget frames refuse visibly without entering/moving the camera;
they are not trimmed, rasterized into a source attachment or rewritten. Ordinary
large-board authoring is unaffected. This bounds fallback composition as well as
presentation text shaping; a later increase needs a measured acceptance gate.
No source writes on entry/navigation/exit. Editing commands and task checkboxes
must not mutate sources while showing slides. Exit restores the prior camera.
Source replacement/removal of the active frame exits safely. Presentation is
local and pane-scoped; no OS fullscreen request, share or recording permission.

Tests must cover create/wrap/rename/notes/reorder/remove-only/Undo/reopen,
unknown fields, future versions, external source edits, locks and both renderers.
Clipping and export bounds share the frame rectangle; export still requires its
own bounded Rust I/O, font/media compatibility report and actual image inspection.

## Foundation checkpoint

Writer/schema and pure operations implemented: nine preset groups, wrap selection,
shared move/lock/remove-only semantics, notes/title editing, explicit presentation
sequence with retained missing slots, deterministic append of pasted frames, and
shared camera/screen rectangle calculations. Namespace/schema/budget checks are
wired into the common parser. This was the foundation commit `7dd6111`, before
enabling frame UI, native clipping or any in-app acceptance.

Eleven new frame tests and **130 frontend contracts total** PASS, including the
existing Canvas/ink and editor tests. The first locked-insertion fixture tried
to place a frame larger than its locked container and correctly did not refuse;
the final test uses a truly contained frame rather than changing group semantics.
Logs: `target/qa/r3b-frames-{frontend,lint,typecheck,build}.log`. This foundation
did not change Rust; it was not a native build or in-app acceptance run.

## Frame UI / clipping implementation checkpoint — 19 October 2026

Implemented over `7dd6111ba8046f03e7cc4519851655edfd02ab75`:

- Shared frame commands, authoring/notes dialog, ordered frame panel, focus,
  reorder and remove-only controls. Dialog submission checks the source snapshot
  before invoking the common writer and committing one Undo. Frame commands
  are available to the app palette as well as the board surface.
- Pane-scoped presentation controls and plain speaker notes, previous/next,
  PageUp/PageDown/arrows/Home/End/Escape. Camera is temporary and not persisted
  as the normal workspace camera; exit restores the prior view. Authoritative
  source replacement ends the show through a layout effect before the next
  scheduled native frame. Search navigation exits before focusing its target.
- Board commands, pointer gestures, child keyboard handlers, task toggles,
  reference drops and file-import initiation are guarded during the show.
  Existing import/task work must finish before entry. Queued pre-show wheel
  work is discarded rather than shifting the fitted slide afterwards.
- The actual Rust frame DTO accepts a validated world clip. Culling intersects
  it with the viewport; all geometry, rasters, ink and glyph batches use the
  resulting camera/DPR/surface-clamped GPU scissor. The Rust renderer independently
  enforces 2000 visible objects for a clipped frame. Ordinary board rendering
  remains outside that presentation-only budget. Native detail rendering keeps
  glyphs/rasters when a fitted slide is below the normal overview zoom threshold.
- Fallback presentation uses the same fitted bounds and a clipped, bounded DOM
  node composition instead of losing labels to ordinary far-zoom overview.
  Both paths still require the live clipping/pixel gate below.

Executed source/build gates: **152 Rust tests + 132 frontend contracts**, all
PASS, strict Clippy, ESLint, typecheck, fmt and standalone Tauri release build.
The frame suite now has 13 pure cases. Three new Rust cases cover clip/DPR/
surface intersections, malformed clip geometry and the independent presentation
budget. `tests/desktop/r3b-frames-smoke.mjs` is authored and syntax-checked, **not
executed**. It covers authoring/Undo/CAS/locks, read-only navigation/source
replacement, native clip validation, fallback, budgets and canonical byte checks.
No old UI suite is re-labelled as a new frame acceptance run.

Logs: `target/qa/r3b-frames-{frontend,rust,clippy,lint,typecheck,fmt,native-build}.log`.
The isolated QA output uses `CARGO_TARGET_DIR` rather than overwriting the running
accepted ink EXE: `target/qa/r3b-native/release/knowledge-desktop.exe`, SHA-256
`21ccb5e2c5d7892d15e6c947c04a8b54af83604b3bc4a93f943c5f99de0a8c8b`.
The close/lifecycle helpers now accept an explicit local QA endpoint and the
lifecycle helper accepts a chosen QA EXE; defaults are unchanged. This enables a
separate-port owned-child run without targeting an existing ordinary instance.

**Live gates pending.** The attempted launch of a second isolated test instance
was rejected by the execution policy before the process started. No alternate
launch path, registry change, auth input or lock-screen bypass was attempted.
The normal accepted-ink process was independently confirmed still running; it
was not closed or force-killed. The final ink visible-window gate also remains
pending after the code-password capture. Owner notified through the allowlisted
Telegram channel. A successful build is not a verified R3B user scenario.

Remaining in this frame slice: actual Windows UI suite, full regression on that
binary, visible native/fallback clipping and narrow-pane review. Remaining R3B:
local template catalogue, linked working formats and bounded PNG export with
compatibility report and image inspection. No frame installation, NSIS, push,
native macOS, physical stylus or server acceptance is claimed.

## PNG export v1 contract (before writer)

The first export is a flattened PNG of one selected frame, not a screenshot of
the window and not a second authoritative board. It runs in a bounded Rust CPU
worker. Prepared node appearance/text bounds, edges and ink are shared with the
native scene adapter; text uses the same cosmic-text layout rather than an
unrelated HTML screenshot. Export can run without an active GPU surface.

Input names the current vault, saved `.canvas` source path, exact source snapshot,
frame ID, scale (1 or 2), background (dark or transparent) and versioned prepared
scene. Rust checks the current root and exact saved source before preparing or
reading referenced rasters. The frame must be a valid v1 group-backed frame.
Prepared geometry/node identity and referenced image paths must match that source;
external URLs, SVG/HTML/script references and hidden/traversing paths are never
fetched. Unsupported or unavailable media get an explicit labelled placeholder
and a per-node report, not a silent successful omission. Text/links are inert.

Budgets: one active job, 2000 intersecting nodes, 4000 edges, 100000 ink points,
2 MiB prepared text, 4096 pixels per output edge and 16 Mi pixels / 64 MiB output
pixels. Up to 64 distinct raster inputs and 64 MiB of decoded raster cache;
decode reuses vault image limits. Oversized output refuses; it never silently
reduces the chosen scale. No thumbnail bytes are routed through the live native
renderer. Preview is a derived small PNG; the full PNG stays in Rust until save.

The user reviews dimensions/background and compatibility warnings, then chooses
a destination folder. Save is create-only into a fresh export bundle directory
containing `frame.png` and versioned `report.json`. No existing destination is
overwritten. The PNG and report publish together from a bounded staging directory;
write failure/cancel keeps the vault intact. Source and relevant raster/task
revisions are revalidated before publish, so an old preview cannot silently
export newly changed data. Tokens are scoped to the vault/job with a short TTL;
switching vault or closing/cancelling the dialog releases the derived buffers.

The report states that PNG loses editing, live links/tasks, notes, locks and
presentation order, records the source fingerprint/frame bounds/scale/font
family and marks image resampling/missing media/font substitutions. Speaker
notes and unrelated sources are not exported. No blanket PDF/SVG or lossless
other-editor claim follows from PNG. Explicit SVG/PDF, selection/whole-board
exports and the template/linked-format slices remain subsequent work.

## PNG implementation checkpoint — 19 October 2026 (approval/live gates open)

Source checkpoint over `e71f9047efcf531a1fb36c7e3262e3d5c798b1cc`:

- Extracted the native prepared node/edge/ink DTO and node adapter; the CPU PNG
  worker uses the same prepared geometry and cosmic-text 0.19.0 text layout.
  Additional bounded work: 64 KiB text per node, 250000 glyphs, 32 MiB / 8192
  glyph-cache entries, 256 Mi pixel paint-area sum. No window capture or URL
  fetch. The transparent background does not remove actual object fills.
- Frame panel PNG buttons and the shared `canvas-export-png` command open a
  snapshot dialog in either pane. Explicit prepare -> small PNG preview ->
  warnings -> folder picker -> save. No automatic source save/overwrite.
  Closing/unmounting sends cancellation even before preparation returns;
  source replacement invalidates saving. Full PNG is retained only in Rust.
- Exact source bytes, node membership/order/bounds/text and raster paths are
  checked. Canonical task labels/revisions are read from the authoritative DB
  without creation/migration, not trusted from cached frontend rows. Source,
  referenced asset hashes and task revisions are checked again before save.
- Save requires a real non-link directory outside the source vault. New
  `frame.png` + `report.json` are synced in a private stage and published via
  `renamore::rename_exclusive`, with no fallback/no replacement. Tests on this
  Windows filesystem refuse existing files, empty directories and nonempty
  directories. This is not a claim of hostile OS-level race immunity or crash
  durability on every filesystem. Cancellation never deletes a committed bundle.
- Report includes source/PNG hashes, dimensions, actual font families, clipped
  text/missing glyphs, resampling/unavailable rasters, canonical task revisions
  and inert file-node IDs. Plain prepared text omits rich Markdown/property
  badges. Speaker notes, unrelated source text and original binaries are absent.

Executed: **193 Rust cases** (desktop/vault/Markdown plus core/crypto), **137
frontend contracts**, strict Clippy, typecheck, ESLint, fmt and production
frontend build all PASS. Rust/TS counts are not GUI cases. The frontend build
retains the existing large-chunk warning; no budget suppression was added.
Logs: `target/qa/r3b-export-{all-rust,all-frontend,clippy,web-build}.log`.
The 6 CPU-render pixel tests, 6 prepare/publish protocol cases and 3 vault
snapshot cases are included in the Rust total; 5 new export contracts are
included in the frontend total, not counted a second time.

Opt-in raster fixture: `target/qa/canvas-export/{render.png,preview.png,
render-report.json}`, generated by `BRANDON_EXPORT_FIXTURE=1` plus the render
test. The actual PNG was visually inspected: 1280x900, 9 objects, 2 strokes,
348 glyphs, Segoe UI, zero missing glyphs/rasters or clipped text. This inspects
a generated artifact, **not** the application dialog or a native/GPU parity run.

**Open approval gate:** license checking explicitly FAILS on new transitive
`arrayref 0.3.9` / BSD-2-Clause. Exact upstream manifests/texts were inspected;
selected notices are retained in `apps/desktop/src-tauri/THIRD_PARTY_CANVAS_EXPORT.txt`.
No exception/allow-list change, release packaging or claim of license-clean
readiness. Owner asked for an exact-version exception or another approach;
Telegram message 321 delivered. `target/qa/r3b-export-licenses.log` records the
rejection. Distribution/user-accessible notices must be completed after the
dependency decision, not silently skipped.

**Open live gates:** no new app launch, CDP or native input was attempted after
the previously rejected launch/password-screen stop. The current running EXE
was not overwritten. New export UI, frame clipping and final ink visible-window
acceptance remain unverified. No new Tauri release-build claim for this source
checkpoint; no install, push, publication, paid action or user-data deletion.
R3B is incomplete; templates/linked formats and all later master-plan packages
remain required. This checkpoint does not complete or pause the overall goal.
