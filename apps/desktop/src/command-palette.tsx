import { useEffect, useMemo, useRef, useState } from 'react'
import { FileIcon, PlusIcon, SearchIcon } from './icons'
import type { VaultFile } from './types'

export type PaletteAction = { id: string; label: string; detail?: string; shortcut?: string; run: () => void }

export function CommandPalette({ mode, files, actions, onClose, onOpen }: {
  mode: 'commands' | 'notes'
  files: VaultFile[]
  actions: PaletteAction[]
  onClose: () => void
  onOpen: (path: string) => void
}) {
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const rows = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase('ru')
    if (mode === 'notes') return files
      .filter((file) => !normalized || file.path.toLocaleLowerCase('ru').includes(normalized))
      .slice(0, 15)
      .map((file) => ({ id: file.path, label: file.name.replace(/\.md$/i, ''), detail: file.folder || 'Корень vault', run: () => onOpen(file.path), kind: 'note' as const, shortcut: undefined }))
    return actions
      .filter((action) => !normalized || `${action.label} ${action.detail ?? ''}`.toLocaleLowerCase('ru').includes(normalized))
      .slice(0, 15)
      .map((action) => ({ ...action, kind: 'command' as const }))
  }, [actions, files, mode, onOpen, query])

  useEffect(() => inputRef.current?.focus(), [])
  useEffect(() => setSelected(0), [query])

  return <div className="palette-backdrop" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose() }}>
    <div className="palette" role="dialog" aria-label={mode === 'notes' ? 'Быстрый переход' : 'Палитра команд'}>
      <div className="palette-input"><SearchIcon /><input ref={inputRef} value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => {
        if (event.key === 'Escape') onClose()
        if (event.key === 'ArrowDown') { event.preventDefault(); setSelected((value) => Math.min(value + 1, Math.max(rows.length - 1, 0))) }
        if (event.key === 'ArrowUp') { event.preventDefault(); setSelected((value) => Math.max(value - 1, 0)) }
        if (event.key === 'Enter' && rows[selected]) { rows[selected].run(); onClose() }
      }} placeholder={mode === 'notes' ? 'Открыть заметку…' : 'Введите команду…'} /><kbd>Esc</kbd></div>
      <div className="palette-caption">{mode === 'notes' ? 'ЗАМЕТКИ' : 'КОМАНДЫ'}</div>
      <div className="palette-results">{rows.map((row, index) => <button key={row.id} className={index === selected ? 'selected' : ''} onMouseEnter={() => setSelected(index)} onClick={() => { row.run(); onClose() }}>
        <span className="palette-icon">{row.kind === 'note' ? <FileIcon /> : <PlusIcon />}</span><span><strong>{row.label}</strong>{row.detail && <small>{row.detail}</small>}</span>{row.shortcut && <kbd>{row.shortcut}</kbd>}
      </button>)}{!rows.length && <div className="palette-empty">Ничего не найдено</div>}</div>
      <footer><span><kbd>↑↓</kbd> выбрать</span><span><kbd>Enter</kbd> открыть</span></footer>
    </div>
  </div>
}
