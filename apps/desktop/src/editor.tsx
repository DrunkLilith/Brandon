import { autocompletion, type CompletionContext, completionKeymap } from '@codemirror/autocomplete'
import { defaultKeymap, history, historyKeymap, indentWithTab, undo, redo, undoDepth, redoDepth, selectAll } from '@codemirror/commands'
import { bracketMatching, HighlightStyle, indentOnInput, syntaxHighlighting, syntaxTree } from '@codemirror/language'
import { markdown } from '@codemirror/lang-markdown'
import { languages } from '@codemirror/language-data'
import { searchKeymap } from '@codemirror/search'
import { Compartment, EditorState, StateField, type Text } from '@codemirror/state'
import { tags } from '@lezer/highlight'
import { Decoration, drawSelection, dropCursor, EditorView, highlightActiveLine, keymap, lineNumbers, ViewPlugin, WidgetType, type DecorationSet, type ViewUpdate } from '@codemirror/view'
import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react'
import { codeHighlighting, codeLanguage, codeLanguages, inlineCodeSource } from './code-language'
import { linkAtPosition } from './note-navigation'
import type { DesktopSettings } from './settings'
import { frontmatterRange } from './properties'

export type EditorAction = 'undo' | 'redo' | 'copy' | 'cut' | 'paste' | 'select-all' | 'bold' | 'italic' | 'strike' | 'inline-code' | 'insert-link'
export type EditorContext = { x: number; y: number; hasSelection: boolean; canFormat: boolean; canUndo: boolean; canRedo: boolean; link: ReturnType<typeof linkAtPosition> }

type MarkdownEditorProps = {
  value: string
  notePaths: string[]
  visual: boolean
  settings: DesktopSettings
  onContext: (context: EditorContext) => void
  onOpenLink: (target: string, kind: 'wiki' | 'markdown') => void
  onChange: (value: string) => void
}

export type MarkdownEditorHandle = {
  position: () => { anchor: number; head: number; top: number; left: number } | undefined
  restorePosition: (position: { anchor: number; head: number; top: number; left: number }) => void
  insertText: (value: string) => void
  insertSnippet: (value: string, cursorOffset?: number) => void
  insertBlock: (value: string, cursorOffset?: number) => void
  insertInlineCode: () => void
  focus: () => void
  revealLine: (line: number) => void
  revealDescription: (fallbackLine: number) => void
  execute: (action: EditorAction) => Promise<void>
  currentLink: () => ReturnType<typeof linkAtPosition>
}

const markdownHighlight = HighlightStyle.define([
  { tag: tags.heading, color: '#dedede', fontWeight: '600' },
  { tag: tags.strong, color: '#e7e7e7', fontWeight: '650' },
  { tag: tags.emphasis, color: '#d0d0d0', fontStyle: 'italic' },
  { tag: [tags.link, tags.url], color: '#a8a2d8', textDecoration: 'none' },
  { tag: tags.monospace, color: '#d7b98e' },
  { tag: [tags.meta, tags.punctuation], color: '#696969' },
  { tag: tags.quote, color: '#aaa' },
])

class CheckboxWidget extends WidgetType {
  constructor(readonly checked: boolean, readonly from: number, readonly to: number) { super() }
  eq(other: CheckboxWidget) { return other.checked === this.checked && other.from === this.from }
  toDOM(view: EditorView) {
    const input = document.createElement('input')
    input.type = 'checkbox'
    input.className = 'cm-task-checkbox'
    input.checked = this.checked
    input.setAttribute('aria-label', this.checked ? 'Выполнено' : 'Не выполнено')
    input.addEventListener('change', () => {
      view.dispatch({ changes: { from: this.from, to: this.to, insert: this.checked ? '- [ ]' : '- [x]' } })
    })
    return input
  }
  ignoreEvent() { return true }
}

class EmptyInlineCodeWidget extends WidgetType {
  constructor(readonly from: number) { super() }
  eq(other: EmptyInlineCodeWidget) { return this.from === other.from }
  toDOM(view: EditorView) {
    const button = document.createElement('button')
    button.type = 'button'
    button.className = 'cm-inline-code-placeholder'
    button.textContent = 'код'
    button.title = 'Нажмите, чтобы ввести код'
    button.addEventListener('mousedown', (event) => event.preventDefault())
    button.addEventListener('click', () => {
      const source = inlineCodeSource('')
      view.dispatch({ changes: { from: this.from, to: this.from + 2, insert: source.value }, selection: { anchor: this.from + source.offset, head: this.from + source.offset + source.length } })
      view.focus()
    })
    return button
  }
  ignoreEvent() { return true }
}

