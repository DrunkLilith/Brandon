import { invoke } from '@tauri-apps/api/core'
import { useCallback, useEffect, useRef, useState, type RefObject } from 'react'
import type { CanvasDocument, CanvasNode } from './canvas-view'
import {canvasAppearance} from './canvas-primitives'
import {samePreparedEdge,type PreparedCanvasEdge} from './canvas-edges'
import {listen} from '@tauri-apps/api/event'
import {canvasRasterLayout,isCanvasRaster,changedRasterPaths,type RasterDelta} from './canvas-raster'
import {prepareCanvasInk} from './canvas-ink'

export type NativeCanvasStats = {
  adapter: string; backend: string; totalNodes: number; visibleNodes: number; visibleEdges: number
  frameCpuMs: number; sceneUpdateMs: number; frames: number; textBuffers: number; surfaceRecoveries: number
  edgeLabels:number
  drawBatches:number;translucentBatches:number
  rasterImages:number;rasterBytes:number;rasterLimited:number;missingImages:string[];rasterPaths:string[]
  inkStrokes:number;inkSegments:number;inkMaskBytes:number
}
type Inputs = { root:string;document: CanvasDocument; labels:ReadonlyMap<string,string>;edges:PreparedCanvasEdge[]; camera: { x: number; y: number; zoom: number }; selected: Set<string>; selectedEdge:string|null; locked: Set<string>; overlayNodes:boolean; editing: string | null; linking: string | null }
type Session = { id:string;root:string;labels:ReadonlyMap<string,string>|null;imagePending:Set<string>;imageDirty:Set<string>;imageVersions:Map<string,number>;imageFailed:Map<string,number>;disposed: boolean; started: boolean; ready: boolean; dirty: boolean; raf: number; pending: Promise<void> | null; document: CanvasDocument | null; nodes: Map<string, CanvasNode>; order: string[]; edges: PreparedCanvasEdge[] | null; publishedAt: number }
let lifecycle: Promise<unknown> = Promise.resolve()

function renderNode(node: CanvasNode,label?:string) {
  const appearance=canvasAppearance(node),raster=isCanvasRaster(node)?canvasRasterLayout(node):null,ink=prepareCanvasInk(node,appearance.border)
  if(ink)return{id:node.id,x:node.x,y:node.y,width:node.width,height:node.height,kind:node.type,fill:ink.color+Math.round(ink.opacity*255).toString(16).padStart(2,'0'),border:appearance.border,style:null,image:null,ink,text:''}
  return { id: node.id, x: node.x, y: node.y, width: node.width, height: node.height, kind: node.type, fill:appearance.fill,border:appearance.border,
    style:raster?{shape:0,centerText:false,radius:8,fontSize:raster.fontSize,align:appearance.textAlign,textColor:appearance.text,strokeWidth:appearance.strokeWidth,bounds:raster.footer}:appearance.styled?{shape:appearance.shape,centerText:appearance.centerText,radius:appearance.radius,fontSize:appearance.fontSize,align:appearance.textAlign,textColor:appearance.text,strokeWidth:appearance.strokeWidth,bounds:appearance.bounds}:null,
    image:raster?{path:node.file,bounds:raster.image}:null,
    text: label??(raster?node.file?.split('/').at(-1)??'Изображение':node.type === 'text' ? node.text ?? '' : node.type === 'file' ? `${node.file?.split('/').at(-1)?.replace(/\.md$/i, '') ?? 'Заметка'}\n${node.file ?? ''}` : node.label ?? node.url ?? 'Группа') }
}

