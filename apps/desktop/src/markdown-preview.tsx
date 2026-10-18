import ReactMarkdown from 'react-markdown'
import { markdownLanguage } from '@codemirror/lang-markdown'
import remarkGfm from 'remark-gfm'
import { useEffect, useRef, useState, type KeyboardEvent, type MouseEvent, type ReactNode } from 'react'
import type { VaultIndex } from './types'
import { highlightedCode } from './code-language'

function CodePreview({ code, language }: { code: string; language: string }) {
  const [tokens, setTokens] = useState([{ text: code, className: '' }])
  const [copied, setCopied] = useState(false)
  useEffect(() => {
    let cancelled = false
    setTokens([{ text: code, className: '' }])
    void highlightedCode(code, language).then((next) => { if (!cancelled) setTokens(next) }).catch(() => {})
    return () => { cancelled = true }
  }, [code, language])
  useEffect(() => {
    if (!copied) return
    const timeout = window.setTimeout(() => setCopied(false), 1200)
    return () => window.clearTimeout(timeout)
  }, [copied])
  return <div className="preview-code"><div className="cm-live-code-header"><div><span>{'</>'}</span><span className="preview-code-language">{language || 'plain text'}</span></div><button onClick={() => { void navigator.clipboard.writeText(code).then(() => setCopied(true)).catch(() => {}) }}>{copied ? 'Скопировано' : 'Копировать'}</button></div><pre><code>{tokens.map((token, index) => <span key={index} className={token.className || undefined}>{token.text}</span>)}</code></pre></div>
}

