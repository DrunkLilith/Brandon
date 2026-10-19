import type {CanvasNode} from './canvas-model'
import {canvasAppearance} from './canvas-primitives.ts'
import {canvasRasterLayout,isCanvasRaster} from './canvas-raster.ts'
import {prepareCanvasInk} from './canvas-ink.ts'

export function renderCanvasNode(node: CanvasNode,label?:string) {
  const appearance=canvasAppearance(node),raster=isCanvasRaster(node)?canvasRasterLayout(node):null,ink=prepareCanvasInk(node,appearance.border)
  if(ink)return{id:node.id,x:node.x,y:node.y,width:node.width,height:node.height,kind:node.type,fill:ink.color+Math.round(ink.opacity*255).toString(16).padStart(2,'0'),border:appearance.border,style:null,image:null,ink,text:''}
  return { id: node.id, x: node.x, y: node.y, width: node.width, height: node.height, kind: node.type, fill:appearance.fill,border:appearance.border,
    style:raster?{shape:0,centerText:false,radius:8,fontSize:raster.fontSize,align:appearance.textAlign,textColor:appearance.text,strokeWidth:appearance.strokeWidth,bounds:raster.footer}:appearance.styled?{shape:appearance.shape,centerText:appearance.centerText,radius:appearance.radius,fontSize:appearance.fontSize,align:appearance.textAlign,textColor:appearance.text,strokeWidth:appearance.strokeWidth,bounds:appearance.bounds}:null,
    image:raster?{path:node.file,bounds:raster.image}:null,
    text: label??(raster?node.file?.split('/').at(-1)??'Изображение':node.type === 'text' ? node.text ?? '' : node.type === 'file' ? `${node.file?.split('/').at(-1)?.replace(/\.md$/i, '') ?? 'Заметка'}\n${node.file ?? ''}` : node.label ?? node.url ?? 'Группа') }
}
