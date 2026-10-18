import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow'
import { open } from '@tauri-apps/plugin-dialog'
import { useEffect, useRef, useState, type RefObject } from 'react'
import { FileIcon } from './icons'

export type AttachmentInfo = { path: string; name: string; kind: 'image' | 'audio' | 'video' | 'pdf' | 'file'; sizeBytes: number; modifiedUnixMs: number }
type Progress = { id: string; name: string; copied: number; total: number }
type Point = { x: number; y: number }
export const isRasterPath = (path: string | undefined) => Boolean(path && /\.(png|jpe?g|gif|webp)$/i.test(path))

export function useCanvasAttachments(viewport: RefObject<HTMLDivElement | null>, onImported: (attachment: AttachmentInfo, position?: Point) => void) {
  const [progress, setProgress] = useState<Progress | null>(null)
  const [error, setError] = useState('')
  const [dragOver, setDragOver] = useState(false)
  const alive = useRef(true), activeId = useRef<string | null>(null), stopped = useRef(false), busy = useRef(false)
  const imported = useRef(onImported); imported.current = onImported
  const importFiles = async (paths: string[], position?: Point) => {
    if (busy.current || !paths.length) return
    if (paths.length > 32) { setError('Добавляйте до 32 файлов за один раз.'); return }
    busy.current = true; stopped.current = false; setError(''); setDragOver(false)
    try {
      const origin = position ?? { x: (viewport.current?.clientWidth ?? 600) / 2, y: (viewport.current?.clientHeight ?? 400) / 2 }
      for (const [index, sourcePath] of paths.entries()) {
        if (stopped.current || !alive.current) break
        const id = crypto.randomUUID(); activeId.current = id
        setProgress({ id, name: sourcePath.split(/[\\/]/).at(-1) ?? 'Файл', copied: 0, total: 0 })
        const result = await invoke<{ attachment: AttachmentInfo; copied: boolean }>('import_attachment', { sourcePath, requestId: id })
        if (alive.current) imported.current(result.attachment, { x: origin.x + index * 28, y: origin.y + index * 28 })
      }
    } catch (reason) { if (alive.current && !stopped.current) setError(String(reason)) }
    finally { activeId.current = null; busy.current = false; if (alive.current) setProgress(null) }
  }
  const importRef = useRef(importFiles); importRef.current = importFiles
  const cancel = () => { stopped.current = true; if (activeId.current) void invoke('cancel_attachment_import', { requestId: activeId.current }) }
  useEffect(() => {
    alive.current = true
    let disposed = false
    const unlisteners: (() => void)[] = []
    const retain = (unlisten: () => void) => { if (!disposed) unlisteners.push(unlisten); else unlisten() }
    void listen<{ id: string; copied: number; total: number }>('attachment-import-progress', ({ payload }) => {
      if (!disposed && payload.id === activeId.current) setProgress((current) => current ? { ...current, ...payload } : current)
    }).then(retain)
    void getCurrentWebviewWindow().onDragDropEvent(({ payload }) => {
      if (disposed) return
      if (payload.type === 'leave') { setDragOver(false); return }
      const element = viewport.current
      if (!element) return
      const rect = element.getBoundingClientRect(), x = payload.position.x / window.devicePixelRatio, y = payload.position.y / window.devicePixelRatio
      const inside = x >= rect.left && y >= rect.top && x <= rect.right && y <= rect.bottom
      setDragOver(inside && payload.type !== 'drop')
      if (inside && payload.type === 'drop') void importRef.current(payload.paths, { x: x - rect.left, y: y - rect.top })
    }).then(retain)
    return () => { disposed = true; alive.current = false; stopped.current = true; if (activeId.current) void invoke('cancel_attachment_import', { requestId: activeId.current }); unlisteners.forEach((unlisten) => unlisten()) }
  }, [viewport])
  const choose = async () => {
    try { const selected = await open({ multiple: true, directory: false, title: 'Добавить файлы на холст' }); if (selected && alive.current) await importFiles(Array.isArray(selected) ? selected : [selected]) }
    catch (reason) { if (alive.current) setError(String(reason)) }
  }
  const referenceFile = async (path: string, position: Point) => {
    try { const attachment = await invoke<AttachmentInfo>('attachment_info', { relativePath: path }); if (alive.current) imported.current(attachment, position) }
    catch (reason) { if (alive.current) setError(String(reason)) }
  }
  return { progress, error, dragOver, choose, cancel, referenceFile, dismissError: () => setError('') }
}

