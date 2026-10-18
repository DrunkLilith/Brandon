import { useMemo, useState, type MouseEvent, type ReactNode } from 'react'
import { ChevronIcon, FileIcon, FolderIcon } from './icons'
import type { VaultEntry } from './types'

type FileTreeProps = {
  entries: VaultEntry[]
  activePath: string | null
  query: string
  onOpen: (path: string) => void
  onOpenResource: (path: string) => void
  onSelectFolder: (path: string) => void
  onContextMenu: (entry: VaultEntry, x: number, y: number) => void
}

export function FileTree({ entries, activePath, query, onOpen, onOpenResource, onSelectFolder, onContextMenu }: FileTreeProps) {
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set())
  const normalized = query.trim().toLocaleLowerCase('ru')
  const visible = useMemo(() => normalized
    ? entries.filter((entry) => entry.path.toLocaleLowerCase('ru').includes(normalized))
    : entries, [entries, normalized])

  const toggle = (path: string) => {
    setCollapsed((current) => {
      const next = new Set(current)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      return next
    })
  }

  const context = (event: MouseEvent, entry: VaultEntry) => {
    event.preventDefault()
    onContextMenu(entry, event.clientX, event.clientY)
  }

  if (!visible.length) return <div className="empty-filter">{query ? 'Совпадений в названиях нет' : 'Vault пока пуст'}</div>

  if (normalized) {
    return <div className="tree-list filtered">{visible.map((entry) => <TreeRow key={entry.path} entry={entry} depth={0} activePath={activePath} collapsed={false} onActivate={() => entry.isNote ? onOpen(entry.path) : entry.kind === 'folder' ? onSelectFolder(entry.path) : isWorkspaceResource(entry) ? onOpenResource(entry.path) : undefined} onContext={(event) => context(event, entry)} />)}</div>
  }

  const renderLevel = (parent: string, depth: number): ReactNode => visible
    .filter((entry) => entry.parent === parent)
    .map((entry) => {
      const isCollapsed = collapsed.has(entry.path)
      return <div key={entry.path}>
        <TreeRow entry={entry} depth={depth} activePath={activePath} collapsed={isCollapsed} onActivate={() => {
          if (entry.kind === 'folder') { toggle(entry.path); onSelectFolder(entry.path) }
          else if (entry.isNote) onOpen(entry.path)
          else if (isWorkspaceResource(entry)) onOpenResource(entry.path)
        }} onContext={(event) => context(event, entry)} />
        {entry.kind === 'folder' && !isCollapsed && renderLevel(entry.path, depth + 1)}
      </div>
    })

  return <div className="tree-list">{renderLevel('', 0)}</div>
}

function isWorkspaceResource(entry: VaultEntry) {
  return entry.kind === 'file' && (entry.extension === 'canvas' || entry.extension === 'base')
}

function TreeRow({ entry, depth, activePath, collapsed, onActivate, onContext }: {
  entry: VaultEntry
  depth: number
  activePath: string | null
  collapsed: boolean
  onActivate: () => void
  onContext: (event: MouseEvent) => void
}) {
  return <button draggable={entry.kind === 'file'} onDragStart={(event) => { event.dataTransfer.setData('application/x-brandon-vault-path', entry.path); event.dataTransfer.effectAllowed = 'copy' }} className={`tree-row ${entry.path === activePath ? 'active' : ''} ${entry.isNote || entry.kind === 'folder' ? '' : 'attachment'}`} style={{ paddingLeft: `${8 + depth * 14}px` }} onClick={onActivate} onContextMenu={onContext} title={entry.path}>
    {entry.kind === 'folder' ? <><ChevronIcon className={`tree-chevron ${collapsed ? 'collapsed' : ''}`} /><FolderIcon /></> : <><span className="tree-spacer" /><FileIcon /></>}
    <span>{entry.isNote ? entry.name.replace(/\.md$/i, '') : entry.name}</span>
  </button>
}
