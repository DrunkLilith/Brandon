# R2B: pinned plugin compatibility audit

Audit date: 19 October 2026. This is a bounded compatibility implementation,
not an Obsidian plugin host. No upstream `main.js`, install script, npm package,
or executable is loaded. The implementations use Brandon commands, Rust I/O,
and the shared metadata/query pipeline. No new dependency is required.

| Upstream | Exact audit revision / manifest | License verified at revision | Selected contract | Status |
| --- | --- | --- | --- | --- |
| [QuickAdd](https://github.com/chhoumann/quickadd/tree/49fbd508be29a5f5a02d60d1672977fd3fcc66e5) | `49fbd508be29a5f5a02d60d1672977fd3fcc66e5`, manifest **2.32.1** | [MIT](https://github.com/chhoumann/quickadd/blob/49fbd508be29a5f5a02d60d1672977fd3fcc66e5/LICENSE), copyright 2021 Christian Bager Bach Houmann | Read selected Capture choice JSON; format/value/date, explicit preview and CAS append/create; command contribution | Independently implemented and native-tested for the exact subset below |
| [Dataview](https://github.com/blacksmithgu/obsidian-dataview/tree/5ad0994ff384cbb797de382e7edff2388141b73a) | `5ad0994ff384cbb797de382e7edff2388141b73a`, manifest **0.5.68** | [MIT](https://github.com/blacksmithgu/obsidian-dataview/blob/5ad0994ff384cbb797de382e7edff2388141b73a/LICENSE.txt), copyright 2021 Michael Brenan | Fenced DQL TABLE/LIST/read-only TASK with FROM/WHERE/SORT/LIMIT over metadata | Independently implemented and native-tested for the exact subset below |

These are the revisions inspected, not a promise of compatibility with future
releases. Repository HEAD was resolved with read-only Git remote lookup; the
manifest, license and relevant source were then read at that exact SHA. One
QuickAdd lookup failed with an empty HTTP response; an HTTP/1.1 retry resolved
the revision. No credentials or private vault data were sent to these sources.

## Reuse and attribution

No upstream source file is copied into the product. This is a semantic adapter
and independently written bounded parser, not a relabelled full plugin. The
upstream copyright and license links above record the origins of the supported
contracts. Any later actual vendoring must retain the full applicable MIT
notice and audit its transitive dependencies separately; this audit does not
grant blanket approval to all plugins mentioned in the master plan.

## Dataview contract

References inspected at the pinned revision:

- [DQL structure and command order](https://github.com/blacksmithgu/obsidian-dataview/blob/5ad0994ff384cbb797de382e7edff2388141b73a/docs/docs/queries/structure.md).
- [Query parser](https://github.com/blacksmithgu/obsidian-dataview/blob/5ad0994ff384cbb797de382e7edff2388141b73a/src/query/parse.ts) for the selected query vocabulary (not bundled).
- [Value semantics](https://github.com/blacksmithgu/obsidian-dataview/blob/5ad0994ff384cbb797de382e7edff2388141b73a/src/data-model/value.ts): empty containers are false, null sorts before non-null, scalar types remain distinct. These differ from Base semantics and have explicit adapters on the shared interpreter.

Supported, line-oriented subset:

```dataview
TABLE status AS "State", rank
FROM "Projects"
WHERE status = "open" AND rank >= 10
SORT rank DESC
LIMIT 20
```

- TABLE, optional WITHOUT ID, field expressions, double-quoted AS labels;
  LIST with an optional value; TASK with source-note links and disabled checkboxes.
- FROM zero or one **single** quoted folder or tag, immediately after the header;
  nested folder/tag membership uses the same metadata as Bases and the graph.
- WHERE/SORT/LIMIT repeat **in written order**, rather than being reordered into
  a SQL-like execution plan. LIMIT is 0–50,000; ASC/DESC/ASCENDING/DESCENDING.
- Raw top-level YAML fields (including hyphenated names), file.name/path/folder/ext/tags;
  file.link is supported as a projected link only, not a comparable DQL Link object.
  Source TASK fields: completed/text/due/scheduled/priority. TASK does **not**
  silently substitute Brandon's SQLite canonical task database for Markdown tasks.
- String/number/bool/null scalars, `=`, `==`, `!=`, `<>`, ordered comparisons,
  AND/OR/NOT and symbolic equivalents, bounded parentheses. Null/scalar sorting
  and empty-container truthiness are tested. Complex list/object comparisons
  fail explicitly; displayed lists/objects are escaped text, not executable HTML.
- Query length 16 KiB, 64 nonempty lines, at most 32 fields/sort keys; individual
  expressions share 4,096-character/256-token/32-depth limits. Source notes and
  expanded source rows each capped at 50,000. Results are virtualized at 44px/row.

Not supported: DataviewJS, inline expressions, inline `field:: value` metadata,
date/duration/link objects and functions, `this`, object traversal, GROUP/FLATTEN,
compound FROM, async/plugin APIs, custom rendering, modifying source checkboxes.
YAML dates remain Brandon raw strings, not Dataview DateTime values; file.mtime
is therefore not exposed as a misleading numeric replacement for a DQL date.
Unknown syntax fails closed even on an empty source, and the original fenced
block remains in the Markdown file. The UI shows the limitation, not an empty
success or a silently unfiltered query. No read query triggers a vault rebuild.

## QuickAdd capture contract

Pinned [Capture choice structure](https://github.com/chhoumann/quickadd/blob/49fbd508be29a5f5a02d60d1672977fd3fcc66e5/src/types/choices/ICaptureChoice.ts),
[choice metadata](https://github.com/chhoumann/quickadd/blob/49fbd508be29a5f5a02d60d1672977fd3fcc66e5/src/types/choices/IChoice.ts),
[settings container](https://github.com/chhoumann/quickadd/blob/49fbd508be29a5f5a02d60d1672977fd3fcc66e5/src/settings.ts),
and [Capture documentation](https://github.com/chhoumann/quickadd/blob/49fbd508be29a5f5a02d60d1672977fd3fcc66e5/docs/src/content/docs/docs/Choices/CaptureChoice.md)
were inspected. Import must select **only** supported declarative choices;
unsupported enabled fields must be reported, not ignored. Preserve the original
JSON and unknown data; never overwrite `.obsidian/plugins/quickadd/data.json`.

Do not import providers/keys/global scripts or run startup macros. Active-file,
selection/clipboard, property/section/Canvas capture, Templater, user scripts,
URI callbacks and arbitrary plugin commands require separate supported
contracts. The first capture writer must preview the exact target and diff,
reject traversal/private paths, protect current dirty buffers and disk revision,
and let the user cancel before any file is created.

### Brandon capture registry and write protocol (before first writer)

`.brandon/captures.v1.json` is a Brandon-only registry: `format:
"brandon.captures"`, `version: 1`, `choices: [{id, sourceVersion, raw}]`.
`raw` retains the selected QuickAdd choice JSON verbatim, including inert
unknown options; the full settings container/providers/keys are not copied.
Unknown registry/entry extension fields survive updates; unknown format versions
are read-only errors. No importer writes the original QuickAdd file. The new
registry joins the backup private allowlist and remains usable after restore.

The first supported Capture contract includes a fixed/date/value Markdown
target, create-if-missing (without a template), enabled format or default VALUE,
anonymous/named VALUE form inputs, DATE with YYYY/MM/DD/HH/mm/ss and simple
separators, task prefix, top-of-body/bottom placement and optional openFile.
QuickAdd's historically named `prepend: true` means **bottom**, verified in its
pinned formatter; false means body start after frontmatter. Existing BOM,
frontmatter/comments and body bytes stay intact; newly inserted text follows
the note's newline convention. User VALUE text is substituted once, never
reinterpreted as template/script syntax. Path VALUEs cannot introduce separators
or control characters. Only bounded portable `.md` paths are accepted.

Import inspects every candidate and lists refusals; only a selected supported
Capture is added, under registry fingerprint CAS (no startup execution). Options
that require unsupported effects are refused, including active file/selection,
clipboard/link insertion, property/section/Canvas writes, eachLine, Templater,
date aliases/day picking, scripts and macros. Disabled stock option structures
are retained but inactive; unfamiliar active options are never silently ignored.

Preview is read-only and returns exact before/after text, target, position,
frozen local datetime, registry/disk fingerprints and a plan hash. Apply
recomputes that plan in Rust and requires the same hash, checks root again and
checks open-buffer/durable-draft guards in the app. New files publish with
no-clobber; existing files use revision check plus atomic replacement. This is
not a cross-process filesystem transaction: external writes are checked at the
commit boundary, with the same atomic-replace limitation as normal note saves.
Reapplying a consumed plan cannot duplicate an entry. No partial index/full
vault rebuild is used; capture emits a normal metadata path touch.

## Historical DQL checkpoint (commit `6cb7367`)

Pure DQL adapter suite: **10 cases**, all passed (query structure/order, scalar
semantics, source task identity, nested tag membership, aliases, unsupported
syntax, prototype/JS rejection, bounded size). Total current frontend suites:
**60 cases**. Typecheck/lint and production native build passed.

Real Windows/Tauri DQL suite: **7 cases**, all passed: TABLE, LIST/TASK, explicit
unsupported/JS refusal, navigation and single-edit watcher update, 450-row
virtualized results, source block anchor, split-pane rendering/navigation.
450-row table body is exactly 19,800px (44px each), bounded DOM, last row reachable.
One external YAML edit reparsed one source and triggered zero full index/refresh
calls. Dashboard bytes and source checkboxes remain unchanged.

Regression suites rerun: properties/Bases 10, task views/backup 14, R1A 25,
cards/images 6. Rust is unchanged from the preceding task checkpoint's 113
tests. Logs: `target/qa/r2b-dql-{frontend,build,ui}.log` and
`r2b-dql-*-regression.log`. Native visual inspection corrected shared Markdown
row geometry and default button colors. This document does not close R2B or
the master plan; QuickAdd capture remains the next required implementation.

DQL checkpoint EXE SHA-256 (superseded by the capture package below):
`edb03af4f043f673348fe06cb2d43f54b126bf79fa7e6bffb0dcaacffcdead9f`.
The original vault/Alpha was restored and normal startup visually checked;
the QA listener on port 9222 is closed. Installer generation/installation is
not claimed for this intermediate slice.

## Closing R2B checkpoint: captures, 19 October 2026

Implemented the bounded capture contract above on top of `6cb7367` with no
upstream runtime or new dependency. The selected raw choice survives import;
provider settings are not copied. The native JSON picker and paste form both
lead to an explicit before/after preview, then a separate confirmed write.
Commands survive restart and backup/clean-folder restore. The pre-edit source
is checkpointed in local version history without replacing a crash draft.
Unsaved main/side buffers and durable drafts, including case aliases, block
capture writes to their target. Source/registry changes invalidate previews.
Capture input remains available on failure; the UI never claims an unknown
write outcome as a success. New files publish no-clobber and trigger only
normal per-path metadata updates. YAML/BOM/comments/newline convention remain
intact, including a YAML-only file without a final newline.

Validation on the final packaged Windows executable:

- **132 Rust tests**, strict Clippy, formatting; **61 frontend contracts**,
  typecheck/lint and production build, all passed. Capture tests include bounds,
  unsupported options/scripts, verbatim raw JSON, unknown registry extensions,
  stale plans, concurrent apply, path/junction protections, backup/restore,
  newline/YAML edge cases, root scope and independent draft/history retention.
- **106 native Tauri UI checks**: captures 11, DQL 7, properties/Bases 10,
  tasks/backup 14, task-scale 3, incremental metadata 11, R1A 25, rich cards 6,
  R1B process lifecycle/draft recovery 19. These final runs used a temporary
  per-process software WebView due the host issue below. No picker or clipboard
  round-trip is inferred from adapted test-dialog responses.
- Separately performed the actual Windows JSON picker → selected choice import
  → typed capture → complete preview → confirmed write → auto-opened real
  Markdown editor. Fixture `target/qa/r2b-native-capture-a86a3bd7`, source
  `target/qa/r2b-native-capture-input.json`; the original JSON was not modified.
- Logs: `target/qa/r2b-capture-{rust-all,clippy,frontend,package}.log`,
  `target/qa/r2b-final-soft-{capture,dql,properties,tasks,task-scale,r2a,r1a,cards,r1b}.log`.
  Task-scale's first additional startup failed as described below; its successful
  final-executable retry is `r2b-final-soft-task-scale-retry.log` (3,000 tasks,
  121.96ms mutation→projection with polling, 371B response, one task delta,
  zero Markdown reparses/full scans). A prior successful scale repeat is also
  retained; no failed log is relabelled PASS.
  R1B assertions were strengthened to wait for actual search/palette focus and
  the selected command before typing/Enter, rather than weakening checks.

Final artifacts (build, not installer installation):

- `target/release/knowledge-desktop.exe`, SHA-256
  `9716db90777addcf8bb897d79e6fdd2cf7e9d124b24955fccd67676ffc5d937c`.
- `target/release/bundle/nsis/Brandon_0.1.0_x64-setup.exe`, SHA-256
  `deb08b72b1bc3d8f1498c74199bb5ff3b60d11a41ce6a9378a0551878a6fab80`.

### Known host-specific WebView failure

Repeated ordinary hardware-accelerated QA runs also passed the individual
functional suites, but one later stress repeat lost its WebView child and left
a black native window. Crashpad exception/module metadata registers
`0xc0000005` inside injected `RTSSHooks64.dll`, offset `0x1490af`; an earlier
dump also faults in that module. Full dumps/memory are not copied or committed.
RTSS and MSI Afterburner were running. This is evidence of an injected-overlay
conflict, not proof that every possible blank window has the same cause.
An additional software-WebView startup also faulted at the same RTSS module
offset; the next launch and task-scale repeat passed. Thus software mode is a
diagnostic aid here, **not** a demonstrated reliable workaround.

The app was recovered without touching user documents. Only the identified
owned, defunct QA process was terminated. Neither RTSS/Afterburner nor global
GPU/security settings were changed. Final repeat tests explicitly used
`--disable-gpu` for that QA WebView process only; native wgpu Canvas remains a
separate renderer. The production default and installer are unchanged.
[Microsoft documents this flag as a troubleshooting override](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/webview-features-flags),
and [injected security/other software may affect WebView reliability](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/measures).
Process-failure recovery/compatibility diagnostics remain stabilization work;
this checkpoint does not promise universal hardware-accelerated stability.

The bounded functional R2B acceptance is complete. Full QuickAdd/Dataview
parity, JS/macros, a general property-type registry, task recurrence/Timeline,
native macOS, installer installation and the unrelated server release gate
are not claimed. The master-plan goal remains active with R3A next.
