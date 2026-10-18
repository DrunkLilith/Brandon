import { useEffect, useMemo, useRef, useState } from 'react'
import { PlusIcon } from './icons'

export type QuickInsert = {
  id: string
  label: string
  detail: string
  group: 'Текст' | 'Структура' | 'Связи'
  glyph: string
  value: string
  cursorOffset?: number
  block?: boolean
}

const INSERTS: QuickInsert[] = [
  { id: 'task', label: 'Задача', detail: 'Интерактивный чекбокс', group: 'Структура', glyph: '☑', value: '- [ ] ', cursorOffset: 6, block: true },
  { id: 'heading-1', label: 'Большой заголовок', detail: 'Заголовок первого уровня', group: 'Структура', glyph: 'H1', value: '# ', cursorOffset: 2, block: true },
  { id: 'heading-2', label: 'Заголовок раздела', detail: 'Заголовок второго уровня', group: 'Структура', glyph: 'H2', value: '## ', cursorOffset: 3, block: true },
  { id: 'bullet', label: 'Маркированный список', detail: 'Обычный список', group: 'Структура', glyph: '•', value: '- ', cursorOffset: 2, block: true },
  { id: 'numbered', label: 'Нумерованный список', detail: 'Последовательность шагов', group: 'Структура', glyph: '1.', value: '1. ', cursorOffset: 3, block: true },
  { id: 'quote', label: 'Цитата', detail: 'Выносной абзац', group: 'Текст', glyph: '❯', value: '> ', cursorOffset: 2, block: true },
  { id: 'callout', label: 'Callout', detail: 'Заметный информационный блок', group: 'Текст', glyph: 'i', value: '> [!NOTE]\n> ', cursorOffset: 12, block: true },
  { id: 'bold', label: 'Жирный текст', detail: 'Сильное выделение', group: 'Текст', glyph: 'B', value: '****', cursorOffset: 2 },
  { id: 'italic', label: 'Курсив', detail: 'Мягкое выделение', group: 'Текст', glyph: 'I', value: '__', cursorOffset: 1 },
  { id: 'inline-code', label: 'Код в строке', detail: 'Код с оформлением · выделенный текст или новый фрагмент', group: 'Текст', glyph: '</>', value: '`код`', cursorOffset: 1 },
  { id: 'code-block', label: 'Блок кода', detail: 'Многострочный фрагмент', group: 'Текст', glyph: '{ }', value: '```\n\n```', cursorOffset: 4, block: true },
  { id: 'table', label: 'Таблица', detail: 'Markdown-таблица с табличным режимом', group: 'Структура', glyph: '▦', value: '| Столбец | Столбец |\n| --- | --- |\n| Значение | Значение |\n', block: true },
  { id: 'divider', label: 'Разделитель', detail: 'Горизонтальная линия', group: 'Структура', glyph: '—', value: '---', cursorOffset: 3, block: true },
  { id: 'wiki-link', label: 'Ссылка на заметку', detail: 'Внутренняя ссылка с подсказками', group: 'Связи', glyph: '↗', value: '[[]]', cursorOffset: 2 },
  { id: 'link', label: 'Ссылка на сайт', detail: 'Текст и внешний адрес', group: 'Связи', glyph: '🔗', value: '[](https://)', cursorOffset: 1 },
  { id: 'embed', label: 'Вложение или заметка', detail: 'Изображение, файл или embed', group: 'Связи', glyph: '⊕', value: '![[]]', cursorOffset: 3 },
]

export function QuickInsertPalette({ onClose, onInsert }: { onClose: () => void; onInsert: (item: QuickInsert) => void }) {
  const [query, setQuery] = useState('')
  const [selected, setSelected] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const rows = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase('ru')
    return INSERTS.filter((item) => !normalized || `${item.label} ${item.detail} ${item.group}`.toLocaleLowerCase('ru').includes(normalized))
  }, [query])

  useEffect(() => inputRef.current?.focus(), [])
  useEffect(() => setSelected(0), [query])

  const choose = (item: QuickInsert | undefined) => {
    if (!item) return
    onInsert(item)
    onClose()
  }

  return <div className="palette-backdrop quick-insert-backdrop" onMouseDown={(event) => { if (event.currentTarget === event.target) onClose() }}>
    <div className="palette quick-insert-palette" role="dialog" aria-label="Быстрая вставка">
      <div className="palette-input"><PlusIcon /><input ref={inputRef} value={query} onChange={(event) => setQuery(event.target.value)} onKeyDown={(event) => {
        if (event.key === 'Escape') onClose()
        if (event.key === 'ArrowDown') { event.preventDefault(); setSelected((value) => Math.min(value + 1, Math.max(rows.length - 1, 0))) }
        if (event.key === 'ArrowUp') { event.preventDefault(); setSelected((value) => Math.max(value - 1, 0)) }
        if (event.key === 'Enter') { event.preventDefault(); choose(rows[selected]) }
      }} placeholder="Что вставить?" /><kbd>Esc</kbd></div>
      <div className="palette-caption">БЫСТРАЯ ВСТАВКА</div>
      <div className="quick-insert-grid">{rows.map((item, index) => <button key={item.id} className={index === selected ? 'selected' : ''} onMouseEnter={() => setSelected(index)} onClick={() => choose(item)}>
        <span className="quick-insert-glyph">{item.glyph}</span><span><strong>{item.label}</strong><small>{item.detail}</small></span><em>{item.group}</em>
      </button>)}{rows.length === 0 && <div className="palette-empty">Такой вставки пока нет</div>}</div>
      <footer><span><kbd>↑↓</kbd> выбрать</span><span><kbd>Enter</kbd> вставить</span><span><kbd>Ctrl /</kbd> открыть</span></footer>
    </div>
  </div>
}