export function AttachmentImage({ path, large = false }: { path: string; large?: boolean }) {
  const [source, setSource] = useState(''), [error, setError] = useState('')
  useEffect(() => {
    let active = true, url = ''
    setSource(''); setError('')
    void invoke<ArrayBuffer>('attachment_thumbnail', { relativePath: path, large }).then((bytes) => {
      url = URL.createObjectURL(new Blob([bytes], { type: 'image/png' }))
      if (active) setSource(url); else URL.revokeObjectURL(url)
    }).catch((reason: unknown) => { if (active) setError(String(reason)) })
    return () => { active = false; if (url) URL.revokeObjectURL(url) }
  }, [path, large])
  return source ? <img className="canvas-image-preview" src={source} alt={path.split('/').at(-1)} draggable={false} /> : <span className="canvas-preview-placeholder" title={error}><FileIcon />{error ? 'Превью недоступно' : 'Загружаю изображение…'}</span>
}

export function AttachmentDetails({ path, onClose }: { path: string; onClose: () => void }) {
  const [info, setInfo] = useState<AttachmentInfo | null>(null), [error, setError] = useState('')
  useEffect(() => { let active = true; setInfo(null); setError(''); void invoke<AttachmentInfo>('attachment_info', { relativePath: path }).then((value) => { if (active) setInfo(value) }).catch((reason: unknown) => { if (active) setError(String(reason)) }); return () => { active = false } }, [path])
  if (info?.kind === 'image') return <ImageViewer path={path} onClose={onClose} />
  return <aside className="canvas-attachment-details" onPointerDown={(event) => event.stopPropagation()} onKeyDown={(event) => { if (event.key !== 'Escape') event.stopPropagation() }}>
    <header><strong>Вложение</strong><button onClick={onClose} aria-label="Закрыть вложение">×</button></header>
    <h3>{info?.name ?? path.split('/').at(-1)}</h3><p className="attachment-path">{path}</p>
    {info && <p>{new Intl.NumberFormat('ru', { style: 'unit', unit: 'megabyte', maximumFractionDigits: 2 }).format(info.sizeBytes / 1024 / 1024)} · {{ image: 'Изображение', video: 'Видео', audio: 'Аудио', pdf: 'PDF', file: 'Файл' }[info.kind]}</p>}
    {error && <p role="alert">{error}</p>}
    {info && <><button className="secondary" onClick={() => { void invoke('reveal_entry', { relativePath: path }).catch((reason: unknown) => setError(String(reason))) }}>Показать в папке</button><small>Оригинал хранится в vault. Удаление карточки не удаляет файл. Проигрыватель видео и PDF-просмотр появятся следующим этапом.</small></>}
  </aside>
}

function ImageViewer({path,onClose}: {path:string;onClose:()=>void}) {
  const [zoom,setZoom]=useState(1)
  const ref=useRef<HTMLDivElement>(null)
  useEffect(() => { const previous=document.activeElement as HTMLElement | null; ref.current?.querySelector<HTMLButtonElement>('button')?.focus(); return () => { if(previous?.isConnected) previous.focus() } },[])
  return <div className="image-viewer-backdrop" onPointerDown={event=>{event.stopPropagation();if(event.target===event.currentTarget)onClose()}}><div ref={ref} className="image-viewer" role="dialog" aria-modal="true" aria-label="Просмотр изображения" onKeyDown={event=>{
    event.stopPropagation()
    if(event.key==='Escape'){event.preventDefault();onClose()}
    if(event.key==='Tab'){const buttons=Array.from(ref.current?.querySelectorAll<HTMLButtonElement>('button')??[]);const index=buttons.indexOf(document.activeElement as HTMLButtonElement);event.preventDefault();buttons[(index+(event.shiftKey?-1:1)+buttons.length)%buttons.length]?.focus()}
  }}><header><div><strong>{path.split('/').at(-1)}</strong><small>Локальное изображение · предпросмотр до 2560 px</small></div><nav><button aria-label="Уменьшить изображение" onClick={()=>setZoom(z=>Math.max(.5,z-.25))}>−</button><button onClick={()=>setZoom(1)}>{Math.round(zoom*100)}%</button><button aria-label="Увеличить изображение" onClick={()=>setZoom(z=>Math.min(3,z+.25))}>+</button><button aria-label="Закрыть вложение" onClick={onClose}>×</button></nav></header><div className={`image-viewer-stage ${zoom===1?'fit':''}`}><div style={{width:`${zoom*100}%`}}><AttachmentImage path={path} large /></div></div></div></div>
}
