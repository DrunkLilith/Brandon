import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useRef } from 'react'
import type { ViewMode } from './types'

export type EditorPosition = { anchor: number; head: number; top: number; left: number }
export type TabView = { mode?: ViewMode; editor?: EditorPosition; previewTop?: number; camera?: { x: number; y: number; zoom: number } }
export type PaneState = { tabs: string[]; active: string | null; pinned: string[]; views: Record<string, TabView> }
export type Bookmark = { id: string; label: string; path: string; heading?: string | null; block?: string | null; query?: string }
export function isDocumentBookmark(bookmark: Bookmark, path: string): boolean {
  return bookmark.path === path && !bookmark.heading && !bookmark.block && !bookmark.query
}
export type WorkspaceSession = { version: 1; main: PaneState; side: PaneState; split: 'none' | 'vertical' | 'horizontal'; ratio: number; sidebarOpen: boolean; rightOpen: boolean; bookmarks: Bookmark[] }
export type CrashDraft = { path: string; pane: 'main' | 'side'; version: string; content: string; baseContent: string; fingerprint: string; updated: number }
export type WorkspaceData = { session: unknown; drafts: CrashDraft[] }
export const emptyPane = (): PaneState => ({ tabs: [], active: null, pinned: [], views: {} })
export const emptySession = (): WorkspaceSession => ({ version: 1, main: emptyPane(), side: emptyPane(), split: 'none', ratio: .5, sidebarOpen: true, rightOpen: false, bookmarks: [] })
export function validResource(path: unknown): path is string {
  return typeof path === 'string' && path.length > 0 && path.length <= 2048 && !/^[\/]|[\\:\0]/.test(path) && path.split('/').every(part => part && part !== '.' && part !== '..') && /\.(md|canvas|base)$/i.test(path)
}
export function validTab(path: unknown): path is string { return validResource(path) || typeof path === 'string' && /^(?:brandon:\/\/(?:graph|planner)|brandon:\/\/new\/[a-z0-9-]+)$/.test(path) }
export function parseSession(raw: unknown): WorkspaceSession {
  const result = emptySession()
  if (!raw || typeof raw !== 'object' || !('version' in raw) || raw.version !== 1) return result
  const value = raw as Record<string, unknown>
  for (const key of ['main', 'side'] as const) {
    if (!value[key] || typeof value[key] !== 'object') continue
    const pane = value[key] as Record<string, unknown>, tabs = Array.isArray(pane.tabs) ? [...new Set(pane.tabs.filter(validTab))].slice(0, 80) : []
    result[key].tabs = tabs
    result[key].active = typeof pane.active === 'string' && tabs.includes(pane.active) ? pane.active : tabs[0] ?? null
    result[key].pinned = Array.isArray(pane.pinned) ? pane.pinned.filter((path): path is string => typeof path === 'string' && tabs.includes(path)) : []
    if (pane.views && typeof pane.views === 'object') for (const [path, item] of Object.entries(pane.views).slice(0, 100)) {
      if (!tabs.includes(path) || !item || typeof item !== 'object') continue
      const view: TabView = {}, data = item as Record<string, unknown>
      if (['edit', 'split', 'preview'].includes(String(data.mode))) view.mode = data.mode as ViewMode
      if (typeof data.previewTop === 'number' && Number.isFinite(data.previewTop)) view.previewTop = Math.max(0, data.previewTop)
      if (data.editor && typeof data.editor === 'object') {
        const e = data.editor as EditorPosition
        if ([e.anchor,e.head,e.top,e.left].every(n => Number.isFinite(n) && n >= 0)) view.editor = { anchor:e.anchor,head:e.head,top:e.top,left:e.left }
      }
      if (data.camera && typeof data.camera === 'object') {
        const c = data.camera as {x:number;y:number;zoom:number}
        if ([c.x,c.y,c.zoom].every(Number.isFinite) && c.zoom >= .05 && c.zoom <= 5) view.camera = {x:c.x,y:c.y,zoom:c.zoom}
      }
      result[key].views[path] = view
    }
  }
  if (value.split === 'vertical' || value.split === 'horizontal') result.split = value.split
  if (typeof value.ratio === 'number' && Number.isFinite(value.ratio)) result.ratio = Math.min(.75,Math.max(.25,value.ratio))
  for (const key of ['sidebarOpen','rightOpen'] as const) if (typeof value[key] === 'boolean') result[key] = value[key]
  if (Array.isArray(value.bookmarks)) result.bookmarks = value.bookmarks.filter((b): b is Bookmark => b && typeof b === 'object' && typeof b.id === 'string' && b.id.length <= 80 && typeof b.label === 'string' && b.label.length <= 300 && (validResource(b.path) || b.path === '' && typeof b.query === 'string')).slice(0,200).map(b => ({id:b.id,label:b.label,path:b.path,heading:typeof b.heading === 'string' ? b.heading.slice(0,500) : null,block:typeof b.block === 'string' ? b.block.slice(0,500) : null,query:typeof b.query === 'string' ? b.query.slice(0,2000) : undefined}))
  return result
}

