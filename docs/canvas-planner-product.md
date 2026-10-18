# Brandon Canvas + Planner

Status: approved product direction, 2026-10-17.

Current unified roadmap: [Brandon master plan](brandon-master-plan.md), 2026-10-18.
It is authoritative for future cross-product priorities and delivery order,
including Obsidian workflows, the Miro board references, MCP and selected plugins.
This document retains research, product decisions and implementation evidence;
its earlier "next" instructions are historical checkpoints, not the current queue.
The "Integrated desktop roadmap" section records the adopted default GPU path,
rich-media Canvas, local hybrid search and scoped Codex/Claude direction.

Brandon will combine an open, local-first visual workspace with a task planner and calendar. The goal is not to bolt a whiteboard and a date grid onto the note editor. Notes, canvas cards, tasks, calendar blocks, Bases rows, backlinks, and AI context must describe the same underlying objects.

## Research synthesis

The strongest interaction patterns from the current products are:

- Miro: an infinite canvas, direct manipulation, sticky notes/cards/shapes, connectors, frames, grouping, alignment, mind maps, Kanban and timeline widgets. Frames are simultaneously structure, navigation, presentation pages, and export boundaries.
- Todoist: list/board/calendar are alternative views of the same tasks; an unscheduled drawer feeds the calendar; dragging a task onto a day schedules it; dragging between days reschedules it; week, three-day, and month scales serve different planning depths.
- Sunsama: backlog items are deliberately less committed than scheduled work; rough time buckets prevent “someday” from becoming a graveyard; daily columns and calendar timeboxing turn intent into an executable day.
- Akiflow: planning date, deadline, duration, recurrence, and calendar lock are separate concepts. This distinction is essential: “I plan to work on it Tuesday” must not silently become “it is due Tuesday.”
- Notion: one data source can have table, board, list, calendar, and timeline views; an item remains editable from every view; the unscheduled list beside the calendar is an effective planning surface.

Primary references:

- https://miro.com/what-is-miro/
- https://help.miro.com/hc/en-us/articles/360017730973-Structuring-board-content
- https://help.miro.com/hc/en-us/articles/360018261813-Frames
- https://help.miro.com/hc/en-us/articles/360017572054-Sticky-notes
- https://help.miro.com/hc/en-us/articles/20185235301650-Timeline
- https://www.todoist.com/help/todoist/features/use-the-calendar-layout-in-todoist-lPHRQTu0o
- https://help.sunsama.com/docs/usage-guides/backlog/
- https://product.akiflow.com/help/articles/0006630-task-features
- https://www.notion.com/help/use-notion-calendar-with-notion
- https://www.notion.com/help/views-filters-and-sorts

## Product rules

1. **One object, many views.** Moving a task in the planner updates its source task. A canvas card that references a note opens that note. A task shown in a Base, board, or calendar is not a copy.
2. **Schedule and deadline are different.** `scheduledAt` means intended work time; `dueAt` is the actual deadline. Duration belongs to the scheduled work block.
3. **Unscheduled is a valid state.** Inbox and backlog are visible next to the calendar. Scheduling must be a drag, not a form-filling ceremony.
4. **Portable source remains canonical.** Markdown tasks and JSON Canvas remain usable outside Brandon. Indexes, layout caches, and calendar projections are disposable.
5. **Progressive disclosure.** The default planner shows only the decisions needed now. Recurrence, dependencies, estimates, assignees, and automation appear when requested.
6. **Keyboard and pointer are peers.** Every frequent drag operation gets a keyboard path and command-palette command.

## Canvas: Miro-class interaction target

### Portable format

- Canonical file: open `.canvas` / JSON Canvas.
- Preserve unknown node, edge, and document fields during every save.
- Use standard `text`, `file`, `link`, and `group` nodes wherever possible.
- Brandon-only presentation state goes into namespaced extension fields or a sidecar under `.brandon/`; it must never make the canvas unreadable elsewhere.

### Essential toolset

- Infinite pan and smooth cursor-centered zoom; zoom-to-fit and minimap.
- Selection rectangle, multi-select, shift-add/remove, keyboard nudging, duplicate, copy/paste, delete, undo/redo.
- Text, sticky/card, note/file, image, web link, shape, freehand, and frame/group tools.
- Straight, elbow, and curved connectors with arrowheads, labels, side anchors, reconnection, and quick-create handles.
- Resize, rotate where meaningful, z-order, lock, group/ungroup, align, distribute, snap guides, and auto layout.
- Frames that provide navigation, presentation order, export boundaries, and templates.
- Mind-map quick creation, Kanban/frame templates, and a timeline widget backed by the same task data as Planner.
- Drag Markdown notes, attachments, tasks, and Base rows from the sidebars directly onto the board.
- Search and outline for large boards; comments, cursors, and presence arrive with the team plane later.

### Performance boundary

- Spatial index and viewport culling are mandatory before claiming large-board readiness.
- Pan/zoom must remain responsive with 10,000 simple objects and 2,000 visible objects.
- Continuous pointer movement updates local state immediately; disk persistence is debounced and conflict-safe.

### Native GPU renderer candidate — added 2026-10-18

User-proposed architecture to investigate: Rust + `wgpu` renders the board to a
native GPU surface, while React/WebView provides the surrounding application UI
in the same desktop window. This is a prototype/benchmark milestone, not an
adopted renderer or an implemented feature.

The prototype must establish an actual Tauri window/surface composition path.
Do not assume that a Rust GPU texture can simply be assigned to a React canvas.
Surface ownership, native child-window placement, WebView layering, clipping,
transparency, input routing and focus need a concrete design and platform tests.

#### Prototype scope and decision gate

- Start with Windows, the current desktop target. Exercise movement, maximize,
  restore, resizing, sidebar changes and monitor switches at 100%, 125% and
  150% display scaling. Check that the native surface follows the board viewport
  without flicker, gaps, incorrect clipping or overlap with menus and dialogs.
- Render text/file cards, connectors, selection outlines and a grid with pan,
  pointer-centred zoom and hit testing. Text must remain sharp at different zoom
  and DPI settings; the test must include font/glyph handling, not only shapes.
- Verify keyboard focus, IME/text editing, wheel/trackpad input, pointer capture,
  drag-and-drop and context menus across native board and WebView controls.
  Specify who owns the active text editor and where it is composited.
- Define the Rust/WebView message boundary: shared object identities, batched
  document changes, camera/viewport updates and interaction state. Avoid a
  per-object IPC exchange on every animation frame. Preserve the existing undo
  model and conflict-safe JSON Canvas persistence across that boundary.
- Benchmark identical documents against the current renderer: 10,000 objects,
  at least 2,000 visible, representative text, images and connectors. Measure
  input latency, frame-time percentiles, CPU/GPU load, memory and startup cost.
  A target is smooth 60 Hz interaction (about 16.7 ms per frame); do not claim
  success from an empty-grid demo or an average FPS number alone.
- Exercise GPU-device/surface loss, sleep/resume and unsupported hardware.
  Define recovery and a usable fallback before making native rendering the
  default. Assess other operating systems separately rather than inferring
  portability from the Windows prototype.

Deliverables: a working isolated prototype, measured results, screenshots or a
short interaction capture, and an architecture decision covering complexity,
text rendering, accessibility, input, platform support and maintenance. Promote
this candidate into the main application only after that decision. Existing
`.canvas` files and task/note references remain the portable storage boundary.

## Tasks: portable canonical model

Historical model: the later approved SQLite task contract supersedes the
Markdown-only task-state proposal in this section. `.brandon/tasks.sqlite3` is
authoritative operational data, with Markdown task descriptions and explicit
backup/export/restore requirements. See the [master plan](brandon-master-plan.md).

Tasks originate in ordinary Markdown and retain checkbox syntax:

```markdown
- [ ] Подготовить демо #project/brandon ^task-a1b2
```

The stable block id links views to the source line. Task metadata is stored in readable Markdown fields or YAML, with a parser that preserves unknown syntax:

- `scheduled`: planned work date/time;
- `due`: hard deadline;
- `duration`: expected focused-work time;
- `priority`, `status`, `project`, `assignees`, and tags;
- `recurrence` plus completion history;
- dependencies (`blockedBy`) and parent/subtask relation.

Every task may also carry working context instead of being limited to a title:

- a Markdown description and acceptance notes;
- nested checklist/subtasks;
- links and embeds to vault notes, headings, blocks, Canvas regions, Bases views, URLs, PDFs, images, audio, video, and arbitrary attachments;
- related people/projects and a lightweight activity/completion history;
- an optional dedicated task note for work that outgrows an inline checkbox.

Inline tasks keep their context as ordinary nested Markdown beneath the checkbox. Dedicated tasks are normal `.md` files with typed YAML task fields. Dropping an existing vault object creates a reference; dropping an external file imports it through the configured attachment policy and then creates a portable link. The task detail side peek edits this Markdown directly, so documentation attached in Planner is immediately visible from the source note and vice versa.

The index exposes tasks as structured records but never becomes their sole copy. Editing or completing a task from any view rewrites the exact Markdown task through a conflict-safe transformation.

## Planner: tasks and calendar as one surface

### Default layout

```text
┌──────── Inbox / backlog ────────┬──────────── Week calendar ────────────┐
│ quick capture                    │ Mon       Tue       Wed       Thu     │
│ no date                          │ all-day scheduled tasks               │
│ next week / month / someday      │ timed tasks + calendar events         │
│ filters: project, tag, priority  │ drag, resize, complete, reschedule    │
└──────────────────────────────────┴───────────────────────────────────────┘
```