class TableWidget extends WidgetType {
  constructor(readonly rows: string[][], readonly from: number, private to: number) { super() }
  eq(other: TableWidget) { return JSON.stringify(other.rows) === JSON.stringify(this.rows) }
  toDOM(view: EditorView) {
    const wrapper = document.createElement('div')
    wrapper.className = 'cm-live-table'
    wrapper.dataset.tableFrom = String(this.from)

    const commit = (nextRows: string[][], preserveDom: boolean) => {
      const source = markdownTableSource(nextRows)
      const previousTo = this.to
      if (preserveDom) {
        this.rows.splice(0, this.rows.length, ...nextRows.map((row) => [...row]))
      }
      this.to = this.from + source.length
      view.dispatch({ changes: { from: this.from, to: previousTo, insert: source } })
    }

    const toolbar = document.createElement('div')
    toolbar.className = 'cm-live-table-toolbar'
    const actions: Array<{ label: string; title: string; disabled: boolean; rows: () => string[][] }> = [
      {
        label: '+ Строка',
        title: 'Добавить строку',
        disabled: false,
        rows: () => [...this.rows.map((row) => [...row]), Array.from({ length: this.rows[0]?.length ?? 2 }, () => '')],
      },
      {
        label: '− Строка',
        title: 'Удалить последнюю строку',
        disabled: this.rows.length <= 2,
        rows: () => this.rows.slice(0, -1).map((row) => [...row]),
      },
      {
        label: '+ Столбец',
        title: 'Добавить столбец',
        disabled: false,
        rows: () => this.rows.map((row) => [...row, '']),
      },
      {
        label: '− Столбец',
        title: 'Удалить последний столбец',
        disabled: (this.rows[0]?.length ?? 0) <= 2,
        rows: () => this.rows.map((row) => row.slice(0, -1)),
      },
    ]
    actions.forEach((action) => {
      const button = document.createElement('button')
      button.type = 'button'
      button.textContent = action.label
      button.title = action.title
      button.disabled = action.disabled
      button.addEventListener('mousedown', (event) => event.preventDefault())
      button.addEventListener('click', () => commit(action.rows(), false))
      toolbar.appendChild(button)
    })
    wrapper.appendChild(toolbar)

    const table = document.createElement('table')
    this.rows.forEach((row, rowIndex) => {
      const tableRow = document.createElement('tr')
      row.forEach((value, columnIndex) => {
        const cell = document.createElement(rowIndex === 0 ? 'th' : 'td')
        const input = document.createElement('input')
        input.type = 'text'
        input.value = value
        input.spellcheck = view.contentDOM.getAttribute('spellcheck') !== 'false'
        input.setAttribute('aria-label', `${rowIndex === 0 ? 'Заголовок' : `Строка ${rowIndex}`}, столбец ${columnIndex + 1}`)
        input.addEventListener('input', () => {
          const nextRows = this.rows.map((currentRow) => [...currentRow])
          nextRows[rowIndex][columnIndex] = input.value
          commit(nextRows, true)
        })
        input.addEventListener('keydown', (event) => {
          if (event.key !== 'Enter') return
          event.preventDefault()
          const nextRow = wrapper.querySelector<HTMLInputElement>(`input[data-row="${rowIndex + 1}"][data-column="${columnIndex}"]`)
          nextRow?.focus()
          nextRow?.select()
        })
        input.dataset.row = String(rowIndex)
        input.dataset.column = String(columnIndex)
        cell.appendChild(input)
        tableRow.appendChild(cell)
      })
      table.appendChild(tableRow)
    })
    wrapper.appendChild(table)
    return wrapper
  }
  ignoreEvent() { return true }
}

type CodeBlockController = { update: (widget: CodeBlockWidget) => void; destroy: () => void }
const codeEditors = new WeakMap<HTMLElement, CodeBlockController>()

class CodeBlockWidget extends WidgetType {
  constructor(
    readonly code: string,
    readonly language: string,
    readonly from: number,
    readonly to: number,
    readonly fence: string,
  ) { super() }

  eq(other: CodeBlockWidget) {
    return other.code === this.code && other.language === this.language && other.fence === this.fence && other.from === this.from && other.to === this.to
  }

  updateDOM(dom: HTMLElement) { codeEditors.get(dom)?.update(this); return true }
  destroy(dom: HTMLElement) { codeEditors.get(dom)?.destroy(); codeEditors.delete(dom) }

