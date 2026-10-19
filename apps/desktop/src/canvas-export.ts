import {parseCanvasDocument} from './canvas-document.ts'
import {assertCanvasFramePresentable} from './canvas-frames.ts'
import {prepareCanvasEdges} from './canvas-edges.ts'
import {renderCanvasNode} from './canvas-render-scene.ts'

type Box={x:number;y:number;width:number;height:number}
const intersects=(a:Box,b:Box)=>a.x<=b.x+b.width&&a.x+a.width>=b.x&&a.y<=b.y+b.height&&a.y+a.height>=b.y
/** A bounded snapshot, not a second document model. Never changes or saves the source. */
export function canvasExportScene(source:string,frameId:string,scale:1|2,transparent:boolean){
  if(new TextEncoder().encode(source).length>16*1024*1024)throw Error('Исходный Canvas превышает 16 МиБ')
  const {document,error}=parseCanvasDocument(source);if(error)throw Error(error)
  const frame=assertCanvasFramePresentable(document,frameId),bounds={x:frame.x,y:frame.y,width:frame.width,height:frame.height}
  if(![1,2].includes(scale)||typeof transparent!=='boolean')throw Error('Некорректные параметры PNG')
  const width=Math.ceil(Math.fround(frame.width)*scale),height=Math.ceil(Math.fround(frame.height)*scale)
  if(width>4096||height>4096||width*height>16*1024*1024)throw Error('PNG превышает 4096 px по стороне или 16 Ми пикселей. Уменьшите масштаб или размер фрейма.')
  const nodes=document.nodes.filter(n=>intersects(n,bounds)).map(n=>renderCanvasNode(n)),edges=prepareCanvasEdges(document).filter(e=>intersects(e,bounds))
  if(edges.length>4000)throw Error('Во фрейме больше 4000 связей; разделите его на несколько сцен')
  let text=0,points=0,work=0;const paths=new Set<string>(),encoder=new TextEncoder()
  for(const node of nodes){
    const bytes=encoder.encode(node.text).length;if(bytes>65536)throw Error('Текст одного объекта превышает 64 КиБ');text+=bytes
    if(node.ink)points+=node.ink.segments.length
    if(node.image?.path)paths.add(node.image.path)
    work+=Math.max(0,Math.min(node.x+node.width,bounds.x+bounds.width)-Math.max(node.x,bounds.x))*Math.max(0,Math.min(node.y+node.height,bounds.y+bounds.height)-Math.max(node.y,bounds.y))*scale*scale
  }
  text+=edges.reduce((sum,e)=>sum+encoder.encode(e.label).length,0)
  if(text>2*1024*1024||points>100000||paths.size>64||work>256*1024*1024)throw Error('Фрейм превышает бюджет текста, штрихов, изображений или закраски PNG; разделите содержимое')
  return{version:1,bounds,scale,transparent,nodes,edges}
}

export type CanvasExportReport={
  frameLabel:string;sourcePath:string;scale:number;transparent:boolean;pngSha256:string
  render:{width:number;height:number;nodes:number;edges:number;inkStrokes:number;glyphs:number;missingGlyphs:number;fontFamilies:string[];clippedText:string[];missingRasters:string[]}
  rasters:{path:string;status:string;resampled?:boolean}[];missingTasks:string[]
}
export function canvasExportWarnings(report:CanvasExportReport){
  const r=report.render,warnings:string[]=[]
  if(r.clippedText.length)warnings.push(`Текст не помещается в ${r.clippedText.length} объектах и будет обрезан, как в подготовленной сцене.`)
  if(r.missingGlyphs)warnings.push(`Не найдено символов в системных шрифтах: ${r.missingGlyphs}. Проверьте предпросмотр.`)
  if(r.missingRasters.length)warnings.push(`Изображения недоступны: ${r.missingRasters.length}. Вместо них — явные заглушки.`)
  const resampled=report.rasters.filter(r=>r.resampled).length
  if(resampled)warnings.push(`Изображения уменьшены до размера экспорта: ${resampled}. Оригиналы не меняются.`)
  if(report.missingTasks.length)warnings.push(`Связанные задачи недоступны: ${report.missingTasks.length}. Их состояние не выдумывается.`)
  return warnings
}
