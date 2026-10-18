# Brandon desktop: Obsidian parity plan

Status: active implementation plan, 2026-10-13.

Brandon Desktop is a local-first, open-source knowledge workspace. The target is practical feature and file-format compatibility with Obsidian while keeping Brandon's own name, visual identity, implementation, and AGPL license.

This is a clean-room compatibility effort based on public behavior and documentation. It does not copy Obsidian source code, trademarks, private assets, or proprietary implementation details.

## Product contract

1. **The vault is the product boundary.** A vault is a normal local folder. Markdown, attachments, JSON Canvas files, and configuration remain usable without Brandon.
2. **Offline is the default.** Opening, editing, linking, searching, and navigating a vault never requires an account, Docker, or a server.
3. **The server is additive.** Sync, teams, ACL, audit, encrypted storage, semantic search, and shared AI are optional capabilities layered over a working local vault.
4. **The index is disposable.** Search, backlinks, graph data, embeddings, and metadata caches can always be rebuilt from portable source files.
5. **External edits are first-class.** Changes made by Git, another editor, or a sync tool must be detected without restarting the application.
6. **No silent data loss.** File writes are atomic, conflicting external changes are surfaced, and destructive operations are recoverable by default.
7. **Extensibility is an architecture layer.** Commands, views, file events, editor extensions, settings, themes, and metadata APIs are registries from the beginning, not hard-coded UI branches.

The storage behavior follows Obsidian's documented model: plain Markdown in a local folder, per-vault configuration in `.obsidian`, and a rebuildable metadata cache. Sources: [How Obsidian stores data](https://obsidian.md/help/Files%2Band%2Bfolders/How%2BObsidian%2Bstores%2Bdata), [Vault API](https://docs.obsidian.md/Plugins/Vault).

## Compatibility surface

### Portable files

