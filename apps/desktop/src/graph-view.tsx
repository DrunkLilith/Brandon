import { useEffect, useMemo, useRef, useState, type PointerEvent, type WheelEvent } from 'react'
import { FitIcon, SearchIcon } from './icons'
import type { VaultIndex } from './types'

type GraphNode = {
  path: string
  title: string
  tags: string[]
  degree: number
  x: number
  y: number
}

type GraphEdge = { source: string; target: string }
type Camera = { x: number; y: number; zoom: number }

const WIDTH = 1200
const HEIGHT = 760

function hash(value: string) {
  let result = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    result ^= value.charCodeAt(index)
    result = Math.imul(result, 16777619)
  }
  return result >>> 0
}

function buildGraph(index: VaultIndex, mode: 'global' | 'local', activePath: string | null, query: string, showOrphans: boolean) {
  const documents = new Map(index.documents.map((document) => [document.path, document]))
  const edgeKeys = new Set<string>()
  const allEdges: GraphEdge[] = []
  for (const document of index.documents) {
    for (const link of document.links) {
      if (!link.resolvedPath || !documents.has(link.resolvedPath) || link.resolvedPath === document.path) continue
      const key = `${document.path}\u0000${link.resolvedPath}`
      if (edgeKeys.has(key)) continue
      edgeKeys.add(key)
      allEdges.push({ source: document.path, target: link.resolvedPath })
    }
  }

  let allowed = new Set(index.documents.map((document) => document.path))
  if (mode === 'local' && activePath) {
    allowed = new Set([activePath])
    for (const edge of allEdges) {
      if (edge.source === activePath) allowed.add(edge.target)
      if (edge.target === activePath) allowed.add(edge.source)
    }
  }
  const normalized = query.trim().toLocaleLowerCase('ru')
  if (normalized) {
    const matches = new Set(index.documents
      .filter((document) => `${document.title} ${document.path} ${document.tags.join(' ')}`.toLocaleLowerCase('ru').includes(normalized))
      .map((document) => document.path))
    const expanded = new Set(matches)
    for (const edge of allEdges) {
      if (matches.has(edge.source)) expanded.add(edge.target)
      if (matches.has(edge.target)) expanded.add(edge.source)
    }
    allowed = new Set([...allowed].filter((path) => expanded.has(path)))
  }

  let edges = allEdges.filter((edge) => allowed.has(edge.source) && allowed.has(edge.target))
  const degree = new Map<string, number>()
  for (const edge of edges) {
    degree.set(edge.source, (degree.get(edge.source) ?? 0) + 1)
    degree.set(edge.target, (degree.get(edge.target) ?? 0) + 1)
  }
  if (!showOrphans) allowed = new Set([...allowed].filter((path) => (degree.get(path) ?? 0) > 0))
  edges = edges.filter((edge) => allowed.has(edge.source) && allowed.has(edge.target))

  const selected = index.documents
    .filter((document) => allowed.has(document.path))
    .sort((left, right) => (degree.get(right.path) ?? 0) - (degree.get(left.path) ?? 0))
    .slice(0, 500)
  const selectedPaths = new Set(selected.map((document) => document.path))
  edges = edges.filter((edge) => selectedPaths.has(edge.source) && selectedPaths.has(edge.target))
  const count = Math.max(selected.length, 1)
  const nodes: GraphNode[] = selected.map((document, nodeIndex) => {
    const angle = (nodeIndex / count) * Math.PI * 2 + (hash(document.path) % 100) / 100
    const radius = 110 + ((hash(`${document.path}:radius`) % 1000) / 1000) * 220
    return {
      path: document.path,
      title: document.title,
      tags: document.tags,
      degree: degree.get(document.path) ?? 0,
      x: WIDTH / 2 + Math.cos(angle) * radius,
      y: HEIGHT / 2 + Math.sin(angle) * radius,
    }
  })

  if (nodes.length <= 220) {
    const byPath = new Map(nodes.map((node, nodeIndex) => [node.path, nodeIndex]))
    for (let iteration = 0; iteration < 130; iteration += 1) {
      const force = nodes.map(() => ({ x: 0, y: 0 }))
      for (let left = 0; left < nodes.length; left += 1) {
        for (let right = left + 1; right < nodes.length; right += 1) {
          const dx = nodes[right].x - nodes[left].x
          const dy = nodes[right].y - nodes[left].y
          const distanceSquared = Math.max(dx * dx + dy * dy, 80)
          const strength = 1500 / distanceSquared
          force[left].x -= dx * strength
          force[left].y -= dy * strength
          force[right].x += dx * strength
          force[right].y += dy * strength
        }
      }
      for (const edge of edges) {
        const sourceIndex = byPath.get(edge.source)
        const targetIndex = byPath.get(edge.target)
        if (sourceIndex === undefined || targetIndex === undefined) continue
        const source = nodes[sourceIndex]
        const target = nodes[targetIndex]
        const dx = target.x - source.x
        const dy = target.y - source.y
        const distance = Math.max(Math.hypot(dx, dy), 1)
        const strength = (distance - 115) * 0.0028
        force[sourceIndex].x += dx * strength
        force[sourceIndex].y += dy * strength
        force[targetIndex].x -= dx * strength
        force[targetIndex].y -= dy * strength
      }
      const cooling = 1 - iteration / 150
      nodes.forEach((node, nodeIndex) => {
        node.x += (force[nodeIndex].x + (WIDTH / 2 - node.x) * 0.002) * cooling
        node.y += (force[nodeIndex].y + (HEIGHT / 2 - node.y) * 0.002) * cooling
        node.x = Math.min(WIDTH - 70, Math.max(70, node.x))
        node.y = Math.min(HEIGHT - 55, Math.max(55, node.y))
      })
    }
  }
  return { nodes, edges, total:allowed.size }
}

