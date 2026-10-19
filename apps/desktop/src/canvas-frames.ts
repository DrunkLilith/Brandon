import type {CanvasDocument,CanvasNode,CanvasFrame,CanvasFramePreset,CanvasPresentation,CanvasDocumentExtension} from './canvas-model'
import {canvasExtension,withCanvasExtension} from './canvas-model.ts'
import {canvasSelectionEditable,expandCanvasSelection,insertCanvasNodes} from './canvas-operations.ts'
import {parseCanvasDocument} from './canvas-document.ts'

export const FRAME_LIMIT=512,FRAME_ORDER_LIMIT=4096,FRAME_NOTES_LIMIT=16384
export const FRAME_OBJECT_LIMIT=2000
export const CANVAS_FRAME_PRESETS:ReadonlyArray<{id:CanvasFramePreset;label:string;width:number;height:number}>=[
  {id:'custom',label:'Свободный',width:960,height:640},{id:'a4',label:'A4',width:794,height:1123},{id:'letter',label:'Letter',width:816,height:1056},
  {id:'16:9',label:'16 : 9',width:1280,height:720},{id:'4:3',label:'4 : 3',width:1024,height:768},{id:'1:1',label:'1 : 1',width:900,height:900},
  {id:'mobile',label:'Телефон',width:390,height:844},{id:'tablet',label:'Планшет',width:768,height:1024},{id:'desktop',label:'Экран',width:1440,height:900},
]
const object=(value:unknown):value is Record<string,unknown>=>Boolean(value)&&typeof value==='object'&&!Array.isArray(value)
export function validateCanvasFrame(value:unknown):asserts value is CanvasFrame{
  if(!object(value)||value.version!==1||!CANVAS_FRAME_PRESETS.some(p=>p.id===value.preset)||value.notes!==undefined&&(typeof value.notes!=='string'||value.notes.length>FRAME_NOTES_LIMIT))throw Error('Неизвестный или некорректный фрейм Brandon; исходный файл не изменён')
}
export function validateCanvasPresentation(value:unknown){
  if(value===undefined)return
  if(!object(value)||value.version!==1)throw Error('Неизвестная версия документа Brandon; исходный файл доступен только для чтения')
  const p=value.presentation;if(p===undefined)return
  if(!object(p)||p.version!==1||!Array.isArray(p.frames)||p.frames.length>FRAME_ORDER_LIMIT||p.frames.some(id=>typeof id!=='string'||!id||id.length>512)||new Set(p.frames).size!==p.frames.length)throw Error('Неизвестный или некорректный порядок фреймов Brandon; исходный файл не изменён')
}
export const canvasFrame=(node:CanvasNode|undefined)=>canvasExtension(node)?.frame
export const canvasPresentation=(document:CanvasDocument)=>(document.brandon as CanvasDocumentExtension|undefined)?.presentation
export function orderedCanvasFrames(document:CanvasDocument){
  const frames=document.nodes.filter(n=>canvasFrame(n)),byId=new Map(frames.map(n=>[n.id,n])),result:CanvasNode[]=[]
  for(const id of canvasPresentation(document)?.frames??[]){const frame=byId.get(id);if(frame){result.push(frame);byId.delete(id)}}
  return[...result,...frames.filter(n=>byId.has(n.id))]
}
export function validateFrameBudget(document:CanvasDocument){if(document.nodes.filter(n=>canvasFrame(n)).length>FRAME_LIMIT)throw Error('Лимит 512 фреймов; исходный файл не обрезан')}
const checked=(document:CanvasDocument)=>{const parsed=parseCanvasDocument(JSON.stringify(document));if(parsed.error)throw Error(parsed.error);return document}
function withOrder(document:CanvasDocument,frames:string[]){
  const extension=document.brandon as CanvasDocumentExtension|undefined,presentation:CanvasPresentation={...extension?.presentation,version:1,frames}
  return{...document,brandon:{...extension,version:1,presentation}}
}
function presetSize(preset:CanvasFramePreset){const value=CANVAS_FRAME_PRESETS.find(p=>p.id===preset);if(!value)throw Error('Неизвестный размер фрейма');return value}
function title(value:string){if(!value.trim()||value.length>512)throw Error('Название фрейма должно содержать от 1 до 512 символов');return value.trim()}
/** No copied nodes or source data: a frame is an ordinary geometric group. */
export function createCanvasFrame(document:CanvasDocument,selected:ReadonlySet<string>,id:string,preset:CanvasFramePreset,label:string,center:{x:number;y:number}){
  if(!id||id.length>512)throw Error('Некорректный ID фрейма')
  if(selected.size&&!canvasSelectionEditable(document,selected))throw Error('Выбранные объекты или их потомки заблокированы')
  const definition=presetSize(preset),members=selected.size?expandCanvasSelection(document,selected):new Set<string>(),nodes=document.nodes.filter(n=>members.has(n.id))
  if(selected.size&&!nodes.length)throw Error('Выделение больше не существует')
  let width=definition.width,height=definition.height,cx=center.x,cy=center.y
  if(nodes.length){
    const x=Math.min(...nodes.map(n=>n.x))-48,y=Math.min(...nodes.map(n=>n.y))-48,right=Math.max(...nodes.map(n=>n.x+n.width))+48,bottom=Math.max(...nodes.map(n=>n.y+n.height))+48
    cx=(x+right)/2;cy=(y+bottom)/2;width=right-x;height=bottom-y
    if(preset!=='custom'){const ratio=definition.width/definition.height;if(width/height>ratio)height=width/ratio;else width=height*ratio}
  }
  const frame:CanvasNode={id,type:'group',x:Math.floor(cx-width/2),y:Math.floor(cy-height/2),width:Math.ceil(width),height:Math.ceil(height),label:title(label),brandon:{version:1,frame:{version:1,preset,notes:''}}}
  const inserted=insertCanvasNodes(document,[frame]),at=nodes.length?inserted.nodes.findIndex(n=>members.has(n.id)):0,without=inserted.nodes.filter(n=>n.id!==id)
  without.splice(at,0,frame)
  return checked(withOrder({...inserted,nodes:without},[...(canvasPresentation(document)?.frames??[]),id]))
}
export function editCanvasFrame(document:CanvasDocument,id:string,patch:{label?:string;notes?:string}){
  const selected=new Set([id]),node=document.nodes.find(n=>n.id===id),frame=canvasFrame(node)
  if(!node||!frame)throw Error('Фрейм больше не существует')
  if(!canvasSelectionEditable(document,selected))throw Error('Фрейм или его потомки заблокированы')
  const next=withCanvasExtension({...node,...(patch.label!==undefined?{label:title(patch.label)}:{})},{frame:{...frame,...(patch.notes!==undefined?{notes:patch.notes}:{})}})
  if(JSON.stringify(node)===JSON.stringify(next))return document
  return checked({...document,nodes:document.nodes.map(n=>n.id===id?next:n)})
}
/** Reorder slides, not node paint order. Dangling slots are intentionally kept. */
export function reorderCanvasFrames(document:CanvasDocument,id:string,direction:-1|1){
  const ordered=orderedCanvasFrames(document),index=ordered.findIndex(n=>n.id===id),neighbor=ordered[index+direction]
  if(index<0)throw Error('Фрейм больше не существует')
  if(!canvasSelectionEditable(document,new Set([id])))throw Error('Фрейм или его потомки заблокированы')
  if(!neighbor)return document
  const order=[...(canvasPresentation(document)?.frames??[])],known=new Set(order)
  for(const frame of ordered)if(!known.has(frame.id)){order.push(frame.id);known.add(frame.id)}
  const a=order.indexOf(id),b=order.indexOf(neighbor.id);[order[a],order[b]]=[order[b],order[a]]
  return checked(withOrder(document,order))
}
export type FrameCamera={x:number;y:number;zoom:number}
export function assertCanvasFramePresentable(document:CanvasDocument,id:string){
  const frame=document.nodes.find(n=>n.id===id&&canvasFrame(n));if(!frame)throw Error('Фрейм больше не существует')
  let count=0
  for(const node of document.nodes)if(node.x<=frame.x+frame.width&&node.x+node.width>=frame.x&&node.y<=frame.y+frame.height&&node.y+node.height>=frame.y&&++count>FRAME_OBJECT_LIMIT)throw Error('До 2000 объектов на один показываемый фрейм. Разделите содержимое на несколько фреймов; файл не изменён.')
  return frame
}
export function fitCanvasFrame(frame:CanvasNode,size:{width:number;height:number},padding=48):FrameCamera{
  if(![frame.x,frame.y,frame.width,frame.height,size.width,size.height,padding].every(Number.isFinite)||frame.width<=0||frame.height<=0||size.width<=0||size.height<=0||padding<0)throw Error('Некорректная область показа фрейма')
  const zoom=Math.min(3,Math.max(.000001,Math.min(Math.max(1,size.width-padding*2)/frame.width,Math.max(1,size.height-padding*2)/frame.height)))
  return{x:-(frame.x+frame.width/2)*zoom,y:-(frame.y+frame.height/2)*zoom,zoom}
}
export function canvasFrameScreenRect(frame:CanvasNode,camera:FrameCamera,size:{width:number;height:number}){
  return{x:size.width/2+camera.x+frame.x*camera.zoom,y:size.height/2+camera.y+frame.y*camera.zoom,width:frame.width*camera.zoom,height:frame.height*camera.zoom}
}
