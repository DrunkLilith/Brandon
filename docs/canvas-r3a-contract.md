# R3A: shared Canvas operations and extension contract

Contract fixed before the first R3A writer, 19 October 2026. R3A is in progress;
this document is not an acceptance claim. JSON Canvas 1.0 remains the base:
[official specification](https://jsoncanvas.org/spec/1.0/). Array order is paint
order. Groups use standard geometric containment, not hidden parent IDs.

The operations slice implements grouping/layout/z-order/locking and shared
command dispatch. The following primitive/style slice implements the seven
tools, bounded typography/paint and native/fallback rendering described below.
The connector slice also implements the common prepared geometry and native
labels described below. R3A remains open for the complete task/note mixed
scenario; bounded mixed-layer acceptance is recorded below, not full-plan completion.

## Storage and compatibility

Standard text/file/link/group nodes and edges retain every unknown property.
Group, position, size, color, edge sides/endpoints/label and array order use
standard fields. A group contains fully enclosed nodes; a strictly smaller
enclosed group is nested. Equal-size/overlapping groups are not an implicit
cyclic parent graph. Moving a group moves descendants once; selected children
are not moved twice. Ambiguous overlapping selected groups must refuse a
conflicting transform rather than apply arbitrary double movement.

Optional node `brandon` is a namespaced object with `version: 1`. Its bounded
fields are `primitive` (sticky/text/rectangle/ellipse/diamond/triangle/divider),
`locked` (boolean), `fontSize` (12/16/24/32), `textAlign` (left/center/right),
and `strokeWidth` (1/2/4). Optional edge `brandon` v1 has `dash` (solid/dashed)
and, fixed before the connector writer, `strokeWidth` (1/2/4).
Before the primitive/style writer, v1 adds inert paint fields `fillColor`,
`strokeColor`, `textColor` (six-digit hex; fill alone may be `transparent`).
Standard `color` remains the portable fallback. The shader's shape IDs and
relative text bounds are derived renderer DTOs, never authoritative file data.
Unknown fields survive edits; an unknown version or invalid known field makes
the board read-only with a visible error, never a blank writable replacement.
An existing unrelated/non-object `brandon` is not silently overwritten.
Unsafe integer IDs, non-finite/underflowed numbers or decimals which cannot be
represented without changing their decimal value also produce a read-only
error. This prevents an unrelated move from silently rounding foreign JSON
data. Ordinary exact decimal/scientific notation and numeric-looking strings
remain supported. Preservation is semantic, not byte-for-byte whitespace.

Each primitive is a standard **text** node with readable Markdown fallback,
standard bounds and color. Shape/lock/typography are **extended with fallback**,
not standard JSON Canvas. A writer that strips unknown fields loses them.
Copies retain extension values; copies remap only standard node/edge IDs.
No provider key, user path, script, iframe or remote asset is embedded.

Before the canonical-task writer, v1 adds optional `taskId` (1..128 printable
ASCII identifier characters `[A-Za-z0-9_-]`) on **file nodes only**, with a
relative `.md` `file` fallback. It references an existing managed TaskId in the
current vault; no checkbox import, task-row clone, cached title/status/revision
or database path is written into Canvas. Duplicate/copy remaps Canvas IDs only,
never TaskId; removal and Canvas Undo affect representation/layout, not task DB.
Live display resolves TaskId, not a guessed filename. A missing task, unavailable
metadata or conflicting source path is visibly unavailable and never rebound
to a different task. An explicit reference repair may update the fallback path
to the same TaskId; it must be one undoable Canvas command, not a silent write.
Task edits use the existing root-scoped Rust TaskService/revision check and
refresh shared metadata. Task DB changes are not claimed to be Canvas Undo.
Locked cards/groups refuse task edits through that board, not through independent
authorized task views. Other editors see an ordinary linked Markdown file and
may ignore task identity/state; this additive extension has no new cross-editor
lossless guarantee. Acceptance of this task slice is recorded below.

Before broad user-data writes or a cross-editor lossless claim, run an isolated
actual other-editor open/edit/save/reopen probe. The first actual probe is
recorded below; it confirms one writer/version, not all other applications.
An eventual compatible-copy export must report lost shape/lock/style data and
never overwrite the original. No plugin host or Advanced Canvas source is
included by this initial slice.

## Commands and interaction

Pure document transforms own selection expansion, lock policy, grouping,
layout, z-order and style changes. Native GPU, DOM/overview fallback, toolbar,
context menu and shortcuts call the same command effects, not renderer-specific
mutations. A gesture or batch operation contributes one undo item. Unchanged
operations create no history entry; unknown fields stay on unchanged objects.

Locked nodes and descendants of locked groups remain selectable and can be
copied/unlocked, but cannot be moved, resized, edited, styled or removed. A
group transform containing a locked child refuses as a whole. Grouping does
not transfer ownership of linked notes/tasks: file nodes still point at the
same canonical source. Ungroup removes containers, not their content.

Align/distribute operate on selected roots, carry group descendants and preserve
relative child geometry. Grid/stack are explicit layout actions, not hidden
snap on drag. Changes use finite bounded coordinates and round changed geometry
to integer Canvas units. Group traversal/work has a bounded refusal instead
of unbounded recursion. Reopen reads saved JSON; camera remains session state.
Undo history is local to the mounted board, not a durable cross-process journal.
Saved content after undo reopens consistently; persistent agent/batch undo is
separate R6B work. Pointer cancellation rolls back a transient gesture; a new
external document revision cancels the old gesture before pointer release.

Straight connectors come first. Explicit sides, arrow/none endpoints, color,
label and dashed style share prepared geometry across renderers. Complex elbow/
curve routing, pen/highlighter/strokes, hidden layers and advanced text styling
remain separate bounded acceptance work, not silently declared implemented.

Connector writer contract: omitted `fromEnd` means `none`, omitted `toEnd`
means `arrow`, as JSON Canvas 1.0 specifies. Omitted sides use a deterministic
facing cardinal side. Triangles attach at their actual side midpoint; standard
rectangular fallback bounds remain unchanged. A degenerate/coincident connection
uses a bounded visible loop, not an invisible zero-length line. Prepared world
segments, arrow wings, label layout and hit bounds are shared by all renderers;
the native engine consumes changed edge DTOs, not a second side/arrow resolver.
Labels render as bounded plain text while the complete original label survives
unrelated edits. Edge edits/reversal/deletion are one command and respect locked
endpoints, preserve foreign fields and reject stale source proposals. New links
cannot bypass endpoint locks. Its bounded acceptance is recorded below, not
claimed as full R3A or full-plan completion.
When an edge crosses a group, the common 64-unit header zone selects the group;
the edge stroke in the remaining group body selects the edge. Group selection
tests use the visible header explicitly, rather than an ambiguous bounding-box
center that may land on a connection. Body drags away from strokes still select
the group. Both header and interior-edge paths require native/fallback coverage.

## Actual other-writer probe: Obsidian 1.14.4

An isolated `target/qa/r3a-obsidian-probe` was opened through the real Windows
folder picker in the already installed Obsidian. Its running UI reported
**1.14.4** (not the launcher's older executable metadata version). No community
plugins, login or Sync were used; the user's existing vault was not opened or
edited. The app's usual startup update check was not disabled or reconfigured.

Opened `Probe.canvas`, dragged beta from `(60,-100)` to `(100,-80)`, saved with
Ctrl+S, closed the tab and opened it again. The group, both Markdown texts,
colors, explicit edge sides/label and node order survived. Obsidian normalized
the default `toEnd: "arrow"` to omission, which retains the same standard
meaning. It retained node/edge `brandon` extensions, future nested data and
foreign root/node/group/edge properties. The Brandon lock is **not enforced by
Obsidian**; retaining a field does not mean understanding it.

The semantic save snapshot is committed as
`tests/desktop/fixtures/r3a-obsidian-1.14.4.canvas`. Its values are checked in
unit tests and reopened by the actual Brandon UI, which again enforces the
preserved node lock. The original post-Obsidian file hash (before whitespace
normalization of the committed fixture) was
`5e5418357b6a5e9a379bc2ba2ff071005117524b509671356044d86dbd5e17e0`.
The source fixture/app-created `.obsidian` state stays in the ignored QA
directory; no private Obsidian configuration or application code is vendored.

## Operations-slice checkpoint, 19 October 2026

Implemented over R2B commit `54ca5852df68462cc7bf0654f4837d28b0cb17af`:

- Shared pure transforms for geometric/nested groups, root selection,
  align/distribute, stack/grid, z-order, subtree copy/delete and locks. Linked
  resources remain references; internal copied edge IDs/endpoints are remapped.
- One board registry for toolbar/context effects and local editing shortcuts;
  namespaced bindings belong to the app's global collision-checked registry.
  Palette dispatch uses the focused native or secondary fallback board. Custom
  RU-layout bindings replace defaults, rather than causing double execution.
- Main/secondary rendering now consistently uses single-click selection and
  double-click text editing. Locked descendants cannot move/delete/edit/resize;
  their resize handles are absent. A child cannot unlock its containing group.
- Native, DOM and overview share a translucent group fill. Actual Windows visual
  inspection found and fixed opaque groups hiding their connections. Small-board
  DOM text overlays explicitly suppress underlying GPU glyphs, preventing doubled
  text with transparent backgrounds. No authoritatively stored format changed
  for the paint-only fix.
- Future extensions and numerically lossy JSON refuse edits. Cancelled or stale
  gestures do not publish source changes. Work limits reject pathological group
  expansion instead of recursing indefinitely. Current mounted-board undo is
  separate from saving/reopening or the later durable agent-operation journal.

Final validation for this **operations slice**, not all R3A:

- **134 Rust tests**, strict Clippy/fmt, **80 frontend contracts**, lint/typecheck
  and native production build passed. No dependency was added.
- **136 actual Windows/Tauri UI cases**: R3A layout 23, Canvas render 10, R1A 25,
  cards/images 6, capture 11, DQL 7, properties 10, tasks/backup 14, R2A 11,
  R1B process/draft lifecycle 19. The layout suite covers native/DOM parity,
  copied group edges, lock guards, global palette undo, custom bindings, reopen,
  the Obsidian snapshot, pointer cancellation and external edit mid-gesture.
- One intermediate regression caught a lost asynchronous editor return value:
  stale clipboard rejection still protected the source but its notice was lost.
  Fixed by retaining the editor promise through command delegation, then reran
  the full R1A suite. The failed diagnostic log was retained separately; no
  assertion was removed or failure reclassified as PASS.
- Native large-board repeat, RTX 3090 / DX12, DPR 1, synthetic 10k/20k nodes:
  no DOM cards at far zoom; input-ack p95 **21.1/23.0ms**, native CPU p95
  **6.42/9.91ms** on this final run. Input-ack includes two animation frames,
  IPC and polling; CPU time includes preparation/present submission, not measured
  GPU execution or display FPS. Seventy iterations, first ten excluded. This is
  one local repeat, not a universal performance or cross-device guarantee.
- Logs: `target/qa/r3a-layout-{rust,clippy,contracts,build}.log`,
  `target/qa/r3a-final-{r3a,render,r1a,cards,capture,dql,properties,tasks,r2a,r1b}.log`,
  `target/qa/canvas-render.json`. Windows visual fixture:
  `target/qa/r3a-native-layout-vault`; separate real Obsidian probe above.

The verified executable is `target/release/knowledge-desktop.exe`, SHA-256
`f4308a93ae31dd4c235a6c1534c0159bd54dc7be1f48520981ccc568a32b5de9`.
This intermediate slice does **not** regenerate the R2B NSIS installer; that
installer retains its separately recorded R2B hash. Nothing was installed or
pushed. Native macOS and the unrelated server release gate were not run.

Final regression runs used per-process software WebView for the already
documented [RTSS host issue](plugin-compatibility-r2b.md#known-host-specific-webview-failure).
Native wgpu remained enabled. System settings, overlay processes and production
defaults were unchanged; software mode is not claimed as a reliable cure.
After QA, restored `r1b-visual-vault` / Alpha, closed the debug instance and
visually checked an ordinary production-default startup. Its WebView process
had neither `--disable-gpu` nor port 9222, and the QA listener was closed. That
successful startup is not a claim that the intermittent host issue is solved.

The remainder recorded at this operations checkpoint was primitive/style tools,
connectors and the mixed task scenario. The next checkpoint below supersedes
only its primitive/style portion. Pen is a subsequent bounded slice;
frames/formats belong to R3B. The whole master-plan goal remains active.

## Primitive/style checkpoint, 19 October 2026

Implemented over operations commit `02f5aa30eae642bca708606798f7fe8b5ef24dc9`:

- Sticky, plain text, rectangle, ellipse, diamond, triangle and divider tools;
  portable text/color/bounds fallback plus versioned extension. Shape IDs,
  interior text bounds and centered labels are shared derived presentation,
  not a second authoritative format. Filled shape labels default to centered;
  sticky/plain text retain their own defaults. Text is plain with wrapping,
  not an inline Markdown/WYSIWYG formatting claim.
- The actual native wgpu shader draws geometry and bounded glyphs on larger
  boards. Exact hit regions exclude empty ellipse/diamond/triangle corners;
  separate resize handles remain usable. Fully transparent shader fragments
  do not become invisible depth occluders. The DOM/SVG and far-overview paths
  use the same appearance and shape geometry.
- Explicit style dialog: fallback palette, custom fill/stroke/text colors,
  transparent fill, font size/alignment, stroke width, shape, text/group label.
  Apply is one undo item; cancel writes nothing. A changed source invalidates
  an open proposal. Explicit color reset removes only that override and retains
  other/future fields. Dark controls and a wrapping narrow-board toolbar were
  visually checked in the packaged app, not just in a browser mock.
- Up to 100 nonempty pasted lines become separate stickies in one command.
  Fresh tool placement wholly inside an existing locked group refuses visibly,
  without creating an immediately uneditable child or changing board bytes.
- Small raster boards (at most 200 objects, zoom at least 0.5) deliberately use
  a bounded DOM geometry/image compositor for correct shape/image stacking;
  native wgpu still owns grid/edges. This is an explicitly mixed renderer,
  not a claim that those nodes are GPU pixels. A separate 300-object fixture
  has zero DOM nodes and visibly renders all primitive kinds on the real GPU.

Final acceptance for this **primitive/style slice**:

- **136 Rust tests**, strict Clippy/fmt, **89 frontend contracts**, lint/typecheck
  and native production build passed. No new dependency or plugin source.
- **154 actual Windows/Tauri UI cases**: primitives/style 18, layout 23,
  render 10, R1A 25, cards/images 6, capture 11, DQL 7, properties 10,
  tasks/backup 14, R2A 11 and owned-process R1B lifecycle/drafts 19.
  New cases exercise both main native and secondary DOM panels, custom paints
  and reset, one Undo/Redo, cancel, stale source, batch creation, reopen,
  exact corner hit-through to an underlying image and locked-group refusal.
- RTX 3090/DX12, DPR 1, synthetic 10k/20k native boards, zero far-zoom DOM cards:
  input-ack p95 **22.5/21.4 ms**, native CPU p95 **7.29/10.47 ms**. Same bounded
  70-iteration/10-warmup method as above; not GPU execution, FPS or a different
  device/platform guarantee. The 300-shape screenshot is a separate correctness
  inspection, not the 10k/20k performance workload.
- Logs: `target/qa/r3a-primitives-{build,rust-all,clippy,contracts,lint,typecheck,fmt}.log`,
  `target/qa/r3a-primitives-final-{primitives,layout,render,r1a,cards,capture,dql,properties,tasks,r2a,r1b}.log`,
  and `target/qa/canvas-render.json`. Final primitive fixture:
  `C:/Users/ADMIN/AppData/Local/Temp/brandon-r3a-primitives-JdXSYw`.
  An accidentally overbroad store test invocation stopped on missing
  `DATABASE_URL`; its diagnostic is retained separately as
  `target/qa/r3a-primitives-store-prerequisite.log`. It is neither a desktop
  failure nor a passed server gate. The scoped desktop/vault/Markdown suite
  was then run successfully in full.

Final EXE: `target/release/knowledge-desktop.exe`, SHA-256
`0d514456a15fe4fa9914ecf4f373b04b0186ccfd0c42d30efba7edbd7c40f975`.
The R2B NSIS installer is not regenerated by this intermediate slice. Nothing
installed or pushed; native macOS and the server release gate remain untested.
The existing RTSS host caveat and per-process QA-only software WebView scope
above still apply; no production default or system setting was changed.
After the lifecycle suite, the ordinary production-default EXE was launched
and visually verified on restored `r1b-visual-vault` / Alpha. Its WebView child
had neither software-GPU override nor remote-debugging-port flag. This successful
startup is not a cure claim for the intermittent injected-overlay host issue.

The remainder at this primitive/style checkpoint was connectors,
the complete canonical task/note/sticky/shape workflow, and broader translucent
image/group/text cross-layer parity on larger boards. Small-compositor/image
and ordinary large native fixtures do not prove arbitrary mixed-media layering.
Cross-editor preservation was verified for the earlier Obsidian v1 snapshot,
not separately for every new custom-paint combination. No full-parity or
lossless export claim is made. The full master-plan goal remains active.

## Connector checkpoint, 19 October 2026

Implemented over primitive/style commit
`5fb97736dca8e82c038b474b2f25124ae9780ed0`:

- One prepared-edge module resolves automatic/explicit sides, actual triangle
  side anchors, standard none/arrow ends, open arrow wings, bounded coincident
  loops, colors, label layout/bounds and exact line/label hit tests. The renderer
  DTO is derived; the original standard edge and every unknown field stay in
  the Canvas file. Dangling references are retained, not silently deleted.
- Native wgpu consumes those segments directly, including dashed lines and
  GPU-shaped label glyphs. DOM/SVG and far overview use the same prepared
  geometry. Golden fixtures are recomputed in frontend tests and deserialized/
  validated by Rust; the native renderer does not have another side resolver.
  Source labels remain complete; display is compact plain text (up to 49
  codepoints in two explicit lines), not Markdown or arbitrary rich layout.
- Selection by stroke or label, real double-click/Enter configuration, context
  menu, global palette commands, standard endpoint reconnection, reversal,
  deletion, two-selected-node linking and the two-click tool. Apply/reverse/
  delete/create each contribute one Undo; cancel writes nothing. Open proposals
  reject a changed source. Locked endpoints/ancestors cannot be bypassed by
  reconnecting, deleting, reversing or creating a link.
- Prepared edge deltas update only changed geometry/styles. A real node-drag
  test observes one incident edge in IPC `edgeUpdates`, no full edge resend and
  no unrelated-edge update. Structural edge additions/removals still send the
  new ordered list. No authoritative Markdown/task source is copied or changed.
- The initial regression exposed ambiguous group-body/line selection. Fixed a
  common header priority zone, retained all original grouping/lock assertions,
  made the group-selection helper target its visible header, and added explicit
  header-versus-interior-stroke scenarios in both renderers. The diagnostic
  remains in `target/qa/r3a-edges-layout-selection-diagnostic.log`.
- Computer-use visually verified actual GPU arrows/dashes/labels on a 303-node
  board with zero DOM cards/SVG paths, then physical double-click opened the
  configuration form. The compact form keeps Apply/Cancel visible. All visual
  inspection changes were cancelled; the synthetic source was not edited by
  that inspection. No external plugin source or dependency was introduced.

Final validation for this **connector slice**, not all R3A:

- **138 Rust tests**, **103 frontend contracts**, strict Clippy/fmt,
  lint/typecheck and native production build passed.
- **180 Windows/Tauri UI cases**: connectors 26, primitives 18, layout 23,
  render 10, R1A 25, cards 6, capture 11, DQL 7, properties 10, tasks/backup 14,
  R2A 11 and owned-child R1B lifecycle/drafts 19. New cases cover main native
  and secondary fallback, actual pointer/key/menu actions, endpoint mutations,
  cancellation, source CAS, locks, one Undo/Redo, native edge deltas and reopen.
- RTX 3090/DX12/DPR 1, ordinary synthetic 10k/20k boards, far-zoom DOM cards 0:
  input-ack p95 **22.1/24.2 ms**, native CPU p95 **7.32/9.67 ms**. Same
  70-iteration/10-warmup method above; not GPU time, FPS or a dense-label/mixed-
  media performance guarantee. The 303-node labeled connector fixture is a
  separate correctness case, not a mislabeled 20k label benchmark.
- Logs: `target/qa/r3a-edges-{build,rust,clippy,contracts,lint,typecheck,fmt}.log`,
  `target/qa/r3a-edges-final-{edges,primitives,layout,render,r1a,cards,capture,dql,properties,tasks,r2a,r1b}.log`
  and `target/qa/canvas-render.json`. Connector fixture:
  `C:/Users/ADMIN/AppData/Local/Temp/brandon-r3a-edges-DpQ0vc`.
  Two initial harness diagnostics are separate: a read-only Tauri invoke property
  required observing the actual fetch transport, and a narrow secondary pane
  required a visible interior node point instead of an offscreen center. The
  source/delta assertions were retained; neither run was reclassified as PASS.

Final EXE `target/release/knowledge-desktop.exe`, SHA-256
`89cc0c78077939d97c87073b8c19e87e80b610e6815fb727b4465dbd4dc218f7`.
NSIS was not regenerated; nothing installed or pushed. macOS and server gate
were not run. QA software WebView was per-process only; native wgpu stayed on
and the documented RTSS host caveat remains, with no system-setting change.
After the tests restored Alpha, an ordinary production-default launch was
visually verified again. Its WebView child had neither software-GPU override
nor remote-debugging-port flag. The successful startup does not resolve the
intermittent injected-overlay host issue.

**Still required for R3A:** the full canonical task/note/sticky/shape workflow
and acceptance of arbitrary larger translucent image/group/text layering.
The new connector tests do not establish arbitrary mixed-media pixel parity,
complex elbow/curve routing, or new cross-editor lossless guarantees. Pen and
R3B frames remain subsequent work. The full master-plan goal stays active.

## Mixed-layer implementation contract

Alpha geometry and text must be interleaved in source paint order. A group fill
must tint lower glyphs, not discard them through a depth proxy; upper glyphs
remain above it. Opaque native scenes and far overview keep a single batch.
Small rich-text scenes (at most 200 objects, zoom at least 0.5) use one bounded
DOM node compositor, with native grid/connectors underneath. Larger native
scenes use ordered geometry/glyph batches; no DOM substitution is claimed there.

Raster previews retain the standard file reference. Native texture/session IDs,
fit bounds, cache entries and draw spans are disposable renderer DTOs, never
stored in Canvas. Rust resolves public vault attachments and bounded decoding;
no pixels or filesystem reads are delegated to JavaScript for the native path.
The native image path is scoped to the current vault, renderer session and
visible referenced paths, rechecked after background decoding. Two preview
workers, at most 64 visible distinct raster paths, 128 cached textures and a
64 MiB decoded-pixel cache budget are explicit limits (not total GPU memory).
Watcher invalidation, stale-session refusal and image failures must be tested.
The implementation boundary above is exercised by the acceptance below; it is
not a claim of pixel identity across operating systems or arbitrary media.

## Mixed-layer/raster checkpoint, 19 October 2026

Implemented over connector commit
`99fe67ea57013e16108bfa5c477d0d118622d504`:

- Geometry and glyph batches now interleave at translucent source-order cuts.
  A foreground group tints existing lower glyphs instead of erasing them via
  depth. Upper text stays above it; edge-label glyphs precede node content.
  Opaque native scenes and far overview keep one batch. Glyphon does not write
  depth for transparent glyph texels. Small rich scenes explicitly share one
  bounded DOM compositor for node geometry/text, not an invisible native proxy.
- Genuine native raster textures use the same card/stage/footer geometry as
  fallback previews. Rust validates/resolves/decodes bounded RGBA and uploads
  directly to wgpu; native previews do not transfer PNG/RGBA bytes through JS.
  Aspect-contain, alpha and rounded stage clipping are retained. No legacy
  large-board DOM photo overlay or SVG occlusion mask remains. Large boards
  also wait for native startup instead of decoding duplicate DOM thumbnails.
- Texture and worker budgets are explicit above. Two successive sets of 64
  visible distinct textures in one session evict inactive pixels under the
  64 MiB decoded-pixel budget; the 65th path shows a preview-limit notice.
  This is not a measurement of total VRAM or a dense-alpha throughput promise.
- File/folder metadata deltas and exact raw image changes refresh only matching
  previews, including live fallback cards. A broken image keeps originals,
  reports failure and avoids infinite retry; repairing its file or removing
  its reference clears the corresponding error. Same-size edits are covered
  by raw path invalidation. Routine directory metadata noise is not treated
  as a whole-image-cache reset.
- Native requests are bound to vault + random renderer-session owner + current
  visible file references. Scope is checked before and after worker decoding.
  Old image/scene commands cannot mutate a newly opened renderer; cleanup of an
  old owner cannot destroy the new one. Root-scoped DOM previews reject a
  foreign vault. Traversal/private/unreferenced raster paths fail closed.
- Common narrow-phase picking lets group bodies pass through to real children,
  retains header priority and selects the group on empty body. DOM groups have
  matching pointer behavior. A real Windows click selected the foreground
  text first; a separate unobstructed photo point opened the full local image
  through the group. The same action is asserted in native and fallback suites.

Acceptance: `tests/desktop/r3a-layers-smoke.mjs`, 17 Windows/Tauri cases,
including source-byte equality, 307-node text/photo scenes with zero DOM
substitutes, 365-node raster-budget scene, external image edits, failed-reference
removal, far/near cache reuse, scope refusal and reopening. Actual Windows
computer-use inspection also verified tinted photo/text/ellipse, an alpha photo
above its group, captions and the loaded original-image viewer. These are
bounded representative acceptance fixtures, not arbitrary rich-text pixel
identity or macOS coverage. Canonical task/note/sticky/shape workflow remains
required before closing R3A; pen, frames and full master-plan work remain active.

Final validation for this **mixed-layer/raster slice**, not all R3A:

- **146 Rust tests**, **107 frontend contracts**, strict Clippy/fmt,
  lint/typecheck and native production build passed. The initial format check
  found unformatted new Rust tests; formatting was applied and the gate rerun.
- **197 Windows/Tauri UI cases** on the final build: layers 17, connectors 26,
  primitives 18, layout 23, render 10, R1A 25, cards 6, capture 11, DQL 7,
  properties 10, tasks/backup 14, R2A 11 and owned-child R1B lifecycle/drafts 19.
  Source/Undo assertions were retained. An initial primitive harness run caught
  a busy secondary save: a disabled Save button is not evidence of completion.
  The helper now also awaits the visible saving state; subsequent full runs
  passed. A layer navigation probe similarly waits for the real note mount
  before asserting a new renderer owner, not merely the outgoing Canvas title.
- RTX 3090/DX12/DPR 1, ordinary synthetic 10k/20k repeat, far-zoom DOM cards 0:
  input-ack p95 **22.4/25.0 ms**, native CPU p95 **7.47/9.96 ms**, same
  70-iteration/10-warmup method above. Not GPU time, FPS or a dense-alpha,
  dense-label or 20k-photo guarantee. Correctness uses separate bounded mixed
  scenes and two 64-texture working sets, not a mislabeled large-media benchmark.
- Logs: `target/qa/r3a-layers-{build,rust,clippy,contracts,lint,typecheck,fmt}.log`,
  `target/qa/r3a-layers-final-{layers,edges,primitives,layout,render,r1a,cards,capture,dql,properties,tasks,r2a,r1b}.log`
  and `target/qa/canvas-render.json`. Final mixed fixture:
  `C:/Users/ADMIN/AppData/Local/Temp/brandon-r3a-layers-rT1gAL`.

Final EXE `target/release/knowledge-desktop.exe`, SHA-256
`221b13ea98ebf18a5885c85ceb7467c2b49284afb1f19dd241e967c537f07970`.
NSIS was not regenerated; nothing installed or pushed. macOS/server gate were
not run. QA software WebView was per-process only; native wgpu stayed enabled
and no system/RTSS setting was changed. The final ordinary launch restored
Alpha and was visually checked; its WebView child had neither software-GPU
override nor remote-debugging-port flag. One successful launch is not a cure
for the previously recorded intermittent injected-RTSS host issue.

## Canonical-task/mixed-workflow checkpoint, 19 October 2026

Implemented over mixed-layer commit
`d3b9d35d5322bcbbc2d65eedfca2c71b855182d2`:

- A bounded searchable picker adds an existing managed task using the storage
  contract above. Ordinary Markdown checkboxes remain unmanaged and are not
  silently imported. TaskId is never generated by a card duplicate or group copy.
- Small native rich-card scenes and secondary fallback share an interactive
  task card. Checkbox and shared TaskDetailFields write through the existing
  root-scoped Rust TaskService/CAS; status/project/priority/tags/duration are not
  persisted in Canvas. Description opens the actual canonical Markdown editor.
  The original row count, TaskId and source bytes are checked across operations.
- A separate derived node-label map feeds large native scenes. Changing one
  task updates only that task's affected node DTOs, not the whole renderer or
  authoritative Canvas. Actual 305-node GPU acceptance observes two changed
  task labels, zero DOM cards and no reset/unrelated-node/edge resend.
- Shared metadata refresh updates all representations in both panes. A stale
  task form retains its local input while Rust rejects the old revision. Node
  and ancestor-group locks disable task mutation through this board; their
  canonical source remains independently accessible in authorized task views.
- Missing TaskId and a mismatched description path are visibly unavailable,
  not rebound by a same-named file. Explicit fallback-path repair retains the
  same TaskId and is one undoable Canvas command. External removal of a card
  clears its open form owner; reintroducing the reference does not reopen it.
- The mixed scenario is built through the real toolbar/pointer/keyboard:
  task + linked note + sticky + shape, placement, grouping, copy/paste, one
  Undo of the batch, group lock/unlock, movement/Undo and reopen. Canonical task
  edits are expressly not undone by Canvas Undo. Opening/editing the task's
  Markdown changes the source, not a hidden Canvas description copy.
- Task titles honor the existing bounded font-size/alignment style fields.
  Visual inspection identified a lone wrapped zoom button after adding the
  task tool; viewport/history controls now wrap as one group. No dependency,
  plugin source, database schema or new filesystem/IPC writer was introduced.

The explicit core R3A mixed-workflow exit scenario is implemented. Pen,
highlighter/eraser, complex elbow/curve routing, hidden-layer/object navigation
and R3B frames remain separate bounded work, not silently claimed here. The
whole master-plan goal remains active. The additive TaskId extension has not
been separately opened/resaved by another editor; the older Obsidian probe
above does not establish that new cross-editor guarantee.

Final validation for the **canonical-task/core mixed-workflow slice**:

- **146 Rust tests**, **112 frontend contracts**, strict Clippy/fmt,
  lint/typecheck and native production build passed. The Rust packages remain
  knowledge-desktop, knowledge-vault and knowledge-markdown; server/DB-service
  prerequisites are not misreported as a desktop run.
- **214 Windows/Tauri UI cases** on the final build: Canvas tasks/mixed workflow
  17, layers 17, connectors 26, primitives 18, layout 23, render 10, R1A 25,
  cards 6, capture 11, DQL 7, properties 10, tasks/backup 14, R2A 11 and
  owned-child R1B lifecycle/drafts 19. No source/Undo checks were removed. Two
  early harness runs exposed expected UI state: a newly created sticky starts
  in text editing (Escape is required before dragging), and a reopened split
  restores its prior document rather than automatically cloning the main one.
  The final helper acts on those explicit states, not offscreen fallback inputs.
- Native 305-node task board: zero DOM node substitutes, actual double-click
  opens shared fields, one TaskId change produces exactly two node-label DTOs.
  This is a correctness case, not a 20k-task label throughput measurement.
- RTX 3090/DX12/DPR 1 ordinary 10k/20k repeat, far-zoom DOM cards 0:
  input-ack p95 **22.6/30.8 ms**, native CPU p95 **7.08/11.72 ms**, same
  70-iteration/10-warmup method above. Reported as measured, not an improvement
  claim or GPU/FPS/dense-media guarantee.
- Computer-use inspected the final mixed composition, linked task form,
  grouped viewport toolbar and actual large-board native labels. A physical
  double-click opened task fields from the GPU scene; inspection was cancelled
  without submitting changes. Logs:
  `target/qa/r3a-tasks-{build,rust,clippy,contracts,lint,typecheck,fmt}.log`,
  `target/qa/r3a-tasks-final-{canvas-tasks,layers,edges,primitives,layout,render,r1a,cards,capture,dql,properties,tasks,r2a,r1b}.log`
  and `target/qa/canvas-render.json`. Final task fixture:
  `C:/Users/ADMIN/AppData/Local/Temp/brandon-r3a-tasks-Sp06Iw`.

Final EXE `target/release/knowledge-desktop.exe`, SHA-256
`b48d07323627dd7cc7bc78c312ac18f84e36e0e7270784c2379a2e7f1832a9bc`.
NSIS was not regenerated; no install/push, macOS or server gate. After the
owned-child lifecycle tests, Alpha was restored and an ordinary final launch
visually checked. Its WebView child had neither software-GPU override nor
remote debugging. Per-process QA software WebView did not disable native wgpu
or change RTSS/system settings; the existing intermittent host caveat remains.

## Ink writer contract

A stroke is a standard text node, readable fallback `Штрих пера` or `Маркер`,
standard bounds/color and `brandon.version:1, ink:{version:1, tool, size, basis,
pressure, points}`. `tool` is pen/highlighter; size is an integer 1..64 world
units at creation, basis the positive integer minimum creation bound. Points
are 1..2048 integer triples `[u,v,p]`, coordinates 0..65535 within the node,
pressure 0..1024. Creation uses at least 32-unit selection bounds so a dot can
be moved without hitting its resize handle. Width scales with min(width,height)/basis on resize; mouse
uses constant width, pen pressure has an explicit toggle and 25% minimum width.
The node has no `primitive`/`taskId` ambiguity. Unknown fields survive; invalid
known data/version or more than 100000 points/4096 strokes makes the board
read-only, never truncated and rewritten. No SVG script/file attachment is
created. Standard color is portable fallback; custom strokeColor overrides it.

Geometry, hit testing, eraser and renderer DTOs share one prepared local shape.
DOM/overview use one nonzero-fill path per stroke. Native uses a reusable R8
coverage mask with max-union before compositing color once; a highlighter's
single stroke does not darken itself at joins/self-intersections. Separate
strokes still blend in source order. The mask is disposable GPU state, bounded
to 64 MiB, never read back or sent as image IPC. Normal boards keep the existing
fast path; alpha cuts preserve lower text/images and higher objects.

One pointer gesture creates/removes one undoable batch on pointer-up. Preview
is transient; Escape, pointer cancellation, window blur, source replacement or
point-budget overflow must not publish a partial edit. Dot strokes are valid.
The eraser removes whole intersected ink nodes, never arbitrary cards/sources;
any locked target refuses the batch rather than partially deleting it. Group
locks/insertion policy apply. Select/move/resize/group/copy retain ink identity
and source points; bounding-box corners alone are not stroke hits.

Primary mouse/pen draws only with a drawing tool active. Touch pans, never
inks; touches during an active pen gesture are ignored (palm rejection).
Pressure is pointer data, not a claim of tested physical pen hardware. Secondary
mouse/context and space/middle-button pan remain navigation. Tool/options are
session UI state. No stylus driver/system setting or external dependency is
introduced. A later other-editor export must report the visual fallback loss.
Default tool bindings use Alt+P / Alt+H / Alt+E, respecting the shared registry's
modifier requirement; unmodified typing is not a global drawing shortcut.

## Ink checkpoint — 19 October 2026

Implemented over canonical-task commit `81b51c925005ecc1f86f4d9436e661a7274003c4`.
Pen, highlighter and whole-stroke eraser share the command registry, active-pane
routing and a bounded toolbar popover/options strip. Stored integer points,
geometry/hit/union paths and native prepared segments follow the contract above.
One pointer gesture is one Undo; no image attachment or duplicated canonical
note/task data is created. The pressure toggle and brush sizes are session UI
state. Native ink uses max-union R8 coverage then source-order color composition;
selected bounds do not become an opaque stroke background. Ordinary no-ink
boards retain their existing geometry fast path.

Acceptance executed on the final Windows standalone EXE:

- **149 Rust tests**, **119 frontend contracts**, strict Clippy, fmt, ESLint,
  typecheck and production Tauri build: PASS. Three new Rust tests cover prepared
  geometry, incremental scene budgets and mask allocation; seven frontend tests
  cover the writer, geometry, common transforms, locks, commands and refusal.
- **235 app scenarios**: 21 new ink + 17 Canvas tasks + 17 layers + 26 edges +
  18 primitives + 23 layout + 10 renderer + 25 R1A + 6 cards + 11 capture +
  7 DQL + 10 properties + 14 task/backup + 11 metadata + 19 owned-child
  R1B lifecycle/drafts. All PASS. Tests act through the actual compiled Tauri
  app, source files and native renderer; no removed Undo/source assertions.
- Main bounded compositor and secondary fallback both exercised real pointer
  draw/erase, one-step Undo/Redo, task-checkbox click suppression, cancellation
  and exact persisted source. Injected CDP pen pressure reaches the PointerEvent
  handlers and changes saved widths; injected touch pans without ink. These
  are not physical stylus/touch-device tests. A simulated blur and synthetic
  2050-event overflow prove cancellation without partial source publication.
- External authoritative replacement during an unfinished stroke retains exact
  external bytes, no conflict or phantom edit. Locked erasing/insertion refuse
  as a whole; future ink remains a read-only error. Reopen preserves points,
  foreign fields and unchanged canonical note/task/photo bytes. No broad
  get-vault-index/refresh call occurs during the ink scenario.
- Actual 309-node wgpu scene: 5 ink strokes / 11 segments, 1 native raster,
  **0 DOM ink/cards**, 1,142,400-byte reusable mask, 8 composed batches including
  4 alpha cuts. A pointer gesture adds the sixth native stroke, one Undo restores
  the fixture. Far zoom retains ink and remains DOM-free. This is a correctness
  case, not a throughput benchmark of 100000 ink points or a total VRAM measure.
- Ordinary 10k/20k repeat on RTX 3090 / DX12 / DPR 1: input-ack p95
  **21.9/25.4 ms**, native CPU p95 **11.74/10.86 ms**; same 70-iteration /
  10-warmup method as prior checkpoints, no far-zoom DOM cards. Not GPU time,
  input-to-photon/FPS or a dense-ink performance claim.

New tests: `tests/desktop/r3a-ink.test.mjs`, `r3a-ink-smoke.mjs`,
`canvas-ink-fixtures.mjs`; scripts `test:r3a`, `test:r3a:ink:ui`.
Logs: `target/qa/r3a-ink-{build,rust,frontend,clippy,fmt,lint,typecheck,smoke}.log`,
`target/qa/r3a-ink-final-{r3a-tasks,r3a-layers,r3a-edges,r3a-primitives,r3a-layout,canvas-render,r1a,canvas-card-ui,r2b-capture,r2b-dataview,r2b-properties,r2b-tasks,r2a,r1b}.log`,
`target/qa/canvas-render.json`. Ink acceptance fixture:
`C:/Users/ADMIN/AppData/Local/Temp/brandon-r3a-ink-X7ddFB`.

Final EXE SHA-256:
`d13c80a7e281256f67cc18e7fc3b7d9479e62e3f03fa5557e4eb72edd0451448`.
Final tests used a per-process software WebView; native wgpu remained active.
After the lifecycle suite Alpha was restored and the EXE launched normally.
No NSIS regeneration/install/push, macOS, physical stylus or server gate.

**Visual gate still pending on this final build.** An earlier ink build was
inspected in the actual Windows window and a real Sky mouse drag created a
persisted stroke. When attempting final computer-use inspection after all
regressions, capture displayed a code-password screen instead of the board.
Computer-use inputs stopped immediately; no password or security UI was touched.
The owner was asked to unlock the session. Automated in-app acceptance above is
complete, but it is not relabelled as a final visible-window acceptance. The
existing host RTSS caveat and other-editor visual-fallback limitation remain.
