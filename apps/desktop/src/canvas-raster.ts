import type {CanvasNode} from './canvas-model'
import {canvasExtension} from './canvas-model.ts'
export const isCanvasRaster=(node:CanvasNode)=>node.type==='file'&&typeof node.file==='string'&&/\.(png|jpe?g|gif|webp)$/i.test(node.file)
export function canvasRasterLayout(node:CanvasNode){
  const fontSize=canvasExtension(node)?.fontSize??12,pad=Math.min(8,node.width/4,node.height/4),footerHeight=Math.min(node.height/2,fontSize*1.55+2),footer={x:pad,y:node.height-pad-footerHeight,width:node.width-2*pad,height:footerHeight},image={x:pad,y:pad,width:node.width-2*pad,height:Math.max(Number.EPSILON,footer.y-pad-4)}
  return{fontSize,footer,image}
}
export function containRaster(bounds:{x:number;y:number;width:number;height:number},width:number,height:number){const scale=Math.min(bounds.width/width,bounds.height/height),w=width*scale,h=height*scale;return{x:bounds.x+(bounds.width-w)/2,y:bounds.y+(bounds.height-h)/2,width:w,height:h}}
const normalized=(path:string)=>path.replaceAll('\\','/').replace(/^\/\/\?\//,'').replace(/\/+$/,'').toLowerCase()
export function changedRasterPaths(root:string,paths:string[]|null,references:string[],absolute=false){if(paths===null)return references;const base=normalized(root)+'/';return references.filter(reference=>{const target=absolute?base+normalized(reference):normalized(reference);return paths.some(path=>{const changed=normalized(path);return changed===target||!absolute&&target.startsWith(changed+'/')})})}
export type RasterDelta={root:string;paths:string[]|null}
