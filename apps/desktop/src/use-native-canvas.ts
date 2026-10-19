import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useRef, useState, type RefObject } from 'react'
import type { CanvasDocument, CanvasNode } from './canvas-view'
import {canvasAppearance} from './canvas-primitives'
import {samePreparedEdge,type PreparedCanvasEdge} from './canvas-edges'

export type NativeCanvasStats = {
  adapter: string; backend: string; totalNodes: number; visibleNodes: number; visibleEdges: number
  frameCpuMs: number; sceneUpdateMs: number; frames: number; textBuffers: number; surfaceRecoveries: number
  edgeLabels:number
}
type Inputs = { document: CanvasDocument; edges:PreparedCanvasEdge[]; camera: { x: number; y: number; zoom: number }; selected: Set<string>; selectedEdge:string|null; locked: Set<string>; overlayNodes:boolean; editing: string | null; linking: string | null }
type Session = { disposed: boolean; started: boolean; ready: boolean; dirty: boolean; raf: number; pending: Promise<void> | null; document: CanvasDocument | null; nodes: Map<string, CanvasNode>; order: string[]; edges: PreparedCanvasEdge[] | null; publishedAt: number }
let lifecycle: Promise<unknown> = Promise.resolve()

function renderNode(node: CanvasNode) {
  const appearance=canvasAppearance(node)
  return { id: node.id, x: node.x, y: node.y, width: node.width, height: node.height, kind: node.type, fill:appearance.fill,border:appearance.border,
    style:appearance.styled?{shape:appearance.shape,centerText:appearance.centerText,radius:appearance.radius,fontSize:appearance.fontSize,align:appearance.textAlign,textColor:appearance.text,strokeWidth:appearance.strokeWidth,bounds:appearance.bounds}:null,
    text: node.type === 'text' ? node.text ?? '' : node.type === 'file' ? `${node.file?.split('/').at(-1)?.replace(/\.md$/i, '') ?? 'Заметка'}\n${node.file ?? ''}` : node.label ?? node.url ?? 'Группа' }
}

export function useNativeCanvas(viewport: RefObject<HTMLDivElement | null>, inputs: Inputs, enabled = true) {
  const [failed, setFailed] = useState(false)
  const [active, setActive] = useState(false)
  const [starting, setStarting] = useState(true)
  const [error, setError] = useState('')
  const [stats, setStats] = useState<NativeCanvasStats | null>(null)
  const latest = useRef(inputs)
  const current = useRef<Session | null>(null)
  latest.current = inputs

  const draw = useCallback(async (session: Session) => {
    const element = viewport.current
    if (!element || session.disposed) return
    const value = latest.current
    if (session.document !== value.document) {
      const nextNodes = new Map(value.document.nodes.map((node) => [node.id, node]))
      const nodes = value.document.nodes.filter((node) => session.nodes.get(node.id) !== node)
      const removed = [...session.nodes.keys()].filter((id) => !nextNodes.has(id))
      const order = value.document.nodes.map((node) => node.id)
      const orderChanged = order.length !== session.order.length || order.some((id, index) => id !== session.order[index])
      const edgeOrderChanged=!session.edges||session.edges.length!==value.edges.length||value.edges.some((edge,i)=>edge.id!==session.edges![i].id),previousEdges=new Map(session.edges?.map(edge=>[edge.id,edge])),edgeUpdates=edgeOrderChanged?[]:value.edges.filter(edge=>!samePreparedEdge(previousEdges.get(edge.id),edge))
      if (nodes.length || removed.length || edgeOrderChanged || edgeUpdates.length || orderChanged) {
        await invoke('canvas_gpu_scene', { delta: { reset: session.edges === null, nodes: nodes.map(renderNode), removed, order: orderChanged ? order : null, edges: edgeOrderChanged?value.edges:null,edgeUpdates } })
        session.nodes = nextNodes; session.order = order; session.edges = value.edges
      }
      session.document = value.document
    }
    if (session.disposed) return
    const rect = element.getBoundingClientRect()
    const selection = [...value.selected]
    if (value.linking && !value.selected.has(value.linking)) selection.push(value.linking)
    const result = await invoke<NativeCanvasStats>('canvas_gpu_frame', { frame: { viewport: { x: rect.left, y: rect.top, width: rect.width, height: rect.height }, camera: value.camera, pixelRatio: window.devicePixelRatio, selected: selection, selectedEdge:value.selectedEdge, locked: selection.filter(id=>value.locked.has(id)), overlayNodes:value.overlayNodes, overlayText: value.document.nodes.length<=200&&value.camera.zoom>=.5, editing: value.editing } })
    if (!session.disposed) {
      element.dataset.frameCpuMs = String(result.frameCpuMs)
      element.dataset.nativeFrames = String(result.frames)
      if (performance.now() - session.publishedAt > 250 || !session.ready) { setStats(result); session.publishedAt = performance.now() }
    }
  }, [viewport])

  const schedule: () => void = useCallback(() => {
    const session = current.current
    if (!session || !session.ready || session.disposed) return
    session.dirty = true
    if (session.pending || session.raf) return
    session.raf = requestAnimationFrame(() => {
      session.raf = 0; session.dirty = false
      session.pending = draw(session).catch((reason: unknown) => {
        if (!session.disposed) { setError(String(reason)); setActive(false); setFailed(true) }
      }).finally(() => { session.pending = null; if (session.dirty && !session.disposed) schedule() })
    })
  }, [draw])

  useEffect(() => {
    if (!enabled || failed) { setActive(false); setStarting(false); return }
    const session: Session = { disposed: false, started: false, ready: false, dirty: false, raf: 0, pending: null, document: null, nodes: new Map(), order: [], edges: null, publishedAt: 0 }
    current.current = session
    setStarting(true); setError('')
    lifecycle = lifecycle.catch(() => {}).then(async () => {
      try {
        if (session.disposed) return
        const supported = await invoke<boolean>('canvas_gpu_supported')
        if (!supported || session.disposed) return
        await invoke('canvas_gpu_start')
        session.started = true
        if (session.disposed) return
        await draw(session)
        if (session.disposed) return
        session.ready = true
        document.documentElement.classList.add('native-canvas-active')
        setActive(true)
      } catch (reason) {
        if (!session.disposed) { setError(String(reason)); setActive(false); setFailed(true) }
      } finally { if (!session.disposed) setStarting(false) }
    })
    return () => {
      session.disposed = true; cancelAnimationFrame(session.raf)
      document.documentElement.classList.remove('native-canvas-active')
      lifecycle = lifecycle.catch(() => {}).then(async () => { await session.pending; if (session.started) await invoke('canvas_gpu_stop').catch(() => {}) })
      if (current.current === session) current.current = null
    }
  }, [draw, failed, enabled])

  useEffect(() => { schedule() }, [inputs.document, inputs.edges, inputs.camera, inputs.selected, inputs.selectedEdge, inputs.locked, inputs.overlayNodes, inputs.editing, inputs.linking, active, schedule])
  useEffect(() => {
    if (!active) return
    window.addEventListener('resize', schedule); window.addEventListener('focus', schedule)
    document.addEventListener('visibilitychange', schedule)
    const timer = window.setInterval(schedule, 1000)
    const observer = new ResizeObserver(schedule)
    if (viewport.current) observer.observe(viewport.current)
    return () => { window.removeEventListener('resize', schedule); window.removeEventListener('focus', schedule); document.removeEventListener('visibilitychange', schedule); clearInterval(timer); observer.disconnect() }
  }, [active, schedule, viewport])
  return { active, starting, error, stats }
}
