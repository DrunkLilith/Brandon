import type {CanvasDocument,CanvasNode,CanvasInk} from './canvas-model'
import {canvasExtension} from './canvas-model.ts'
export const INK_POINT_LIMIT=2048,INK_BOARD_POINTS=100000,INK_STROKE_LIMIT=4096,INK_COORD=65535
export type InkSample={x:number;y:number;pressure:number}
export type InkSegment=[number,number,number,number,number,number]
export type PreparedInk={segments:InkSegment[];color:string;opacity:number}
export const canvasInk=(node:CanvasNode|undefined)=>canvasExtension(node)?.ink
export function validateCanvasInk(value:unknown):asserts value is CanvasInk{
  if(!value||typeof value!=='object'||Array.isArray(value))throw Error('Некорректный штрих Brandon')
  const s=value as CanvasInk
  if(s.version!==1||!['pen','highlighter'].includes(s.tool)||!Number.isInteger(s.size)||s.size<1||s.size>64||!Number.isInteger(s.basis)||s.basis<1||s.basis>10000000||typeof s.pressure!=='boolean'||!Array.isArray(s.points)||s.points.length<1||s.points.length>INK_POINT_LIMIT)throw Error('Неизвестный или слишком большой штрих Brandon; исходный файл сохранён')
  for(const p of s.points)if(!Array.isArray(p)||p.length!==3||p.some((v,i)=>!Number.isInteger(v)||v<0||v>(i===2?1024:INK_COORD)))throw Error('Некорректные точки штриха Brandon')
}
export function validateInkBudget(document:CanvasDocument){let count=0,points=0;for(const node of document.nodes){const ink=canvasInk(node);if(ink){count++;points+=ink.points.length}}if(count>INK_STROKE_LIMIT||points>INK_BOARD_POINTS)throw Error('Лимит доски: 4096 штрихов и 100000 точек. Исходный файл не обрезан.')}
export function inkNode(id:string,samples:InkSample[],tool:CanvasInk['tool'],size:number,color:string,pressure:boolean):CanvasNode{
  if(!samples.length||samples.length>INK_POINT_LIMIT||samples.some(p=>![p.x,p.y,p.pressure].every(Number.isFinite)||Math.abs(p.x)>9000000||Math.abs(p.y)>9000000||p.pressure<0||p.pressure>1))throw Error('Слишком длинный или некорректный штрих; жест отменён')
  if(!/^#[\da-f]{6}$/i.test(color)||!Number.isInteger(size)||size<1||size>64)throw Error('Некорректный цвет или размер кисти')
  const pad=size/2+1;let x=Math.floor(Math.min(...samples.map(p=>p.x))-pad),y=Math.floor(Math.min(...samples.map(p=>p.y))-pad),right=Math.ceil(Math.max(...samples.map(p=>p.x))+pad),bottom=Math.ceil(Math.max(...samples.map(p=>p.y))+pad)
  if(right-x<32){x-=Math.floor((32-right+x)/2);right=x+32}if(bottom-y<32){y-=Math.floor((32-bottom+y)/2);bottom=y+32}const width=right-x,height=bottom-y
  if(width>10000000||height>10000000)throw Error('Штрих выходит за пределы холста')
  const ink:CanvasInk={version:1,tool,size,basis:Math.min(width,height),pressure:pressure&&tool==='pen',points:samples.map(p=>[Math.round((p.x-x)/width*INK_COORD),Math.round((p.y-y)/height*INK_COORD),Math.round(p.pressure*1024)])}
  validateCanvasInk(ink)
  return{id,type:'text',x,y,width,height,color,text:tool==='pen'?'Штрих пера':'Маркер',brandon:{version:1,ink}}
}
export function prepareCanvasInk(node:CanvasNode,color='#dad8e0'):PreparedInk|null{
  const ink=canvasInk(node);if(!ink)return null
  const radius=ink.size*.5*Math.min(node.width,node.height)/ink.basis,points=ink.points.map(([u,v,p])=>[u/INK_COORD*node.width,v/INK_COORD*node.height,radius*(ink.pressure?.25+.75*p/1024:1)]),segments:InkSegment[]=[]
  for(let i=0;i<Math.max(1,points.length-1);i++){const a=points[i],b=points[Math.min(i+1,points.length-1)];segments.push([a[0],a[1],b[0],b[1],a[2],b[2]])}
  return{segments,color,opacity:ink.tool==='highlighter'?.24:1}
}
type Point={x:number;y:number}
function quad(s:InkSegment):Point[]{const [ax,ay,bx,by,ra,rb]=s,d=Math.hypot(bx-ax,by-ay);if(d<1e-9)return[];const nx=-(by-ay)/d,ny=(bx-ax)/d;return[{x:ax+nx*ra,y:ay+ny*ra},{x:ax-nx*ra,y:ay-ny*ra},{x:bx-nx*rb,y:by-ny*rb},{x:bx+nx*rb,y:by+ny*rb}]}
const number=(n:number)=>Number(n.toFixed(5))
export function inkPath(ink:PreparedInk){return ink.segments.map(s=>{const q=quad(s),polygon=q.length?'M'+q.map(p=>`${number(p.x)},${number(p.y)}`).join('L')+'Z':'';return polygon+[[s[0],s[1],s[4]],[s[2],s[3],s[5]]].map(([x,y,r])=>`M${number(x+r)},${number(y)}A${number(r)},${number(r)} 0 1 1 ${number(x-r)},${number(y)}A${number(r)},${number(r)} 0 1 1 ${number(x+r)},${number(y)}Z`).join('')}).join('')}
const cross=(a:Point,b:Point,p:Point)=>(b.x-a.x)*(p.y-a.y)-(b.y-a.y)*(p.x-a.x)
function distance(p:Point,a:Point,b:Point){const dx=b.x-a.x,dy=b.y-a.y,t=Math.max(0,Math.min(1,((p.x-a.x)*dx+(p.y-a.y)*dy)/(dx*dx+dy*dy||1)));return Math.hypot(p.x-a.x-t*dx,p.y-a.y-t*dy)}
const inside=(p:Point,q:Point[])=>q.length>0&&q.every((a,i)=>cross(a,q[(i+1)%q.length],p)>=-1e-8)
function crossing(a:Point,b:Point,c:Point,d:Point){return cross(a,b,c)*cross(a,b,d)<=0&&cross(c,d,a)*cross(c,d,b)<=0&&Math.max(Math.min(a.x,b.x),Math.min(c.x,d.x))<=Math.min(Math.max(a.x,b.x),Math.max(c.x,d.x))&&Math.max(Math.min(a.y,b.y),Math.min(c.y,d.y))<=Math.min(Math.max(a.y,b.y),Math.max(c.y,d.y))}
const segmentDistance=(a:Point,b:Point,c:Point,d:Point)=>crossing(a,b,c,d)?0:Math.min(distance(a,c,d),distance(b,c,d),distance(c,a,b),distance(d,a,b))
export function inkSweptHit(node:CanvasNode,from:Point,to:Point,radius=0){const ink=prepareCanvasInk(node);if(!ink)return false;const a={x:from.x-node.x,y:from.y-node.y},b={x:to.x-node.x,y:to.y-node.y};return ink.segments.some(s=>{if(distance({x:s[0],y:s[1]},a,b)<=s[4]+radius||distance({x:s[2],y:s[3]},a,b)<=s[5]+radius)return true;const q=quad(s);return inside(a,q)||inside(b,q)||q.some((c,i)=>segmentDistance(a,b,c,q[(i+1)%q.length])<=radius)})}
export const inkHit=(node:CanvasNode,point:Point,tolerance=0)=>inkSweptHit(node,point,point,tolerance)