function withWikiLinks(source: string, index: VaultIndex | null) {
  const codeRanges: Array<{ from: number; to: number }> = []
  markdownLanguage.parser.parse(source).iterate({ enter(node) {
    if (!['InlineCode', 'FencedCode', 'CodeBlock'].includes(node.name)) return
    codeRanges.push({ from: node.from, to: node.to })
    return false
  } })
  return source.replace(/(?<embed>!)?\[\[(?<body>[^\]]+)\]\]/g, (full, embed: string | undefined, body: string, offset: number) => {
    if (codeRanges.some((range) => offset >= range.from && offset < range.to)) return full
    const [destination, label] = body.split('|', 2)
    const target = destination.split(/[#^]/, 1)[0].trim()
    if (embed) return `\`${label ?? destination}\``
    const resolved = index?.documents.find((document) => {
      const path = document.path.replace(/\.md$/i, '')
      const stem = path.split('/').at(-1) ?? path
      return [path, stem, document.title, ...document.aliases].some((candidate) => candidate.toLocaleLowerCase('ru') === target.toLocaleLowerCase('ru'))
    })?.path ?? target
    return `[${label ?? destination}](#brandon-note=${encodeURIComponent(resolved)})`
  })
}

function tableMatrix(table: HTMLTableElement) {
  const rows = Array.from(table.rows).map((row) => Array.from(row.cells).map((cell) => cell.innerText.trim()))
  const width = Math.max(1, ...rows.map((row) => row.length))
  return rows.map((row) => [...row, ...Array.from({ length: width - row.length }, () => '')])
}

function markdownCell(value: string) {
  return value.replaceAll('|', '\\|').replace(/\r?\n/g, '<br>').trim()
}

function matrixMarkdown(matrix: string[][]) {
  const width = Math.max(1, ...matrix.map((row) => row.length))
  const normalized = matrix.length ? matrix.map((row) => [...row, ...Array.from({ length: width - row.length }, () => '')]) : [Array.from({ length: width }, () => '')]
  const row = (values: string[]) => `| ${values.map(markdownCell).join(' | ')} |`
  return [row(normalized[0]), row(Array.from({ length: width }, () => '---')), ...normalized.slice(1).map(row)].join('\n')
}

function replaceTable(source: string, startLine: number, endLine: number, matrix: string[][]) {
  const separator = source.includes('\r\n') ? '\r\n' : '\n'
  const lines = source.split(/\r?\n/)
  lines.splice(startLine - 1, Math.max(endLine - startLine + 1, 1), ...matrixMarkdown(matrix).split('\n'))
  return lines.join(separator)
}

function SmartTable({ children, source, startLine, endLine, onChange }: {
  children: ReactNode
  source: string
  startLine: number
  endLine: number
  onChange?: (value: string) => void
}) {
  const wrapperRef = useRef<HTMLDivElement>(null)
  const tableRef = useRef<HTMLTableElement>(null)
  const [editing, setEditing] = useState(false)
  const [expanded, setExpanded] = useState(false)

  useEffect(() => {
    const cells = tableRef.current?.querySelectorAll('th, td') ?? []
    for (const cell of cells) {
      if (editing) {
        cell.setAttribute('contenteditable', 'true')
        cell.setAttribute('tabindex', '0')
      } else {
        cell.removeAttribute('contenteditable')
        cell.removeAttribute('tabindex')
      }
    }
  }, [editing, source])

  const commit = (matrix = tableRef.current ? tableMatrix(tableRef.current) : []) => {
    if (!onChange || matrix.length === 0) return
    onChange(replaceTable(source, startLine, endLine, matrix))
  }

  const changeMatrix = (transform: (matrix: string[][]) => string[][]) => {
    if (!tableRef.current) return
    commit(transform(tableMatrix(tableRef.current)))
  }

  const navigate = (event: KeyboardEvent<HTMLTableElement>) => {
    if (!editing || (event.key !== 'Tab' && event.key !== 'Enter')) return
    const cell = (event.target as HTMLElement).closest('th, td') as HTMLTableCellElement | null
    const cells = Array.from(tableRef.current?.querySelectorAll<HTMLElement>('th, td') ?? [])
    const index = cell ? cells.indexOf(cell) : -1
    if (index < 0) return
    event.preventDefault()
    const direction = event.shiftKey ? -1 : 1
    cells[Math.max(0, Math.min(cells.length - 1, index + direction))]?.focus()
  }

  return <div ref={wrapperRef} className={`smart-table ${editing ? 'editing' : ''} ${expanded ? 'expanded' : ''}`} onBlurCapture={(event) => {
    if (!editing || wrapperRef.current?.contains(event.relatedTarget as Node | null)) return
    commit()
  }}>
    <div className="smart-table-toolbar">
      <span>Таблица</span>
      <div>
        {editing && <>
          <button title="Добавить строку" onClick={() => changeMatrix((matrix) => [...matrix, Array.from({ length: matrix[0]?.length ?? 1 }, () => '')])}>+ строка</button>
          <button title="Убрать последнюю строку" disabled={(tableRef.current?.rows.length ?? 0) <= 2} onClick={() => changeMatrix((matrix) => matrix.slice(0, -1))}>− строка</button>
          <button title="Добавить столбец" onClick={() => changeMatrix((matrix) => matrix.map((row) => [...row, '']))}>+ столбец</button>
          <button title="Убрать последний столбец" disabled={(tableRef.current?.rows[0]?.cells.length ?? 0) <= 1} onClick={() => changeMatrix((matrix) => matrix.map((row) => row.slice(0, -1)))}>− столбец</button>
        </>}
        <button onClick={() => setExpanded((value) => !value)}>{expanded ? 'Свернуть' : 'Развернуть'}</button>
        {onChange && <button className={editing ? 'active' : ''} onClick={() => { if (editing) commit(); setEditing((value) => !value) }}>{editing ? 'Готово' : 'Редактировать'}</button>}
      </div>
    </div>
    <div className="smart-table-scroll"><table ref={tableRef} onKeyDown={navigate}>{children}</table></div>
    {editing && <div className="smart-table-hint"><kbd>Tab</kbd> следующая ячейка · изменения сохраняются в Markdown</div>}
  </div>
}

export function MarkdownPreview({ source, index, onOpen, onChange }: { source: string; index: VaultIndex | null; onOpen: (path: string) => void; onChange?: (value: string) => void }) {
  const renderedSource = withWikiLinks(source || '*Пустая заметка*', index)
  return <div className="markdown-body"><ReactMarkdown remarkPlugins={[remarkGfm]} components={{
    pre: ({ node, children }) => {
      const element = node?.children.find((child) => child.type === 'element' && child.tagName === 'code')
      if (!element || element.type !== 'element') return <pre>{children}</pre>
      const classes = element.properties.className
      const language = (Array.isArray(classes) ? classes.map(String).find((value) => value.startsWith('language-')) : '')?.slice(9) ?? ''
      const code = element.children.map((child) => child.type === 'text' ? child.value : '').join('').replace(/\n$/, '')
      return <CodePreview code={code} language={language} />
    },
    a: ({ href, children }) => {
      const open = (event: MouseEvent<HTMLAnchorElement>) => {
        if (!href?.startsWith('#brandon-note=')) return
        event.preventDefault()
        onOpen(decodeURIComponent(href.slice('#brandon-note='.length)))
      }
      return <a href={href} onClick={open}>{children}</a>
    },
    table: ({ node, children }) => <SmartTable source={source} startLine={node?.position?.start.line ?? 1} endLine={node?.position?.end.line ?? 1} onChange={onChange}>{children}</SmartTable>,
  }}>{renderedSource}</ReactMarkdown></div>
}
