import type {CanvasDocument,CanvasNode,NodeExtension} from './canvas-model'
import {canvasExtension,withCanvasExtension} from './canvas-model.ts'

const MAX_WORK=2_000_000
function contains(parent:CanvasNode,child:CanvasNode){
  return parent.id!==child.id&&child.x>=parent.x&&child.y>=parent.y&&child.x+child.width<=parent.x+parent.width&&child.y+child.height<=parent.y+parent.height&&(child.type!=='group'||child.width*child.height<parent.width*parent.height)
}
export function expandCanvasSelection(document:CanvasDocument,selected:ReadonlySet<string>):Set<string>{
  const ids=new Set(document.nodes.filter(n=>selected.has(n.id)).map(n=>n.id)),queue=document.nodes.filter(n=>ids.has(n.id)&&n.type==='group')
  let work=0
  for(let i=0;i<queue.length;i++)for(const child of document.nodes){
    if(++work>MAX_WORK)throw Error('Слишком сложная вложенность групп: операция отменена')
    if(!ids.has(child.id)&&contains(queue[i],child)){ids.add(child.id);if(child.type==='group')queue.push(child)}
  }
  return ids
}
export function lockedCanvasNodes(document:CanvasDocument){
  return expandCanvasSelection(document,new Set(document.nodes.filter(n=>canvasExtension(n)?.locked).map(n=>n.id)))
}
export function canvasSelectionEditable(document:CanvasDocument,selected:ReadonlySet<string>){
  if(!selected.size)return false
  const ids=expandCanvasSelection(document,selected),locked=lockedCanvasNodes(document)
  return ids.size>0&&![...ids].some(id=>locked.has(id))
}
export function canvasSelectionRoots(document:CanvasDocument,selected:ReadonlySet<string>){
  const nodes=document.nodes.filter(n=>selected.has(n.id)),groups=nodes.filter(n=>n.type==='group')
  let work=0
  return nodes.filter(n=>!groups.some(g=>{if(++work>MAX_WORK)throw Error('Слишком сложная вложенность групп: операция отменена');return contains(g,n)}))
}
function bounded(value:number){if(!Number.isFinite(value)||Math.abs(value)>10_000_000)throw Error('Геометрия выходит за пределы холста');return Math.round(value)}
function moved(document:CanvasDocument,offsets:Map<string,{x:number;y:number}>):CanvasDocument{
  let changed=false
  const nodes=document.nodes.map(node=>{
    const delta=offsets.get(node.id);if(!delta)return node
    const x=bounded(node.x+delta.x),y=bounded(node.y+delta.y)
    if(x===node.x&&y===node.y)return node
    changed=true;return {...node,x,y}
  })
  return changed?{...document,nodes}:document
}
export function moveCanvasNodes(document:CanvasDocument,selected:ReadonlySet<string>,dx:number,dy:number){
  if(!canvasSelectionEditable(document,selected))return document
  return moved(document,new Map([...expandCanvasSelection(document,selected)].map(id=>[id,{x:dx,y:dy}])))
}
export function patchCanvasSelection(document:CanvasDocument,selected:ReadonlySet<string>,patch:{color?:string;extension?:Partial<NodeExtension>}){
  if(!canvasSelectionEditable(document,selected))return document
  // Styles target explicit selection, not the contents of a group.
  return {...document,nodes:document.nodes.map(n=>selected.has(n.id)?(patch.extension?withCanvasExtension({...n,...(patch.color!==undefined?{color:patch.color}:{})},patch.extension):{...n,color:patch.color}):n)}
}
export function toggleCanvasLock(document:CanvasDocument,selected:ReadonlySet<string>){
  const nodes=canvasSelectionRoots(document,selected);if(!nodes.length)return document
  const unlock=nodes.every(n=>canvasExtension(n)?.locked===true)
  // A locked ancestor is not unlocked by targeting a child.
  const ancestorLocked=lockedCanvasNodes(document)
  if(!unlock&&nodes.some(n=>ancestorLocked.has(n.id)&&!canvasExtension(n)?.locked))return document
  const targets=new Set(nodes.map(n=>n.id))
  return {...document,nodes:document.nodes.map(n=>targets.has(n.id)?withCanvasExtension(n,{locked:!unlock}):n)}
}
export function deleteCanvasSelection(document:CanvasDocument,selected:ReadonlySet<string>){
  if(!canvasSelectionEditable(document,selected))return document
  const ids=expandCanvasSelection(document,selected)
  return {...document,nodes:document.nodes.filter(n=>!ids.has(n.id)),edges:document.edges.filter(e=>!ids.has(e.fromNode)&&!ids.has(e.toNode))}
}
export function groupCanvasSelection(document:CanvasDocument,selected:ReadonlySet<string>,id:string){
  if(!canvasSelectionEditable(document,selected)||selected.size<2)return document
  if(document.nodes.some(n=>n.id===id))throw Error('Повторяющийся ID группы')
  const members=document.nodes.filter(n=>selected.has(n.id)),left=Math.min(...members.map(n=>n.x))-24,top=Math.min(...members.map(n=>n.y))-40
  const right=Math.max(...members.map(n=>n.x+n.width))+24,bottom=Math.max(...members.map(n=>n.y+n.height))+24
  const group:CanvasNode={id,type:'group',x:bounded(left),y:bounded(top),width:bounded(right-left),height:bounded(bottom-top),label:'Группа'}
  const at=document.nodes.findIndex(n=>selected.has(n.id)),nodes=[...document.nodes];nodes.splice(at,0,group)
  return {...document,nodes}
}
export function ungroupCanvasSelection(document:CanvasDocument,selected:ReadonlySet<string>){
  if(!canvasSelectionEditable(document,selected))return document
  const ids=new Set(document.nodes.filter(n=>selected.has(n.id)&&n.type==='group').map(n=>n.id))
  if(!ids.size)return document
  return {...document,nodes:document.nodes.filter(n=>!ids.has(n.id)),edges:document.edges.filter(e=>!ids.has(e.fromNode)&&!ids.has(e.toNode))}
}
export function reorderCanvasSelection(document:CanvasDocument,selected:ReadonlySet<string>,front:boolean){
  if(!canvasSelectionEditable(document,selected))return document
  const ids=expandCanvasSelection(document,selected),picked=document.nodes.filter(n=>ids.has(n.id)),rest=document.nodes.filter(n=>!ids.has(n.id)),nodes=front?[...rest,...picked]:[...picked,...rest]
  return nodes.every((n,i)=>n===document.nodes[i])?document:{...document,nodes}
}
export type CanvasLayout='left'|'center-x'|'right'|'top'|'center-y'|'bottom'|'distribute-x'|'distribute-y'|'stack'|'grid'
export function layoutCanvasSelection(document:CanvasDocument,selected:ReadonlySet<string>,mode:CanvasLayout){
  if(!canvasSelectionEditable(document,selected))return document
  const roots=canvasSelectionRoots(document,selected)
  if(roots.length<2||mode.startsWith('distribute')&&roots.length<3)return document
  const left=Math.min(...roots.map(n=>n.x)),right=Math.max(...roots.map(n=>n.x+n.width)),top=Math.min(...roots.map(n=>n.y)),bottom=Math.max(...roots.map(n=>n.y+n.height)),offsets=new Map<string,{x:number;y:number}>()
  const horizontal=mode==='distribute-x',ordered=mode.startsWith('distribute')?[...roots].sort((a,b)=>(horizontal?a.x-b.x:a.y-b.y)||a.id.localeCompare(b.id)):roots
  const gap=mode.startsWith('distribute')?((horizontal?right-left:bottom-top)-roots.reduce((sum,n)=>sum+(horizontal?n.width:n.height),0))/(roots.length-1):24
  const columns=Math.ceil(Math.sqrt(roots.length)),cellWidth=Math.max(...roots.map(n=>n.width))+24,cellHeight=Math.max(...roots.map(n=>n.height))+24
  let cursor=horizontal?left:top
  ordered.forEach((root,index)=>{
    let x=root.x,y=root.y
    if(mode==='left')x=left;else if(mode==='right')x=right-root.width;else if(mode==='center-x')x=(left+right-root.width)/2
    else if(mode==='top')y=top;else if(mode==='bottom')y=bottom-root.height;else if(mode==='center-y')y=(top+bottom-root.height)/2
    else if(mode==='distribute-x'){x=cursor;cursor+=root.width+gap}else if(mode==='distribute-y'){y=cursor;cursor+=root.height+gap}
    else if(mode==='stack'){x=left;y=cursor;cursor+=root.height+24}else if(mode==='grid'){x=left+index%columns*cellWidth;y=top+Math.floor(index/columns)*cellHeight}
    const delta={x:x-root.x,y:y-root.y}
    for(const id of expandCanvasSelection(document,new Set([root.id]))){
      const prior=offsets.get(id)
      if(prior&&(prior.x!==delta.x||prior.y!==delta.y))throw Error('Пересекающиеся группы требуют раздельного выравнивания')
      offsets.set(id,delta)
    }
  })
  return moved(document,offsets)
}