export function useNativeCanvas(viewport: RefObject<HTMLDivElement | null>, inputs: Inputs, enabled = true) {
  const [failed, setFailed] = useState(false)
  const [active, setActive] = useState(false)
  const [starting, setStarting] = useState(true)
  const [error, setError] = useState('')
  const [stats, setStats] = useState<NativeCanvasStats | null>(null)
  const [imageErrors,setImageErrors]=useState(0)
  const scheduleRef=useRef<()=>void>(()=>{})
  const latest = useRef(inputs)
  const current = useRef<Session | null>(null)
  latest.current = inputs

  const draw = useCallback(async (session: Session) => {
    const element = viewport.current
    if (!element || session.disposed) return
    const value = latest.current
    if(value.root!==session.root)return
    if (session.document !== value.document||session.labels!==value.labels) {
      const references=new Set(value.document.nodes.filter(isCanvasRaster).map(node=>node.file!))
      for(const path of session.imageFailed.keys())if(!references.has(path))session.imageFailed.delete(path)
      for(const path of session.imageDirty)if(!references.has(path))session.imageDirty.delete(path)
      for(const path of session.imageVersions.keys())if(!references.has(path)&&!session.imagePending.has(path))session.imageVersions.delete(path)
      setImageErrors(session.imageFailed.size)
      const nextNodes = new Map(value.document.nodes.map((node) => [node.id, node]))
      const nodes = value.document.nodes.filter((node) => session.nodes.get(node.id) !== node||session.labels?.get(node.id)!==value.labels.get(node.id))
      const removed = [...session.nodes.keys()].filter((id) => !nextNodes.has(id))
      const order = value.document.nodes.map((node) => node.id)
      const orderChanged = order.length !== session.order.length || order.some((id, index) => id !== session.order[index])
      const edgeOrderChanged=!session.edges||session.edges.length!==value.edges.length||value.edges.some((edge,i)=>edge.id!==session.edges![i].id),previousEdges=new Map(session.edges?.map(edge=>[edge.id,edge])),edgeUpdates=edgeOrderChanged?[]:value.edges.filter(edge=>!samePreparedEdge(previousEdges.get(edge.id),edge))
      if (nodes.length || removed.length || edgeOrderChanged || edgeUpdates.length || orderChanged) {
        await invoke('canvas_gpu_scene', { sessionId:session.id,delta: { reset: session.edges === null, nodes: nodes.map(node=>renderNode(node,value.labels.get(node.id))), removed, order: orderChanged ? order : null, edges: edgeOrderChanged?value.edges:null,edgeUpdates } })
        session.nodes = nextNodes; session.order = order; session.edges = value.edges
      }
      session.document = value.document
      session.labels=value.labels
    }
    if (session.disposed) return
    const rect = element.getBoundingClientRect()
    const selection = [...value.selected]
    if (value.linking && !value.selected.has(value.linking)) selection.push(value.linking)
    const result = await invoke<NativeCanvasStats>('canvas_gpu_frame', { sessionId:session.id,frame: { viewport: { x: rect.left, y: rect.top, width: rect.width, height: rect.height }, camera: value.camera, pixelRatio: window.devicePixelRatio, selected: selection, selectedEdge:value.selectedEdge, locked: selection.filter(id=>value.locked.has(id)), overlayNodes:value.overlayNodes, overlayText: value.document.nodes.length<=200&&value.camera.zoom>=.5, editing: value.editing } })
    if (!session.disposed) {
      element.dataset.frameCpuMs = String(result.frameCpuMs)
      element.dataset.nativeFrames = String(result.frames)
      if (performance.now() - session.publishedAt > 250 || !session.ready) { setStats(result); session.publishedAt = performance.now() }
      const paths=[...new Set([...session.imageDirty].filter(path=>result.rasterPaths.includes(path)).concat(result.missingImages))]
      for(const path of paths){if(session.imagePending.size>=2)break;const version=session.imageVersions.get(path)??0;if(session.imagePending.has(path)||session.imageFailed.get(path)===version)continue;session.imagePending.add(path);const force=session.imageDirty.has(path);let reschedule=true
        void invoke<string>('canvas_gpu_image',{request:{root:session.root,sessionId:session.id,relativePath:path,force}}).then(status=>{if(session.disposed)return;if(status==='busy')reschedule=false;if(status!=='busy'&&(session.imageVersions.get(path)??0)===version)session.imageDirty.delete(path)}).catch(()=>{if(!session.disposed&&(session.imageVersions.get(path)??0)===version&&latest.current.document.nodes.some(node=>isCanvasRaster(node)&&node.file===path)){session.imageFailed.set(path,version);setImageErrors(session.imageFailed.size)}}).finally(()=>{session.imagePending.delete(path);if(!session.disposed&&reschedule)scheduleRef.current()})
      }
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
  scheduleRef.current=schedule

  useEffect(()=>{
    let disposed=false;const invalidate=(paths:string[]|null,absolute=false)=>{const session=current.current;if(!session||session.disposed||session.root!==inputs.root)return;const references=[...new Set(latest.current.document.nodes.filter(isCanvasRaster).map(node=>node.file!))];for(const path of changedRasterPaths(inputs.root,paths,references,absolute)){session.imageDirty.add(path);session.imageVersions.set(path,(session.imageVersions.get(path)??0)+1);session.imageFailed.delete(path)}setImageErrors(session.imageFailed.size);schedule()}
    const delta=(event:Event)=>{const detail=(event as CustomEvent<RasterDelta>).detail;if(detail?.root===inputs.root)invalidate(detail.paths)};window.addEventListener('brandon-asset-delta',delta)
    let unlisten:(()=>void)|undefined;void listen<string[]>('vault-changed',({payload})=>invalidate(payload.length?payload:null,true)).then(stop=>{if(disposed)stop();else unlisten=stop})
    return()=>{disposed=true;window.removeEventListener('brandon-asset-delta',delta);unlisten?.()}
  },[inputs.root,schedule])

  useEffect(() => {
    if (!enabled || failed) { setActive(false); setStarting(false); return }
    const session: Session = { id:crypto.randomUUID(),root:inputs.root,labels:null,imagePending:new Set(),imageDirty:new Set(),imageVersions:new Map(),imageFailed:new Map(),disposed: false, started: false, ready: false, dirty: false, raf: 0, pending: null, document: null, nodes: new Map(), order: [], edges: null, publishedAt: 0 }
    current.current = session
    setStarting(true); setError('');setImageErrors(0)
    lifecycle = lifecycle.catch(() => {}).then(async () => {
      try {
        if (session.disposed) return
        const supported = await invoke<boolean>('canvas_gpu_supported')
        if (!supported || session.disposed) return
        await invoke('canvas_gpu_start',{root:session.root,sessionId:session.id})
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
      lifecycle = lifecycle.catch(() => {}).then(async () => { await session.pending; if (session.started) await invoke('canvas_gpu_stop',{sessionId:session.id}).catch(() => {}) })
      if (current.current === session) current.current = null
    }
  }, [draw, failed, enabled,inputs.root])

  useEffect(() => { schedule() }, [inputs.document, inputs.labels, inputs.edges, inputs.camera, inputs.selected, inputs.selectedEdge, inputs.locked, inputs.overlayNodes, inputs.editing, inputs.linking, active, schedule])
  useEffect(() => {
    if (!active) return
    window.addEventListener('resize', schedule); window.addEventListener('focus', schedule)
    document.addEventListener('visibilitychange', schedule)
    const timer = window.setInterval(schedule, 1000)
    const observer = new ResizeObserver(schedule)
    if (viewport.current) observer.observe(viewport.current)
    return () => { window.removeEventListener('resize', schedule); window.removeEventListener('focus', schedule); document.removeEventListener('visibilitychange', schedule); clearInterval(timer); observer.disconnect() }
  }, [active, schedule, viewport])
  return { active, starting, error, stats,imageErrors,retryImages:()=>{const session=current.current;if(session){for(const path of session.imageFailed.keys())session.imageDirty.add(path);session.imageFailed.clear();setImageErrors(0);schedule()}} }
}
