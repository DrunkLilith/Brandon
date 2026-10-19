import { useLayoutEffect, useRef } from 'react'
import type { CanvasNode } from './canvas-view'
import {canvasAppearance,canvasShapePath} from './canvas-primitives'
import type {PreparedCanvasEdge} from './canvas-edges'
import {prepareCanvasInk,inkPath} from './canvas-ink'

export type RenderEdge = PreparedCanvasEdge

/** Far overview: one bitmap layer instead of thousands of off-scale textareas. */
export function CanvasOverview({ nodes, edges, camera, size, selected, selectedEdge, linking }: {
  nodes: CanvasNode[]; edges: RenderEdge[]; camera: { x: number; y: number; zoom: number }
  size: { width: number; height: number }; selected: Set<string>; selectedEdge:string|null; linking: string | null
}) {
  const canvas = useRef<HTMLCanvasElement>(null)
  useLayoutEffect(() => {
    const element = canvas.current
    const context = element?.getContext('2d', { alpha: true })
    if (!element || !context) return
    const ratio = window.devicePixelRatio
    const width = Math.round(size.width * ratio), height = Math.round(size.height * ratio)
    if (element.width !== width || element.height !== height) { element.width = width; element.height = height }
    context.setTransform(1, 0, 0, 1, 0, 0)
    context.clearRect(0, 0, width, height)
    context.setTransform(ratio * camera.zoom, 0, 0, ratio * camera.zoom, (size.width / 2 + camera.x) * ratio, (size.height / 2 + camera.y) * ratio)
    for(const edge of edges){context.strokeStyle=edge.id===selectedEdge?'#b8a9ef':edge.color;context.lineWidth=Math.max(edge.strokeWidth,.75/camera.zoom);context.setLineDash(edge.dashed?[8,6]:[]);context.beginPath();context.moveTo(edge.segments[0][0],edge.segments[0][1]);for(const s of edge.segments){context.lineTo(s[2],s[3])}context.stroke();context.setLineDash([]);context.beginPath();for(const s of edge.heads){context.moveTo(s[0],s[1]);context.lineTo(s[2],s[3])}context.stroke()}
    for (const node of nodes) {
      const a=canvasAppearance(node)
      context.save();context.translate(node.x,node.y)
      const ink=prepareCanvasInk(node,a.border)
      if(ink){context.globalAlpha=ink.opacity;context.fillStyle=ink.color;context.fill(new Path2D(inkPath(ink)));context.globalAlpha=1;if(selected.has(node.id)){context.strokeStyle='#aca1ee';context.lineWidth=1/camera.zoom;context.strokeRect(0,0,node.width,node.height)}context.restore();continue}
      const path=new Path2D(canvasShapePath(node))
      context.fillStyle=a.fill
      if(a.primitive!=='text'&&a.primitive!=='divider')context.fill(path)
      context.strokeStyle=selected.has(node.id)||linking===node.id?'#aca1ee':a.border
      context.lineWidth=a.primitive?Math.max(a.strokeWidth,selected.has(node.id)?2/camera.zoom:0):1/camera.zoom
      if(a.primitive!=='text'||selected.has(node.id))context.stroke(path)
      context.restore()
    }
  }, [nodes, edges, camera, size, selected, selectedEdge, linking])
  return <canvas className="canvas-overview" ref={canvas} aria-hidden="true" />
}
