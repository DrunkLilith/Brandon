import { invoke } from '@tauri-apps/api/core'
import { useEffect, useRef, useState } from 'react'
import { anchorLine, resolveNoteTarget } from './note-navigation'
import type { NoteDocument, VaultIndex } from './types'

export function NoteHoverPreview({ modifier, root, path, source, index }: {
  modifier: 'ctrl' | 'none'; root: string; path: string; source: string; index: VaultIndex | null
}) {
  const [preview, setPreview] = useState<{ x: number; y: number; path: string; text: string } | null>(null)
  const current = useRef({ source, index })
  const refresh = useRef<() => void>(() => {})
  current.current = { source, index }
  useEffect(() => {
    let target: HTMLElement | null = null, timer = 0, epoch = 0, control = false
    setPreview(null)
    function clear() { window.clearTimeout(timer); epoch++; setPreview(null) }
    function schedule() {
      clear()
      const element = target
      if (!element || modifier === 'ctrl' && !control) return
      const version = epoch
      timer = window.setTimeout(() => { void (async () => {
        const destination = element.dataset.noteTarget
        if (!destination) return
        const resolved = resolveNoteTarget(destination, path, current.current.index?.documents ?? [], element.dataset.noteKind === 'markdown' ? 'markdown' : 'wiki')
        if (resolved.status !== 'resolved') return
        try {
          const text = resolved.path === path ? current.current.source : (await invoke<NoteDocument>('read_note', { relativePath: resolved.path })).content
          if (version !== epoch || !element.isConnected) return
          const line = anchorLine(text, resolved) ?? 1
          const excerpt = text.split('\n').slice(Math.max(0, line - 1), line + 10).join('\n').slice(0, 650)
          const bounds = element.getBoundingClientRect()
          setPreview({ path: resolved.path, text: excerpt || 'Пустая заметка', x: Math.max(8, Math.min(bounds.left, window.innerWidth - 350)), y: Math.max(48, Math.min(bounds.bottom + 10, window.innerHeight - 245)) })
        } catch { if (version === epoch) setPreview(null) }
      })() }, 320)
    }
    refresh.current = schedule
    const over = (event: PointerEvent) => {
      const element = event.target instanceof Element ? event.target.closest<HTMLElement>('[data-note-target]') : null
      if (element === target) return
      target = element; control = event.ctrlKey || event.metaKey; schedule()
    }
    const out = (event: PointerEvent) => { if (target && !(event.relatedTarget instanceof Node && target.contains(event.relatedTarget))) { target = null; clear() } }
    const key = (event: KeyboardEvent) => {
      if (event.key === 'Escape') { target = null; clear(); return }
      const next = event.ctrlKey || event.metaKey
      if (next !== control) { control = next; schedule() }
    }
    document.addEventListener('pointerover', over); document.addEventListener('pointerout', out)
    window.addEventListener('keydown', key); window.addEventListener('keyup', key)
    const hide = () => { target = null; clear() }
    window.addEventListener('blur', hide); window.addEventListener('scroll', hide, true)
    return () => {
      window.clearTimeout(timer); epoch++
      refresh.current = () => {}
      document.removeEventListener('pointerover', over); document.removeEventListener('pointerout', out)
      window.removeEventListener('keydown', key); window.removeEventListener('keyup', key)
      window.removeEventListener('blur', hide); window.removeEventListener('scroll', hide, true)
    }
  }, [modifier, root, path])
  useEffect(() => { refresh.current() }, [source, index])
  return preview && <div className="note-hover-preview" role="tooltip" style={{ left: preview.x, top: preview.y }}><strong>{preview.path}</strong><pre>{preview.text}</pre></div>
}
