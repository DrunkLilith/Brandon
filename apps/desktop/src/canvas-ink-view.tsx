import {useMemo} from 'react'
import type {CanvasNode} from './canvas-model'
import {prepareCanvasInk,inkPath} from './canvas-ink'
import {canvasAppearance} from './canvas-primitives'
export function CanvasInkView({node}:{node:CanvasNode}){
  const color=canvasAppearance(node).border,shape=useMemo(()=>{const ink=prepareCanvasInk(node,color)!;return{path:inkPath(ink),opacity:ink.opacity}},[node,color])
  return <svg className="canvas-ink-path" viewBox={`0 0 ${node.width} ${node.height}`} preserveAspectRatio="none" aria-hidden="true"><path d={shape.path} fill={color} fillRule="nonzero" opacity={shape.opacity} style={{pointerEvents:'visiblePainted'}}/></svg>
}
