import {useEffect,useMemo,useRef,useState,type PointerEvent} from 'react'
import type {CanvasDocument} from './canvas-model'
import {canvasInk,inkNode,inkSweptHit,INK_POINT_LIMIT,type InkSample} from './canvas-ink'
import {CanvasSpatialIndex} from './canvas-spatial'
import {insertCanvasNodes,deleteCanvasSelection} from './canvas-operations'
export type InkTool='pen'|'highlighter'|'eraser'
type Point={x:number;y:number}
type Inputs={document:CanvasDocument;locked:ReadonlySet<string>;disabled:boolean;zoom:number;toWorld:(x:number,y:number)=>Point;onPreview:(document:CanvasDocument|null)=>void;onCommit:(document:CanvasDocument,createdId?:string)=>void;onError:(message:string)=>void}
type Gesture={pointerId:number;pointerType:string;base:CanvasDocument;tool:InkTool;id:string;size:number;color:string;pressure:boolean;samples:InkSample[];last:Point;removed:Set<string>;locked:ReadonlySet<string>;work:number}
const isUi=(target:EventTarget|null)=>target instanceof Element&&Boolean(target.closest('.canvas-toolbar,.canvas-ink-options,.canvas-note-picker,.canvas-minimap,.command-menu,.canvas-selection-actions,.canvas-operation-error,.dialog-backdrop,.image-viewer-backdrop,.canvas-attachment-details,.canvas-import-status,.canvas-raster-notice'))
export function useCanvasInk(inputs:Inputs){
  const [mode,setMode]=useState<InkTool|null>(null),[penSize,setPenSize]=useState(4),[markerSize,setMarkerSize]=useState(16),[eraserSize,setEraserSize]=useState(24),[color,setColor]=useState('#c3a4dc'),[pressure,setPressure]=useState(true)
  const size=mode==='highlighter'?markerSize:penSize,setSize=mode==='highlighter'?setMarkerSize:setPenSize
  const latest=useRef(inputs);latest.current=inputs
  const gesture=useRef<Gesture|null>(null),raf=useRef(0),suppressClick=useRef(false)
  const spatial=useMemo(()=>new CanvasSpatialIndex(inputs.document.nodes.filter(node=>canvasInk(node))),[inputs.document]),spatialRef=useRef(spatial);spatialRef.current=spatial
  const cancel=()=>{gesture.current=null;cancelAnimationFrame(raf.current);raf.current=0;latest.current.onPreview(null)}
  const fail=(error:unknown)=>{cancel();latest.current.onError(error instanceof Error?error.message:String(error))}
  const preview=()=>{raf.current=0;const g=gesture.current;if(!g)return;if(latest.current.document!==g.base){cancel();return}try{latest.current.onPreview(g.tool==='eraser'?{...g.base,nodes:g.base.nodes.filter(node=>!g.removed.has(node.id)),edges:g.base.edges.filter(edge=>!g.removed.has(edge.fromNode)&&!g.removed.has(edge.toNode))}:{...g.base,nodes:[...g.base.nodes,inkNode(g.id,g.samples,g.tool,g.size,g.color,g.pressure)]})}catch(error){fail(error)}}
  const schedule=()=>{if(!raf.current)raf.current=requestAnimationFrame(preview)}
  const erase=(g:Gesture,point:Point)=>{
    const radius=g.size/2,box={x:Math.min(g.last.x,point.x)-radius,y:Math.min(g.last.y,point.y)-radius,width:Math.abs(point.x-g.last.x)+radius*2,height:Math.abs(point.y-g.last.y)+radius*2}
    for(const node of spatialRef.current.query(box)){if(g.removed.has(node.id))continue;g.work+=canvasInk(node)!.points.length;if(g.work>2000000)throw Error('Слишком сложный проход ластика; жест отменён')
      if(inkSweptHit(node,g.last,point,radius)){if(g.locked.has(node.id))throw Error('Проход затронул заблокированный штрих; ничего не удалено');g.removed.add(node.id)}
    }g.last=point
  }
  const append=(g:Gesture,event:Pick<globalThis.PointerEvent,'clientX'|'clientY'|'pressure'>)=>{
    const point=latest.current.toWorld(event.clientX,event.clientY)
    if(g.tool==='eraser'){erase(g,point);return}
    const p=g.pointerType==='pen'?Math.max(0,Math.min(1,event.pressure)):1,last=g.samples.at(-1)!,gap=Math.hypot(point.x-last.x,point.y-last.y)
    if(gap<.5/latest.current.zoom){if(gap<1e-7)last.pressure=p;return}
    if(g.samples.length>=INK_POINT_LIMIT)throw Error('Лимит 2048 точек на штрих. Жест отменён; нарисуйте несколько коротких штрихов.')
    g.samples.push({...point,pressure:p})
  }
  const down=(event:PointerEvent<HTMLDivElement>)=>{
    if(gesture.current){if(event.pointerId!==gesture.current.pointerId)return true;return false}
    if(!isUi(event.target))suppressClick.current=false
    if(!mode||isUi(event.target)||event.pointerType==='touch'||event.button!==0)return false
    suppressClick.current=true;if(latest.current.disabled){latest.current.onError('Доска недоступна для рисования');return true}
    const point=latest.current.toWorld(event.clientX,event.clientY),g:Gesture={pointerId:event.pointerId,pointerType:event.pointerType,base:latest.current.document,tool:mode,id:'ink-'+crypto.randomUUID(),size:mode==='eraser'?eraserSize:size,color,pressure:pressure&&event.pointerType==='pen',samples:[{...point,pressure:event.pointerType==='pen'?Math.max(0,Math.min(1,event.pressure)):1}],last:point,removed:new Set(),locked:new Set(latest.current.locked),work:0}
    gesture.current=g;try{if(mode==='eraser')erase(g,point);event.currentTarget.setPointerCapture(event.pointerId);schedule()}catch(error){fail(error)}return true
  }
  const move=(event:PointerEvent<HTMLDivElement>)=>{const g=gesture.current;if(!g)return false;if(event.pointerId!==g.pointerId)return true;try{const samples=event.nativeEvent.getCoalescedEvents?.();for(const sample of samples?.length?samples:[event.nativeEvent])append(g,sample);schedule()}catch(error){fail(error)}return true}
  const up=(event:PointerEvent<HTMLDivElement>)=>{
    const g=gesture.current;if(!g)return false;if(event.pointerId!==g.pointerId)return true
    gesture.current=null;cancelAnimationFrame(raf.current);raf.current=0;latest.current.onPreview(null)
    try{if(latest.current.document!==g.base)throw Error('Доска изменилась; незавершённый жест отменён');append(g,{clientX:event.clientX,clientY:event.clientY,pressure:g.samples.at(-1)?.pressure??event.pressure});const next=g.tool==='eraser'?deleteCanvasSelection(g.base,g.removed):insertCanvasNodes(g.base,[inkNode(g.id,g.samples,g.tool,g.size,g.color,g.pressure)]);latest.current.onCommit(next,g.tool==='eraser'?undefined:g.id)}catch(error){latest.current.onError(error instanceof Error?error.message:String(error))}
    if(event.currentTarget.hasPointerCapture(event.pointerId))event.currentTarget.releasePointerCapture(event.pointerId);return true
  }
  useEffect(()=>{if(gesture.current)cancel()},[inputs.document,mode]) // Authoritative source/tool changes cancel, never publish a stale preview.
  useEffect(()=>{const blur=()=>{if(gesture.current)cancel()};window.addEventListener('blur',blur);return()=>{window.removeEventListener('blur',blur);cancelAnimationFrame(raf.current)}},[])
  return{mode,setMode,size,setSize,eraserSize,setEraserSize,color,setColor,pressure,setPressure,cancel,isActive:()=>Boolean(gesture.current),owns:(pointerId:number)=>gesture.current?.pointerId===pointerId,down,move,up,isUi,consumeClick:(target:EventTarget|null)=>{if(isUi(target))return false;const block=suppressClick.current||Boolean(mode);suppressClick.current=false;return block}}
}
