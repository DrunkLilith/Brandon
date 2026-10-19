import { useMemo, useState, type MouseEvent } from 'react'
import { useVirtualRows } from './virtual-rows'
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
const collator=new Intl.Collator('ru',{numeric:true,sensitivity:'base'})

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

  const rows=useMemo(()=>{
    if(normalized)return visible.map(entry=>({entry,depth:0}))
    const children=new Map<string,VaultEntry[]>()
    for(const entry of visible){const group=children.get(entry.parent)??[];group.push(entry);children.set(entry.parent,group)}
    for(const group of children.values())group.sort((a,b)=>Number(b.kind==='folder')-Number(a.kind==='folder')||collator.compare(a.name,b.name))
    const result:{entry:VaultEntry;depth:number}[]=[],stack=(children.get('')??[]).toReversed().map(entry=>({entry,depth:0}))
    while(stack.length){const row=stack.pop()!;result.push(row);if(row.entry.kind==='folder'&&!collapsed.has(row.entry.path))for(const entry of (children.get(row.entry.path)??[]).toReversed())stack.push({entry,depth:row.depth+1})}
    return result
  },[visible,collapsed,normalized])
  const window=useVirtualRows(rows.length,36,true)
  if (!visible.length) return <div className="empty-filter">{query ? 'Совпадений в названиях нет' : 'Vault пока пуст'}</div>
  return <div ref={window.ref} className={`tree-list ${normalized?'filtered':''}`} data-total-rows={rows.length} onKeyDown={event=>{
    if(!['ArrowDown','ArrowUp','Home','End'].includes(event.key))return
    const path=(event.target as HTMLElement).closest<HTMLButtonElement>('.tree-row')?.title;if(!path)return
    event.preventDefault();const index=rows.findIndex(row=>row.entry.path===path),next=event.key==='Home'?0:event.key==='End'?rows.length-1:Math.max(0,Math.min(rows.length-1,index+(event.key==='ArrowDown'?1:-1)))
    window.reveal(next);requestAnimationFrame(()=>requestAnimationFrame(()=>window.ref.current?.querySelectorAll<HTMLButtonElement>('.tree-row').forEach(button=>{if(button.title===rows[next]?.entry.path)button.focus()})))
  }}><div aria-hidden="true" style={{height:window.top}}/>{rows.slice(window.start,window.end).map(({entry,depth})=><TreeRow key={entry.path} entry={entry} depth={depth} activePath={activePath} collapsed={collapsed.has(entry.path)} onActivate={()=>{if(entry.kind==='folder'){toggle(entry.path);onSelectFolder(entry.path)}else if(entry.isNote)onOpen(entry.path);else if(isWorkspaceResource(entry))onOpenResource(entry.path)}} onContext={event=>context(event,entry)}/>)}<div aria-hidden="true" style={{height:window.bottom}}/></div>
}

function isWorkspaceResource(entry: VaultEntry) {
  return entry.kind === 'file'
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
