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

## Presentation gate (next integration)

One active frame, explicit previous/next/Home/End/Escape controls, plain speaker
notes, camera fit plus clipping to its bounds in native and DOM/overview paths.
No source writes on entry/navigation/exit. Editing commands and task checkboxes
must not mutate sources while showing slides. Exit restores the prior camera.
Source replacement/removal of the active frame exits safely. Presentation is
local and pane-scoped; no OS fullscreen request, share or recording permission.

Tests must cover create/wrap/rename/notes/reorder/remove-only/Undo/reopen,
unknown fields, future versions, external source edits, locks and both renderers.
Clipping and export bounds share the frame rectangle; export still requires its
own bounded Rust I/O, font/media compatibility report and actual image inspection.

## Current status

Writer/schema and pure operations implemented: nine preset groups, wrap selection,
shared move/lock/remove-only semantics, notes/title editing, explicit presentation
sequence with retained missing slots, deterministic append of pasted frames, and
shared camera/screen rectangle calculations. Namespace/schema/budget checks are
wired into the common parser. There is still **no enabled frame UI writer**,
native clipping, template/export acceptance or full R3B completion.

Eleven new frame tests and **130 frontend contracts total** PASS, including the
existing Canvas/ink and editor tests. The first locked-insertion fixture tried
to place a frame larger than its locked container and correctly did not refuse;
the final test uses a truly contained frame rather than changing group semantics.
Logs: `target/qa/r3b-frames-{frontend,lint,typecheck,build}.log`. This foundation
does not change Rust; it is not a new native build or in-app acceptance run.
The final ink visible-window gate is still waiting for the owner to unlock the
session; the code-password screen is not automated or bypassed.