- Drag an unscheduled task onto a day to assign a date; drag it onto a time to create a time block.
- Drag a task between days/times to reschedule; drag an edge to change duration.
- A deadline is rendered as a separate marker and does not move unless explicitly edited.
- Tasks without a time stay in the all-day lane. Calendar events and task blocks are visually distinct.
- The left panel can show Inbox, no date, overdue, rough backlog buckets, or a saved filter.
- Day, three-day, week, month, agenda, board, and timeline are projections over the same task index.
- “Plan my day” mode shows available capacity, estimated workload, rollover choices, and one-click placement into free gaps.
- Quick capture supports natural date phrases eventually, but always previews the parsed result before saving.
- Completing a recurring task creates the next occurrence without destroying completion history.
- External Google/Outlook/CalDAV calendars are optional overlays; Brandon works fully offline without them.

### Convenience bar

- `Ctrl+Shift+A`: quick task capture from anywhere.
- `T`: return to today in calendar views.
- `Shift+Left/Right`: previous/next period.
- `P`: plan selected task; `D`: set deadline; `=`: set duration.
- Drag a Markdown checkbox, note, email/link, or canvas card into the planner to create or reference a task.
- A task detail side peek keeps the calendar usable while editing notes, properties, subtasks, and links.
- The detail peek includes a large Markdown brief plus an attachment/link rail. Notes, files, URLs, and Canvas cards can be dragged onto it without opening a separate upload form.

## Delivery plan

### A. Visual knowledge foundation

- Real graph tab from the Markdown index.
- `.canvas` create/open/save round-trip.
- Basic cards, note nodes, connectors, pan, zoom, and conflict-safe persistence.
- `.base` create/open and table projection.

Implemented: the complete foundation above is available in the desktop build. Canvas and Bases use the same conflict-safe resource boundary as Markdown and preserve unknown top-level and object fields while editing.

### B. Miro-class canvas editor

- Selection model, undo stack, clipboard, resize, multi-select, snapping, alignment, groups/frames, connector editing.
- Images/files/shapes/freehand, minimap, navigation outline, templates, export.
- Viewport culling and spatial index; large-board performance fixtures.
- Native Rust + `wgpu` surface experiment and renderer decision using the gate
  above; evaluate it as a separate prototype before an application migration.

Current slice: selection rectangle, Ctrl/Command/Shift multi-select, direct linking of two selected nodes, group movement, resize handles, undo/redo, copy/paste/duplicate, cursor-centred zoom, zoom-to-fit, selection actions, and a live minimap are implemented. Snapping, alignment, frames, richer node tools, editable connectors, export, and large-board culling remain in this milestone.

### C. Task index and safe Markdown transforms

- Parse checkbox tasks and stable ids across the vault.
- Implement typed scheduling, due dates, duration, recurrence, priority, project, status, dependency, and assignee fields.
- Add nested Markdown details, subtasks, linked notes/blocks, portable attachment references, drag-and-drop import, and “convert to task note”.
- Incremental task index, exact source navigation, conflict-safe completion/editing, and tests for external edits.

Current slice: each Brandon task is a dedicated `Tasks/<title>.md` document with
human-readable description content. Operational state lives in the hidden local
SQLite database `.brandon/tasks.sqlite3`, so ids, completion, scheduling and
deadlines do not leak into the editor. Portable checkbox tasks with `⏳`, `📅`,
`⏱` and priority metadata are still indexed as an optional imported layer, kept
behind an explicit Planner filter. Legacy Brandon task notes migrate into the
database without changing their ids or dates.

### D. Unified planner

- Inbox/backlog plus day/week calendar in one split surface.
- Drag scheduling, timeboxing, resizing, rescheduling, completion, rollover, and capacity hints.
- Agenda, month, board, and timeline views over the same records.
- Daily-note integration and task embedding in Canvas/Bases.

Current slice: Planner is a native virtual tab with an inbox, three-column task
board, Monday-first week calendar, pointer-driven drag scheduling and
rescheduling, completion, exact task-note navigation, explicit and automatic
deadlines, and an inline task detail peek. A manual deadline also moves the task
to that day; without one, the scheduled day is the effective deadline. Board
and calendar are projections over the same Markdown task index.

The note surface also exposes an explicit Visual/Markdown switch and a compact
quick-insert palette. GFM tables render as ordinary document tables by default
and can opt into an expanded spreadsheet-like mode with in-place cell editing,
row/column operations, and Markdown round-trip persistence.

### E. Calendar and team plane

- Local `.ics` import/export, then opt-in CalDAV/Google/Outlook adapters.
- E2EE sync, ACL-aware assignments, comments, presence, audit, and permission-aware AI planning.

## Acceptance boundaries

- A task changed in calendar, board, Canvas, or its source note resolves to one stable object and one canonical Markdown source.
- Dragging from backlog to a time slot, moving it, resizing it, completing it, and undoing each step works without a modal.
- Existing `.canvas` files round-trip with unknown fields intact; external edits surface conflicts instead of being overwritten.
- The planner and canvas remain useful with no account, network, Docker, or server.
- Desktop behavior is verified at 100%, 125%, and 150% Windows scaling.

## Integrated desktop roadmap — planning update, 2026-10-18

This section records the user's latest decisions and supersedes earlier
prototype-only delivery ordering where they conflict. It is a plan, not a claim
that these capabilities are implemented or verified. Implementation was paused
at the user's request; this update changes documentation only.

### Confirmed product decisions

- Make the Windows GPU renderer the ordinary Canvas path and remove the separate
  render-lab tab and user-facing renderer switch. Keep automatic recovery and a
  usable fallback, including the ordinary renderer on macOS until a native macOS
  renderer is separately implemented and verified.
- Canvas must accept arbitrary file attachments, including photos, video, audio,
  PDFs, documents and archives. Accepting a file does not imply every format can
  be previewed, decoded or semantically understood.
- Support both an AI panel inside Brandon and connections from external Codex /
  Claude clients through the same permission-checked tools.
- Users configure capabilities and resource scopes. The initial capability set
  is search, answers and reading explicitly permitted files/boards, not writes,
  deletion, command execution or unrestricted access to the computer.
- Search indexes and embedding inference are local by default. Cloud chat remains
  a separate, explicit data disclosure decision; local embeddings do not make
  an external chat model local.

### Architecture and canonical data boundaries

One Rust application-service boundary must serve desktop IPC, search and agent
tools. Do not let agents, renderers or indexers write SQLite or Canvas JSON through
separate bypass paths.

- Portable content: Markdown, JSON Canvas and attachment files inside the vault.
- Authoritative operational task state: the existing `.brandon/tasks.sqlite3`.
  Include it in backup/migration/export; it is NOT a disposable search cache.
- Rebuildable derivatives: extracted text, thumbnails, text/vector indexes and
  model caches, each separately versioned and removable without losing tasks.
- `ResourceRef` identifies a vault resource and an optional Markdown block,
  Canvas node, PDF page, spreadsheet location or media time range. Store source
  revision/content hash with derived chunks and citations.
- Agent tools and all search results enforce the same current authorization
  policy in Rust, not only in React or model instructions.

### 0. Promote GPU Canvas and complete the bug bash

1. Remove the lab route, rail action, command and renderer-toggle UI. Move large
   synthetic scenes into test fixtures instead of shipping a separate workspace.
2. Automatically initialize the native renderer on supported Windows systems.
   Handle cancellation during startup, fast tab changes, surface/device failure,
   minimization and restore without leaving a transparent or blank workspace.
3. Use one Canvas document, interaction state, undo model and atomic/conflict-safe
   persistence for native and fallback rendering. Preserve unknown JSON fields.
4. Fix regressions before promotion: text/edit overlay alignment and focus, new
   card editing, Ctrl/Cmd shortcuts with Russian layouts, selection/linking,
   resize, clipboard, cursor-centred wheel zoom and stale state between boards.
5. Reject malformed Canvas input visibly; never silently turn an invalid document
   into an empty board that can overwrite the original.
6. Test real temporary `.canvas` files, save/reopen, external edits and conflicts;
   preserve the user's active vault during QA.

Gate: ordinary boards automatically use GPU where supported; no separate GPU
workspace is needed. Measure 10k/20k scenes and at least 2k visible simple objects,
including input-to-update latency and frame-time percentiles. CPU preparation
time alone is not display FPS. Actual Windows DPI/monitor, sleep/resume and macOS
checks must be reported separately from app-zoom or build-only checks.

Implementation checkpoint, 18 October 2026:

- Ordinary Windows `.canvas` files now start native wgpu automatically after a
  platform capability check; the render-lab route/action/toggle were removed.
  Large deterministic scenes live in `tests/desktop/canvas-fixtures.mjs`.
- Native startup/stop is serialized across tab changes. A failed renderer restores
  the same editable document in the DOM/2D fallback; reopening retries GPU.
  Unsupported platforms keep the fallback, without pretending native support.
- GPU and edit overlays share dark card colors and text placement. Depth testing
  prevents lower-card text from bleeding through overlapping higher cards.
  New cards focus their editor; Escape exits it; physical Ctrl/Cmd key codes work
  with Russian layouts. Switching boards resets per-board interaction state.
- Malformed JSON/invalid geometry produces an explicit non-editable error, not a
  silently empty document. Unknown valid Canvas fields survive edits and saves.
- Standalone Windows release and NSIS installer built. Five Canvas unit tests,
  ten real-vault render smoke scenarios and seventeen editor/navigation scenarios
  passed; desktop Clippy and formatting checks passed. Native-window visual QA
  also checked Russian text, colored overlapping cards and edit-overlay alignment.