| Surface | Target |
| --- | --- |
| Markdown | CommonMark plus GitHub-flavored tables, task lists, strikethrough, and autolinks |
| Wiki links | `[[note]]`, aliases, headings, block references, unresolved links, rename updates |
| Embeds | Notes, headings, blocks, images, audio, video, PDF, Canvas, and web links |
| Properties | YAML frontmatter with typed text, list, number, checkbox, date, date-time, and tags |
| Tags | Inline and frontmatter tags, nested tags, tag navigation |
| Canvas | Read/write the open [JSON Canvas](https://jsoncanvas.org/) `.canvas` format |
| Bases | Read/write `.base` files and provide table, list, cards, filtering, sorting, grouping, and formulas |
| Attachments | Preserve arbitrary files and configurable attachment locations |
| Configuration | Store Brandon state separately; import compatible `.obsidian` preferences where safe |

### Workspace and interaction

- File explorer with folders, drag-and-drop, multi-select, rename, move, duplicate, trash, and reveal in Explorer.
- Tabs, tab groups, split panes, pinned tabs, history, saved workspaces, and layout restore.
- Source mode, reading mode, and Live Preview with consistent cursor behavior.
- Command palette, quick switcher, slash commands, configurable hotkeys, ribbon, status bar, context menus, and notices.
- Left and right sidebars containing outline, backlinks, outgoing links, tags, properties, bookmarks, search, and plugin views.
- Hover previews, aliases, heading/block navigation, link autocomplete, and drag-to-create-links.
- Theme variables, CSS snippets, light/dark/system appearance, zoom, readable line length, and custom fonts.

### Core feature matrix

The inventory follows the current official [core plugin list](https://obsidian.md/help/Plugins/Core%2Bplugins).

| Priority | Capability | Acceptance boundary |
| --- | --- | --- |
| P0 | File explorer | Complete safe CRUD, nested folders, attachment visibility, external-change watcher |
| P0 | Editor | Reliable source mode, Live Preview, reading view, undo/redo, selections, Markdown shortcuts |
| P0 | Metadata cache | Parse frontmatter, headings, blocks, tags, links, embeds, aliases; incremental rebuild |
| P0 | Links | Wiki/Markdown links, autocomplete, link following, unresolved links, safe rename updates |
| P0 | Search | Filename and full-text search with path, tag, property, task, line, section, and regex operators |
| P0 | Command palette / quick switcher | Fuzzy ranking, keyboard-only control, command registry, recent files |
| P0 | Workspace | Tabs, splits, navigation history, sidebars, persistence, multiple windows later |
| P1 | Backlinks / outgoing links | Linked and unlinked mentions, context snippets, filters, live refresh |
| P1 | Properties / tags / outline | Editable typed properties and navigable metadata views |
| P1 | Page preview / bookmarks / templates | Hover previews, saved targets/searches, variables and template folders |
| P1 | Daily / unique / random notes | Configurable naming and folders, date templates, command integration |
| P1 | File recovery | Local snapshots, diff, restore, recoverable trash |
| P1 | Graph view | Global/local graphs, filters, groups, link types, physics controls |
| P1 | Note composer | Merge and split without losing links, properties, or attachments |
| P1 | Footnotes / word count | Contextual panes and accurate live status |
| P2 | Canvas | JSON Canvas nodes, edges, groups, pan/zoom, media, note cards, export |
| P2 | Bases | Typed property queries, formulas, views, editing, embedding |
| P2 | Slides / audio recorder / web viewer | Desktop parity and portable outputs |
| P2 | Plugin host | Sandboxed lifecycle, commands, vault/workspace/metadata APIs, settings, views, cleanup |
| P2 | Theme ecosystem | Stable CSS variable contract, snippets, packaging, safe mode |
| P3 | Sync | E2EE/local-key and server-encrypted team modes, conflicts, history, selective sync |
| P3 | Publish | Static/exportable knowledge sites with explicit access policy |
| P3 | AI | Local or remote providers, permission-aware RAG, citations, user-owned indexes |

## Architecture

```text
┌──────────────────────────────── Desktop UI ────────────────────────────────┐
│ workspace · tabs/splits · editor · panels · commands · themes · plugins  │
└─────────────────────────────── Tauri IPC ─────────────────────────────────┘
                                      │
┌──────────────────────────── Rust local runtime ────────────────────────────┐
│ vault filesystem │ atomic writes │ watcher │ history │ plugin permissions │
├──────────────────┼───────────────┼─────────┼─────────┼────────────────────┤
│ Markdown parser  │ metadata cache│ search  │ graph   │ semantic index     │
└──────────────────┴───────────────┴─────────┴─────────┴────────────────────┘
                                      │ optional
┌──────────────────────────── Team/server plane ─────────────────────────────┐
│ sync · ACL · groups · audit · encrypted objects · shared search/AI        │
└────────────────────────────────────────────────────────────────────────────┘
```

The UI talks to a narrow Rust command surface. The renderer never receives unrestricted filesystem access. Each plugin declares capabilities; filesystem and network access are scoped and revocable.

The local runtime is split into reusable Rust crates instead of growing inside `src-tauri/main.rs`:

- `knowledge-vault`: normalized paths, file operations, watchers, atomic writes, trash, conflict detection.
- `knowledge-markdown`: syntax tree, frontmatter, links, blocks, headings, tags, transformations.
- `knowledge-index`: incremental metadata/search/graph index stored outside canonical content.
- `knowledge-plugin-host`: manifests, lifecycle, capabilities, command/view registries, sandbox bridge.
- `knowledge-desktop`: Tauri integration and OS adapters only.

## Delivery sequence

### Milestone 0 — native vertical slice

- [x] Tauri 2 Windows application with a custom desktop shell.
- [x] Native folder picker and remembered local vault.
- [x] Recursive Markdown discovery with symlink and path-escape protection.
- [x] Read, create, edit, and atomic-save Markdown.
- [x] Idle auto-save with explicit disk status and `Ctrl+S` fallback.
- [x] Source, split, and reading modes with GFM rendering.
- [x] Keyboard entry points: search, new note, save.
- [ ] External filesystem watcher and conflict UI.
- [ ] Real hierarchical file tree and full file operations.

### Milestone 1 — daily-driver foundation

Exit criterion: a user can move an existing Markdown vault into Brandon and do a full week of ordinary note work without opening another editor.

1. Extract `knowledge-vault` and add watcher, file identity, trash, rename/move, folders, attachments, and conflict-safe writes.
2. Replace the textarea with CodeMirror 6 and implement editor commands, history, selections, Vim option, and source/reading parity.
3. Build the incremental metadata cache and expose headings, properties, tags, links, embeds, and blocks.
4. Add internal-link navigation/autocomplete, safe rename propagation, backlinks, outgoing links, outline, tags, and properties panes.
5. Add command registry, palette, quick switcher, hotkey settings, context menus, tabs/splits, and persisted layout.
6. Add indexed full-text search and operator grammar.

### Milestone 2 — visual knowledge and structured data

Exit criterion: Canvas, Graph, and Bases round-trip their portable files without data loss and cover the documented core workflows.

### Milestone 3 — extension ecosystem

Exit criterion: Brandon's own SDK is stable and a selected compatibility suite of popular Obsidian plugins can run through the adapter without source changes or with documented minimal changes.

Full arbitrary Obsidian plugin compatibility cannot be promised before testing: desktop-only plugins may depend directly on Electron, Node internals, or undocumented DOM structure. Compatibility is measured plugin-by-plugin with fixtures and an explicit scorecard.

### Milestone 4 — team knowledge system

Exit criterion: the same vault can opt into encrypted synchronization, groups/ACL, version history, semantic search, and permission-aware AI without losing offline operation or Markdown portability.

## Implemented interaction checkpoint — 2026-10-18

The desktop now has real empty tabs (`+` / `Ctrl+T`), closing and reopening
(`Ctrl+W` / `Ctrl+Shift+T`), tab cycling (`Ctrl+Tab` / `Ctrl+Shift+Tab`), and
back/forward history (`Alt+Left` / `Alt+Right`). Navigation waits for pending
writes and flushes text typed during a write; external conflicts keep the
current editor open. The selected tab stays visible in the scrolling tab strip.
Tab groups, pinning, split workspaces and persisted sessions remain future work.

Quick insert formats selected text as inline code, or selects an editable code
placeholder. Visual mode hides code delimiters, including on the active line.
Fenced blocks use an embedded CodeMirror editor with line numbers, language
suggestions, syntax highlighting, indentation, undo/redo and copying. Reading
mode shares the same token colors. Language grammars are bundled and load
locally on demand; unknown languages stay editable as plain text. Both kinds
of code round-trip to portable Markdown, including content with backticks.

`pnpm --dir apps/desktop test:smoke` checks these workflows against a running
release exposed on the local CDP port 9222. It uses a temporary vault and
restores the previous vault and visual-editing preference afterwards.

## Definition of parity

A feature is only marked complete when all of the following are true:

1. The documented user workflow works from keyboard and pointer.
2. Existing portable files round-trip without loss.
3. External edits and failure paths are handled.
4. The UI is usable at 100%, 125%, and 150% Windows scaling.
5. Startup and interaction budgets are measured on a 10,000-note fixture.
6. The behavior has automated tests at the Rust boundary and a manual desktop acceptance script.
7. The last UI change is visually rechecked in the packaged Windows build.

## Performance budgets

Initial budgets, subject to fixture measurements:

- Cold shell visible: under 700 ms on the development machine.
- First useful vault view for 10,000 Markdown files: under 2 seconds with a warm index, under 8 seconds during a rebuild.
- Keystroke-to-paint p95: under 16 ms for ordinary notes.
- Quick switcher results: under 50 ms after the first query.
- External file change visible: under 500 ms.
- No full-vault rescans on individual file saves.

## Immediate next implementation batch

1. Extract the filesystem boundary from the Tauri binary into `knowledge-vault`.
2. Add a native watcher and a disposable metadata index.
3. Render a real folder tree and support create, rename, move, and recoverable trash.
4. Introduce the command registry and quick switcher instead of hard-coded keyboard branches.
5. Move editing to CodeMirror 6 and implement auto-save plus external-change conflict detection.

The approved Miro-class canvas and unified task/calendar direction is specified in [canvas-planner-product.md](canvas-planner-product.md). It extends the parity matrix without weakening the local-first vault boundary: JSON Canvas and Markdown remain canonical, while the task index, calendar projection, graph, and semantic data remain rebuildable.
