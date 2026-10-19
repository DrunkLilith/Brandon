# R2B: pinned plugin compatibility audit

Audit date: 19 October 2026. This is a bounded compatibility implementation,
not an Obsidian plugin host. No upstream `main.js`, install script, npm package,
or executable is loaded. The implementations use Brandon commands, Rust I/O,
and the shared metadata/query pipeline. No new dependency is required.

| Upstream | Exact audit revision / manifest | License verified at revision | Selected contract | Status |
| --- | --- | --- | --- | --- |
| [QuickAdd](https://github.com/chhoumann/quickadd/tree/49fbd508be29a5f5a02d60d1672977fd3fcc66e5) | `49fbd508be29a5f5a02d60d1672977fd3fcc66e5`, manifest **2.32.1** | [MIT](https://github.com/chhoumann/quickadd/blob/49fbd508be29a5f5a02d60d1672977fd3fcc66e5/LICENSE), copyright 2021 Christian Bager Bach Houmann | Read selected Capture choice JSON; format/value/date, explicit preview and CAS append/create; command contribution | Audit complete; capture implementation/acceptance remains pending |
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

## QuickAdd contract to implement next

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

## Verification status

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

Final EXE SHA-256:
`edb03af4f043f673348fe06cb2d43f54b126bf79fa7e6bffb0dcaacffcdead9f`.
The original vault/Alpha was restored and normal startup visually checked;
the QA listener on port 9222 is closed. Installer generation/installation is
not claimed for this intermediate slice.
