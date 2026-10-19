import { useLayoutEffect, useRef } from 'react'
import type { CanvasNode } from './canvas-view'
import {canvasAppearance,canvasShapePath} from './canvas-primitives'

export type RenderEdge = { id: string; x: number; y: number; width: number; height: number; x1: number; y1: number; x2: number; y2: number }

/** Far overview: one bitmap layer instead of thousands of off-scale textareas. */
export function CanvasOverview({ nodes, edges, camera, size, selected, linking }: {
  nodes: CanvasNode[]; edges: RenderEdge[]; camera: { x: number; y: number; zoom: number }
  size: { width: number; height: number }; selected: Set<string>; linking: string | null
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
    context.strokeStyle = '#817aab'; context.lineWidth = 1 / camera.zoom
    context.beginPath()
    for (const edge of edges) { context.moveTo(edge.x1, edge.y1); context.lineTo(edge.x2, edge.y2) }
    context.stroke()
    for (const node of nodes) {
      const a=canvasAppearance(node)
      context.save();context.translate(node.x,node.y)
      const path=new Path2D(canvasShapePath(node))
      context.fillStyle=a.fill
      if(a.primitive!=='text'&&a.primitive!=='divider')context.fill(path)
      context.strokeStyle=selected.has(node.id)||linking===node.id?'#aca1ee':a.border
      context.lineWidth=a.primitive?Math.max(a.strokeWidth,selected.has(node.id)?2/camera.zoom:0):1/camera.zoom
      if(a.primitive!=='text'||selected.has(node.id))context.stroke(path)
      context.restore()
    }
  }, [nodes, edges, camera, size, selected, linking])
  return <canvas className="canvas-overview" ref={canvas} aria-hidden="true" />
}
