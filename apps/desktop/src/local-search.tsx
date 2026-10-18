import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
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
  const [recovering, setRecovering] = useState(false)
  const [recoveryMessage, setRecoveryMessage] = useState('')
  const [recoveryError, setRecoveryError] = useState('')
  const [refresh, setRefresh] = useState(0)
  const recoveryActive = useRef(false)
  const currentRoot = useRef(root)
  currentRoot.current = root
  const panel = useRef<HTMLDivElement>(null)
  const lastRequestKey = useRef('')
  const hasQuery = Boolean(query.trim() || kind)
  const requestKey = JSON.stringify({ root, query, mode, kind })
  const pending = hasQuery && (busy || recovering || completedKey !== requestKey)
  useEffect(() => { setRecoveryMessage(''); setRecoveryError('') }, [root])
  useEffect(() => {
    let disposed = false
    let stop: (() => void) | undefined
    let timer = 0
    void listen('search-index-updated', () => {
      if (timer) return
      timer = window.setTimeout(() => { timer = 0; if (!disposed) setRefresh((value) => value + 1) }, 250)
    }).then((unlisten) => { if (disposed) unlisten(); else stop = unlisten })
    return () => { disposed = true; stop?.(); window.clearTimeout(timer) }
  }, [root])
  useEffect(() => {
    let stopRequest = () => {}
    const typing = lastRequestKey.current !== requestKey
    lastRequestKey.current = requestKey
    setResponse(null); setError(''); setBusy(hasQuery); setCompletedKey('')
    if (!hasQuery) return
    const timer = window.setTimeout(() => {
      stopRequest = observeSearch(invoke<SearchResponse>('search_vault', { root, query: `${query}${kind ? ` type:${kind}` : ''}`, mode, limit: 80 }), {
        result: setResponse,
        error: (reason) => setError(String(reason)),
        complete: () => { setBusy(false); setCompletedKey(requestKey) },
      })
    }, typing ? 220 : 0)
    return () => { stopRequest(); window.clearTimeout(timer) }
  }, [root, revision, query, mode, kind, hasQuery, requestKey, refresh])

  async function rebuild() {
    if (recoveryActive.current) return
    recoveryActive.current = true
    setRecovering(true); setRecoveryError(''); setRecoveryMessage('')
    try {
      const result = await invoke<{ search: SearchResponse; backupPath: string | null }>('rebuild_search_index', { root, query: `${query}${kind ? ` type:${kind}` : ''}`, mode, limit: 80 })
      if (currentRoot.current !== root) return
      setRecoveryMessage(result.backupPath ? `Индекс восстановлен. Предыдущий кэш сохранён: ${result.backupPath}` : 'Индекс построен заново.')
      // Always run the current query after recovery, never display the query
      // captured before the user changed their text or filters.
      setRefresh((value) => value + 1)
    } catch (reason) {
      if (currentRoot.current === root) setRecoveryError(String(reason))
    } finally {
      recoveryActive.current = false
      setRecovering(false)
    }
  }

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
    <div className="local-search-sync">
      <span title="Внешние изменения поступают через watcher. Фоновая сверка запускается через минуту после предыдущей; пропущенные события могут появиться с задержкой.">{response?.syncing ? `Обновляю индекс${response.pendingSources ? ` · ${response.pendingSources}` : ''}` : response?.lastReconciledUnixMs ? `Сверка: ${new Date(response.lastReconciledUnixMs).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' })}` : 'Локальный индекс'}{response?.staleSources ? ` · скрыто устаревших: ${response.staleSources}` : ''}</span>
      <button aria-label="Сверить поисковый индекс" onClick={() => { void invoke('reconcile_search_index', { root }).then(() => setRefresh((value) => value + 1)).catch((reason: unknown) => setError(String(reason))) }}>Сверить</button>
    </div>
    {recovering && <p className="local-search-status" role="status">Восстанавливаю индекс из сохранённых файлов…</p>}
    {recoveryMessage && <p className="local-search-recovery-message" role="status">{recoveryMessage}</p>}
    {recoveryError && <p className="local-search-error local-search-recovery-error" role="alert">{recoveryError}</p>}
    <div className="sidebar-content" aria-busy={pending} data-completed-query={completedKey === requestKey ? query : undefined}>
      {!hasQuery ? <div className="local-search-help"><p>Текст заметок и карточек, названия файлов.</p><p>{mode === 'words' ? <>Фраза: <code>"план на неделю"</code></> : 'Фрагмент целиком: кавычки, переносы и повторные пробелы сохраняются.'}<br />Фильтры: <code>path:Проект</code> <code>tag:работа</code></p><p>Поиск локальный, по сохранённым файлам. «Точно» подходит для кода и частей слов.</p></div> : error && !pending ? <div className="local-search-recovery"><p className="local-search-error" role="alert">{error}</p><button onClick={() => { void rebuild() }} disabled={recovering}>Восстановить индекс</button><p>Только поисковый кэш. Его копия сохранится в .brandon; заметки и база задач не меняются.</p></div> : <>
        <p className="local-search-status" role="status">{pending ? 'Ищу…' : response ? `${resultCount(response.hits.length)}${response.syncing ? ' · индекс обновляется' : ''}` : ''}</p>
        {response && !pending && <>
          <div className="search-results">{response.hits.map((hit, index) => <button key={`${hit.path}:${hit.nodeId ?? ''}:${hit.line}`} data-path={hit.path} data-node-id={hit.nodeId ?? undefined} onClick={() => onOpen(hit)} onKeyDown={(event) => {
            const buttons = panel.current?.querySelectorAll<HTMLButtonElement>('.search-results button')
            if (event.key === 'Enter') { event.preventDefault(); onOpen(hit) }
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') { event.preventDefault(); buttons?.[Math.max(0, Math.min(response.hits.length - 1, index + (event.key === 'ArrowDown' ? 1 : -1)))]?.focus() }
            if (event.key === 'Escape') { event.preventDefault(); inputRef.current?.focus() }
          }}><strong>{hit.title}</strong><span>{hit.kind === 'canvas' && !hit.nodeId ? 'Холст' : labels[hit.kind]} · {hit.path}{hit.nodeId || !hit.contentIndexed ? '' : ` · строка ${hit.line}`}</span><p>{hit.snippet || (hit.contentIndexed ? 'Совпадение в названии' : 'Только название — содержимое не индексировалось')}</p></button>)}</div>
          <p className="local-search-coverage" title={`Обновлено: ${response.changedSources}; проверено источников при обновлении: ${response.scannedSources}; удалено из индекса: ${response.removedSources}; ${Math.round(response.elapsedMs)} мс`}>По содержимому: {response.indexedSources} · только имена: {response.nameOnlySources}{response.failedSources ? ` · ошибки: ${response.failedSources}` : ''}{response.skippedSources ? ` · пропущено: ${response.skippedSources}` : ''}</p>
        </>}
      </>}
    </div>
  </div>
}