- Latest local stress run: RTX 3090/DX12, DPR 1, 60 samples after 10 warm-ups,
  approximately 5% zoom, text LOD off. 10k total/10k visible: CPU preparation p95
  7.72 ms, input acknowledgement p95 29.60 ms. 20k total/13,770 visible: CPU p95
  11.73 ms, acknowledgement p95 28.00 ms. Acknowledgement includes two RAF callbacks
  and IPC, not input-to-photon latency; these are NOT measured display FPS.
  Reproduce with `pnpm --dir apps/desktop test:render` against the running standalone
  build with WebView2 remote debugging on localhost:9222; report:
  `target/qa/canvas-render.json`. QA uses disposable vaults and restores the prior
  vault in `finally`; it never stress-tests the user's own board.

Remaining promotion gate coverage: real mixed-DPI/multi-monitor changes,
sleep/resume/device removal and native macOS integration. Do not describe those
as tested based on app zoom, injected failure recovery or a Windows build.

### 1. Shared resources, attachments and access policy

Add focused modules/crates for resource services, search and agent integration
on top of `knowledge-vault`, rather than embedding everything in `main.rs` or
`app.tsx`. Final crate boundaries follow dependency direction and tests.

- Attachment import accepts Explorer/Finder drop, clipboard and file picker;
  dragging an existing vault object creates a reference rather than a copy.
- Default external import copies into a configurable vault attachment directory.
  Deduplicate safely, resolve name collisions without overwrite, report progress,
  support cancellation and clean up incomplete imports. Do not load a large video
  or archive into a JSON/base64 IPC message.
- Canonicalize paths; block traversal, escaping symlinks and accidental access to
  credentials. Do not follow external Canvas paths without an explicit grant.
- Identify content types and select safe preview/extraction handlers. Keep the
  original bytes; unknown/unsupported types get an ordinary file card.
- Define per-connection capabilities: search, read, send-to-selected-model,
  web access, propose changes, apply changes, import, delete and execute commands.
  Only the first read-oriented capabilities are initial candidates, and even
  these apply exclusively to the user's granted scope.
- A selected board permits reading its own nodes/layout. It does not implicitly
  authorize opening every referenced file or recursively traversing linked notes.
- Filter agent search by scope before snippets/context are returned. Recheck
  authorization at read/apply time. Revocation stops future reads and pending
  operations; already transmitted external context cannot be recalled.

Gate: an unauthorized resource cannot leak through filenames, snippets, cached
extractions, thumbnails, search results or tool error details. Default sessions
have no shell, deletion or write capabilities.

### 2. Rich-media Canvas

Use standard JSON Canvas `file` references for attachments and namespaced optional
presentation extensions. Preserve other applications' unknown fields and source
bytes; do not invent incompatible mandatory media-node types.

- Images: thumbnails, aspect-ratio-aware resizing, large-image limits and explicit
  full-size view. PDFs: lazy page thumbnails and a page-aware reader.
- Video/audio: poster/waveform where available, controls and time-position links;
  instantiate players only when needed. Unsupported codecs show a useful fallback
  rather than a blank card or silently transcoding originals.
- Other files: name, type, size and explicit open/reveal action. Archives are not
  automatically unpacked and executable/HTML attachments are never auto-executed.
- Pasted text/URLs and imported documents use the same resource graph. Remote
  embeds and network-fetched previews are optional and clearly distinguished from
  local files, so opening a board does not silently contact arbitrary hosts.
- GPU renders static card geometry and thumbnails. Focused text editors, document
  readers and active players use bounded WebView overlays with shared transforms
  and hit testing. Do not create a DOM player or decoder per offscreen node.
- Separate thumbnail/decode/texture caches with RAM/VRAM budgets and eviction.
  Stop offscreen playback/decoding; background extraction must not stall input.

Gate: mixed media survives save, rename, move and reopen; relative links remain
portable. Test corruption, missing files, huge dimensions, codec gaps, cancelled
imports, overlapping players, window resizing and near/far zoom.

