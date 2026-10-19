export type CanvasNode = {
  id: string; type: 'text' | 'file' | 'link' | 'group'
  x: number; y: number; width: number; height: number
  text?: string; file?: string; url?: string; label?: string; color?: string
  [key: string]: unknown
}
export type CanvasEdge = {
  id: string; fromNode: string; toNode: string; fromSide?: string; toSide?: string
  fromEnd?: string; toEnd?: string; color?: string; label?: string
  [key: string]: unknown
}
export type CanvasDocument = { nodes: CanvasNode[]; edges: CanvasEdge[]; [key: string]: unknown }
export type CanvasPrimitive = 'sticky' | 'text' | 'rectangle' | 'ellipse' | 'diamond' | 'triangle' | 'divider'
export type CanvasInk={version:1;tool:'pen'|'highlighter';size:number;basis:number;pressure:boolean;points:[number,number,number][];[key:string]:unknown}
export type CanvasFramePreset='custom'|'a4'|'letter'|'16:9'|'4:3'|'1:1'|'mobile'|'tablet'|'desktop'
export type CanvasFrame={version:1;preset:CanvasFramePreset;notes?:string;[key:string]:unknown}
export type CanvasPresentation={version:1;frames:string[];[key:string]:unknown}
export type CanvasDocumentExtension={version:1;presentation?:CanvasPresentation;[key:string]:unknown}
export type NodeExtension = {
  version: 1; primitive?: CanvasPrimitive; locked?: boolean
  fontSize?: 12 | 16 | 24 | 32; textAlign?: 'left' | 'center' | 'right'; strokeWidth?: 1 | 2 | 4
  fillColor?: string; strokeColor?: string; textColor?: string
  taskId?: string
  ink?:CanvasInk
  frame?:CanvasFrame
  [key: string]: unknown
}
export function canvasExtension(node: CanvasNode | undefined): NodeExtension | undefined {
  return node?.brandon as NodeExtension | undefined
}
export function canvasPrimitive(node: CanvasNode): CanvasPrimitive | null {
  return canvasExtension(node)?.primitive ?? null
}
export function withCanvasExtension(node: CanvasNode, patch: Partial<NodeExtension>): CanvasNode {
  return {...node,brandon:{...canvasExtension(node),...patch,version:1}}
}
