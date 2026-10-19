import {
  useEffect, useMemo, useRef, useState,
  type KeyboardEvent, type PointerEvent,
} from 'react'
import { FileIcon, FitIcon, LinkIcon, PlusIcon } from './icons'
import type { VaultIndex } from './types'
import { CanvasSpatialIndex } from './canvas-spatial'
import { CanvasOverview } from './canvas-overview'
import { useNativeCanvas } from './use-native-canvas'
import { parseCanvasDocument } from './canvas-document'
import { AttachmentDetails, AttachmentImage, isRasterPath, useCanvasAttachments } from './canvas-attachments'
import { CommandMenu } from './command-menu'
import { createCommandRegistry, type CommandRegistry, type HotkeyOverrides } from './commands'
import { MarkdownPreview } from './markdown-preview'
import { displayProperty } from './properties'
import type {CanvasDocument,CanvasNode,CanvasPrimitive} from './canvas-model'
import {canvasExtension,canvasPrimitive} from './canvas-model'
import {CANVAS_COMMANDS,CANVAS_CREATE_COMMANDS} from './canvas-commands'
import {canvasAppearance,canvasNodeHit,primitiveNode,canvasTopHit,canvasGroupHeaderHit as groupHeaderHit} from './canvas-primitives'
import {canvasRasterLayout} from './canvas-raster'
import {CanvasPrimitiveView} from './canvas-primitive-view'
import {CanvasStyleDialog} from './canvas-style-dialog'
import {CanvasStickyStack} from './canvas-sticky-stack'
import {prepareCanvasEdges,canvasEdgeHit,canvasEdgeEditable,patchCanvasEdge,reverseCanvasEdge,deleteCanvasEdge,connectCanvasNodes} from './canvas-edges'
import {CanvasEdgeDialog} from './canvas-edge-dialog'
import {CanvasEdgeView} from './canvas-edge-view'
import {insertCanvasNodes,canvasSelectionEditable,canvasSelectionRoots,expandCanvasSelection,lockedCanvasNodes,moveCanvasNodes,deleteCanvasSelection,groupCanvasSelection,ungroupCanvasSelection,reorderCanvasSelection,layoutCanvasSelection,toggleCanvasLock,patchCanvasSelection,type CanvasLayout} from './canvas-operations'
export type {CanvasDocument,CanvasNode,CanvasEdge} from './canvas-model'

import type {CanvasEdge} from './canvas-model'
type Camera = { x: number; y: number; zoom: number }
type Point = { x: number; y: number }
type SelectionBox = { start: Point; end: Point; additive: boolean }
type DragState =
  | { type: 'pan'; start: Point; camera: Camera }
  | { type: 'select'; selection: SelectionBox; base: Set<string> }
  | { type: 'nodes'; before: string; start: Point; base:CanvasDocument; ids:Set<string> }
  | { type: 'resize'; before: string; start: Point; id: string; width: number; height: number }

const MIN_ZOOM = .05
const MAX_ZOOM = 3
function editableCanvasText(node:CanvasNode|null|undefined):node is CanvasNode{return node?.type==='text'&&canvasPrimitive(node)!=='divider'}
function CanvasRasterContent({node,root}:{node:CanvasNode;root:string}){const layout=canvasRasterLayout(node);return <><div className="canvas-raster-stage" style={{left:layout.image.x,top:layout.image.y,width:layout.image.width,height:layout.image.height}}><AttachmentImage root={root} path={node.file!}/></div><strong className="canvas-raster-caption" style={{left:layout.footer.x,top:layout.footer.y,width:layout.footer.width,height:layout.footer.height,fontSize:layout.fontSize,lineHeight:'1.55'}}>{node.file?.split('/').at(-1)}</strong></>}
function CanvasFileProperties({properties}:{properties:Record<string,unknown>|undefined}){
  if(!properties)return null
  const entries=Object.entries(properties).filter(([,value])=>value===null||typeof value!=='object'||Array.isArray(value)).slice(0,4)
  return entries.length?<dl className="canvas-file-properties">{entries.map(([key,value])=><div key={key}><dt>{key}</dt><dd>{displayProperty(value)}</dd></div>)}</dl>:null
}
const MIN_NODE_WIDTH = 150
const MIN_NODE_HEIGHT = 80

function parseCanvas(source: string): CanvasDocument {
  return parseCanvasDocument(source).document
}

function serializeCanvas(document: CanvasDocument) {
  return `${JSON.stringify(document, null, 2)}\n`
}

function nextId(prefix: string) {
  return `${prefix}-${crypto.randomUUID()}`
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value))
}

function boundsFor(nodes: CanvasNode[]) {
  if (!nodes.length) return null
  const left = Math.min(...nodes.map((node) => node.x))
  const top = Math.min(...nodes.map((node) => node.y))
  const right = Math.max(...nodes.map((node) => node.x + node.width))
  const bottom = Math.max(...nodes.map((node) => node.y + node.height))
  return { left, top, right, bottom, width: right - left, height: bottom - top }
}

function boxesIntersect(node: CanvasNode, left: number, top: number, right: number, bottom: number) {
  return node.x < right && node.x + node.width > left && node.y < bottom && node.y + node.height > top
}

function additiveSelection(event: { ctrlKey: boolean; metaKey: boolean; shiftKey: boolean }) {
  return event.ctrlKey || event.metaKey || event.shiftKey
}

