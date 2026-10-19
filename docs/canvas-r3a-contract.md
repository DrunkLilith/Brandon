# R3A: shared Canvas operations and extension contract

Contract fixed before the first R3A writer, 19 October 2026. R3A is in progress;
this document is not an acceptance claim. JSON Canvas 1.0 remains the base:
[official specification](https://jsoncanvas.org/spec/1.0/). Array order is paint
order. Groups use standard geometric containment, not hidden parent IDs.

The operations slice implements grouping/layout/z-order/locking and shared
command dispatch. The following primitive/style slice implements the seven
tools, bounded typography/paint and native/fallback rendering described below.
R3A remains open for connectors, the complete task/note mixed scenario and
remaining cross-layer composition acceptance; this is not full-plan completion.

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
and `strokeWidth` (1/2/4). Optional edge `brandon` v1 has `dash` (solid/dashed).
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

Still required before closing **R3A**: connector sides/endpoints/labels/styles,
the complete canonical task/note/sticky/shape workflow, and broader translucent
image/group/text cross-layer parity on larger boards. Small-compositor/image
and ordinary large native fixtures do not prove arbitrary mixed-media layering.
Cross-editor preservation was verified for the earlier Obsidian v1 snapshot,
not separately for every new custom-paint combination. No full-parity or
lossless export claim is made. The full master-plan goal remains active.
