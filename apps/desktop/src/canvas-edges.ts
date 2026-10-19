import type {CanvasDocument,CanvasEdge,CanvasNode} from './canvas-model'
import {canvasPrimitive} from './canvas-model.ts'
import {canvasColors,parseCanvasDocument} from './canvas-document.ts'
import {lockedCanvasNodes} from './canvas-operations.ts'

export type EdgeSide='top'|'right'|'bottom'|'left'
export type EdgeEnd='none'|'arrow'
export type EdgeExtension={version:1;dash?:'solid'|'dashed';strokeWidth?:1|2|4;[key:string]:unknown}
type Point={x:number;y:number}
export type EdgeSegment=[number,number,number,number]
export type PreparedCanvasEdge={id:string;fromNode:string;toNode:string;x:number;y:number;width:number;height:number;segments:EdgeSegment[];heads:EdgeSegment[];color:string;strokeWidth:number;dashed:boolean;label:string;labelRect:{x:number;y:number;width:number;height:number}}
export type CanvasEdgePatch=Partial<Pick<CanvasEdge,'fromNode'|'toNode'|'fromSide'|'toSide'|'fromEnd'|'toEnd'|'color'|'label'>>&{extension?:Partial<EdgeExtension>}
export const EDGE_SIDES:ReadonlyArray<{id:EdgeSide;label:string}>=[{id:'top',label:'Сверху'},{id:'right',label:'Справа'},{id:'bottom',label:'Снизу'},{id:'left',label:'Слева'}]
export function edgeExtension(edge:CanvasEdge){return edge.brandon as EdgeExtension|undefined}
function facing(node:CanvasNode,other:CanvasNode):EdgeSide{const dx=other.x+other.width/2-node.x-node.width/2,dy=other.y+other.height/2-node.y-node.height/2;return Math.abs(dx)>=Math.abs(dy)?dx>=0?'right':'left':dy>=0?'bottom':'top'}
export function canvasEdgeAnchor(node:CanvasNode,side:EdgeSide):Point{
  const triangle=canvasPrimitive(node)==='triangle',divider=canvasPrimitive(node)==='divider'
  const u=side==='left'?(triangle?.25:0):side==='right'?(triangle?.75:1):.5,v=divider?.5:side==='top'?0:side==='bottom'?1:.5
  return{x:node.x+node.width*u,y:node.y+node.height*v}
}
const segment=(a:Point,b:Point):EdgeSegment=>[a.x,a.y,b.x,b.y]
const length=(s:EdgeSegment)=>Math.hypot(s[2]-s[0],s[3]-s[1])
function wings(tip:Point,other:Point):EdgeSegment[]{const d=Math.hypot(tip.x-other.x,tip.y-other.y);if(d<.001)return[];const ux=(tip.x-other.x)/d,uy=(tip.y-other.y)/d,size=Math.min(12,d*.4),spread=size*.45;return[-1,1].map(sign=>[tip.x-ux*size-uy*spread*sign,tip.y-uy*size+ux*spread*sign,tip.x,tip.y])}
function labelLayout(source:string){
  const chars=Array.from(source.replace(/\s+/gu,' ').trim()),truncated=chars.length>48,text=chars.slice(0,48).join('')+(truncated?'…':'')
  const lines=Array.from(text),split=lines.length>24?24:lines.length
  return{label:lines.slice(0,split).join('')+(lines.length>split?'\n'+lines.slice(split).join(''):''),width:Math.min(240,Math.max(64,Math.min(24,lines.length)*9+20)),height:lines.length>24?50:30}
}
export function prepareCanvasEdge(edge:CanvasEdge,from:CanvasNode,to:CanvasNode):PreparedCanvasEdge{
  const a=canvasEdgeAnchor(from,(edge.fromSide as EdgeSide|undefined)??facing(from,to)),b=canvasEdgeAnchor(to,(edge.toSide as EdgeSide|undefined)??facing(to,from))
  let points=[a,b]
  if(Math.hypot(a.x-b.x,a.y-b.y)<.001){const right=Math.max(from.x+from.width,to.x+to.width)+48,top=Math.min(from.y,to.y)-48;points=[a,{x:right,y:a.y},{x:right,y:top},{x:a.x,y:top},b]}
  const segments=points.slice(1).map((p,i)=>segment(points[i],p)).filter(s=>length(s)>.001),heads=[...(edge.fromEnd==='arrow'?wings(points[0],points[1]):[]),...(edge.toEnd!=='none'?wings(points.at(-1)!,points.at(-2)!):[])]
  const total=segments.reduce((sum,s)=>sum+length(s),0);let remaining=total/2,mid={...a}
  for(const s of segments){const d=length(s);if(remaining<=d){mid={x:s[0]+(s[2]-s[0])*remaining/d,y:s[1]+(s[3]-s[1])*remaining/d};break}remaining-=d}
  const layout=labelLayout(edge.label??''),labelRect={x:mid.x-layout.width/2,y:mid.y-layout.height/2,width:layout.width,height:layout.height},all=[...segments,...heads].flatMap(s=>[{x:s[0],y:s[1]},{x:s[2],y:s[3]}]);if(layout.label)all.push({x:labelRect.x,y:labelRect.y},{x:labelRect.x+labelRect.width,y:labelRect.y+labelRect.height})
  const x=Math.min(...all.map(p=>p.x))-4,y=Math.min(...all.map(p=>p.y))-4,right=Math.max(...all.map(p=>p.x))+4,bottom=Math.max(...all.map(p=>p.y))+4,extension=edgeExtension(edge)
  return{id:edge.id,fromNode:edge.fromNode,toNode:edge.toNode,x,y,width:right-x,height:bottom-y,segments,heads,color:edge.color?canvasColors({type:'text',color:edge.color}).border:'#817aab',strokeWidth:extension?.strokeWidth??2,dashed:extension?.dash==='dashed',label:layout.label,labelRect}
}
export function prepareCanvasEdges(document:CanvasDocument){const nodes=new Map(document.nodes.map(n=>[n.id,n]));return document.edges.flatMap(edge=>{const from=nodes.get(edge.fromNode),to=nodes.get(edge.toNode);return from&&to?[prepareCanvasEdge(edge,from,to)]:[]})}
export function samePreparedEdge(a:PreparedCanvasEdge|undefined,b:PreparedCanvasEdge){return a!==undefined&&a.id===b.id&&a.color===b.color&&a.strokeWidth===b.strokeWidth&&a.dashed===b.dashed&&a.label===b.label&&a.labelRect.x===b.labelRect.x&&a.labelRect.y===b.labelRect.y&&a.labelRect.width===b.labelRect.width&&a.labelRect.height===b.labelRect.height&&a.fromNode===b.fromNode&&a.toNode===b.toNode&&a.segments.length===b.segments.length&&a.heads.length===b.heads.length&&a.segments.every((s,i)=>s.every((v,j)=>v===b.segments[i][j]))&&a.heads.every((s,i)=>s.every((v,j)=>v===b.heads[i][j]))}
export function canvasEdgeHit(edge:PreparedCanvasEdge,p:Point,tolerance:number){
  const r=edge.labelRect;if(edge.label&&p.x>=r.x&&p.y>=r.y&&p.x<=r.x+r.width&&p.y<=r.y+r.height)return true
  return[...edge.segments,...edge.heads].some(s=>{const dx=s[2]-s[0],dy=s[3]-s[1],t=Math.max(0,Math.min(1,((p.x-s[0])*dx+(p.y-s[1])*dy)/(dx*dx+dy*dy)));return Math.hypot(p.x-s[0]-t*dx,p.y-s[1]-t*dy)<=tolerance})
}
export function canvasEdgeEditable(document:CanvasDocument,id:string){const edge=document.edges.find(e=>e.id===id);if(!edge)return false;const locked=lockedCanvasNodes(document),ids=new Set(document.nodes.map(n=>n.id));return ids.has(edge.fromNode)&&ids.has(edge.toNode)&&!locked.has(edge.fromNode)&&!locked.has(edge.toNode)}
export function patchCanvasEdge(document:CanvasDocument,id:string,patch:CanvasEdgePatch){
  if(!canvasEdgeEditable(document,id))throw Error('Связь отсутствует или её объект заблокирован')
  const current=document.edges.find(e=>e.id===id)!,{extension,...fields}=patch,nextEdge={...current,...fields,...(extension?{brandon:{...edgeExtension(current),...extension,version:1}}:{})},next={...document,edges:document.edges.map(edge=>edge.id===id?nextEdge:edge)},checked=parseCanvasDocument(JSON.stringify(next))
  if(checked.error)throw Error(checked.error);if(!canvasEdgeEditable(next,id))throw Error('Нельзя подключить связь к отсутствующему или заблокированному объекту');return next
}
export function reverseCanvasEdge(document:CanvasDocument,id:string){const edge=document.edges.find(e=>e.id===id);if(!edge)return document;return patchCanvasEdge(document,id,{fromNode:edge.toNode,toNode:edge.fromNode,fromSide:edge.toSide,toSide:edge.fromSide,fromEnd:edge.toEnd??'arrow',toEnd:edge.fromEnd??'none'})}
export function deleteCanvasEdge(document:CanvasDocument,id:string){if(!canvasEdgeEditable(document,id))throw Error('Связь отсутствует или её объект заблокирован');return{...document,edges:document.edges.filter(e=>e.id!==id)}}
export function connectCanvasNodes(document:CanvasDocument,fromNode:string,toNode:string,id:string){const next={...document,edges:[...document.edges,{id,fromNode,toNode}]},checked=parseCanvasDocument(JSON.stringify(next));if(checked.error)throw Error(checked.error);if(!canvasEdgeEditable(next,id))throw Error('Нельзя связать отсутствующие или заблокированные объекты');return next}
