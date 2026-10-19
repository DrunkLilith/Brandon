import {useId} from 'react'
import type {CanvasNode} from './canvas-model'
import {canvasAppearance,canvasShapePath} from './canvas-primitives'

export function CanvasPrimitiveView({node,native,selected,editing,locked,zoom,onChange}:{node:CanvasNode;native:boolean;selected:boolean;editing:boolean;locked:boolean;zoom:number;onChange:(text:string)=>void}){
  const a=canvasAppearance(node),clip=useId().replace(/[^a-zA-Z0-9_-]/g,''),textStyle={left:a.bounds.x,top:a.bounds.y,width:a.bounds.width,height:a.bounds.height,fontSize:a.fontSize,textAlign:a.textAlign,color:a.text}
  const geometry=(props:Record<string,string|number>)=>a.primitive==='rectangle'||a.primitive==='sticky'||a.primitive==='text'?<rect x="0" y="0" width={node.width} height={node.height} rx={a.radius} {...props}/>:<path d={canvasShapePath(node)} {...props}/>
  return <>
    {!native&&<svg className="canvas-primitive-shape" viewBox={`0 0 ${node.width} ${node.height}`} preserveAspectRatio="none" aria-hidden="true"><defs><clipPath id={clip}>{geometry({})}</clipPath></defs><g clipPath={a.primitive==='divider'?undefined:`url(#${clip})`}>{geometry({fill:a.primitive==='divider'?'none':a.fill,stroke:selected?'#aca1ee':a.primitive==='text'?'none':a.border,strokeWidth:(a.primitive==='divider'?1:2)*Math.max(a.strokeWidth,selected?2/zoom:0)})}</g></svg>}
    <div className="canvas-primitive-hit" style={{clipPath:a.primitive==='divider'?`inset(${Math.max(0,node.height/2-6/zoom)}px 0)` :a.clipPath}}>
      {a.primitive!=='divider'&&!editing&&<div className={`canvas-primitive-text ${a.centerText?'center-content':''}`} style={textStyle}><span>{node.text}</span></div>}
    </div>
    {editing&&<textarea className="canvas-primitive-editor" aria-label="Текст объекта доски" autoFocus readOnly={locked} maxLength={65536} value={node.text??''} style={textStyle} onChange={event=>onChange(event.target.value)}/>}
  </>
}
