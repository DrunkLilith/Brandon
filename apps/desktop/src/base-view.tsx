import { useMemo, useState } from 'react'
import { FileIcon, SearchIcon } from './icons'
import { useVirtualRows } from './virtual-rows'
import type { DocumentMetadata, VaultIndex } from './types'
const collator=new Intl.Collator('ru',{numeric:true})

function displayValue(value: unknown) {
  if (value === undefined || value === null) return '—'
  if (Array.isArray(value)) return value.join(', ')
  if (typeof value === 'object') return JSON.stringify(value)
  if (typeof value === 'boolean') return value ? 'Да' : 'Нет'
  return String(value)
}

export function BaseView({ index, onOpen }: { index: VaultIndex | null; onOpen: (path: string) => void }) {
  const [query, setQuery] = useState('')
  const [sort, setSort] = useState('title')
  const [descending, setDescending] = useState(false)
  const columns = useMemo(() => {
    const frequency = new Map<string, number>()
    for (const document of index?.documents ?? []) for (const key of Object.keys(document.properties)) frequency.set(key, (frequency.get(key) ?? 0) + 1)
    return [...frequency].sort((left, right) => right[1] - left[1]).slice(0, 5).map(([key]) => key)
  }, [index])
  const rows = useMemo(() => {
    const normalized = query.trim().toLocaleLowerCase('ru')
    const result = (index?.documents ?? []).filter((document) => !normalized || `${document.title} ${document.path} ${document.tags.join(' ')} ${Object.values(document.properties).map(displayValue).join(' ')}`.toLocaleLowerCase('ru').includes(normalized))
    const value = (document: DocumentMetadata) => sort === 'title' ? document.title : sort === 'path' ? document.path : displayValue(document.properties[sort])
    return result.sort((left, right) => collator.compare(value(left),value(right)) * (descending ? -1 : 1))
  }, [descending, index, query, sort])
  const window=useVirtualRows(rows.length,47)
  const toggleSort = (key: string) => {
    if (sort === key) setDescending((value) => !value)
    else { setSort(key); setDescending(false) }
  }

  return <div className="base-view">
    <div className="base-toolbar"><label><SearchIcon /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Фильтр записей…" /></label><span>{rows.length} записей</span></div>
    <div ref={window.ref} className="base-table-wrap" data-total-rows={rows.length}><table className="base-table"><thead><tr><Sortable label="Имя" value="title" active={sort} descending={descending} onSort={toggleSort} /><Sortable label="Путь" value="path" active={sort} descending={descending} onSort={toggleSort} />{columns.map((column) => <Sortable key={column} label={column} value={column} active={sort} descending={descending} onSort={toggleSort} />)}<th>Теги</th></tr></thead><tbody>{window.top>0&&<tr aria-hidden="true"><td colSpan={columns.length+3} style={{height:window.top,padding:0,border:0}}/></tr>}{rows.slice(window.start,window.end).map((document) => <tr key={document.path} onDoubleClick={() => onOpen(document.path)}><td><button onClick={() => onOpen(document.path)}><FileIcon /><span>{document.title}</span></button></td><td className="base-path">{document.path}</td>{columns.map((column) => <td key={column}>{displayValue(document.properties[column])}</td>)}<td>{document.tags.map((tag) => <span className="base-tag" key={tag}>#{tag}</span>)}</td></tr>)}{window.bottom>0&&<tr aria-hidden="true"><td colSpan={columns.length+3} style={{height:window.bottom,padding:0,border:0}}/></tr>}</tbody></table>{!rows.length && <div className="base-empty">Нет заметок, подходящих под фильтр.</div>}</div>
  </div>
}

function Sortable({ label, value, active, descending, onSort }: { label: string; value: string; active: string; descending: boolean; onSort: (value: string) => void }) {
  return <th><button onClick={() => onSort(value)}>{label}{active === value && <span>{descending ? '↓' : '↑'}</span>}</button></th>
}