export function CanvasView({ root,value, index, onChange, onOpen, initialZoom = 1, initialCamera, onCameraChange, nativeEnabled = true, focusTarget, onMissingSearchTarget, hotkeys = {} }: {
  root:string
  value: string
  index: VaultIndex | null
  onChange: (value: string) => void
  onOpen: (path: string) => void
  initialZoom?: number
  initialCamera?: Camera
  onCameraChange?: (camera: Camera) => void
  nativeEnabled?: boolean
  focusTarget?: { nodeId: string | null; nonce: string } | null
  onMissingSearchTarget?: () => void
  hotkeys?: HotkeyOverrides
}) {
  const validation = useMemo(() => parseCanvasDocument(value), [value])
  const stored = validation.document
  const [gestureDocument, setGestureDocument] = useState<CanvasDocument | null>(null)
  const parsed = gestureDocument ?? stored
  const extended=useMemo(()=>parsed.nodes.some(n=>n.brandon!==undefined)||parsed.edges.some(e=>e.brandon!==undefined),[parsed.nodes,parsed.edges])
  const preparedEdges=useMemo(()=>prepareCanvasEdges(parsed),[parsed])
  const noteProperties=useMemo(()=>new Map(index?.documents.map(document=>[document.path,document.properties])),[index?.documents])
  const [camera, setCamera] = useState<Camera>(initialCamera ?? { x: 0, y: 0, zoom: initialZoom })
  const cameraChanged = useRef(onCameraChange); cameraChanged.current = onCameraChange
  useEffect(() => { cameraChanged.current?.(camera) }, [camera])
  const [tool, setTool] = useState<'select' | 'pan'>('select')
  const [spacePanning, setSpacePanning] = useState(false)
  const [selected, setSelected] = useState<Set<string>>(() => new Set())
  const [selectedEdge,setSelectedEdge]=useState<string|null>(null)
  const [selectionBox, setSelectionBox] = useState<SelectionBox | null>(null)
  const [linking, setLinking] = useState<string | null>(null)
  const [notePicker, setNotePicker] = useState(false)
  const [historyVersion, setHistoryVersion] = useState(0)
  const [,setClipboardVersion]=useState(0)
  const [viewportSize, setViewportSize] = useState({ width: 1, height: 1 })
  const [editingId, setEditingId] = useState<string | null>(null)
  const [attachmentPath, setAttachmentPath] = useState<string | null>(null)
  const [context, setContext] = useState<{x:number;y:number;id:string|null;edgeId?:string;arrange?:boolean;create?:boolean} | null>(null)
  const [styleTarget,setStyleTarget]=useState<{source:string;ids:Set<string>;nodes:CanvasNode[]}|null>(null)
  const [stickyStack,setStickyStack]=useState(false)
  const [edgeTarget,setEdgeTarget]=useState<{source:string;edge:CanvasEdge}|null>(null)
  const [operationError,setOperationError]=useState('')
  const viewport = useRef<HTMLDivElement>(null)
  const lastSearchFocus = useRef<string | null>(null)
  const documentRef = useRef(parsed)
  const lastEmitted = useRef<string | null>(null)
  const drag = useRef<DragState | null>(null)
  const suppressNodeClick = useRef(false)
  const undoStack = useRef<string[]>([])
  const redoStack = useRef<string[]>([])
  const clipboard = useRef<{ nodes: CanvasNode[]; edges: CanvasEdge[] } | null>(null)
  const boardRegistry=useRef<CommandRegistry|null>(null)
  const locks=useMemo(()=>{try{return {ids:lockedCanvasNodes(parsed),error:''}}catch(error){return {ids:new Set(parsed.nodes.map(n=>n.id)),error:String(error)}}},[parsed])
  // Rich text and node geometry must share a compositor to honor alpha order.
  // This is bounded; larger scenes retain native geometry and glyphs.
  const domGeometry=parsed.nodes.length<=200&&camera.zoom>=.5
  const native = useNativeCanvas(viewport, { root,document: parsed, edges:preparedEdges, camera, selected, selectedEdge, locked: locks.ids, overlayNodes:domGeometry, editing: editingId, linking }, nativeEnabled && !validation.error)
  const spatial = useMemo(() => new CanvasSpatialIndex(parsed.nodes), [parsed.nodes])
  const worldBounds = useMemo(() => ({ x: (-viewportSize.width / 2 - camera.x - 80) / camera.zoom, y: (-viewportSize.height / 2 - camera.y - 80) / camera.zoom, width: (viewportSize.width + 160) / camera.zoom, height: (viewportSize.height + 160) / camera.zoom }), [camera, viewportSize])
  const visibleNodes = useMemo(() => spatial.query(worldBounds), [spatial, worldBounds])
  const nodes = useMemo(() => new Map(parsed.nodes.map((node) => [node.id, node])), [parsed.nodes])
  const edgeIndex = useMemo(() => new CanvasSpatialIndex(preparedEdges),[preparedEdges])
  const visibleEdges = useMemo(() => edgeIndex.query(worldBounds), [edgeIndex, worldBounds])
  const overview = !native.active && camera.zoom < .5
  const spatialInput = native.active || overview
  const imageNodes = useMemo(()=>{if(camera.zoom<.5)return new Set<string>();const images=visibleNodes.filter(node=>node.type==='file'&&isRasterPath(node.file)),paths=new Set<string>();for(const node of images.toReversed()){if(paths.size<64||paths.has(node.file!))paths.add(node.file!)}return new Set(images.filter(node=>paths.has(node.file!)).map(node=>node.id))},[camera.zoom,visibleNodes])
  const richCards = parsed.nodes.length <= 200 && camera.zoom >= .5
  const domNodes = useMemo(() => native.starting && nativeEnabled && !richCards ? [] : native.active && !richCards ? visibleNodes.filter((node) => node.id === editingId) : overview ? [] : visibleNodes, [native.starting, nativeEnabled, native.active, richCards, overview, visibleNodes, editingId])
  const attachments = useCanvasAttachments(viewport, (attachment, position) => {
    const document = documentRef.current
    const point = screenToWorld(position ?? { x: viewportSize.width / 2, y: viewportSize.height / 2 })
    const id = nextId('file')
    const image = attachment.kind === 'image'
    commit(insertCanvasNodes(document,[{ id, type: 'file', x: point.x - 150, y: point.y - 75, width: 300, height: image ? 230 : 110, file: attachment.path }]))
    setSelected(new Set([id]))
  })
  const openFile = (path: string) => { if (/\.(md|canvas|base)$/i.test(path)) onOpen(path); else setAttachmentPath(path) }

  useEffect(() => {
    if (!focusTarget || lastSearchFocus.current === focusTarget.nonce || viewportSize.width <= 1 || viewportSize.height <= 1) return
    lastSearchFocus.current = focusTarget.nonce
    if (!focusTarget.nodeId) {
      const bounds = boundsFor([...nodes.values()])
      if (bounds) {
        const zoom = clamp(Math.min(1.6, (viewportSize.width - 180) / Math.max(1, bounds.width), (viewportSize.height - 160) / Math.max(1, bounds.height)), MIN_ZOOM, MAX_ZOOM)
        setCamera({ x: -(bounds.left + bounds.width / 2) * zoom, y: -(bounds.top + bounds.height / 2) * zoom, zoom })
      }
      setSelected(new Set())
      return
    }
    const node = nodes.get(focusTarget.nodeId)
    if (!node) { onMissingSearchTarget?.(); return }
    const zoom = clamp(Math.min(1, (viewportSize.width - 120) / node.width, (viewportSize.height - 160) / node.height), MIN_ZOOM, MAX_ZOOM)
    setSelected(new Set([node.id]))
    setEditingId(null)
    setCamera({ x: -(node.x + node.width / 2) * zoom, y: -(node.y + node.height / 2) * zoom, zoom })
    viewport.current?.focus({ preventScroll: true })
  }, [focusTarget, nodes, viewportSize, onMissingSearchTarget])

  const nudgeHistory = () => setHistoryVersion((current) => current + 1)
  const applySerialized = (source: string) => {
    documentRef.current = parseCanvas(source)
    lastEmitted.current = source
    onChange(source)
  }
  const pushHistory = (source: string) => {
    if (undoStack.current.at(-1) !== source) undoStack.current.push(source)
    if (undoStack.current.length > 80) undoStack.current.shift()
    redoStack.current = []
    nudgeHistory()
  }
  const commit = (document: CanvasDocument, record = true) => {
    const before = serializeCanvas(documentRef.current)
    const next = serializeCanvas(document)
    if (next === before) return
    if (record) pushHistory(before)
    documentRef.current = document
    lastEmitted.current = next
    onChange(next)
  }
  const finishGesture = (before: string) => {
    const next = serializeCanvas(documentRef.current)
    if (before !== next) { pushHistory(before); lastEmitted.current = next; onChange(next) }
    setGestureDocument(null)
  }
  const updateNode = (id: string, patch: Partial<CanvasNode>, record = true) => {
    const document = documentRef.current
    if(locks.ids.has(id))return
    commit({ ...document, nodes: document.nodes.map((node) => node.id === id ? { ...node, ...patch } : node), edges: document.edges }, record)
  }

  useEffect(() => {
    if (!editingId) return
    const card = Array.from(viewport.current?.querySelectorAll<HTMLElement>('.canvas-node') ?? []).find((node) => node.dataset.nodeId === editingId)
    const element = card?.querySelector('textarea')
    if (element) { element.focus(); element.setSelectionRange(element.value.length, element.value.length) }
  }, [editingId, native.active])

  useEffect(() => {
    if (lastEmitted.current === value) {
      lastEmitted.current = null
      documentRef.current = stored
      return
    }
    documentRef.current = stored
    setGestureDocument(null)
    drag.current=null
    undoStack.current = []
    redoStack.current = []
    setSelected((current) => new Set([...current].filter((id) => stored.nodes.some((node) => node.id === id))))
    nudgeHistory()
  }, [stored, value])

  useEffect(() => {
    const element = viewport.current
    if (!element) return
    const measure = () => setViewportSize({ width: element.clientWidth, height: element.clientHeight })
    measure()
    const observer = new ResizeObserver(measure)
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  useEffect(()=>{
    const element=viewport.current
    if(!element)return
    const listener=(event:Event)=>{
      const detail=(event as CustomEvent<{id:string;handled:boolean}>).detail
      if(detail&&boardRegistry.current)detail.handled=boardRegistry.current.execute(detail.id)!==false
    }
    element.addEventListener('brandon-canvas-command',listener)
    return()=>element.removeEventListener('brandon-canvas-command',listener)
  },[])

  useEffect(() => {
    const press = (event: globalThis.KeyboardEvent) => {
      const target = event.target instanceof Element ? event.target : null
      if (event.code !== 'Space' || !target || !viewport.current?.contains(target) || target.closest('textarea, input, button, [role="menu"], [contenteditable="true"]')) return
      event.preventDefault()
      setSpacePanning(true)
    }
    const release = (event: globalThis.KeyboardEvent) => {
      if (event.code === 'Space') setSpacePanning(false)
    }
    const reset = () => setSpacePanning(false)
    window.addEventListener('keydown', press)
    window.addEventListener('keyup', release)
    window.addEventListener('blur', reset)
    return () => {
      window.removeEventListener('keydown', press)
      window.removeEventListener('keyup', release)
      window.removeEventListener('blur', reset)
    }
  }, [])

  useEffect(() => {
    if (linking && !parsed.nodes.some((node) => node.id === linking)) setLinking(null)
    setSelected((current) => new Set([...current].filter((id) => parsed.nodes.some((node) => node.id === id))))
  }, [linking, parsed.nodes])

  const localPoint = (clientX: number, clientY: number) => {
    const rect = viewport.current?.getBoundingClientRect()
    return { x: clientX - (rect?.left ?? 0), y: clientY - (rect?.top ?? 0) }
  }
  const screenToWorld = (point: Point) => ({
    x: (point.x - viewportSize.width / 2 - camera.x) / camera.zoom,
    y: (point.y - viewportSize.height / 2 - camera.y) / camera.zoom,
  })
  const hitNode=(point:Point)=>canvasTopHit(spatial.query({x:point.x-12/camera.zoom,y:point.y-12/camera.zoom,width:24/camera.zoom,height:24/camera.zoom}).filter(node=>canvasNodeHit(node,point,6/camera.zoom)||selected.has(node.id)&&!locks.ids.has(node.id)&&Math.abs(point.x-node.x-node.width)*camera.zoom<12&&Math.abs(point.y-node.y-node.height)*camera.zoom<12),point)
  const hitEdge=(point:Point)=>edgeIndex.query({x:point.x-8/camera.zoom,y:point.y-8/camera.zoom,width:16/camera.zoom,height:16/camera.zoom}).filter(edge=>canvasEdgeHit(edge,point,Math.max(6/camera.zoom,edge.strokeWidth/2))).at(-1)
  const selectEdge=(id:string)=>{setSelected(new Set());setSelectedEdge(id);setEditingId(null);viewport.current?.focus()}
  const openEdgeStyle=(id=selectedEdge)=>{const edge=documentRef.current.edges.find(e=>e.id===id);if(edge)setEdgeTarget({source:serializeCanvas(documentRef.current),edge})}
  const addPrimitive=(primitive:CanvasPrimitive)=>{
    const id=nextId(primitive),node=primitiveNode(id,primitive,screenToWorld({x:viewportSize.width/2,y:viewportSize.height/2})),document=documentRef.current
    commit(insertCanvasNodes(document,[node]));setSelected(new Set([id]));setTool('select');setEditingId(primitive==='sticky'||primitive==='text'?id:null)
  }

  const addText = () => {
    const document = documentRef.current
    const center = screenToWorld({ x: viewportSize.width / 2, y: viewportSize.height / 2 })
    const id = nextId('text')
    commit(insertCanvasNodes(document,[{ id, type: 'text', x: center.x - 140, y: center.y - 75, width: 280, height: 150, text: 'Новая карточка' }]))
    setSelected(new Set([id]))
    setEditingId(id)
    setTool('select')
  }
  const addFile = (path: string) => {
    const document = documentRef.current
    const center = screenToWorld({ x: viewportSize.width / 2, y: viewportSize.height / 2 })
    const id = nextId('file')
    commit(insertCanvasNodes(document,[{ id, type: 'file', x: center.x - 150, y: center.y - 55, width: 300, height: 110, file: path }]))
    setSelected(new Set([id]))
    setNotePicker(false)
  }
  const chooseForLink = (id: string) => {
    const document = documentRef.current
    if (!linking) { setLinking(id); return }
    if (linking === id) { setLinking(null); return }
    const exists = document.edges.some((edge) => (edge.fromNode === linking && edge.toNode === id) || (edge.fromNode === id && edge.toNode === linking))
    if (!exists)try{commit(connectCanvasNodes(document,linking,id,nextId('edge')))}catch(error){setOperationError(String(error))}
    setLinking(null)
  }
  const connectSelected = () => {
    const ids = [...selected]
    if (ids.length !== 2) {
      setLinking((current) => current !== null ? null : '')
      return
    }
    const [fromNode, toNode] = ids
    const document = documentRef.current
    const exists = document.edges.some((edge) =>
      (edge.fromNode === fromNode && edge.toNode === toNode) || (edge.fromNode === toNode && edge.toNode === fromNode)
    )
    if (!exists)commit(connectCanvasNodes(document,fromNode,toNode,nextId('edge')))
    setLinking(null)
  }

  const undo = () => {
    const previous = undoStack.current.pop()
    if (!previous) return
    redoStack.current.push(serializeCanvas(documentRef.current))
    applySerialized(previous)
    nudgeHistory()
  }
  const redo = () => {
    const next = redoStack.current.pop()
    if (!next) return
    undoStack.current.push(serializeCanvas(documentRef.current))
    applySerialized(next)
    nudgeHistory()
  }
  const copySelection = () => {
    if (!selected.size) return
    const document = documentRef.current
    const copied=expandCanvasSelection(document,selected)
    clipboard.current = {
      nodes: document.nodes.filter((node) => copied.has(node.id)).map((node) => ({ ...node })),
      edges: document.edges.filter((edge) => copied.has(edge.fromNode) && copied.has(edge.toNode)).map((edge) => ({ ...edge })),
    }
    setClipboardVersion(version=>version+1)
  }
  const pasteSelection = () => {
    const copied = clipboard.current
    if (!copied?.nodes.length) return
    const document = documentRef.current
    const idMap = new Map(copied.nodes.map((node) => [node.id, nextId(node.type)]))
    const nodes = copied.nodes.map((node) => ({ ...node, id: idMap.get(node.id)!, x: node.x + 32, y: node.y + 32 }))
    const edges = copied.edges.map((edge) => ({ ...edge, id: nextId('edge'), fromNode: idMap.get(edge.fromNode)!, toNode: idMap.get(edge.toNode)! }))
    commit({ ...document, nodes: [...document.nodes, ...nodes], edges: [...document.edges, ...edges] })
    setSelected(new Set(nodes.map((node) => node.id)))
    clipboard.current = { nodes: nodes.map((node) => ({ ...node })), edges: edges.map((edge) => ({ ...edge })) }
  }
  const duplicateSelection = () => {
    copySelection()
    pasteSelection()
  }
  const deleteSelection = () => {
    if (!selected.size) return
    const document = documentRef.current
    const next=deleteCanvasSelection(document,selected)
    if(next!==document){commit(next);setSelected(new Set())}
  }

  const fitToContent = () => {
    const bounds = boundsFor(documentRef.current.nodes)
    if (!bounds) { setCamera({ x: 0, y: 0, zoom: 1 }); return }
    const zoom = clamp(Math.min((viewportSize.width - 180) / Math.max(bounds.width, 1), (viewportSize.height - 160) / Math.max(bounds.height, 1)), MIN_ZOOM, 1.6)
    setCamera({ x: -(bounds.left + bounds.right) / 2 * zoom, y: -(bounds.top + bounds.bottom) / 2 * zoom, zoom })
  }
  const zoomFromCenter = (factor: number) => {
    setCamera((current) => {
      const zoom = clamp(current.zoom * factor, MIN_ZOOM, MAX_ZOOM)
      const ratio = zoom / current.zoom
      return { x: current.x * ratio, y: current.y * ratio, zoom }
    })
  }

  const pointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.target !== event.currentTarget) return
    event.preventDefault()
    viewport.current?.focus()
    const point = localPoint(event.clientX, event.clientY)
    if (event.button === 2) return
    if (tool === 'pan' || spacePanning || event.button === 1 || event.altKey) {
      event.preventDefault()
      event.currentTarget.setPointerCapture(event.pointerId)
      drag.current = { type: 'pan', start: point, camera }
      return
    }
    if (event.button !== 0) return
    const worldPoint=screenToWorld(point),objectHit=hitNode(worldPoint),edgeHit=linking===null&&(!objectHit||objectHit.type==='group'&&!groupHeaderHit(objectHit,worldPoint))?hitEdge(worldPoint):undefined
    if(edgeHit){selectEdge(edgeHit.id);return}
    setSelectedEdge(null)
    if (spatialInput) {
      const world = screenToWorld(point)
      const hit = hitNode(world)
      if (hit) {
        if (hit.id !== editingId) setEditingId(null)
        if (linking !== null) { chooseForLink(hit.id); return }
        if (selected.has(hit.id) && Math.abs(world.x - hit.x - hit.width) * camera.zoom < 12 && Math.abs(world.y - hit.y - hit.height) * camera.zoom < 12) resizePointerDown(event, hit)
        else nodePointerDown(event, hit.id)
        return
      }
      setEditingId(null)
    }
    event.currentTarget.setPointerCapture(event.pointerId)
    const additive = additiveSelection(event)
    const selection = { start: point, end: point, additive }
    drag.current = { type: 'select', selection, base: new Set(selected) }
    setSelectionBox(selection)
    if (!additive) setSelected(new Set())
  }
  const nodePointerDown = (event: PointerEvent<HTMLDivElement>, id: string) => {
    if ((event.target as HTMLElement).closest('textarea, button, a, .canvas-resize-handle')) return
    if (event.button !== 0 || linking !== null) return
    const object=nodes.get(id),worldPoint=screenToWorld(localPoint(event.clientX,event.clientY))
    if(object?.type==='group'&&!groupHeaderHit(object,worldPoint)){const edge=hitEdge(worldPoint);if(edge){event.preventDefault();event.stopPropagation();selectEdge(edge.id);return}}
    event.preventDefault()
    event.stopPropagation()
    suppressNodeClick.current = false
    viewport.current?.focus()
    event.currentTarget.setPointerCapture(event.pointerId)
    const nextSelected = additiveSelection(event)
      ? new Set(selected.has(id) ? [...selected].filter((item) => item !== id) : [...selected, id])
      : selected.has(id) ? selected : new Set([id])
    setSelected(nextSelected)
    const document = documentRef.current
    try{if(!canvasSelectionEditable(document,nextSelected))return}catch(error){setOperationError(String(error));return}
    drag.current = {
      type: 'nodes',
      before: serializeCanvas(document),
      start: localPoint(event.clientX, event.clientY),
      base:document,ids:nextSelected,
    }
  }
  const resizePointerDown = (event: PointerEvent<HTMLDivElement>, node: CanvasNode) => {
    event.preventDefault()
    event.stopPropagation()
    try{if(!canvasSelectionEditable(documentRef.current,new Set([node.id])))return}catch(error){setOperationError(String(error));return}
    event.currentTarget.setPointerCapture(event.pointerId)
    drag.current = {
      type: 'resize', before: serializeCanvas(documentRef.current), id: node.id,
      start: localPoint(event.clientX, event.clientY), width: node.width, height: node.height,
    }
  }
  const pointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current
    if (!current) return
    const point = localPoint(event.clientX, event.clientY)
    if (current.type === 'pan') {
      setCamera({ ...current.camera, x: current.camera.x + point.x - current.start.x, y: current.camera.y + point.y - current.start.y })
      return
    }
    if (current.type === 'select') {
      const selection = { ...current.selection, end: point }
      current.selection = selection
      setSelectionBox(selection)
      const start = screenToWorld(selection.start)
      const end = screenToWorld(selection.end)
      const hit = documentRef.current.nodes.filter((node) => boxesIntersect(node, Math.min(start.x, end.x), Math.min(start.y, end.y), Math.max(start.x, end.x), Math.max(start.y, end.y))).map((node) => node.id)
      setSelected(selection.additive ? new Set([...current.base, ...hit]) : new Set(hit))
      return
    }
    if (current.type === 'nodes') {
      const dx = (point.x - current.start.x) / camera.zoom
      const dy = (point.y - current.start.y) / camera.zoom
      if (Math.abs(dx) + Math.abs(dy) > 3) suppressNodeClick.current = true
      try{const next=moveCanvasNodes(current.base,current.ids,dx,dy);documentRef.current=next;setGestureDocument(next)}catch(error){setOperationError(String(error))}
      return
    }
    const document = documentRef.current
    const next = { ...document, nodes: document.nodes.map((node) => node.id === current.id ? {
      ...node,
      width: Math.round(clamp(current.width + (point.x - current.start.x) / camera.zoom,canvasPrimitive(node)==='divider'?80:MIN_NODE_WIDTH,10_000_000)),
      height: Math.round(clamp(current.height + (point.y - current.start.y) / camera.zoom,canvasPrimitive(node)==='divider'?16:MIN_NODE_HEIGHT,10_000_000)),
    } : node), edges: document.edges }
    documentRef.current = next; setGestureDocument(next)
  }
  const finishPointer = () => {
    const current = drag.current
    if (!current) return
    if (current.type === 'nodes' || current.type === 'resize') finishGesture(current.before)
    drag.current = null
    setSelectionBox(null)
  }
  const cancelPointer=()=>{
    const current=drag.current
    if(current?.type==='nodes'||current?.type==='resize'){documentRef.current=parseCanvas(current.before);setGestureDocument(null)}
    drag.current=null;setSelectionBox(null);suppressNodeClick.current=false
  }
  useEffect(() => {
    const element = viewport.current
    if (!element) return
    let raf = 0
    let queued: { x: number; y: number; dx: number; dy: number; zoom: boolean }[] = []
    const wheel = (event: globalThis.WheelEvent) => {
      if (event.target instanceof Element && event.target.closest('.canvas-toolbar, .canvas-note-picker, .canvas-minimap, .command-menu, .canvas-selection-actions, .canvas-operation-error')) return
      if (!event.ctrlKey && event.target instanceof HTMLTextAreaElement && event.target.scrollHeight > event.target.clientHeight) return
      event.preventDefault()
      const rect = element.getBoundingClientRect()
      const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? rect.height : 1
      queued.push({ x: event.clientX - rect.left - rect.width / 2, y: event.clientY - rect.top - rect.height / 2, dx: (event.shiftKey && !event.deltaX ? event.deltaY : event.deltaX) * unit, dy: (event.shiftKey && !event.deltaX ? 0 : event.deltaY) * unit, zoom: event.ctrlKey })
      if (raf) return
      raf = requestAnimationFrame(() => {
        raf = 0
        const events = queued; queued = []
        setCamera((previous) => events.reduce((current, step) => {
          if (!step.zoom) return { ...current, x: current.x - step.dx, y: current.y - step.dy }
          const zoom = clamp(current.zoom * Math.exp(-step.dy * .00105), MIN_ZOOM, MAX_ZOOM)
          const ratio = zoom / current.zoom
          return { x: step.x - (step.x - current.x) * ratio, y: step.y - (step.y - current.y) * ratio, zoom }
        }, previous))
      })
    }
    element.addEventListener('wheel', wheel, { passive: false })
    return () => { element.removeEventListener('wheel', wheel); cancelAnimationFrame(raf) }
  }, [])
  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if(event.key==='ContextMenu'||event.key==='F10'&&event.shiftKey){event.preventDefault();const rect=viewport.current!.getBoundingClientRect();setContext({x:rect.left+rect.width/2,y:rect.top+Math.min(180,rect.height/2),id:[...selected][0]??null,edgeId:selectedEdge??undefined});return}
    if (event.key === 'Escape') { setAttachmentPath(null); setEditingId(null); setLinking(null); setSelected(new Set());setSelectedEdge(null); viewport.current?.focus(); return }
    if ((event.target as HTMLElement).closest('textarea, input, [contenteditable="true"]')) return
    if(event.key==='Enter'&&selectedEdge){event.preventDefault();boardRegistry.current?.execute('canvas-edge-style');return}
    const command=boardRegistry.current?.forEvent(event.nativeEvent)
    // The app owns namespaced command bindings, including cross-context
    // collision/disabled checks; never revive one with the smaller local list.
    if(command&&!command.id.startsWith('canvas-')){event.preventDefault();event.stopPropagation();boardRegistry.current?.execute(command.id);return}
    if (event.key === 'Delete' || event.key === 'Backspace') { event.preventDefault();boardRegistry.current?.execute('delete');return }
  }

  const attempt=(action:()=>void)=>()=>{try{setOperationError('');action()}catch(error){setOperationError(error instanceof Error?error.message:String(error))}}
  const editable=useMemo(()=>{try{return !locks.error&&canvasSelectionEditable(parsed,selected)}catch{return false}},[parsed,selected,locks.error])
  const edgeEditable=useMemo(()=>{try{return Boolean(selectedEdge)&&!locks.error&&canvasEdgeEditable(parsed,selectedEdge!)}catch{return false}},[parsed,selectedEdge,locks.error])
  useEffect(()=>{if(selected.size)setSelectedEdge(null)},[selected])
  useEffect(()=>{if(selectedEdge&&!parsed.edges.some(edge=>edge.id===selectedEdge))setSelectedEdge(null)},[parsed.edges,selectedEdge])
  const selectionRoots=useMemo(()=>{try{return canvasSelectionRoots(parsed,selected)}catch{return []}},[parsed,selected])
  const rootCount=selectionRoots.length
  const unlockedSelection=selectionRoots.some(node=>!canvasExtension(node)?.locked)
  const ancestorLockedSelection=selectionRoots.some(node=>locks.ids.has(node.id)&&!canvasExtension(node)?.locked)
  const arrange=(id:string)=>{
    const document=documentRef.current
    if(id==='canvas-edge-style')openEdgeStyle()
    else if(id==='canvas-edge-reverse'&&selectedEdge)commit(reverseCanvasEdge(document,selectedEdge))
    else if(id==='canvas-edge-delete'&&selectedEdge)commit(deleteCanvasEdge(document,selectedEdge))
    else if(id==='canvas-add-stack')setStickyStack(true)
    else if(id.startsWith('canvas-add-'))addPrimitive(id.slice('canvas-add-'.length)as CanvasPrimitive)
    else if(id==='canvas-style')setStyleTarget({source:serializeCanvas(document),ids:new Set(selected),nodes:document.nodes.filter(node=>selected.has(node.id))})
    else if(id==='canvas-group'){
      const groupId=nextId('group'),next=groupCanvasSelection(document,selected,groupId)
      if(next!==document){commit(next);setSelected(new Set([groupId]))}
    }else if(id==='canvas-ungroup'){
      const members=expandCanvasSelection(document,selected),next=ungroupCanvasSelection(document,selected)
      if(next!==document){commit(next);setSelected(new Set([...members].filter(id=>next.nodes.some(n=>n.id===id))))}
    }else if(id==='canvas-lock'){setEditingId(null);commit(toggleCanvasLock(document,selected))}
    else if(id==='canvas-front'||id==='canvas-back')commit(reorderCanvasSelection(document,selected,id==='canvas-front'))
    else commit(layoutCanvasSelection(document,selected,id.slice('canvas-'.length) as CanvasLayout))
  }
  boardRegistry.current=createCommandRegistry([
    {id:'open',label:context?.id&&nodes.get(context.id)?.type==='text'?'Редактировать текст':'Открыть',enabled:Boolean(context?.id&&(nodes.get(context.id)?.type==='file'&&nodes.get(context.id)?.file||editableCanvasText(nodes.get(context.id))&&!locks.ids.has(context.id))),run:attempt(()=>{const node=context?.id?nodes.get(context.id):null;if(node?.type==='file'&&node.file)openFile(node.file);else if(editableCanvasText(node)&&!locks.ids.has(node.id))setEditingId(node.id)})},
    {id:'duplicate',label:'Дублировать',defaultShortcut:'Mod+KeyD',enabled:Boolean(selected.size),run:attempt(duplicateSelection)},
    {id:'copy',label:'Копировать карточки',defaultShortcut:'Mod+KeyC',enabled:Boolean(selected.size),run:attempt(copySelection)},
    {id:'cut',label:'Вырезать карточки',defaultShortcut:'Mod+KeyX',enabled:editable,run:attempt(()=>{copySelection();deleteSelection()})},
    {id:'paste',label:'Вставить карточки',defaultShortcut:'Mod+KeyV',enabled:Boolean(clipboard.current?.nodes.length),run:attempt(pasteSelection)},
    {id:'delete',label:selectedEdge?'Удалить связь':'Убрать с доски',enabled:selectedEdge?edgeEditable:editable,run:attempt(()=>{if(selectedEdge)commit(deleteCanvasEdge(documentRef.current,selectedEdge));else deleteSelection()})},
    {id:'new-text',label:'Добавить текстовую карточку',run:attempt(addText)},
    {id:'fit',label:'Показать всю доску',run:fitToContent},
    {id:'undo',label:'Отменить',defaultShortcut:'Mod+KeyZ',enabled:Boolean(undoStack.current.length),run:undo},
    {id:'redo',label:'Повторить',defaultShortcut:'Mod+Shift+KeyZ',enabled:Boolean(redoStack.current.length),run:redo},
    {id:'redo-alt',label:'Повторить',defaultShortcut:'Mod+KeyY',enabled:Boolean(redoStack.current.length),run:redo},
    {id:'select-all',label:'Выделить всё',defaultShortcut:'Mod+KeyA',run:()=>setSelected(new Set(parsed.nodes.map(n=>n.id)))},
    ...CANVAS_COMMANDS.map(command=>({...command,label:command.id==='canvas-lock'?(ancestorLockedSelection?'В заблокированной группе':unlockedSelection?'Заблокировать':'Разблокировать'):command.label,enabled:command.id.startsWith('canvas-edge-')?edgeEditable:command.id.startsWith('canvas-add-')?true:command.id==='canvas-style'?editable:command.id==='canvas-lock'?Boolean(selected.size)&&!locks.error&&!ancestorLockedSelection:editable&&(command.id==='canvas-group'?selected.size>=2:command.id==='canvas-ungroup'?[...selected].some(id=>nodes.get(id)?.type==='group'):command.id==='canvas-front'||command.id==='canvas-back'?true:rootCount>=(command.id.includes('distribute')?3:2)),run:attempt(()=>arrange(command.id))})),
  ],hotkeys)

  const contentBounds = useMemo(() => boundsFor(parsed.nodes), [parsed.nodes])
  const mini = useMemo(() => contentBounds ? (() => {
    const width = 172
    const height = 108
    const padding = 10
    const scale = Math.min((width - padding * 2) / Math.max(contentBounds.width, 1), (height - padding * 2) / Math.max(contentBounds.height, 1), 1)
    const offsetX = (width - contentBounds.width * scale) / 2 - contentBounds.left * scale
    const offsetY = (height - contentBounds.height * scale) / 2 - contentBounds.top * scale
    const viewLeft = (-viewportSize.width / 2 - camera.x) / camera.zoom
    const viewTop = (-viewportSize.height / 2 - camera.y) / camera.zoom
    return { width, height, scale, offsetX, offsetY, viewLeft, viewTop, viewWidth: viewportSize.width / camera.zoom, viewHeight: viewportSize.height / camera.zoom }
  })() : null, [contentBounds, viewportSize, camera])
  const selectionStyle = selectionBox ? {
    left: Math.min(selectionBox.start.x, selectionBox.end.x),
    top: Math.min(selectionBox.start.y, selectionBox.end.y),
    width: Math.abs(selectionBox.end.x - selectionBox.start.x),
    height: Math.abs(selectionBox.end.y - selectionBox.start.y),
  } : undefined

  if (validation.error) return <div className="canvas-invalid" role="alert"><strong>Не удалось открыть холст</strong><p>{validation.error}</p><p>Исходный файл не изменён. Исправьте JSON во внешнем редакторе — пустая доска не будет записана поверх него.</p></div>

  return <div
    ref={viewport}
    className={`canvas-view tool-${spacePanning ? 'pan' : tool} ${native.active ? 'native-renderer' : ''} ${attachments.dragOver ? 'attachment-drag-over' : ''}`}
    data-renderer={native.active ? 'wgpu' : overview ? 'overview' : 'dom'}
    data-dom-geometry={native.active&&domGeometry?'true':undefined}
    data-zoom={camera.zoom}
    data-visible-nodes={visibleNodes.length}
    data-total-nodes={parsed.nodes.length}
    data-selected-edge={selectedEdge??undefined}
    data-native-edge-labels={native.stats?.edgeLabels}
    data-native-draw-batches={native.stats?.drawBatches}
    data-native-translucent-batches={native.stats?.translucentBatches}
    data-native-raster-images={native.stats?.rasterImages}
    data-native-raster-bytes={native.stats?.rasterBytes}
    data-search-focused-node={focusTarget?.nodeId && selected.has(focusTarget.nodeId) ? focusTarget.nodeId : undefined}
    data-frame-cpu-ms={native.stats?.frameCpuMs}
    data-native-frames={native.stats?.frames}
    data-render-adapter={native.stats?.adapter}
    data-render-backend={native.stats?.backend}
    data-render-error={native.error || undefined}
    tabIndex={0}
    onKeyDown={keyDown}
    onPointerDown={pointerDown}
    onPointerMove={pointerMove}
    onPointerUp={finishPointer}
    onPointerCancel={cancelPointer}
    onContextMenu={(event) => {
      if ((event.target as HTMLElement).closest('textarea,input,.command-menu,.image-viewer')) return
      event.preventDefault();event.stopPropagation();finishPointer()
      const element=(event.target as HTMLElement).closest<HTMLElement>('[data-node-id]')
      const point=screenToWorld(localPoint(event.clientX,event.clientY))
      const hit=hitNode(point),edge=(!hit||hit.type==='group'&&!groupHeaderHit(hit,point))?hitEdge(point):undefined
      if(edge){selectEdge(edge.id);setContext({x:event.clientX,y:event.clientY,id:null,edgeId:edge.id});return}
      setSelectedEdge(null)
      const id=element?.dataset.nodeId ?? hit?.id ?? null
      if(id && !selected.has(id))setSelected(new Set([id]))
      setContext({x:event.clientX,y:event.clientY,id})
    }}
    onClick={event=>{
      if(event.target!==event.currentTarget || !spatialInput || suppressNodeClick.current || linking!==null || tool==='pan') { suppressNodeClick.current=false;return }
      const point=screenToWorld(localPoint(event.clientX,event.clientY)),hit=hitNode(point)
      if(hit?.type==='file' && hit.file && isRasterPath(hit.file)&&!additiveSelection(event))openFile(hit.file)
    }}
    onDragOver={(event) => { if (event.dataTransfer.types.includes('application/x-brandon-vault-path')) { event.preventDefault(); event.dataTransfer.dropEffect = 'copy' } }}
    onDrop={(event) => { const path = event.dataTransfer.getData('application/x-brandon-vault-path'); if (path) { event.preventDefault(); const rect = event.currentTarget.getBoundingClientRect(); void attachments.referenceFile(path, { x: event.clientX - rect.left, y: event.clientY - rect.top }) } }}
    onDoubleClick={(event) => {
      if (event.target !== event.currentTarget) return
      const point = screenToWorld(localPoint(event.clientX, event.clientY))
      const hit = hitNode(point)
      const edge=(!hit||hit.type==='group'&&!groupHeaderHit(hit,point))?hitEdge(point):undefined
      if(edge){selectEdge(edge.id);if(canvasEdgeEditable(documentRef.current,edge.id))openEdgeStyle(edge.id);return}
      if(!spatialInput)return
      if (hit?.type === 'file' && hit.file) openFile(hit.file)
      if (overview && hit) setCamera({ x: -(hit.x + hit.width / 2), y: -(hit.y + hit.height / 2), zoom: 1 })
      if (editableCanvasText(hit)&&!locks.ids.has(hit.id)) setEditingId(hit.id)
    }}
  >
    <div className={`canvas-toolbar ${viewportSize.width<1100?'compact':''}`}>
      <div className="canvas-tool-toggle"><button className={tool === 'select' ? 'active' : ''} onClick={() => setTool('select')}>Выбор</button><button className={tool === 'pan' ? 'active' : ''} onClick={() => setTool('pan')}>Рука</button></div>
      <i />
      <button onClick={attempt(addText)}><PlusIcon />Карточка</button>
      <button aria-label="Добавить фигуру или стикер" aria-expanded={Boolean(context?.create)} onClick={event=>{const rect=event.currentTarget.getBoundingClientRect();setContext(current=>current?.create?null:{x:rect.left,y:rect.bottom+8,id:null,create:true})}}>Фигуры</button>
      <button aria-label="Упорядочить объекты" aria-expanded={Boolean(context?.arrange)} onClick={event=>{const rect=event.currentTarget.getBoundingClientRect();setContext(current=>current?.arrange?null:{x:rect.left,y:rect.bottom+8,id:[...selected][0]??null,arrange:true})}}>Упорядочить</button>
      <button aria-label={selectedEdge?'Стиль выбранной связи':'Стиль выбранных объектов'} disabled={selectedEdge?!edgeEditable:!editable} onClick={()=>boardRegistry.current?.execute(selectedEdge?'canvas-edge-style':'canvas-style')}>Стиль</button>
      <button onClick={() => setNotePicker((current) => !current)}><FileIcon />Заметка</button>
      <button disabled={Boolean(attachments.progress)} onClick={() => void attachments.choose()}><FileIcon />Файл</button>
      <button className={linking !== null || selected.size === 2 ? 'active' : ''} title={selected.size === 2 ? 'Связать выбранные объекты' : 'Выбрать два объекта для связи'} onClick={attempt(connectSelected)}><LinkIcon />{selected.size === 2 ? 'Связать выбранные' : linking !== null ? 'Выберите два' : 'Связать'}</button>
      <i />
      <button className="canvas-history" title="Отменить (Ctrl+Z)" disabled={!undoStack.current.length} onClick={undo}>↶</button>
      <button className="canvas-history" title="Повторить (Ctrl+Shift+Z)" disabled={!redoStack.current.length} onClick={redo}>↷</button>
      <button title="Показать весь холст" onClick={fitToContent}><FitIcon /></button>
      <button className="canvas-zoom" title="Уменьшить" onClick={() => zoomFromCenter(1 / 1.16)}>−</button>
      <button className="canvas-zoom-value" title="Масштаб 100%" onClick={() => setCamera((current) => ({ ...current, x: current.x / current.zoom, y: current.y / current.zoom, zoom: 1 }))}>{Math.round(camera.zoom * 100)}%</button>
      <button className="canvas-zoom" title="Увеличить" onClick={() => zoomFromCenter(1.16)}>+</button>
      {extended&&<span className="canvas-compatibility" title="Расширение Brandon v1: текст и геометрия читаются другими редакторами, но блокировка и стили могут не учитываться.">Расширенный Canvas</span>}
    </div>
    {notePicker && <div className="canvas-note-picker"><strong>Добавить заметку</strong>{index?.documents.slice(0, 80).map((document) => <button key={document.path} onClick={attempt(() => addFile(document.path))}><FileIcon /><span>{document.title}</span><small>{document.path}</small></button>)}{!index?.documents.length && <p>В vault пока нет заметок.</p>}</div>}
    {overview && <CanvasOverview nodes={visibleNodes} edges={visibleEdges} camera={camera} size={viewportSize} selected={selected} selectedEdge={selectedEdge} linking={linking} />}
    <div className="canvas-world" style={{ transform: `translate(${camera.x}px, ${camera.y}px) scale(${camera.zoom})` }}>
      {!native.active && !overview && <CanvasEdgeView edges={visibleEdges} selected={selectedEdge} zoom={camera.zoom}/>}
      {domNodes.map((node) => <div
        key={node.id}
        className={`canvas-node type-${node.type} ${canvasPrimitive(node)?'primitive-node':''} ${canvasAppearance(node).styled?'is-styled':''} ${node.color?'is-colored':''} ${locks.ids.has(node.id)?'is-locked':''} ${native.active ? 'native-editing' : ''} ${richCards ? 'rich-card' : ''} ${imageNodes.has(node.id) ? 'has-image-preview' : ''} ${linking === node.id ? 'link-source' : ''} ${selected.has(node.id) ? 'selected' : ''}`}
        data-node-id={node.id}
        style={{ left: node.x, top: node.y, width: node.width, height: node.height, color:canvasAppearance(node).text,fontSize:canvasAppearance(node).fontSize,textAlign:canvasAppearance(node).textAlign,backgroundColor:canvasPrimitive(node)?'transparent':native.active&&!domGeometry&&node.type==='group'?'transparent':canvasAppearance(node).fill, borderColor: selected.has(node.id) ? '#aca1ee' : canvasAppearance(node).border }}
        onPointerDown={(event) => nodePointerDown(event, node.id)}
        onDoubleClick={event=>{const p=screenToWorld(localPoint(event.clientX,event.clientY)),edge=node.type==='group'&&!groupHeaderHit(node,p)?hitEdge(p):undefined;if(edge&&canvasEdgeEditable(documentRef.current,edge.id)){selectEdge(edge.id);openEdgeStyle(edge.id);return}if(editableCanvasText(node)&&!locks.ids.has(node.id))setEditingId(node.id)}}
        onClick={(event) => {
          if (suppressNodeClick.current) { suppressNodeClick.current = false; return }
          if (linking !== null) chooseForLink(node.id)
          else if (node.type === 'file' && node.file && isRasterPath(node.file)&&!additiveSelection(event)) openFile(node.file)
          else if ((event.target as HTMLElement).closest('textarea, button, a')) setSelected((current) => additiveSelection(event)
            ? new Set(current.has(node.id) ? [...current].filter((item) => item !== node.id) : [...current, node.id])
            : new Set([node.id]))
        }}
      >
        <div className="canvas-node-grip"><span>{locks.ids.has(node.id)?'🔒 ':''}{node.type === 'text' ? 'Текст' : node.type === 'file' ? 'Заметка' : node.type === 'group' ? 'Группа' : 'Ссылка'}</span><button title="Удалить карточку" disabled={locks.ids.has(node.id)} onClick={(event) => { event.stopPropagation();attempt(()=>commit(deleteCanvasSelection(documentRef.current,new Set([node.id]))))() }}>×</button></div>
        {node.type === 'text' && (canvasPrimitive(node)?<CanvasPrimitiveView node={node} native={native.active&&!domGeometry} selected={selected.has(node.id)} editing={editingId===node.id} locked={locks.ids.has(node.id)} zoom={camera.zoom} onChange={text=>updateNode(node.id,{text})}/>:editingId!==node.id?(richCards?<div className="canvas-rich-text"><MarkdownPreview root={root} source={node.text??''} index={index} onOpen={path=>onOpen(path.split('#')[0])}/><small className="canvas-card-hint">{locks.ids.has(node.id)?'Заблокировано':'Двойной клик — редактировать'}</small></div>:<div className="canvas-plain-text">{node.text}</div>):<textarea readOnly={locks.ids.has(node.id)} autoFocus value={node.text ?? ''} onFocus={() => setSelected((current) => current.has(node.id) ? current : new Set([node.id]))} onChange={(event) => updateNode(node.id, { text: event.target.value })} />)}
        {node.type === 'file' && <div className={`canvas-file-card ${imageNodes.has(node.id)?'canvas-raster-card':''}`} role="button" tabIndex={0} onDoubleClick={() => node.file && openFile(node.file)} onKeyDown={(event) => { if (event.key === 'Enter' && node.file) { event.stopPropagation(); openFile(node.file) } }}>{imageNodes.has(node.id) && node.file ? <CanvasRasterContent root={root} node={node}/> : <FileIcon />}{!imageNodes.has(node.id)&&<strong>{node.file?.replace(/\.md$/i, '').split('/').at(-1) ?? 'Вложение'}</strong>}<small>{node.file}</small><CanvasFileProperties properties={noteProperties.get(node.file??'')}/></div>}
        {node.type === 'link' && <a href={node.url} target="_blank" rel="noreferrer">{node.url}</a>}
        {node.type === 'group' && <strong className="canvas-group-label">{node.label || 'Группа'}</strong>}
        {selected.has(node.id) && !locks.ids.has(node.id) && <div className="canvas-resize-handle" onPointerDown={(event) => resizePointerDown(event, node)} />}
      </div>)}
    </div>
    {attachmentPath && <AttachmentDetails root={root} path={attachmentPath} onClose={() => setAttachmentPath(null)} />}
    {styleTarget&&<CanvasStyleDialog nodes={styleTarget.nodes} onClose={()=>setStyleTarget(null)} onApply={patch=>{if(serializeCanvas(documentRef.current)!==styleTarget.source)throw Error('Доска изменилась. Закройте стиль и откройте его заново.');if(!canvasSelectionEditable(documentRef.current,styleTarget.ids))throw Error('Объекты заблокированы');commit(patchCanvasSelection(documentRef.current,styleTarget.ids,patch))}}/>}
    {edgeTarget&&<CanvasEdgeDialog edge={edgeTarget.edge} nodes={parsed.nodes} locked={locks.ids} onClose={()=>setEdgeTarget(null)} onApply={patch=>{if(serializeCanvas(documentRef.current)!==edgeTarget.source)throw Error('Доска изменилась. Закройте связь и откройте её заново.');commit(patchCanvasEdge(documentRef.current,edgeTarget.edge.id,patch))}}/>}
    {stickyStack&&<CanvasStickyStack onClose={()=>setStickyStack(false)} onApply={lines=>{const center=screenToWorld({x:viewportSize.width/2,y:viewportSize.height/2}),newNodes=lines.map((text,i)=>({...primitiveNode(nextId('sticky'),'sticky',{x:center.x+(i%3-1)*272,y:center.y+Math.floor(i/3)*272}),text})),document=documentRef.current;commit(insertCanvasNodes(document,newNodes));setSelected(new Set(newNodes.map(node=>node.id)))}}/>}
    {context && <CommandMenu x={context.x} y={context.y} registry={boardRegistry.current!} ids={context.create?CANVAS_CREATE_COMMANDS.map(c=>c.id):context.arrange?CANVAS_COMMANDS.filter(c=>!c.id.startsWith('canvas-add-')&&!c.id.startsWith('canvas-edge-')&&c.id!=='canvas-style').map(c=>c.id):context.edgeId?['canvas-edge-style','canvas-edge-reverse','canvas-edge-delete',null,'undo']: ['open','duplicate','copy','paste','delete',null,'canvas-group','canvas-ungroup','canvas-lock','canvas-front','canvas-back','canvas-style',null,'new-text','fit','undo']} onClose={(restore=true)=>{setContext(null);if(restore)viewport.current?.focus()}} onExecute={id=>{boardRegistry.current?.execute(id);setContext(null);viewport.current?.focus()}} />}
    {(operationError||locks.error)&&<div className="canvas-operation-error" role="alert">{operationError||locks.error}<button aria-label="Закрыть сообщение доски" onClick={()=>setOperationError('')}>×</button></div>}
    {native.active&&native.imageErrors>0&&<div className="canvas-raster-notice" role="status">Некоторые превью недоступны; оригиналы сохранены.<button onClick={native.retryImages}>Повторить превью</button></div>}
    {native.active&&Boolean(native.stats?.rasterLimited)&&<div className="canvas-raster-notice">Часть превью появится при приближении: одновременно до 64 разных изображений.</div>}
    {attachments.progress && <div className="canvas-import-status" onPointerDown={(event) => event.stopPropagation()}><span>{attachments.progress.name}</span><progress value={attachments.progress.copied} max={Math.max(1, attachments.progress.total)} /><button onClick={attachments.cancel}>Отменить</button></div>}
    {attachments.error && <div className="canvas-import-status" role="alert"><span>{attachments.error}</span><button onClick={attachments.dismissError}>Закрыть</button></div>}
    {attachments.dragOver && <div className="canvas-drop-hint">Добавить файлы на холст<span>Оригиналы будут скопированы в Attachments</span></div>}
    {selectionBox && <div className="canvas-selection-box" style={selectionStyle} />}
    {mini && <button className="canvas-minimap" title="Показать весь холст" onClick={fitToContent} style={{ width: mini.width, height: mini.height }}>
      <CanvasMiniMap nodes={parsed.nodes} selected={selected} mini={mini} />
    </button>}
    {!!selected.size && <div className="canvas-selection-actions"><span>{selected.size === 1 ? '1 объект' : `${selected.size} объекта`}</span>{selected.size === 2 && <button className="primary" onClick={attempt(connectSelected)}><LinkIcon />Связать</button>}<button disabled={boardRegistry.current.get('canvas-lock')?.enabled===false} onClick={()=>boardRegistry.current?.execute('canvas-lock')}>{boardRegistry.current.get('canvas-lock')?.label}</button><button onClick={()=>boardRegistry.current?.execute('duplicate')}>Дублировать</button><button disabled={!editable} onClick={()=>boardRegistry.current?.execute('delete')}>Удалить</button></div>}
    {selectedEdge&&<div className="canvas-selection-actions canvas-edge-actions"><span>{edgeEditable?'Связь':'Связь · объект заблокирован'}</span><button disabled={!edgeEditable} onClick={()=>boardRegistry.current?.execute('canvas-edge-style')}>Стиль связи</button><button disabled={!edgeEditable} onClick={()=>boardRegistry.current?.execute('canvas-edge-reverse')}>Развернуть</button><button disabled={!edgeEditable} onClick={()=>boardRegistry.current?.execute('canvas-edge-delete')}>Удалить связь</button></div>}
    {!parsed.nodes.length && <div className="canvas-empty"><strong>Свободное пространство</strong><p>Добавьте текстовую карточку или заметку. Собирайте идеи, связывайте их и раскладывайте без ограничений.</p><button onClick={attempt(addText)}><PlusIcon />Первая карточка</button></div>}
    <span hidden>{historyVersion}</span>
  </div>
}