  toDOM(view: EditorView) {
    const wrapper = document.createElement('div')
    wrapper.className = 'cm-live-code'
    wrapper.dataset.codeFrom = String(this.from)
    let model = { from: this.from, to: this.to, code: this.code, language: this.language, fence: this.fence }
    let updating = false
    let disposed = false
    let languageVersion = 0
    const grammar = new Compartment()

    const header = document.createElement('div')
    header.className = 'cm-live-code-header'
    const identity = document.createElement('div')
    const mark = document.createElement('span')
    mark.textContent = '</>'
    const language = document.createElement('input')
    language.className = 'cm-live-code-language'
    language.value = this.language
    language.placeholder = 'plain text'
    language.spellcheck = false
    language.setAttribute('aria-label', 'Язык блока кода')
    const suggestions = document.createElement('datalist')
    suggestions.id = `code-languages-${this.from}-${Math.random().toString(36).slice(2)}`
    for (const item of codeLanguages) {
      const option = document.createElement('option')
      option.value = item.alias[0] ?? item.name.toLowerCase()
      option.label = item.name
      suggestions.appendChild(option)
    }
    language.setAttribute('list', suggestions.id)
    identity.append(mark, language)
    const copy = document.createElement('button')
    copy.type = 'button'
    copy.textContent = 'Копировать'
    copy.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(model.code)
        copy.textContent = 'Скопировано'
        window.setTimeout(() => { copy.textContent = 'Копировать' }, 1200)
      } catch {
        copy.textContent = 'Не удалось'
      }
    })
    header.append(identity, copy)

    const editor = document.createElement('div')
    editor.className = 'cm-live-code-editor'
    const commit = (code: string, name: string) => {
      const source = markdownCodeSource(code, name, model.fence)
      const previousTo = model.to
      model = { ...model, code, language: name, fence: source.fence, to: model.from + source.value.length }
      view.dispatch({ changes: { from: model.from, to: previousTo, insert: source.value } })
    }
    const inner = new EditorView({
      parent: editor,
      doc: this.code,
      extensions: [
        history(), lineNumbers(), drawSelection(), indentOnInput(), bracketMatching(), codeHighlighting,
        keymap.of([indentWithTab, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
        grammar.of([]),
        EditorView.contentAttributes.of({ 'aria-label': 'Содержимое блока кода', spellcheck: 'false' }),
        EditorView.theme({
          '&.cm-editor': { height: 'auto', minHeight: '132px', backgroundColor: '#191919', color: '#d6d6d6' },
          '&.cm-editor .cm-scroller': { fontFamily: 'var(--font-mono)', fontSize: '14px', lineHeight: '1.65', overflow: 'auto', maxHeight: '430px', justifyContent: 'normal' },
          '&.cm-editor .cm-content': { flex: '1 0 auto', minWidth: '0', width: 'auto', margin: '0', padding: '14px 0', caretColor: '#b8aff3' },
          '&.cm-editor .cm-line': { padding: '0 16px' },
          '.cm-gutters': { minWidth: '42px', borderRight: '1px solid #303030', color: '#666', backgroundColor: '#1d1d1d' },
          '.cm-gutterElement': { padding: '0 10px' },
          '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { backgroundColor: '#474163 !important' },
          '.cm-cursor': { borderLeftColor: '#b8aff3' },
        }, { dark: true }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !updating) commit(update.state.doc.toString(), language.value.trim())
          if (update.geometryChanged || update.docChanged) view.requestMeasure()
        }),
      ],
    })
    const updateLanguage = (name: string) => {
      const version = ++languageVersion
      const description = codeLanguage(name)
      inner.dispatch({ effects: grammar.reconfigure([]) })
      if (!description) return
      void description.load().then((support) => {
        if (!disposed && version === languageVersion) inner.dispatch({ effects: grammar.reconfigure(support) })
      }).catch(() => { /* Unknown or unavailable grammars remain editable plain text. */ })
    }
    language.addEventListener('input', () => {
      updateLanguage(language.value)
      commit(inner.state.doc.toString(), language.value.trim())
    })
    codeEditors.set(wrapper, {
      update: (widget) => {
        const changedLanguage = model.language !== widget.language
        model = { from: widget.from, to: widget.to, code: widget.code, language: widget.language, fence: widget.fence }
        wrapper.dataset.codeFrom = String(widget.from)
        if (language.value.trim() !== widget.language) language.value = widget.language
        if (inner.state.doc.toString() !== widget.code) {
          updating = true
          try { inner.dispatch({ changes: { from: 0, to: inner.state.doc.length, insert: widget.code } }) } finally { updating = false }
        }
        if (changedLanguage) updateLanguage(widget.language)
      },
      destroy: () => { disposed = true; inner.destroy() },
    })
    updateLanguage(this.language)
    wrapper.append(header, editor, suggestions)
    return wrapper
  }

  ignoreEvent() { return true }
}

function markdownTableRow(value: string) {
  const source = value.trim().replace(/^\|/, '').replace(/\|$/, '')
  const cells: string[] = []
  let current = ''
  let escaped = false
  for (const character of source) {
    if (character === '|' && !escaped) {
      cells.push(current.trim())
      current = ''
    } else {
      current += character
    }
    escaped = character === '\\' && !escaped
    if (character !== '\\') escaped = false
  }
  cells.push(current.trim())
  return cells.map((cell) => cell.replaceAll('\\|', '|'))
}

function markdownTableSource(rows: string[][]) {
  const width = Math.max(2, ...rows.map((row) => row.length))
  const normalized = rows.map((row) => [...row, ...Array.from({ length: width - row.length }, () => '')])
  const escapeCell = (value: string) => value.replace(/\r?\n/g, ' ').replaceAll('|', '\\|').trim()
  const renderRow = (row: string[]) => `| ${row.map(escapeCell).join(' | ')} |`
  const header = normalized[0] ?? Array.from({ length: width }, () => '')
  const body = normalized.slice(1)
  return [renderRow(header), renderRow(Array.from({ length: width }, () => '---')), ...body.map(renderRow)].join('\n')
}

type MarkdownCodeBlock = {
  from: number
  to: number
  startLine: number
  endLine: number
  code: string
  language: string
  fence: string
}