export function GraphView({ index, activePath, onOpen }: {
  index: VaultIndex | null
  activePath: string | null
  onOpen: (path: string) => void
}) {
  const [mode, setMode] = useState<'global' | 'local'>('global')
  const [query, setQuery] = useState('')
  const [showOrphans, setShowOrphans] = useState(true)
  const [camera, setCamera] = useState<Camera>({ x: 0, y: 0, zoom: 1 })
  const graph = useMemo(() => index ? buildGraph(index, mode, activePath, query, showOrphans) : { nodes: [], edges: [], total:0 }, [activePath, index, mode, query, showOrphans])
  const [positions, setPositions] = useState<Record<string, { x: number; y: number }>>({})
  const drag = useRef<{ type: 'node' | 'pan'; path?: string; x: number; y: number; moved: boolean } | null>(null)
  const suppressClick = useRef(false)

  useEffect(() => setPositions(Object.fromEntries(graph.nodes.map((node) => [node.path, { x: node.x, y: node.y }]))), [graph])

  const beginNodeDrag = (event: PointerEvent<SVGGElement>, path: string) => {
    event.stopPropagation()
    event.currentTarget.setPointerCapture(event.pointerId)
    drag.current = { type: 'node', path, x: event.clientX, y: event.clientY, moved: false }
  }
  const beginPan = (event: PointerEvent<SVGSVGElement>) => {
    if (event.target !== event.currentTarget) return
    event.currentTarget.setPointerCapture(event.pointerId)
    drag.current = { type: 'pan', x: event.clientX, y: event.clientY, moved: false }
  }
  const movePointer = (event: PointerEvent<SVGSVGElement>) => {
    const current = drag.current
    if (!current) return
    const dx = event.clientX - current.x
    const dy = event.clientY - current.y
    if (Math.abs(dx) + Math.abs(dy) > 2) current.moved = true
    current.x = event.clientX
    current.y = event.clientY
    if (current.type === 'pan') setCamera((value) => ({ ...value, x: value.x + dx, y: value.y + dy }))
    else if (current.path) setPositions((value) => ({ ...value, [current.path!]: { x: value[current.path!].x + dx / camera.zoom, y: value[current.path!].y + dy / camera.zoom } }))
  }
  const endPointer = () => {
    suppressClick.current = drag.current?.moved ?? false
    drag.current = null
  }
  const zoom = (event: WheelEvent<SVGSVGElement>) => {
    event.preventDefault()
    setCamera((value) => ({ ...value, zoom: Math.min(2.4, Math.max(.35, value.zoom * (event.deltaY > 0 ? .9 : 1.1))) }))
  }

  return <div className="graph-view">
    <div className="graph-toolbar">
      <div className="graph-segmented"><button className={mode === 'global' ? 'active' : ''} onClick={() => setMode('global')}>Весь граф</button><button className={mode === 'local' ? 'active' : ''} disabled={!activePath} onClick={() => setMode('local')}>Локальный</button></div>
      <label className="graph-search"><SearchIcon /><input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Фильтр узлов…" /></label>
      <label className="graph-check"><input type="checkbox" checked={showOrphans} onChange={(event) => setShowOrphans(event.target.checked)} />Без связей</label>
      <span className="graph-count" title="Граф использует общие метаданные всего vault. Для читаемости показаны до 500 наиболее связанных узлов; сузьте фильтр или откройте локальный граф.">{graph.nodes.length}{graph.total>graph.nodes.length?` из ${graph.total}`:''} узлов · {graph.edges.length} связей</span>
      <button className="graph-tool" title="Вписать граф" onClick={() => setCamera({ x: 0, y: 0, zoom: 1 })}><FitIcon /></button>
      <button className="graph-tool" title="Уменьшить" onClick={() => setCamera((value) => ({ ...value, zoom: Math.max(.35, value.zoom / 1.18) }))}>−</button>
      <button className="graph-tool" title="Увеличить" onClick={() => setCamera((value) => ({ ...value, zoom: Math.min(2.4, value.zoom * 1.18) }))}>+</button>
    </div>
    {graph.nodes.length ? <svg className="graph-canvas" viewBox={`0 0 ${WIDTH} ${HEIGHT}`} onPointerDown={beginPan} onPointerMove={movePointer} onPointerUp={endPointer} onPointerCancel={endPointer} onWheel={zoom}>
      <g transform={`translate(${camera.x} ${camera.y}) scale(${camera.zoom})`}>
        <g className="graph-edges">{graph.edges.map((edge) => {
          const source = positions[edge.source]
          const target = positions[edge.target]
          return source && target ? <line key={`${edge.source}:${edge.target}`} x1={source.x} y1={source.y} x2={target.x} y2={target.y} /> : null
        })}</g>
        <g className="graph-nodes">{graph.nodes.map((node) => {
          const point = positions[node.path] ?? node
          const radius = Math.min(13, 5 + Math.sqrt(node.degree) * 1.8)
          const highlighted = node.path === activePath || (!!query && `${node.title} ${node.path}`.toLocaleLowerCase('ru').includes(query.toLocaleLowerCase('ru')))
          return <g key={node.path} className={highlighted ? 'highlighted' : ''} transform={`translate(${point.x} ${point.y})`} onPointerDown={(event) => beginNodeDrag(event, node.path)} onClick={() => { if (suppressClick.current) { suppressClick.current = false; return }; onOpen(node.path) }}>
            <circle r={radius} /><text x={radius + 7} y="4">{node.title}</text><title>{node.path}{node.tags.length ? `\n#${node.tags.join(' #')}` : ''}</title>
          </g>
        })}</g>
      </g>
    </svg> : <div className="graph-empty"><span>∙</span><strong>Здесь пока нечего связывать</strong><p>Добавьте в заметки ссылки вида <code>[[Другая заметка]]</code> — граф появится автоматически.</p></div>}
  </div>
}