JSON Canvas already includes attachment-capable file nodes; this is an
implementation extension of the existing format, not a new storage format.
[JSON Canvas 1.0](https://jsoncanvas.org/spec/1.0/).

### 3. One local text, vector and hybrid search

- One UI with explicit Text / Meaning / Hybrid modes, path/type/tag/date filters,
  snippets, keyboard navigation and exact source links. The agent calls this
  service rather than maintaining a different search corpus.
- Text search: start with SQLite FTS5/BM25, boosted names/headings, exact phrases
  and identifier/path lookup. Keep literal/substring search distinct from token
  matching. Test Russian and English, punctuation and code; English-only stemming
  is not an acceptable Russian search strategy.
- Chunk notes by structure and Canvas by node, not whole-board JSON. Add task
  titles/descriptions/operational fields through their existing canonical service.
  Link each chunk back to an exact source anchor and revision.
- Extract supported PDF/document text in background. OCR and audio/video
  transcription are separately opt-in jobs, local by default. Show which sources
  are indexed, unsupported, failed or still pending; do not imply raw binary data
  was understood by a text embedding model.
- Local multilingual embeddings with an `EmbeddingProvider` interface. Download
  the selected pinned model with visible size/progress/cancel and checksum/license
  metadata. Benchmark Russian/English quality, CPU/RAM and Windows/macOS behavior
  before committing to a model/runtime.
- Keep lexical search usable without a model download, GPU, network or AI account.
  Version chunking/model/dimensions in the cache. Update changed content only,
  remove deleted content and rebuild derivatives safely after schema/model changes.
- Choose a local vector backend from measured candidates. Start with an exact
  filtered baseline for correctness; add an ANN index only where it is needed.
  Do not require a separate vector server or Docker for the local product.
- Hybrid combines lexical and vector ranks (initial candidate: reciprocal-rank
  fusion), deduplicates source chunks and may add optional local reranking.
  Authorization and filters must apply consistently across both retrieval paths.

Gate: a reproducible Russian/English query set covers paraphrases, exact IDs,
paths, snippets, task dates and citations. Compare lexical/vector/hybrid relevance
and report cold/warm latency, index cost, memory and rebuild behavior on named
hardware; no unmeasured universal performance promises.

FTS5 provides BM25, snippets and match highlighting as a suitable local lexical
baseline. [SQLite FTS5](https://www.sqlite.org/fts5.html).

### 3a. Local OCR, transcription and video extraction

Research checkpoint: **18 October 2026**. These are candidates and integration
decisions, not model benchmarks run by this project. No model weights have been
downloaded or engines bundled at this checkpoint. There is no universal "best
OCR": Russian screenshots, scanned tables and clean text PDFs need different
paths. Select a measured default per workload/platform, not a leaderboard winner
from an unrelated language or server batch size.

#### Document/image shortlist

| Workload | Provisional route | Decision gate |
| --- | --- | --- |
| PDF with usable embedded text | Extract text, layout/page anchors and tables where supported; OCR only deficient pages/regions | Reading order, encoding and duplicate invisible OCR layers |
| Screenshots, simple scans, photos of text | Compare RapidOCR/ONNX with an explicitly selected Cyrillic-capable recognizer against Tesseract `rus+eng` | Russian CER, small text, CPU latency and packaging on both OSes |
| Complex scans, tables, formulas | PaddleOCR-VL 1.6 quality-mode candidate; compare GLM-OCR before pinning | Structure accuracy, hallucinations, memory and a viable offline desktop runtime |
| Document orchestration | Evaluate Docling as an adapter, not as a competing OCR model | Output anchors, native-text routing, runtime size and dependency licenses |

[RapidOCR](https://github.com/RapidAI/RapidOCR) supports ONNX Runtime and other
backends; do not ship its default Chinese/English recognizer and assume Russian
support. [Tesseract](https://github.com/tesseract-ocr/tesseract) is the CPU baseline,
not a claim of state-of-the-art layout understanding.

[PaddleOCR-VL 1.6](https://huggingface.co/PaddlePaddle/PaddleOCR-VL-1.6) is a compact
0.9B document model with structured text/table/formula recognition. Its official
Mac installation currently advises Docker; the Transformers example only handles
element recognition/text spotting, not the complete page pipeline. It therefore
cannot yet be our mandatory cross-platform default. Published benchmark results
are the authors' measurements, not evidence of Brandon's Russian-document quality.

[GLM-OCR](https://github.com/zai-org/GLM-OCR) is another compact 0.9B candidate;
verify Russian quality and explicitly configure local inference, not a cloud
quickstart. [Docling](https://github.com/docling-project/docling) provides local
document conversion/layout and pluggable OCR; keep this distinction from the
recognition model. [Surya](https://github.com/datalab-to/surya) remains a comparison
candidate, but its code and weight licensing differ: check the exact weight
license and redistribution terms before bundling. The same license/revision audit
applies to every model and runtime, including PDF renderers and media decoders.

PDF preprocessing must not rasterize every document by default. Detect bad text
layers rather than trusting mere presence of text; preserve page number, rotation,
bounding boxes and original source hash. Mixed scanned/text PDFs can take mixed
paths. Never execute PDF scripts/macros or overwrite the original with OCR output.

#### Audio shortlist

- Primary quality/compactness experiment: **Qwen3-ASR 0.6B vs 1.7B**, both with
  Russian support. The official architecture combines an audio encoder with a
  language-model decoder; parameter count alone does not establish desktop speed
  or memory. Test a pinned local Transformers runtime first, without requiring a
  server, Docker or CUDA. Native Apple Silicon support/performance is a separate
  packaging gate, not implied by the model architecture.
- **Qwen3-ForcedAligner 0.6B** can add word timestamps, including Russian. Chunk
  long inputs with overlap and reconstruct original time offsets. Forced alignment
  aligns words to audio; it does NOT distinguish different speakers. Diarization
  is a separately evaluated optional provider.

[Official Qwen3-ASR models and inference documentation](https://github.com/QwenLM/Qwen3-ASR).

- Cross-platform baseline/fallback: **whisper.cpp**, with a multilingual Whisper
  checkpoint and quantization when appropriate. Its CPU, Windows and Apple
  Silicon/Metal paths make it a useful packaging baseline; it still needs our
  Russian quality/real-time-factor measurements.
  [whisper.cpp](https://github.com/ggml-org/whisper.cpp).
- Do not accidentally choose English-only Distil-Whisper for Russian. Compare
  multilingual Whisper large-v3-turbo instead where the hardware budget allows.
  [Distil-Whisper language scope](https://github.com/huggingface/distil-whisper).

Use VAD and bounded chunks, retain punctuation and language metadata, check silence
hallucinations/repetition and allow manual correction. Keep corrected transcript
data as user-owned content, separate from a rebuildable raw ASR result. Do not
claim automatic speaker names; optional speaker labels are not identity proof.

#### Video is a synchronized pipeline, not just a VLM prompt

1. Probe local streams/codecs/duration safely. Preserve media timestamps, including
   variable-frame-rate sources and offsets. Originals remain untouched.
2. Extract audio to the same ASR path. Embedded subtitles can be imported with
   their provenance/language, but may be wrong, translated or offset; never label
   them as independently verified speech recognition.
3. Sample scene-change/key frames with time/frame/resolution budgets and duplicate
   suppression. OCR changed slides, screen recordings and other textual regions.
   A quiet/no-audio video must remain searchable through this visual path.
4. Optional visual understanding: benchmark **Qwen3-VL 2B/4B** on sampled frames
   or bounded clips. The model family supports video and timestamp-based temporal
   reasoning. [Qwen3-VL](https://github.com/QwenLM/Qwen3-VL).
5. Merge ASR, subtitles, frame OCR and visual descriptions into separate typed
   evidence. Every search hit opens the original file at its page/time/region.
   A generated scene summary is NOT a verbatim transcript or proof of an event
   between sampled frames. Preserve sampling gaps and uncertainty visibly.

#### Runtime, permissions and acceptance tests

- Rust owns the job queue and resource authorization; adapters run in isolated
  background workers with bounded IPC, cancellation, progress, timeouts and crash
  recovery. An initial Python research worker must not become an unconditional
  Python/PyTorch/Docker dependency for opening the desktop application.
- Models are optional, pinned, checksum-verified downloads with visible disk/RAM
  requirements and license metadata. No background weight downloads on startup,
  private test-file uploads or silent cloud fallback. A local mode remains offline
  after installation. Respect the user's machine-wide model-cache preference.
- GPU Canvas input has foreground priority. Limit worker concurrency, RAM/VRAM,
  image size and total video frame/token budgets; pause/reduce heavy jobs if they
  starve the board. OOM must fail a job, not the application. CPU paths are slower
  but must not freeze the UI. Do not load every optional model simultaneously.
- Extraction caches are keyed by source hash plus engine/model/config versions;
  store page/region/time anchors, provenance and quality flags. Jobs never bypass
  current resource grants. Agent retrieval must filter cached text and media
  metadata under the same policy as direct file reads.
- Build a small consented/licensed RU/EN evaluation corpus: dark-theme screenshots,
  small fonts, rotated/noisy scans, mixed PDFs, tables/formulas, Russian technical
  speech, code switching, silence/music, meetings and screen-recorded video.
  Keep separate development and held-out samples; publish failures, not only wins.
- Measure OCR CER/WER and reading order/table structure, ASR normalized and strict
  WER plus timestamp error, speaker errors if enabled, and video retrieval/source
  timestamp accuracy. Also report cold/warm latency, real-time factor, peak RAM/
  VRAM, download/install size and active-board responsiveness on named Windows
  CPU/NVIDIA and Apple Silicon configurations. Authors' throughput at large server
  concurrency is not a desktop performance promise.

Delivery: attachments/anchors + lexical search first; then embedded PDF text and
light OCR; ASR baseline plus Qwen comparison; optional complex-document quality
mode; finally frame OCR and visual video summaries. Text-only search must remain
usable while any of these optional components are absent or still processing.

Autonomous work checkpoint, 18 October 2026, midday (MSK):

- User authorized local GPU/Docker experiments and scoped continuation until
  approximately 16:20 MSK. Temporary thread heartbeat `brandon` checks every five
  minutes; it must stop/remove itself at that deadline or on the user's explicit
  return. Keep the Windows machine awake and the app open; no cloud-hosted
  continuation was configured.
- Telegram is connected through `scripts/telegram-notify.ps1`, using only official
  outbound HTTPS requests, no listening server, no Telegram-to-shell bridge.
  Both `from.id` and private `chat.id` must match the locally configured owner. Setup verified
  `getMe`; outbound send and an inbound ping/reply were verified. Eight mocked
  tests in `tests/desktop/telegram-notify.tests.ps1` cover DPAPI storage, allowlist,
  groups/mismatched senders, cursor/acknowledgement, safe errors and tampering.
  Read messages with `Poll` then `Inbox`; after responding with `Send -Message`,
  use `Ack -UpdateId` to avoid repeated processing. This is periodic agent polling,
  not instant forwarding into the chat or automatic execution of remote commands.
  This owner-specific helper and its tests are intentionally local-only and are
  excluded from the public source distribution.
- Runtime token/configuration/cursor/inbox live in the current Windows user's
  LocalAppData/Brandon/notifications, with restricted directory ACL and DPAPI
  current-user token encryption. The supplied root `token.txt` remains in place
  but is ignored and untracked; never print, commit, archive or mount it into
  model containers. No token is stored in this document or automation prompt.
- Docker startup hit a stale AF_UNIX socket. With the failed instance stopped,
  verified that its runtime directory contained only a zero-byte `engine.sock`;
  preserved it as `docker-secrets-engine.preserved-20261018-1231` in LocalAppData.
  No factory reset, image/volume removal or reinstall. Docker Engine 29.8.2 is
  healthy again. A real `--gpus all` CUDA container sees RTX 3090 / 24,576 MiB.
  [Matching upstream issue](https://github.com/docker/for-win/issues/15064).
- `scripts/local-ai/` contains isolated, opt-in OCR/ASR smoke-probe tooling. Docker
  base is pinned by digest; package versions and Qwen model revisions are explicit.
  Only the scripts and generated `target/qa/local-ai` fixtures/results are mounted,
  never a user vault, token file or Docker socket. Network can be disabled for
  cached runs. Synthetic DejaVu RU/EN screenshots and eSpeak speech are plumbing
  tests, not a held-out real-world quality benchmark. Runtime and model packaging
  remain experimental and are NOT integrated into the desktop product yet.
- OCR smoke now executes successfully with RapidOCR 3.10.0 / ONNX Runtime 1.31.0
  and Tesseract 5.3.4 `rus+eng`; it also passes with Docker `--network none` after
  model download. The three ONNX files total 18,589,218 bytes, pinned by SHA-256 in
  `scripts/local-ai/ocr-models.lock.json` and verified before inference. Selection:
  PP-OCRv6 small detector + v4 orientation + v5 Cyrillic recognizer. On the two
  simple synthetic screenshots Tesseract made zero errors, whereas the Cyrillic
  neural recognizer confused Latin/Cyrillic lookalikes in `notes.md`. Do not infer
  broad quality superiority from this tiny corpus. Exact raw text/boxes/timing and
  model hashes are in ignored `target/qa/local-ai/ocr*.json`. ONNX telemetry is
  disabled through its API; the offline run additionally blocks container network.
- During foundation review, found that saves removed the original before rename
  and shared a fixed temporary filename across same-stem `.md`/`.canvas` files.
  Changed to unique create-new temporary files and replacement by rename, without
  first deleting the original. Two new regression tests cover unrelated temp-file
  preservation, failed replacement cleanup and parallel saves across formats;
  all eleven vault tests and desktop/vault Clippy pass. This source change still
  needs the next standalone desktop rebuild and regression run before delivery.
- ASR image `brandon/local-ai-asr:20261018` is built. Both Qwen3-ASR 0.6B and
  1.7B execute on the actual RTX 3090 using BF16/SDPA; 0.6B also passed a cached
  Docker `--network none` run. The initial librosa/Numba cache error was fixed by
  setting `NUMBA_CACHE_DIR` to the writable isolated model cache before imports.
  Synthetic Russian eSpeak WER was 69.2% / 46.2%, English WER 0% / 0%; these are
  plumbing observations, not a natural-speech quality ranking. Approximate warm
  Russian timings were 0.51 s / 0.93–0.99 s for 7.16 s audio; peak allocated CUDA
  memory 1.64 / 4.15 GB. Raw reports: `target/qa/local-ai/asr-{0.6b,1.7b}.json`.
  Five public Russian FLEURS recordings have now been fetched for a follow-up
  comparison, with CC-BY-4.0 attribution, immutable revision and per-file SHA-256
  in `target/qa/local-ai/fleurs-ru.json`. Only 3,081,567 compressed archive bytes
  were streamed; no full dataset/archive or user files were copied. This tiny
  convenience sample is not held-out or representative. Model revisions are
  pinned in `probe.py`; the `qwen-asr` package is deliberately NOT installed:
  release 0.0.6 hard-pins Transformers 4.57.6, whereas the official `-hf` model
  uses the native Transformers 5.13+ path (our pin is 5.20.1). These are different
  integration routes; do not mix their snippets/dependencies. Neither OCR nor ASR
  is integrated into product search or background workers at this checkpoint.
- Follow-up natural-speech comparison is complete, both runs under Docker
  `--network none`: five unchanged FLEURS Russian clips total 64.44 s and 88
  reference words. After the explicitly documented punctuation/case/whitespace
  normalization, 0.6B has 3 word edits (3.41% corpus WER), 1.7B has 1 (1.14%).
  Median warm real-time factor across ten warmed inferences is 0.0676 / 0.0664;
  peak PyTorch allocated memory 1,733,156,864 / 4,245,486,080 bytes. Allocated
  tensors are not total process/driver VRAM. This is a tiny convenience sample;
  no held-out status, broad quality ranking or speed advantage is claimed.
  Raw per-clip outputs, references, model revisions, timings and source/license
  provenance are retained in `target/qa/local-ai/asr-*-fleurs-ru.json`; reproducible
  unique-clip aggregation is `scripts/local-ai/summarize_asr.py` and its ignored
  `asr-comparison.json` result. Seven offline tooling tests pass. Next experiment:
  a pinned Whisper baseline on the same clips, then more varied licensed speech;
  complex-document OCR candidates remain researched, not executed.

Attachment delivery checkpoint, 18 October 2026, 13:05 MSK:

- Normal Canvas now has a `Файл` picker and native drop-event import path. Existing
  vault files dragged from the tree stay relative references; external files are
  streamed into `Attachments/` with progress/cancel, no base64 originals in JSON.
  Imports refuse private/hidden paths, credential/key names, links/junctions and
  files above 20 GiB. Staging is hidden, cancellation removes only its own partial
  staging file, and publication never overwrites an existing destination. Same-name
  identical content is reused; changed bytes receive a collision suffix. This is
  not yet global content-addressed deduplication.
- Raster cards show bounded normalized PNG thumbnails (640×400 maximum); binary
  IPC, a two-job decoding semaphore, original/decode limits and at most 24 visible
  DOM previews bound resource use. DOM image overlays are clipped against upper
  native GPU cards; complex overlaps safely fall back to the file card. Active
  SVG/HTML content is not rendered. Non-note files open a metadata/preview side
  peek rather than the Markdown editor, with an explicit reveal-in-folder action.
- Arbitrary files, PDFs, audio and video can be imported/referenced, but embedded
  PDF/audio/video viewers, disk preview cache, agent resource grants and full
  media codec support are NOT implemented. Their limits remain visible in UI.
- Latest standalone Windows build includes the atomic-save fix above. Validation:
  19 Rust tests, vault/desktop Clippy, 5 Canvas unit tests, 10 automatic GPU/fallback
  smoke scenarios, 17 editor/navigation scenarios and 7 attachment scenarios all
  pass. Attachment tests cover copies/reuse, rejected private filename, bounded
  preview, internal reference drop and save/reopen. Import/drop events in the
  automated suite are injected, not a physical Explorer drag.
- Native-window visual QA additionally verified that images appear correctly on
  the GPU board and remain occluded by upper cards during a real pointer drag.
  Isolated generated QA vaults were used; the original user vault was restored.
  Actual Explorer/Finder external drag gestures, macOS, mixed-DPI and sleep/resume
  remain unverified. No push or publication was performed.

Next continuation should implement the next bounded product slice: incremental
local lexical search, starting from the existing `Vault::search`/sidebar. It is
currently a full Markdown scan on every query, not an FTS service; it omits Canvas
card text and has no agent scope enforcement. Keep the new cache separate from
authoritative `.brandon/tasks.sqlite3`, move work off the UI thread, distinguish
token/phrase search from literal matching, provide exact note/card anchors, and
test Russian/English, code/punctuation, external edits/deletes and private paths.
Do not expose this full-vault UI search to an agent before grant filtering exists.

Manual follow-up while autonomy is paused, 18 October 2026:

- User requested removal of browser text selection on Canvas controls and of the
  visible `wgpu · visible / total` debug badge. The badge/help overlay is removed;
  renderer diagnostics remain nonvisual data attributes for regression tooling.
  Canvas chrome and gestures suppress document selection, while actual inputs,
  textareas and editable content retain normal text selection. Pointer handling
  also prevents native selection during board movement/resize/marquee gestures.
- Rebuilt standalone Windows executable and NSIS installer. Six targeted live
  application checks pass: toolbar pointer drag, marquee/Ctrl+A object selection,
  pointer/keyboard selection inside a card, editing/save, fallback card dragging
  and normal note-editor selection. Native-window visual inspection confirms the
  clean GPU board without the diagnostic overlay. Five Canvas unit tests, nineteen
  Rust tests, desktop lint and vault/desktop Clippy pass.
- Unfinished FTS work remains in source only. The ordinary `Vault::search` IPC path
  explicitly retains the previous Markdown search behavior, so this unrelated UI
  rebuild does not promote the experimental index. Automation `brandon` remains
  PAUSED at the user's request; this fix does not resume autonomous work.

Afternoon autonomy and local-search delivery checkpoint, 18 October 2026:

- The user resumed product work at approximately 15:20 MSK for three hours.
  Existing heartbeat `brandon` is ACTIVE until **18:20 MSK**, not a second timer.
  The authenticated Telegram request to add two hours to the old 16:20 boundary
  agrees with this deadline. Questions and significant reports go to the existing
  private-chat allowlist (6411921436); both pending messages were answered and
  acknowledged through update 949576652. Do not send secrets or vault contents.
- Desktop search now uses the existing SQLite FTS5/BM25 service rather than the
  legacy Markdown-only scan. Content reads/reindexing are incremental by source
  metadata and task-state fingerprint, but directory metadata is still scanned
  per query: this is NOT a fully watcher-maintained index or constant-time search.
  One serialized blocking worker keeps the filesystem/SQLite work off the UI
  thread, and obsolete queued jobs are skipped. The derivative search cache stays
  separate from authoritative `.brandon/tasks.sqlite3`, which search opens read-only.
- The restrained sidebar has Words / Literal modes, a type selector and existing
  `path:`, `tag:` and `type:` filters. Words supports quoted phrases and explicit
  trailing-prefix `*`; Literal preserves code quotes, repeated spaces and newlines
  as contiguous text (case-insensitive), while retaining explicit filter tokens.
  Filter-only browsing lists resources once instead of duplicating each section;
  text queries retain note-section/Canvas-node anchors. Task hits are deduplicated.
- Results open an exact Markdown line, select and centre a Canvas node, or open
  the existing attachment peek for non-note files. Operational task fields anchor
  to the task title, not fictitious Markdown lines. Changed source revisions are
  visible on opening. A whole-board search result fits the board instead of
  opening an unrelated file at the origin. No agent or MCP access was enabled.
- Private/hidden/credential paths and symlinked source/cache paths are excluded.
  Referenced Canvas files and URLs are not dereferenced, and arbitrary binary
  contents are not treated as extracted text. Coverage distinguishes text-indexed,
  name-only, failed and skipped sources. First-slice limits: 50,000 sources,
  16 MiB/source, 128 MiB of changed-source reads per update, 1,024 Markdown chunks
  and 20,480 Canvas nodes/source. Notes beyond the chunk budget become visibly
  name-only rather than silently partial. Existing task properties remain a
  separate searchable canonical source. Cache policy stamps invalidate old chunks.
- Cache/journal/access-only watcher events no longer cause search-refresh loops.
  An explicit request observer prevents canceled results, errors and completions
  from overwriting current UI state. Four deterministic promise-order regressions
  exercise this; live UI additionally checks rapid query changes and idle stability.
  Native Tauri IPC is immutable and was NOT replaced or weakened for test injection.
- Verified so far: 27 vault tests on Windows (17 existing + 10 search), 28 Linux
  vault tests including Unix symlink protection, 2 native desktop unit tests,
  5 Canvas unit tests and 4 request-order tests on both Windows and Linux.
  Eleven real desktop search scenarios (including one-shot source navigation), 17 editor/navigation,
  7 attachment and 10 GPU/fallback scenarios. Formatting, desktop lint/build and
  vault/desktop Clippy pass. Synthetic fixtures only; the prior vault is restored.
  Reproduce UI checks against a standalone WebView2 debug build using
  `node tests/desktop/local-search-smoke.mjs`; report `target/qa/local-search.json`.
  Screenshot inspection also covers the compact dark search sidebar and line jump.
- Standalone Windows executable and NSIS installer have been built locally at
  `target/release/knowledge-desktop.exe` and
  `target/release/bundle/nsis/Brandon_0.1.0_x64-setup.exe`. Eleven search UI checks,
  including the final one-shot-jump regression, pass on this build. Full local
  `scripts/release-check.ps1` on **1b9713d** finished with `Release gate PASS`,
  including both restore drills, policy/security checks and 18 browser E2E.
  This source commit remains local, not published. Installation itself and native
  macOS search UI are not claimed as tested.
- A single synthetic 20k-card Rust debug run measured roughly 0.8 s cold indexing
  and 1–2 ms warm lookup. This is not a multi-run benchmark, UI latency or display
  FPS. The latest GPU stress report is `target/qa/canvas-render.json`; it was a
  concurrent regression run, not a clean performance comparison.
- CI repairs already pushed under the prior CI request: portable cross-platform
  path checks; draining fake-Docker output before Unix exit; Compose-configured
  private maintenance tmpfs instead of unsupported `compose run --tmpfs`; and
  waiting for the rendered space selector, not just history URL navigation.
  `293d63f` passed the complete local release gate including 18 browser E2E.
  Remote run 39128420194 on `8268a69` completed successfully: `Release gate PASS`
  and 18 browser E2E passed; conclusion verified on 18 October at 16:10 MSK.
  New product changes stay local;
  a prior remote CI result is not verification of unpublished product changes.

#### Canvas relocation checkpoint — 18 October, afternoon

The Canvas reference bug is repaired locally; do not implement it again. The
initial isolated IPC probe left stale `Old.md` / `Attachments/picture.png` refs.
Three new regressions failed before the fix, then passed. Automatic link updates
now cover note, attachment and Canvas file rename/move, folder prefixes and boards
inside a moved folder, including self references. `updateLinks: false` remains an
explicit opt-out; destination collisions do not change source files or backlinks.

- Prepare the backlink plan before the filesystem move, then check each planned
  source fingerprint again before its atomic replacement. Changed/unavailable
  documents are left intact and returned in structured `linkWarnings`; the move
  itself still succeeds. The UI remaps tabs and shows a persistent, dismissible
  warning with affected paths/reasons instead of reporting the whole move failed.
  This is optimistic conflict detection, NOT an OS file lock, all-or-nothing
  multi-file transaction or protection against every possible external-write race.
- Canvas rewrites patch only known `type: file` nodes' `file` string byte spans.
  They do not reserialize the board: layout, IDs, subpaths, edges, unknown fields,
  exact unknown number tokens and all other bytes survive. Text nodes, URLs,
  unknown node kinds, private paths and symlink/reparse targets are not rewritten.
  The raw preservation guarantee here concerns automatic relocation, not arbitrary
  later JavaScript JSON edits. Duplicate known keys/malformed Canvas stay intact
  with a warning. No URL or referenced file is fetched during backlink scanning.
- Existing Markdown wiki-link behavior is retained, now with the same optimistic
  pre-write source check. Managed task IDs, completion, scheduled/due dates and
  descriptions survive task-folder/note moves. This does NOT add a complete
  Markdown relative-image/link rewriting engine or solve ambiguous wiki basenames.
- Limits are explicit: 16 MiB/source and rewritten document, 128 MiB source reads
  and separately 128 MiB planned output, 50,000 candidate sources and 20,480 nodes
  per Canvas. Beyond-limit documents stay intact with a warning; directory metadata
  still uses the existing snapshot scan. This is not a large-vault performance claim.
- Windows vault checks pass: 28 unit + 10 search integration tests,
  including 11 new relocation cases. Actual Linux Docker checks pass: 29 unit +
  11 search integration tests, including internal symlink exclusion.
  Native desktop units, 5 Canvas and 4 request-order tests, formatting and Clippy
  pass. Seven real desktop UI/IPC relocation scenarios pass: rename dialogs and
  GPU-card opening, stale save conflict, attachment byte/preview survival, moved
  active board tab, edit/save/reload, and persistent partial-update warning.
  Existing 11 search, 17 editor/navigation, 7 attachment and 10 GPU/fallback UI
  checks pass on this stage. Synthetic fixtures only; original vault restored.
  Reproduce via `node tests/desktop/canvas-relocation-smoke.mjs` against the debug
  WebView2 endpoint; report is `target/qa/canvas-relocation.json`.
- A local Windows executable and NSIS are rebuilt for this source; installation
  and native macOS UI remain untested. Source commit **4ed672c55ea9b7cad6c462f5c629c7bf241c53a9**
  passed the complete local `scripts/release-check.ps1` with exit 0 and explicit
  `Release gate PASS`: isolated workspace Rust tests, 115 web tests, policy/security
  checks, fresh Compose acceptance, maintenance ownership, both restore drills
  and **18 browser E2E**. Existing allowed dependency advisories were not relaxed.
  This is a local result, not a new GitHub CI run. No push.
  Rebuilt EXE SHA-256: `C29EB48EE27006B9043A9033B3638FE338C1698E69A62CB3706391045B2D481D`;
  NSIS SHA-256: `566566C6E6308E4A8226B03682B3E9DCC94EBE79397F61BB814D2A9475756EDF`.

#### Desktop draft recovery checkpoint — 18 October, afternoon

The external-rename draft loss is repaired locally; do not implement it again.
The earlier synthetic probe demonstrated both `draftStillVisible: false` and
`diskContainsDraft: false`. The new real UI regression failed on the old build
before the recovery UI was implemented; the exact Markdown-copy unit also failed
before adding its create-new path.

- A missing source no longer clears a dirty/conflicted/pending-save document.
  Its editor and tab stay open, failed autosave stops, and stale refresh snapshots
  cannot prune a just-opened/recovered document. Other save failures, including a
  real Windows read-only file, also retain the draft and offer recovery.
- Save Copy writes the exact submitted Markdown/Canvas bytes into a separate
  vault file through exclusive create-new. Existing names fail rather than being
  overwritten. Later typing is retained as dirty content on the new path, then
  saved there. If the newly created copy changes before it is read back, it is not
  acknowledged as this draft or silently overwritten. Canonical task identities
  and operational properties are not inferred or duplicated by this text-copy flow.
- A reappeared source does not silently restart autosave. Check Source opens the
  existing comparison flow; only explicit load/overwrite resolves it. Explicit
  overwrite uses the compared source fingerprint and resumes ordinary autosave.
  Discard has its own in-app confirmation; Cancel retains the draft, and confirming
  removes only the volatile editor draft, not disk files. WebView `confirm` was
  unsuitable for this test and is not used for the new recovery action.
- Navigation, Ctrl+R/F5 and native window-close requests are guarded while work is
  pending. A normal dirty close flushes the latest text before actually closing.
  The JS close listener exists only while needed, cleans up on unload, and uses
  the already granted close command after unsubscribing; **no Tauri capability or
  agent permission was added**. The clean QA close helper now verifies the target
  actually disappeared instead of assuming that an accepted close request closed it.
- This is in-memory protection while Brandon runs, NOT crash/restart or forced
  WebView-reload draft durability. Do not advertise a persistent draft journal.
  Save Copy recovers document content, not automatic association of an externally
  moved task's database identity. Filesystem conflict protection remains optimistic.
- Verified: 40 vault tests on Windows and 42 in an actual Linux container, 2 native
  desktop tests, 5 Canvas and 4 search-request units, formatting and Clippy.
  Eleven native desktop recovery scenarios pass, including a delayed local copy
  request with later typing, a delayed failing save, read-only attributes, real
  close/relaunch, explicit discard, and a GPU board copy/save/reload. Existing 7
  relocation, 11 search, 17 editor/navigation, 7 attachment and 10 GPU/fallback UI
  checks pass. Only synthetic data; prior vault restored, no QA process remains.
  `node tests/desktop/draft-recovery-smoke.mjs` reproduces this on WebView2 debug
  port 9222; the runner relaunches the owned test app for the real-close check.
  Report: `target/qa/draft-recovery.json`. Optional `BRANDON_QA_SCREENSHOT` captures
  synthetic wide/900px views. The banner is visually checked and bounded in both
  viewport sizes; this is not native macOS or physical-window-resize verification.
- The Windows EXE and NSIS are rebuilt. Source **9d10ec0baed253fd7c4453288184ec8b5cb05d52**
  passed complete local `scripts/release-check.ps1` with exit 0 and explicit
  `Release gate PASS`, including 115 web tests, both restore drills and all
  **18 browser E2E**. No policy/advisory exceptions were loosened. This does not
  replace a future GitHub run on unpublished source. Installation/macOS remain
  untested; changes remain local, without push.
  EXE SHA-256: `2E602FB755465ABB2FE02666F9D70113B8FC52CA392E3B197FC70EE1318DED0D`;
  NSIS SHA-256: `AEF615B908716348EB586EDB29D9265884310D7166E7DF16794ABA45E7558078`.

#### Explicit search-cache recovery checkpoint — 18 October, afternoon

The corrupt-cache gap reproduced in `target/qa/search-cache-recovery-probe.mjs`
is now implemented and regression-tested, not merely a diagnostic:

- Search errors offer a compact **Restore index** action with a clear scope.
  Recovery is explicit, never automatic on an ordinary failed query. The existing
  search semaphore serializes rebuild and queries; a root check rejects requests
  targeting another/stale vault. Pending typing and type/mode filters survive;
  completion reruns the latest query rather than displaying stale captured hits.
- Only the derivative `search-v1.sqlite3` and its three known SQLite companions
  move into a newly/exclusively created `.brandon/search-recovery-<random>` folder.
  Old bytes are retained, without deleting unknown files or pruning older copies.
  Notes, Canvas, media and canonical `tasks.sqlite3` are not reset or rewritten.
  The backup path is visible on success and on a partial-move/rebuild error.
- Private-directory and all DB/companion paths reject symlinks/reparse points and
  non-files; inaccessible metadata fails closed. Canonical task data is checked
  read-only before any cache quarantine. A corrupt task DB gets its own error and
  no cache reset: search does not pretend to repair or silently ignore task data.
- Exact derivative backup paths are ignored by the watcher, not the whole private
  directory or canonical tasks. Windows parent-directory metadata notifications
  can cause one final refresh; the settled UI is checked for no recurring events
  or query loop. Boundary checks no longer try to create an already-present cache
  folder. This is not a multi-process/hostile concurrent filesystem transaction;
  a failed move may leave some old files in place and reports that honestly.
- **44 vault tests on Windows**, **46 on actual Linux**, 2 native desktop unit
  tests, 5 Canvas and 4 search-request unit tests pass. New recovery coverage:
  corrupt main DB and all companion bytes, repeated unique backups, canonical
  task corruption, malformed/empty query, non-file cache; Windows locked-sidecar
  partial failure and Linux symlinked private/companion paths.
- **7 new live WebView2/native-IPC recovery checks** pass, including saved-file
  and canonical-task hashes, Literal/type preservation, reload, delayed native
  recovery with later typing/filter changes, no settled watcher loop, stale root,
  and honest refusal for a corrupt task DB. The previous **63 live UI checks**
  also pass: 11 draft, 11 search, 7 relocation, 17 editor/navigation, 7 attachments,
  10 GPU/fallback including real 10k/20k boards. All data was synthetic; the
  original vault was restored and clean QA window verified actually closed.
  Reports: `target/qa/search-recovery.json` and the previous stage reports.
  `target/qa/search-recovery-before.png` / `search-recovery.png` were visually
  checked at 1360×840; compact-window/macOS recovery is not separately tested.
- fmt, strict clippy, desktop lint/typecheck pass. Windows EXE and NSIS rebuilt,
  not installed. Source **4c8b28a0dcdfb9144e021faadd2ad2b734a8e6f5** passed the
  complete local `scripts/release-check.ps1`: exit 0, explicit `Release gate PASS`,
  115 web tests, both restore drills and all 18 browser E2E. No policy/advisory
  exceptions were loosened. This does not substitute for GitHub CI on unpublished
  source. Changes remain local, without push.
  EXE SHA-256: `7EB48D2D6E21DC44AAF492EFBBC13034313B982F85A5797640BA2B8AE2E8F4DE`;
  NSIS SHA-256: `F57160B0D0F0200013FB88DFF1D8AE7D4AE4DF9229732DB961B3BAF9E06E7383`.

#### Search candidate/ranking checkpoint — 18 October, afternoon

- The ignored `target/qa/search-ranking-probe.rs` first reproduced 10 synthetic
  matching task notes, one with 100 matching sections: Literal `type:task`, limit
  20 returned **1**, not 10. The promoted regression in
  `crates/knowledge-vault/tests/search_ranking.rs` was observed failing before
  implementation. No source changed during the preceding recovery release gate.
- Task duplicates now collapse **before** the SQL candidate LIMIT. A materialized
  matching-ID/rank/anchor set evaluates FTS5 BM25 in its required row context;
  a window selects the best task chunk with stable tie breaks. Only winners join
  back to full text. Large body strings are not copied into the materialized set.
  Weights, word/literal/filter semantics and cache format are unchanged. Notes
  retain individual matching sections; Canvas retains individual node anchors.
- **46 Windows / 48 actual Linux vault tests** pass. Two new integration tests
  check both modes, small limits, mixed queries, unchanged canonical-task bytes,
  best BM25 task section and distinct note/Canvas anchors. Existing 20k-Canvas
  regression also checks a query matching all 20k nodes returns 80 distinct IDs.
- **5 new live native-IPC/UI ranking checks** pass, including the formerly hidden
  task's actual selected source line, unchanged task database and a broad 20k-card
  query. All previous **70 live UI checks** were rerun on the new executable and
  pass, including real GPU/fallback, editor, drafts, relocation and attachments.
  Original vault restored; clean QA window verified actually closed.
- `target/qa/search-ranking.json` records the synthetic fixture and diagnostic
  native-backend elapsed times. Five warm release-build queries matching 20k
  cards took **35.2–37.7 ms** in the final local run. This is not an old/new
  comparative benchmark, cold-index time, UI latency/frame time, or a 50k-file
  measurement. The active query had already indexed the added board; the report
  records `firstChangedSources: 0` instead of mislabeling it as a cold run.
- Strict fmt/clippy and the above tests pass. Windows EXE and NSIS rebuilt, not
  installed; no push. Source **0bfc6bd0f1318420e787d783ae28c94bbea94949** passed
  its own complete local `scripts/release-check.ps1`: exit 0, explicit
  `Release gate PASS`, 115 web tests, both restore drills and all 18 browser E2E.
  No policy/advisory exceptions were loosened. This is local validation, not a
  claim of GitHub CI or installation/native macOS validation.
  EXE SHA-256: `1B1D7C627B13893D5CACB8666EB8F11E726604ED8CAD713EC3DC6E20C6478E54`;
  NSIS SHA-256: `07CE5DA09A09C6569459EB74A8AB21524CBA9BB67B808C5CF33D011373CA8CC7`.

#### 50k-source measurement checkpoint — 18 October, 17:58–18:00 MSK

Completed **one local backend diagnostic**, not a UI scale guarantee or comparative
benchmark: Windows, AMD Ryzen 7 7700 (8C/16T), release-built vault library at the
ranking source above, 50,000 synthetic Markdown files in 10 folders. No other QA
window or release gate was running during measurement. Files were just generated,
so this is an empty-index first build, not physically cold-disk performance.

- Fixture creation: **7,395.9 ms**, outside search time.
- First index plus broad query: **10,377.5 ms**; 50,000 content-indexed sources,
  zero failed/skipped, 80 returned hits. SQLite cache: **29,937,664 bytes**.
- Three warm broad queries, unchanged sources: **845.1 / 856.0 / 843.3 ms**.
  Warm unique Literal query: **760.6 ms**, exact final-file match.
- Adding source 50,001 gives the explicit documented 50,000-object error rather
  than silently dropping entries. No limit was raised or fallback made partial.
- Separate read-only traversal/sort proxy over the 50k fixture files measured
  **731.3 / 671.1 / 674.8 ms**. This is not instrumentation of the complete
  production scan/reconciliation path, but corroborates the visible full metadata
  walk as a scaling concern. Do not claim an exact percentage or constant-time
  warm search. Future incremental indexing must retain path/read-time safety.
- Local reproducibility artifacts (ignored QA files, not CI fixtures):
  `target/qa/search-scale-probe.rs`, `search-scale-probe.exe`,
  `search-scale-20261018-145757.log`, `search-scale-metadata-probe.rs` / `.exe`.
  The main probe was compiled with rustc 1.98.0, opt-level 3, against the most
  recently built release `knowledge_vault` rlib and its release dependencies.
  Synthetic fixture retained at
  `C:/Users/ADMIN/AppData/Local/Temp/brandon-search-scale-1792335477980562200`;
  it now has the one overflow fixture too. Nothing from the user's vault was read
  or changed for this diagnostic. No user-data deletion/cleanup was performed.

Next bounded continuation: with the remaining window before **18:20 MSK**, inspect
the final evidence/Telegram queue and prepare a small incremental-index follow-up
plan; do not start an engine rewrite or another feature/release cycle. A future
incremental prototype must compare the same 50k fixture and verify external edits,
renames/removals, lost watcher events, vault changes and private/symlink boundaries
before skipping full reconciliation. Backend timings do not validate a 50k-row
file-tree UI. Do not repeat completed draft/relocation/recovery/ranking stages or
the expensive full gates when source is unchanged. A persistent draft journal,
PDF/media extraction, vector search, embeddings and agent grants remain
unimplemented in this slice. Recovery backups intentionally accumulate until an
explicit future maintenance policy; they are never silently discarded. Existing
autonomy deadline and no-push restriction remain unchanged.

Incremental-index design recorded before implementation (historical checklist):

- Current seam: `search.rs::search_local` performs scan/reconciliation before
  every query. `main.rs::start_watcher` emits only a path array and drops watcher
  errors; `app.tsx` ignores those paths and schedules a full `refreshVault(true)`.
  Merely caching the React `VaultIndex` or trusting that event as proof of complete
  freshness would not fix these costs or handle missed changes.
- Split querying the last committed index from a root/generation-scoped dirty
  queue. Preserve create/write/remove/rename and directory-change semantics; mark
  watcher errors, overflow/unknown events and missing rename pairs as requiring
  reconciliation. Coalesce work, not permissions. Retain explicit refresh plus a
  bounded background/full reconciliation strategy, and describe possible index
  staleness honestly instead of promising that watchers never miss an event.
- Keep one coordinated index writer and exclusive recovery. Bound work in
  cancellable batches so initial/full sync does not indefinitely block newer
  queries or a vault change; publish coherent committed batches and visible
  progress/coverage. Canonical task DB remains read-only to the search service.
- Cache invalidation is not read authorization: revalidate each returned hit's
  current public path, actual readability and source revision. A removed,
  private/symlinked or newly unreadable source must not disclose its old cached
  snippet while waiting for a background sweep. Never treat SQLite cache contents
  or a watcher payload as future agent grants.
- Avoid a self-invalidating loop on derivative SQLite/quarantine writes. Preserve
  canonical task events; do not ignore the whole `.brandon` folder just to hide
  Windows parent-metadata notifications. Define the handling of ambiguous parent
  events explicitly and exercise it in the existing no-refresh-loop fixture.
- Acceptance before promotion: same named 50k fixture, separately measured query,
  dirty-batch and full-reconcile costs; Words/Literal/filter/ranking parity; edits,
  moves/removals, task date changes, intentionally lost watcher events, permission
  revocation, private/reparse paths, recovery concurrency and rapid vault changes.
  Test small/large vaults and report cold index versus warm filesystem state.
  File-tree virtualization/50k-row UI responsiveness is a separate validation
  surface, not something backend query measurements can establish.

Incremental-index implementation checkpoint — 18 October 2026, user-returned
interactive work (the temporary automation remains deleted):

- Desktop queries now use `Vault::search_cached`, not `search_local`'s full-vault
  scan. One native coordinator serializes derivative writers and explicit cache
  recovery. Its root epoch cancels abandoned work; the UI passes its expected
  root, includes it in the result key, and discards abandoned responses. A failed
  new watcher setup leaves the previous vault/worker selected.
- File notifications coalesce into a dirty queue. Creates/edits/file renames and
  removals are scoped; folder changes are expanded in background inventory,
  rather than recursively reading a large subtree inside a query. Inventory
  feeds 128-path transactions, yielding the writer to queued queries between
  batches. Task-metadata deltas drain in bounded 128-task slices and keep their
  remainder queued; they do not falsely mark the whole task fingerprint current.
- Unknown events, incomplete rename pairs, overflow/rescan signals and watcher
  errors request reconciliation. A new sweep starts 60 seconds after the last
  completed sweep; the explicit **Сверить** action also requests one. Sweeps hash
  bounded source contents, detecting same-length edits with restored mtimes.
  The UI shows syncing/pending coverage and the last completed reconciliation.
  Initial results can be partial, explicitly labelled while batches commit.
- Each returned source is rechecked against the current public path boundary,
  real open/read permission, metadata/task stamp and bounded content SHA-256
  before its cached snippet is released. Removed, private, reparse/symlinked,
  changed or unreadable hits are withheld, then targeted for refresh. A Canvas
  with many hits is verified once per query, not once per card. Search reads the
  canonical task DB without migrating/writing it. These optimistic filesystem
  checks are not an OS lock against an adversary concurrently replacing files.
- Real Windows testing exposed two feedback causes: journal removal is
  `Remove(Any)`, and `.brandon` directory timestamps arrive as `Modify(Any)`.
  Exact derivative paths are filtered before unknown-kind handling. Only the
  private parent's metadata/ModifyAny notices are ignored, not the whole private
  subtree; task-file events remain meaningful and every query checks task DB
  metadata independently. Rescan flags still request a sweep. No permission or
  unknown-license policy was loosened.

Verified on the final local implementation before its full release gate:

- 54 Windows vault tests, 9 desktop native tests, strict Clippy/formatting,
  desktop lint/typecheck, 5 Canvas and 4 search-request unit tests pass.
- 57 Linux vault tests pass in the existing pinned test image, running the whole
  suite as uid/gid 65534 (not root), with read-only source bind, no capabilities
  and no-new-privileges. Permission revocation actually prevents `File::open`;
  this is not a root test that silently bypasses Unix mode bits.
- 82 real standalone desktop UI checks pass: previous 75 plus 7 incremental
  scenarios in `tests/desktop/search-incremental-smoke.mjs`. They cover 1,500
  notes committing/settling, zero-scanned warm queries, external edits, a whole
  folder move, immediate deleted-hit suppression, manual reconciliation/private
  exclusion and expected-root isolation. Existing recovery/ranking suites also
  pass, including typing/filter changes during recovery, canonical-task failure,
  exact anchors and a 20k-match board. Unit tests explicitly simulate changes
  without watcher delivery, restored mtimes, cancellation rollback, stale epochs,
  unknown/overflow events and multi-batch task updates. The tests do not claim
  that an actual OS watcher can be forced never to lose an event.
- EXE and NSIS were built successfully, not installed. EXE SHA-256:
  `80DBBE67AD9EFCC6907F28B0938F4D0491ADE7E7749C45AD4D0B47D326C673A5`;
  NSIS: `4892F4413A295B322359F87D043925727D37B791FE7227B0DD3EF85DD8D9E148`.
  Native macOS and new remote CI are not verified; there is no push in this turn.

50k comparison on the same retained synthetic fixture above, release libraries,
Windows/Ryzen 7 7700, warm OS filesystem cache, no concurrent build/test workload
in the final comparison. Old baseline is the previously built `0bfc6bd` library;
new queries include read-time source verification. Two alternating old/new rounds,
three samples per query per round (six each), not a statistical benchmark suite:

| Backend operation | Old median | Incremental median |
| --- | ---: | ---: |
| Words `scalecommon`, 80 hits | 902.337 ms | 112.091 ms |
| Literal `marker-49999`, one hit | 820.557 ms | 15.630 ms |

Every new warm query reports `scannedSources=0`. A separate single-edit diagnostic
scanned/changed exactly one source in 7.303 ms. Full reconciliation is not free:
the earlier diagnostic inventory took 1,026.415 ms, followed by 12,696.113 ms of
library batches (largest batch 64.536 ms), while other validation work was also
active. These are not cold-disk timings or native/UI frame times. The background
coordinator no longer adds a fixed 20 ms delay to each busy batch. Its inventory
still holds the coordinator permit during that bounded metadata walk, so a query
can briefly wait for it; removing that remaining contention is a measured future
optimization, not a claimed result here. Missed new matches may wait for the
next sweep **and its completion**, not a hard one-minute freshness guarantee.
Stale matched snippets are denied at read time instead of waiting for the sweep.

Retained local evidence (ignored, not published):
`target/qa/search-incremental-50k-warm.log`,
`target/qa/search-incremental-50k.log`, benchmark Rust sources/executables under
`target/qa`, and Windows/Linux test logs `incremental-rust-*.log`. The old fixture's
explicit synthetic `overflow.md` was moved, not deleted, to its private
`.brandon/retained-overflow.md` so the comparison again contains exactly 50,000
public notes. The modified single-note probe restored the original bytes and
reindexed them. No personal file was used as benchmark input.

Full local release gate for implementation commit
`d3de6492f73491d25c4a6264ca5364aaa13caa1e` finished with exit 0 and
**Release gate PASS** (`target/qa/incremental-release-gate.log`). It includes the
workspace Rust suite, strict formatting/Clippy, 115 web tests, dependency/license
and secret policies, fresh Compose acceptance, both PowerShell/Bash restore
drills and all 18 production browser E2E. Generated-file and staged diff checks
finished clean. This result verifies the new local code, not an older remote CI
run. The subsequent checkpoint changes documentation only; executable hashes
above remain unchanged. Next steps remain targeted performance profiling/queue
refinements, not vector/MCP/OCR or an expansion of agent grants.

### 4. Brandon tools and external Codex / Claude connections

Expose the resource/search service through a local MCP adapter. Prefer stdio for
local clients; any optional loopback transport needs pairing/authentication, exact
origin/host checks and no public listener by default.

Initial read tools, with paginated/bounded results:

- `search` (text / vector / hybrid plus scope and filters);
- `read_resource` (permitted file/range/anchor and source revision);
- `read_canvas` (nodes, edges, labels and permitted reference metadata);
- `get_selection` (explicitly shared current board/note selection);
- `read_attachment` (permitted metadata, safe extraction or bounded preview).

Use structured board data for normal reasoning; a permitted screenshot is an
optional visual aid, not the agent's only way to understand cards and edges.
Search inside granted files and web research are separate capabilities.

External Codex and Claude Code support MCP, so both can consume the same tools.
Their own filesystem or shell tools are outside Brandon's enforcement boundary:
an MCP permission cannot sandbox an independently running agent. Make this clear
in setup; provide a restricted application-owned session for stronger isolation.
[Codex MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli),
[Claude Code MCP](https://code.claude.com/docs/en/mcp).

Gate: both clients can connect, search permitted content, inspect a board and cite
exact objects. Denied/out-of-scope requests fail closed, including indirect links,
and disconnect/revocation leaves the vault unchanged.

### 5. AI panel inside Brandon

- A restrained right panel with provider/session controls, an explicit shared
  context list, tool activity, sources and cancellation; no full-vault upload.
- Send selected nodes, a board region, permitted files or search hits as bounded
  context. Source links open the exact node/block/page/time position.
- Codex adapter: investigate the supported app-server protocol for streaming,
  sessions, login and approvals, rather than scraping or controlling the Codex UI.
- Claude adapter: supported API/Agent SDK authentication. External Claude Code
  plus MCP remains a separate route using that client's own login. Do not promise
  third-party reuse of claude.ai subscription login without provider approval.
- A common provider/session adapter, OS credential storage, explicit disconnect,
  rate/cost limits, retry/backoff, context limits and visible errors. No keys or
  OAuth tokens in Canvas files, logs or WebView localStorage.
- In restricted sessions disable native shell/unrestricted file tools, validate
  arguments in Rust and apply resource grants independently of model compliance.
  Do not auto-inherit arbitrary project instructions or plugins as permissions.
- Optional web research returns sources and retrieval dates. Queries must not
  disclose private file contents by accident; separate public web results from
  local evidence. Treat files, pages and tool results as untrusted content, not
  permission to reveal data, run commands or modify the vault.

Codex app-server documents session/event and approval integration. Claude's Agent
SDK documentation explicitly distinguishes third-party authentication from an
external Claude Code user's own client.
[Codex app-server](https://learn.chatgpt.com/docs/app-server),
[Claude Agent SDK](https://code.claude.com/docs/en/agent-sdk/overview).

Gate: an agent answers from selected files/board objects with clickable evidence,
can conduct separately authorized web research, respects cancel/revocation and
recovers from provider logout/rate limit without losing the user's work.

### 6. Optional agent mutations, after the read path is trustworthy

Writes remain disabled by default. When users grant them, use typed operations to
propose a board arrangement, edit a note, create a task, attach a file or connect
nodes. Show the affected objects and a diff/preview; honor the user's per-session
policy rather than assuming read permission includes modification.

Apply approved operations through the existing atomic, conflict-checked service,
checking source revisions and current grants again. Record undo as a coherent
batch and keep an activity log. User edits made during an agent turn are never
overwritten silently. Delete/import/network/command permissions stay independent
of a generic ability to edit a card.

### Delivery order and first complete scenario

Order: **GPU promotion + bug bash → resources/grants → attachments/media → local
text search → local vector/hybrid → external MCP → internal AI panel → optional
mutations**. The service contracts are designed together; each release is usable
before the next layer exists.

End-to-end acceptance scenario: drop a PDF, an image and a video onto a normal
board; link a note and a task; find the relevant note/PDF passage by words and by
paraphrase; grant an agent only those objects; get a sourced answer and optional
web research; explicitly allow a proposed board change, apply it, undo it, and
reopen the portable vault without losing files or task state.

Open engineering decisions (not missing product approval): benchmark the local
embedding model/runtime and vector backend; inventory preview/codec support on
Windows and macOS; confirm provider integration/authentication against the exact
versions used when implementation begins.