function markdownCodeBlocks(document: Text) {
  const blocks: MarkdownCodeBlock[] = []
  for (let lineNumber = 1; lineNumber <= document.lines; lineNumber += 1) {
    const opening = document.line(lineNumber)
    const match = /^\s*(`{3,}|~{3,})(.*)$/.exec(opening.text)
    if (!match) continue
    const fence = match[1]
    const character = fence[0]
    let closingLine = lineNumber + 1
    for (; closingLine <= document.lines; closingLine += 1) {
      const candidate = document.line(closingLine).text.trim()
      if (candidate.length >= fence.length && [...candidate].every((value) => value === character)) break
    }
    if (closingLine > document.lines) continue
    const closing = document.line(closingLine)
    const codeFrom = Math.min(opening.to + 1, document.length)
    const codeTo = Math.max(codeFrom, closing.from)
    const sourceCode = document.sliceString(codeFrom, codeTo)
    blocks.push({
      from: opening.from,
      to: closing.to,
      startLine: lineNumber,
      endLine: closingLine,
      code: sourceCode.endsWith('\n') ? sourceCode.slice(0, -1) : sourceCode,
      language: match[2].trim(),
      fence,
    })
    lineNumber = closingLine
  }
  return blocks
}

function markdownCodeSource(code: string, language: string, preferredFence: string) {
  const normalized = code.replace(/\r\n?/g, '\n')
  const character = preferredFence[0] === '~' ? '~' : '`'
  const runs = normalized.match(character === '`' ? /`+/g : /~+/g) ?? []
  const fence = character.repeat(Math.max(3, preferredFence.length, ...runs.map((run) => run.length + 1)))
  const body = normalized ? `${normalized}\n` : ''
  return { fence, value: `${fence}${language.trim()}\n${body}${fence}` }
}

function markdownTables(document: Text) {
  const tables: Array<{ from: number; to: number; startLine: number; endLine: number; rows: string[][] }> = []
  for (let lineNumber = 1; lineNumber < document.lines; lineNumber += 1) {
    const header = document.line(lineNumber)
    const separator = document.line(lineNumber + 1)
    if (!header.text.includes('|') || !/^\s*\|?\s*:?-{3,}:?\s*(?:\|\s*:?-{3,}:?\s*)+\|?\s*$/.test(separator.text)) continue
    const rows = [markdownTableRow(header.text)]
    let endLine = lineNumber + 1
    while (endLine + 1 <= document.lines) {
      const candidate = document.line(endLine + 1)
      if (!candidate.text.includes('|') || !candidate.text.trim()) break
      rows.push(markdownTableRow(candidate.text))
      endLine += 1
    }
    const width = Math.max(...rows.map((row) => row.length))
    if (width < 2) continue
    tables.push({
      from: header.from,
      to: document.line(endLine).to,
      startLine: lineNumber,
      endLine,
      rows: rows.map((row) => [...row, ...Array.from({ length: width - row.length }, () => '')]),
    })
    lineNumber = endLine
  }
  return tables
}

function buildVisualBlockDecorations(document: Text) {
  const frontmatterEnd=frontmatterRange(document.toString())?.end??0
  const codeBlocks = markdownCodeBlocks(document).filter(block=>block.from>=frontmatterEnd)
  const tables = markdownTables(document).filter((table) => table.from>=frontmatterEnd&&!codeBlocks.some((block) => table.from < block.to && table.to > block.from))
  return Decoration.set([
    ...codeBlocks.map((block) => Decoration.replace({
      widget: new CodeBlockWidget(block.code, block.language, block.from, block.to, block.fence), block: true,
    }).range(block.from, block.to)),
    ...tables.map((table) => Decoration.replace({
      widget: new TableWidget(table.rows, table.from, table.to), block: true,
    }).range(table.from, table.to)),
  ], true)
}

const markdownVisualBlocks = StateField.define<DecorationSet>({
  create(state) { return buildVisualBlockDecorations(state.doc) },
  update(decorations, transaction) {
    return transaction.docChanged ? buildVisualBlockDecorations(transaction.state.doc) : decorations
  },
  provide: (field) => EditorView.decorations.from(field),
})

const hiddenFrontmatter=StateField.define<DecorationSet>({
  create:state=>frontmatterDecorations(state.doc),
  update:(value,transaction)=>transaction.docChanged?frontmatterDecorations(transaction.state.doc):value,
  provide:field=>[EditorView.decorations.from(field),EditorView.atomicRanges.of(view=>view.state.field(field))],
})
function frontmatterDecorations(document:Text){const end=frontmatterRange(document.toString())?.end;return end?Decoration.set([Decoration.replace({block:true}).range(0,document.lineAt(Math.max(0,end-1)).to)]):Decoration.none}

function decoratedLines(view: EditorView) {
  const decorations = []
  const frontmatterEnd=frontmatterRange(view.state.doc.toString())?.end??0
  const inlineRanges: Array<{ from: number; to: number }> = []
  syntaxTree(view.state).iterate({ enter(node) {
    if(node.to<=frontmatterEnd)return false
    if (node.name === 'FencedCode' || node.name === 'CodeBlock' || node.name === 'Table') return false
    if (node.name !== 'InlineCode') return
    inlineRanges.push({ from: node.from, to: node.to })
    decorations.push(Decoration.mark({ class: 'cm-inline-code' }).range(node.from, node.to))
    for (let child = node.node.firstChild; child; child = child.nextSibling) {
      if (child.name === 'CodeMark') decorations.push(Decoration.replace({}).range(child.from, child.to))
    }
    return false
  } })
  const insideInlineCode = (position: number) => inlineRanges.some((range) => position >= range.from && position < range.to)
  const activeLine = view.state.doc.lineAt(view.state.selection.main.head).number
  const tables = markdownTables(view.state.doc)
  const codeBlocks = markdownCodeBlocks(view.state.doc)
  const blockLines = new Set([
    ...tables.flatMap((table) => Array.from({ length: table.endLine - table.startLine + 1 }, (_, index) => table.startLine + index)),
    ...codeBlocks.flatMap((block) => Array.from({ length: block.endLine - block.startLine + 1 }, (_, index) => block.startLine + index)),
  ])
  for (const { from, to } of view.visibleRanges) {
    for (let position = from; position <= to;) {
      const line = view.state.doc.lineAt(position)
      if (line.from<frontmatterEnd||blockLines.has(line.number)) {
        position = line.to + 1
        continue
      }
      const heading = /^(#{1,6})\s/.exec(line.text)
      if (heading) decorations.push(Decoration.line({ attributes: { class: `cm-md-heading cm-md-h${heading[1].length}` } }).range(line.from))
      else if (/^>\s/.test(line.text)) decorations.push(Decoration.line({ attributes: { class: 'cm-md-quote' } }).range(line.from))
      const task = /^(\s*)-\s+\[([ xX])\]/.exec(line.text)
      if (task) {
        const taskFrom = line.from + task[1].length
        const taskTo = taskFrom + task[0].length - task[1].length
        decorations.push(Decoration.replace({ widget: new CheckboxWidget(task[2].toLowerCase() === 'x', taskFrom, taskTo) }).range(taskFrom, taskTo))
        const metadata = /\s+(?:⏳|📅)\s*\d{4}-\d{2}-\d{2}(?:[ T]\d{2}:\d{2})?|\s+\^[A-Za-z0-9-]+/g
        for (let token = metadata.exec(line.text); token; token = metadata.exec(line.text)) {
          decorations.push(Decoration.replace({}).range(line.from + token.index, line.from + token.index + token[0].length))
        }
      }
      if (line.number !== activeLine) {
        if (heading) decorations.push(Decoration.replace({}).range(line.from, line.from + heading[0].length))
        const pairedMarkers = /(\*\*|__|~~)/g
        for (let marker = pairedMarkers.exec(line.text); marker; marker = pairedMarkers.exec(line.text)) {
          if (insideInlineCode(line.from + marker.index)) continue
          decorations.push(Decoration.replace({}).range(line.from + marker.index, line.from + marker.index + marker[0].length))
        }
        const wikiLinks = /(!)?\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g
        for (let link = wikiLinks.exec(line.text); link; link = wikiLinks.exec(line.text)) {
          const start = line.from + link.index
          if (insideInlineCode(start)) continue
          const prefixLength = (link[1]?.length ?? 0) + 2
          decorations.push(Decoration.replace({}).range(start, start + prefixLength))
          if (link[3]) {
            const aliasStart = start + prefixLength + link[2].length
            decorations.push(Decoration.replace({}).range(start + prefixLength, aliasStart + 1))
          }
          decorations.push(Decoration.replace({}).range(start + link[0].length - 2, start + link[0].length))
        }
      }
      const emptyCode = /(?<!`)``(?!`)/g
      for (let pair = emptyCode.exec(line.text); pair; pair = emptyCode.exec(line.text)) {
        const from = line.from + pair.index
        if (!insideInlineCode(from)) decorations.push(Decoration.replace({ widget: new EmptyInlineCodeWidget(from) }).range(from, from + 2))
      }
      position = line.to + 1
    }
  }
  return Decoration.set(decorations, true)
}

const markdownLines = ViewPlugin.fromClass(class {
  decorations: DecorationSet
  constructor(view: EditorView) { this.decorations = decoratedLines(view) }
  update(update: ViewUpdate) {
    if (update.docChanged || update.viewportChanged || update.selectionSet) this.decorations = decoratedLines(update.view)
  }
}, { decorations: (plugin) => plugin.decorations })

function linkMarks(view: EditorView) {
  const marks = []
  for (const range of view.visibleRanges) for (let position = range.from; position <= range.to;) {
    const line = view.state.doc.lineAt(position)
    for (const match of line.text.matchAll(/!?\[\[([^\]|]+)(?:\|[^\]]*)?\]\]|\[[^\]]*\]\(([^\s)]*)(?:\s+"[^"\n]*")?\)/g)) {
      const from = line.from + match.index
      let node = syntaxTree(view.state).resolveInner(from, 1), code = false
      for (;;) { if (['InlineCode', 'FencedCode', 'CodeBlock'].includes(node.name)) code = true; if (!node.parent) break; node = node.parent }
      if (code) continue
      marks.push(Decoration.mark({ class: 'cm-note-link', attributes: { 'data-note-target': match[1] ?? match[2], 'data-note-kind': match[1] !== undefined ? 'wiki' : 'markdown' } }).range(from, from + match[0].length))
    }
    position = line.to + 1
  }
  return Decoration.set(marks, true)
}
const noteLinks = ViewPlugin.fromClass(class {
  decorations: DecorationSet
  constructor(view: EditorView) { this.decorations = linkMarks(view) }
  update(update: ViewUpdate) { if (update.docChanged || update.viewportChanged) this.decorations = linkMarks(update.view) }
}, { decorations: (plugin) => plugin.decorations })

const editorTheme = EditorView.theme({
  '&': { height: '100%', backgroundColor: '#1e1e1e', color: '#c8c8c8' },
  '.cm-scroller': { justifyContent: 'center', fontFamily: 'var(--font-ui)', fontSize: '17px', fontWeight: '400', lineHeight: '1.72', overflow: 'auto' },
  '.cm-content': { flex: '0 1 760px', minWidth: '0', width: 'calc(100% - 80px)', margin: '0 auto', padding: '58px 0 140px', caretColor: '#a99fff' },
  '.cm-line': { padding: '0 3px' },
  '.cm-activeLine': { backgroundColor: 'transparent' },
  '.cm-selectionBackground, &.cm-focused .cm-selectionBackground, ::selection': { backgroundColor: '#474163 !important' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: '#9b91ec' },
  '.cm-tooltip': { border: '1px solid #454545', backgroundColor: '#252525', color: '#ddd' },
  '.cm-tooltip-autocomplete > ul > li[aria-selected]': { backgroundColor: '#403a58', color: '#fff' },
  '.cm-panels': { backgroundColor: '#252525', color: '#ddd' },
  '.cm-searchMatch': { backgroundColor: '#7c673866', outline: '1px solid #927d4d' },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: '#554e7888' },
  '.cm-md-heading': { color: '#f0f0f0', fontFamily: 'var(--font-display)', fontWeight: '680', lineHeight: '1.3' },
  '.cm-md-h1': { fontSize: '1.9em', paddingTop: '18px', paddingBottom: '8px' },
  '.cm-md-h2': { fontSize: '1.45em', paddingTop: '15px', paddingBottom: '6px' },
  '.cm-md-h3': { fontSize: '1.18em', paddingTop: '12px', paddingBottom: '4px' },
  '.cm-md-h4, .cm-md-h5, .cm-md-h6': { fontSize: '1.02em', paddingTop: '9px', fontWeight: '650' },
  '.cm-md-quote': { borderLeft: '2px solid #666', paddingLeft: '14px !important', color: '#aaa' },
  '.cm-task-checkbox': { width: '17px', height: '17px', margin: '0 9px 0 1px', verticalAlign: '-3px', accentColor: '#9c92e3', cursor: 'pointer' },
}, { dark: true })

export const MarkdownEditor = forwardRef<MarkdownEditorHandle, MarkdownEditorProps>(function MarkdownEditor({ value, notePaths, visual, settings, onContext, onOpenLink, onChange }, forwardedRef) {
  const hostRef = useRef<HTMLDivElement>(null)
  const viewRef = useRef<EditorView | null>(null)
  const changeRef = useRef(onChange)
  const pathsRef = useRef(notePaths)
  const visualRef = useRef(visual)
  const visualCompartment = useRef(new Compartment())
  const propertiesCompartment = useRef(new Compartment())
  const settingsCompartment = useRef(new Compartment())
  const lastTarget = useRef<EditorView | null>(null)
  const currentSettings = useRef(settings)
  const contextRef = useRef(onContext)
  const openLinkRef = useRef(onOpenLink)
  currentSettings.current = settings
  contextRef.current = onContext
  openLinkRef.current = onOpenLink
  changeRef.current = onChange
  pathsRef.current = notePaths

  function activeEditor() {
    const element = document.activeElement?.closest<HTMLElement>('.cm-content')
    const focused = element && hostRef.current?.contains(element) ? EditorView.findFromDOM(element) : null
    return focused ?? (lastTarget.current?.dom.isConnected ? lastTarget.current : viewRef.current)
  }

  async function execute(action: EditorAction) {
    const view = activeEditor()
    if (!view || !view.dom.isConnected) return
    view.focus()
    const { from, to } = view.state.selection.main
    const selected = view.state.doc.sliceString(from, to)
    if (action === 'undo' || action === 'redo') { (action === 'undo' ? undo : redo)(view); view.focus(); return }
    if (action === 'select-all') { selectAll(view); view.focus(); return }
    if (['copy', 'cut', 'paste'].includes(action)) {
      const doc = view.state.doc
      const unchanged = () => view.dom.isConnected && view.state.doc === doc && view.state.selection.main.from === from && view.state.selection.main.to === to
      if (action === 'copy' || action === 'cut') {
        if (!selected) return
        await navigator.clipboard.writeText(selected)
        if (action === 'cut') {
          if (!unchanged()) throw new Error('Текст изменился во время копирования. Вырезание отменено.')
          view.dispatch({ changes: { from, to, insert: '' }, selection: { anchor: from }, userEvent: 'delete.cut' })
        }
      } else {
        const text = await navigator.clipboard.readText()
        if (!unchanged()) throw new Error('Текст изменился во время чтения буфера. Повторите вставку.')
        view.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length }, scrollIntoView: true, userEvent: 'input.paste' })
      }
      view.focus(); return
    }
    // Code blocks have their own editor: Markdown formatting only affects prose.
    if (view !== viewRef.current) return
    let insert = '', offset = 0, length = selected.length
    if (action === 'inline-code') { const source = inlineCodeSource(selected); insert = source.value; offset = source.offset; length = source.length }
    else if (action === 'insert-link') { insert = `[[${selected || 'Заметка'}]]`; offset = 2; length = selected.length || 'Заметка'.length }
    else {
      const marker = action === 'bold' ? '**' : action === 'italic' ? '*' : '~~'
      if (selected.startsWith(marker) && selected.endsWith(marker) && selected.length >= marker.length * 2) { insert = selected.slice(marker.length, -marker.length); length = insert.length }
      else if (view.state.doc.sliceString(Math.max(0, from - marker.length), from) === marker && view.state.doc.sliceString(to, to + marker.length) === marker) {
        view.dispatch({ changes: [{ from: from - marker.length, to: from, insert: '' }, { from: to, to: to + marker.length, insert: '' }], selection: { anchor: from - marker.length, head: to - marker.length }, userEvent: 'input.format' }); view.focus(); return
      } else { insert = `${marker}${selected || 'текст'}${marker}`; offset = marker.length; length = selected.length || 'текст'.length }
    }
    view.dispatch({ changes: { from, to, insert }, selection: { anchor: from + offset, head: from + offset + length }, scrollIntoView: true, userEvent: 'input.format' })
    view.focus()
  }

  function requestContext(target: EventTarget | null, x?: number, y?: number) {
    const element = target instanceof HTMLElement ? target : null
    // Native fields (table cells / language selector) keep their native menu.
    if (element?.closest('input, textarea, select')) return false
    const content = element?.closest<HTMLElement>('.cm-content')
    const view = content ? EditorView.findFromDOM(content) : viewRef.current
    if (!view) return false
    lastTarget.current = view
    if (x !== undefined && y !== undefined) {
      const position = view.posAtCoords({ x, y })
      const selection = view.state.selection.main
      if (position !== null && (position < selection.from || position > selection.to || selection.empty)) view.dispatch({ selection: { anchor: position } })
    }
    const selection = view.state.selection.main, rect = view.coordsAtPos(selection.head)
    contextRef.current({ x: x ?? rect?.left ?? 400, y: y ?? rect?.bottom ?? 200, hasSelection: !selection.empty, canFormat: view === viewRef.current, canUndo: undoDepth(view.state) > 0, canRedo: redoDepth(view.state) > 0, link: view === viewRef.current ? linkAtPosition(view.state.doc.toString(), selection.head) : null })
    return true
  }

  useImperativeHandle(forwardedRef, () => ({
    position() { const view = viewRef.current; if (!view) return; const {anchor,head} = view.state.selection.main; return {anchor,head,top:view.scrollDOM.scrollTop,left:view.scrollDOM.scrollLeft} },
    restorePosition(position) { const view = viewRef.current; if (!view) return; view.dispatch({selection:{anchor:Math.min(position.anchor,view.state.doc.length),head:Math.min(position.head,view.state.doc.length)}}); view.scrollDOM.scrollTop=position.top; view.scrollDOM.scrollLeft=position.left },
    execute,
    currentLink() { const view = activeEditor(); return view && view === viewRef.current ? linkAtPosition(view.state.doc.toString(), view.state.selection.main.head) : null },
    insertInlineCode() {
      const view = viewRef.current
      if (!view) return
      const selection = view.state.selection.main
      const source = inlineCodeSource(view.state.doc.sliceString(selection.from, selection.to))
      view.dispatch({ changes: { from: selection.from, to: selection.to, insert: source.value }, selection: { anchor: selection.from + source.offset, head: selection.from + source.offset + source.length }, scrollIntoView: true })
      view.focus()
    },
    insertText(text: string) {
      const view = viewRef.current
      if (!view) return
      const selection = view.state.selection.main
      view.dispatch({
        changes: { from: selection.from, to: selection.to, insert: text },
        selection: { anchor: selection.from + text.length },
        scrollIntoView: true,
      })
      view.focus()
    },
    insertSnippet(text: string, cursorOffset = text.length) {
      const view = viewRef.current
      if (!view) return
      const selection = view.state.selection.main
      const cursor = selection.from + Math.max(0, Math.min(cursorOffset, text.length))
      view.dispatch({
        changes: { from: selection.from, to: selection.to, insert: text },
        selection: { anchor: cursor },
        scrollIntoView: true,
      })
      view.focus()
    },
    insertBlock(text: string, cursorOffset = text.length) {
      const view = viewRef.current
      if (!view) return
      const selection = view.state.selection.main
      const firstLine = view.state.doc.lineAt(selection.from)
      const lastLine = view.state.doc.lineAt(selection.to)
      const prefix = selection.from === firstLine.from ? '' : '\n'
      const suffix = selection.to === lastLine.to ? '' : '\n'
      const inserted = `${prefix}${text}${suffix}`
      view.dispatch({
        changes: { from: selection.from, to: selection.to, insert: inserted },
        selection: { anchor: selection.from + prefix.length + Math.max(0, Math.min(cursorOffset, text.length)) },
        scrollIntoView: true,
      })
      view.focus()
      if (visualRef.current && /^`{3,}/.test(text)) window.requestAnimationFrame(() => {
        const content = view.dom.querySelector<HTMLElement>(`.cm-live-code[data-code-from="${selection.from + prefix.length}"] .cm-content`)
        if (content) EditorView.findFromDOM(content)?.focus()
      })
    },
    focus() { activeEditor()?.focus() },
    revealLine(lineNumber: number) {
      const view = viewRef.current
      if (!view) return
      const line = view.state.doc.line(Math.max(1, Math.min(lineNumber, view.state.doc.lines)))
      view.dispatch({
        selection: { anchor: line.from, head: line.to },
        effects: EditorView.scrollIntoView(line.from, { y: 'center' }),
      })
      view.dom.classList.remove('reveal-task')
      window.requestAnimationFrame(() => view.dom.classList.add('reveal-task'))
      window.setTimeout(() => view.dom.classList.remove('reveal-task'), 1800)
      view.focus()
    },
    revealDescription(fallbackLine: number) {
      const view = viewRef.current
      if (!view) return
      let target = Math.max(1, Math.min(fallbackLine, view.state.doc.lines))
      for (let lineNumber = 1; lineNumber <= view.state.doc.lines; lineNumber += 1) {
        if (/^#{1,6}\s+Описание\s*$/iu.test(view.state.doc.line(lineNumber).text)) {
          target = Math.min(view.state.doc.lines, lineNumber + 2)
          break
        }
      }
      const line = view.state.doc.line(target)
      view.dispatch({
        selection: { anchor: line.from, head: line.to },
        effects: EditorView.scrollIntoView(line.from, { y: 'center' }),
      })
      view.dom.classList.remove('reveal-task')
      window.requestAnimationFrame(() => view.dom.classList.add('reveal-task'))
      window.setTimeout(() => view.dom.classList.remove('reveal-task'), 1800)
      view.focus()
    },
  }), [])

  useEffect(() => {
    if (!hostRef.current) return
    const wikiCompletion = (context: CompletionContext) => {
      const match = context.matchBefore(/\[\[[^\]]*$/)
      if (!match) return null
      const query = match.text.slice(2).toLocaleLowerCase('ru')
      return {
        from: match.from + 2,
        options: pathsRef.current
          .filter((path) => !query || path.toLocaleLowerCase('ru').includes(query))
          .slice(0, 80)
          .map((path) => ({
            label: path.replace(/\.md$/i, ''),
            detail: 'заметка',
            type: 'text',
          })),
      }
    }
    const state = EditorState.create({
      doc: value,
      extensions: [
        history(),
        drawSelection(),
        dropCursor(),
        indentOnInput(),
        bracketMatching(),
        highlightActiveLine(),
        markdown({ codeLanguages: languages }),
        syntaxHighlighting(markdownHighlight),
        codeHighlighting,
        noteLinks,
        visualCompartment.current.of(visual ? [markdownVisualBlocks, markdownLines] : []),
        propertiesCompartment.current.of(visual&&currentSettings.current.propertiesMode!=='source'?[hiddenFrontmatter]:[]),
        autocompletion({ override: [wikiCompletion], activateOnTyping: true }),
        keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, ...completionKeymap, indentWithTab]),
        editorTheme,
        EditorView.lineWrapping,
        settingsCompartment.current.of([...(currentSettings.current.lineNumbers ? [lineNumbers()] : []), EditorView.contentAttributes.of({ spellcheck: String(currentSettings.current.spellcheck), 'aria-label': 'Текст заметки' })]),
        EditorView.domEventHandlers({
          mousedown(event, view) {
            if (event.button !== 0 || !event.ctrlKey && !event.metaKey) return false
            const position = view.posAtCoords({ x: event.clientX, y: event.clientY })
            const link = position !== null ? linkAtPosition(view.state.doc.toString(), position) : null
            if (!link) return false
            event.preventDefault(); openLinkRef.current(link.target, link.kind); return true
          },
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged) changeRef.current(update.state.doc.toString())
        }),
      ],
    })
    const view = new EditorView({ state, parent: hostRef.current })
    viewRef.current = view
    return () => {
      view.destroy()
      viewRef.current = null
    }
  }, [])

  useEffect(() => {
    const view = viewRef.current
    if (!view || visualRef.current === visual) return
    visualRef.current = visual
    view.dispatch({ effects: visualCompartment.current.reconfigure(visual ? [markdownVisualBlocks, markdownLines] : []) })
  }, [visual])

  useEffect(()=>{viewRef.current?.dispatch({effects:propertiesCompartment.current.reconfigure(visual&&settings.propertiesMode!=='source'?[hiddenFrontmatter]:[])})},[visual,settings.propertiesMode])

  useEffect(() => {
    const view = viewRef.current
    if (view) view.dispatch({ effects: settingsCompartment.current.reconfigure([...(settings.lineNumbers ? [lineNumbers()] : []), EditorView.contentAttributes.of({ spellcheck: String(settings.spellcheck), 'aria-label': 'Текст заметки' })]) })
    hostRef.current?.querySelectorAll<HTMLInputElement>('.cm-live-table input').forEach((input) => { input.spellcheck = settings.spellcheck })
  }, [settings.lineNumbers, settings.spellcheck])

  useEffect(() => {
    const view = viewRef.current
    if (!view || value === view.state.doc.toString()) return
    view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: value } })
  }, [value])

  return <div className="codemirror-host" ref={hostRef} onFocusCapture={(event) => { const content = (event.target as HTMLElement).closest<HTMLElement>('.cm-content'); if (content) lastTarget.current = EditorView.findFromDOM(content) ?? null }} onContextMenu={(event) => { if (requestContext(event.target, event.clientX, event.clientY)) { event.preventDefault(); event.stopPropagation() } }} onKeyDownCapture={(event) => {
    if ((event.key === 'ContextMenu' || event.key === 'F10' && event.shiftKey) && requestContext(event.target)) { event.preventDefault(); event.stopPropagation() }
  }} />
})
