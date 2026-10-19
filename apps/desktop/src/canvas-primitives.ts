import type {CanvasNode,CanvasPrimitive} from './canvas-model'
import {canvasExtension,canvasPrimitive} from './canvas-model.ts'
import {canvasColors} from './canvas-document.ts'
import {canvasInk,inkHit} from './canvas-ink.ts'

export const CANVAS_PRIMITIVES:ReadonlyArray<{id:CanvasPrimitive;label:string;symbol:string}>=[
  {id:'sticky',label:'Стикер',symbol:'▰'},{id:'text',label:'Текст',symbol:'T'},
  {id:'rectangle',label:'Прямоугольник',symbol:'□'},{id:'ellipse',label:'Эллипс',symbol:'○'},
  {id:'diamond',label:'Ромб',symbol:'◇'},{id:'triangle',label:'Треугольник',symbol:'△'},
  {id:'divider',label:'Разделитель',symbol:'—'},
]
export type PrimitiveAppearance=ReturnType<typeof canvasAppearance>
export function canvasGroupHeaderHit(node:CanvasNode,point:{x:number;y:number}){return node.type==='group'&&point.y<=node.y+Math.min(64,node.height)}
/** Narrow-phase hits in paint order; transparent group bodies do not trap children. */
export function canvasTopHit(hits:CanvasNode[],point:{x:number;y:number}){let group:CanvasNode|undefined;for(let i=hits.length-1;i>=0;i--){const node=hits[i];if(node.type==='group'&&!canvasGroupHeaderHit(node,point)){group??=node;continue}return node}return group}
export function canvasAppearance(node:CanvasNode){
  const extension=canvasExtension(node),primitive=canvasPrimitive(node),base=canvasColors(node)
  const [horizontal,top,bottom]=primitive==='diamond'?[.25,.25,.25]:primitive==='triangle'?[.25,.5,.15]:primitive==='ellipse'?[.16,.16,.16]:[0,0,0]
  const padding=primitive==='text'?6:primitive?16:20
  const x=horizontal?node.width*horizontal:Math.min(padding,node.width/4),y=top?node.height*top:Math.min(padding,node.height/4)
  const right=x,lower=bottom?node.height*bottom:y
  const bounds={x,y,width:Math.max(Number.EPSILON,node.width-x-right),height:Math.max(Number.EPSILON,node.height-y-lower)}
  const shape=primitive==='ellipse'?3:primitive==='diamond'?4:primitive==='triangle'?5:primitive==='text'?6:primitive==='divider'?7:0
  const centerText=primitive==='rectangle'||primitive==='ellipse'||primitive==='diamond'||primitive==='triangle'
  const fill=primitive==='text'?'#00000000':extension?.fillColor==='transparent'?'#00000000':extension?.fillColor?(node.type==='group'?`${extension.fillColor}33`:extension.fillColor):base.fill
  return {primitive,shape,fill,border:extension?.strokeColor??base.border,text:extension?.textColor??'#dad8e0',fontSize:extension?.fontSize??(primitive==='text'?24:16),textAlign:extension?.textAlign??(centerText?'center':'left'),centerText,strokeWidth:extension?.strokeWidth??1,bounds,
    radius:primitive==='rectangle'||primitive==='divider'||primitive==='text'?0:primitive==='sticky'?4:8,
    clipPath:primitive==='ellipse'?'ellipse(50% 50% at 50% 50%)':primitive==='diamond'?'polygon(50% 0,100% 50%,50% 100%,0 50%)':primitive==='triangle'?'polygon(50% 0,100% 100%,0 100%)':undefined,
    styled:Boolean(extension&&(extension.primitive||extension.fontSize||extension.textAlign||extension.strokeWidth||extension.fillColor||extension.strokeColor||extension.textColor)),
  }
}
export function primitiveNode(id:string,primitive:CanvasPrimitive,center:{x:number;y:number}):CanvasNode{
  const width=primitive==='sticky'?240:primitive==='text'?320:primitive==='divider'?320:260,height=primitive==='sticky'?240:primitive==='text'?100:primitive==='divider'?32:180
  if(!CANVAS_PRIMITIVES.some(item=>item.id===primitive)||![center.x,center.y].every(value=>Number.isFinite(value)&&Math.abs(value)<9_000_000))throw Error('Некорректный инструмент или положение')
  return {id,type:'text',x:Math.round(center.x-width/2),y:Math.round(center.y-height/2),width,height,text:primitive==='divider'?'---':primitive==='sticky'?'Новая мысль':primitive==='text'?'Новый текст':'Идея',...(primitive==='sticky'?{color:'3'}:{}),brandon:{version:1,primitive}}
}
/** Exact interior tests; the spatial index remains a conservative broad phase. */
export function canvasNodeHit(node:CanvasNode,point:{x:number;y:number},tolerance=0){
  if(canvasInk(node))return inkHit(node,point,tolerance)
  const x=point.x-node.x,y=point.y-node.y,primitive=canvasPrimitive(node)
  if(x<0||x>node.width||y<0||y>node.height)return false
  const u=x/node.width,v=y/node.height
  if(primitive==='ellipse')return (u-.5)**2*4+(v-.5)**2*4<=1+1e-10
  if(primitive==='diamond')return Math.abs(u-.5)*2+Math.abs(v-.5)*2<=1+1e-10
  if(primitive==='triangle')return v>=Math.abs(u-.5)*2-1e-10
  if(primitive==='divider')return Math.abs(y-node.height/2)<=Math.max(tolerance,(canvasExtension(node)?.strokeWidth??1)/2)
  return true
}
export function canvasShapePath(node:CanvasNode){
  const primitive=canvasPrimitive(node),w=node.width,h=node.height
  if(primitive==='ellipse')return `M 0 ${h/2} a ${w/2} ${h/2} 0 1 0 ${w} 0 a ${w/2} ${h/2} 0 1 0 ${-w} 0 Z`
  if(primitive==='diamond')return `M ${w/2} 0 L ${w} ${h/2} L ${w/2} ${h} L 0 ${h/2} Z`
  if(primitive==='triangle')return `M ${w/2} 0 L ${w} ${h} L 0 ${h} Z`
  if(primitive==='divider')return `M 0 ${h/2} L ${w} ${h/2}`
  return `M 0 0 H ${w} V ${h} H 0 Z`
}
