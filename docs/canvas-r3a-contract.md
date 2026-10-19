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
scenario and cross-layer composition acceptance; this is not full-plan completion.

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
