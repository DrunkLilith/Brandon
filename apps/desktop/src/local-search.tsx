import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState, type RefObject } from 'react'
import { SearchIcon } from './icons'
import { observeSearch } from './search-request'
import type { SearchHit, SearchResponse, VaultIndex } from './types'

type Mode = 'words' | 'literal'
const labels = { note: 'Заметка', canvas: 'Карточка холста', task: 'Задача', file: 'Файл' }
function resultCount(count: number) {
  const ending = count % 100 >= 11 && count % 100 <= 14 ? 'совпадений' : count % 10 === 1 ? 'совпадение' : count % 10 >= 2 && count % 10 <= 4 ? 'совпадения' : 'совпадений'
  return `${count}${count === 80 ? '+' : ''} ${ending}`
}

export function LocalSearch({ root, revision, query, onQueryChange, inputRef, onOpen }: {
  root: string
  revision: VaultIndex | null
  query: string
  onQueryChange: (query: string) => void
  inputRef: RefObject<HTMLInputElement | null>
  onOpen: (hit: SearchHit) => void
}) {
  const [mode, setMode] = useState<Mode>('words')
  const [kind, setKind] = useState('')
  const [response, setResponse] = useState<SearchResponse | null>(null)
  const [busy, setBusy] = useState(false)
  const [completedKey, setCompletedKey] = useState('')
  const [error, setError] = useState('')
  const panel = useRef<HTMLDivElement>(null)
  const hasQuery = Boolean(query.trim() || kind)
  const requestKey = JSON.stringify({ query, mode, kind })
  const pending = hasQuery && (busy || completedKey !== requestKey)
  useEffect(() => {
    let stopRequest = () => {}
    setResponse(null); setError(''); setBusy(hasQuery); setCompletedKey('')
    if (!hasQuery) return
    const timer = window.setTimeout(() => {
      stopRequest = observeSearch(invoke<SearchResponse>('search_vault', { query: `${query}${kind ? ` type:${kind}` : ''}`, mode, limit: 80 }), {
        result: setResponse,
        error: (reason) => setError(String(reason)),
        complete: () => { setBusy(false); setCompletedKey(requestKey) },
      })
    }, 220)
    return () => { stopRequest(); window.clearTimeout(timer) }
  }, [root, revision, query, mode, kind, hasQuery, requestKey])

  return <div className="local-search" ref={panel} aria-label="Локальный поиск">
    <div className="sidebar-search"><SearchIcon /><input ref={inputRef} aria-label="Поиск по vault" value={query} onChange={(event) => onQueryChange(event.target.value)} placeholder={mode === 'words' ? 'Слова или "точная фраза"…' : 'Точный текст или код…'} onKeyDown={(event) => {
      if (event.key === 'ArrowDown') { event.preventDefault(); panel.current?.querySelector<HTMLButtonElement>('.search-results button')?.focus() }
      if (event.key === 'Escape') { event.preventDefault(); onQueryChange('') }
    }} /><kbd>Ctrl K</kbd></div>
    <div className="local-search-options">
      <div role="group" aria-label="Режим поиска"><button aria-pressed={mode === 'words'} onClick={() => setMode('words')}>Слова</button><button aria-pressed={mode === 'literal'} onClick={() => setMode('literal')}>Точно</button></div>
      <select aria-label="Тип результатов" value={kind} onChange={(event) => setKind(event.target.value)}>
        <option value="">Все типы</option><option value="note">Заметки</option><option value="canvas">Холсты</option><option value="task">Задачи</option><option value="file">Файлы</option>
      </select>
    </div>
    <div className="sidebar-content" aria-busy={pending} data-completed-query={completedKey === requestKey ? query : undefined}>
      {!hasQuery ? <div className="local-search-help"><p>Текст заметок и карточек, названия файлов.</p><p>{mode === 'words' ? <>Фраза: <code>"план на неделю"</code></> : 'Фрагмент целиком: кавычки, переносы и повторные пробелы сохраняются.'}<br />Фильтры: <code>path:Проект</code> <code>tag:работа</code></p><p>Поиск локальный, по сохранённым файлам. «Точно» подходит для кода и частей слов.</p></div> : error && !pending ? <p className="local-search-error" role="alert">{error}</p> : <>
        <p className="local-search-status" role="status">{pending ? 'Проверяю индекс…' : response ? resultCount(response.hits.length) : ''}</p>
        {response && !pending && <>
          <div className="search-results">{response.hits.map((hit, index) => <button key={`${hit.path}:${hit.nodeId ?? ''}:${hit.line}`} data-path={hit.path} data-node-id={hit.nodeId ?? undefined} onClick={() => onOpen(hit)} onKeyDown={(event) => {
            const buttons = panel.current?.querySelectorAll<HTMLButtonElement>('.search-results button')
            if (event.key === 'Enter') { event.preventDefault(); onOpen(hit) }
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); buttons?.[Math.max(0, Math.min(response.hits.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)))]?.focus() }
            if (event.key === 'Escape') { event.preventDefault(); inputRef.current?.focus() }
          }}><strong>{hit.title}</strong><span>{hit.kind === 'canvas' && !hit.nodeId ? 'Холст' : labels[hit.kind]} · {hit.path}{hit.nodeId || !hit.contentIndexed ? '' : ` · строка ${hit.line}`}</span><p>{hit.snippet || (hit.contentIndexed ? 'Совпадение в названии' : 'Только название — содержимое не индексировалось')}</p></button>)}</div>
          <p className="local-search-coverage" title={`Обновлено: ${response.changedSources}; удалено из индекса: ${response.removedSources}; ${Math.round(response.elapsedMs)} мс`}>По содержимому: {response.indexedSources} · только имена: {response.nameOnlySources}{response.failedSources ? ` · ошибки: ${response.failedSources}` : ''}{response.skippedSources ? ` · пропущено: ${response.skippedSources}` : ''}</p>
        </>}
      </>}
    </div>
  </div>
}