let writes: Promise<unknown> = Promise.resolve()
export function workspaceWrite(command: string, args: Record<string, unknown>): Promise<void> {
  const next = writes.catch(() => {}).then(() => invoke<void>(command,args))
  writes = next
  return next
}
export async function flushWorkspaceWrites() { await writes }

export function useCrashDraft(root: string | undefined, pane: 'main' | 'side', path: string | undefined, content: string, baseContent: string, fingerprint: string | undefined, enabled: boolean, onError: (message: string) => void) {
  const last = useRef<{root:string;path:string;version:string;content:string} | null>(null)
  const transfers=useRef(new Map<string,{root:string;path:string;version:string}>())
  const report = useRef(onError); report.current = onError
  const clearTransfer=useCallback((savedPath:string)=>{
    const previous=transfers.current.get(savedPath)
    if(!previous||previous.root!==root)return
    transfers.current.delete(savedPath)
    void workspaceWrite('remove_draft',{root,path:previous.path,pane,version:previous.version}).catch(error=>report.current(String(error)))
  },[root,pane])
  useEffect(() => {
    if (!root || !path || !fingerprint || !enabled) return
    if (content !== baseContent) {
      if (last.current?.root === root && last.current.path === path && last.current.content === content) return
      const version = crypto.randomUUID()
      last.current = {root,path,version,content}
      void workspaceWrite('put_draft',{root,draft:{path,pane,version,content,baseContent,fingerprint,updated:0}}).catch(error => { last.current = null; report.current(`Не удалось сохранить аварийный черновик: ${String(error)}`) })
    } else {
      clearTransfer(path)
      if(last.current?.root===root&&last.current.path===path){const previous=last.current;last.current=null;void workspaceWrite('remove_draft',{root,path,pane,version:previous.version}).catch(error=>report.current(`Не удалось очистить сохранённый черновик: ${String(error)}`))}
    }
  }, [root,pane,path,content,baseContent,fingerprint,enabled,clearTransfer])
  const acknowledge=useCallback((savedPath:string,submitted:string)=>{
    clearTransfer(savedPath)
    const previous=last.current
    if(!previous||previous.root!==root||previous.path!==savedPath||previous.content!==submitted)return
    last.current=null
    void workspaceWrite('remove_draft',{root,path:savedPath,pane,version:previous.version}).catch(error=>report.current(String(error)))
  },[root,pane,clearTransfer])
  const transfer=useCallback((from:string,to:string)=>{const previous=last.current;if(previous&&previous.root===root&&previous.path===from)transfers.current.set(to,{root:previous.root,path:from,version:previous.version})},[root])
  return {acknowledge,transfer}
}

export function remapPane(pane: PaneState, from: string, to: string): PaneState {
  const remap = (path: string) => path === from || path.startsWith(`${from}/`) ? to + path.slice(from.length) : path
  return { tabs:[...new Set(pane.tabs.map(remap))],active:pane.active ? remap(pane.active) : null,pinned:pane.pinned.map(remap),views:Object.fromEntries(Object.entries(pane.views).map(([path,view]) => [remap(path),view])) }
}