function CanvasMiniMap({ nodes, selected, mini }: { nodes: CanvasNode[]; selected: Set<string>; mini: { width: number; height: number; scale: number; offsetX: number; offsetY: number; viewLeft: number; viewTop: number; viewWidth: number; viewHeight: number } }) {
  const canvas = useRef<HTMLCanvasElement>(null)
  const { width, height, scale, offsetX, offsetY } = mini
  useEffect(() => {
    const context = canvas.current?.getContext('2d')
    if (!context) return
    const ratio = window.devicePixelRatio
    canvas.current!.width = width * ratio; canvas.current!.height = height * ratio
    context.scale(ratio, ratio)
    for (const node of nodes) {
      context.fillStyle = selected.has(node.id) ? '#b3a6f0' : '#615c70'
      context.fillRect(node.x * scale + offsetX, node.y * scale + offsetY, Math.max(2, node.width * scale), Math.max(2, node.height * scale))
    }
  }, [nodes, selected, width, height, scale, offsetX, offsetY])
  return <><canvas ref={canvas} style={{ width, height }} /><i className="canvas-minimap-viewport" style={{ left: mini.viewLeft * scale + offsetX, top: mini.viewTop * scale + offsetY, width: mini.viewWidth * scale, height: mini.viewHeight * scale }} /></>
}
