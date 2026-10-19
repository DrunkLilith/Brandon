import { useEffect, useRef, useState } from 'react'

export function virtualRange(count:number,height:number,top:number,rowHeight:number,overscan=8) {
  if(count<=200)return {start:0,end:count}
  const start=Math.max(0,Math.min(count-1,Math.floor(Math.max(0,top)/rowHeight)-overscan))
  return {start,end:Math.min(count,start+Math.ceil(Math.max(200,height)/rowHeight)+overscan*2)}
}

export function useVirtualRows(count:number,rowHeight:number,parentScroll=false) {
  const ref=useRef<HTMLDivElement>(null)
  const [window,setWindow]=useState({top:0,height:700})
  useEffect(()=>{
    const element=ref.current,scroll=parentScroll?element?.parentElement:element
    if(!scroll)return
    let frame=0
    const update=()=>{if(frame)return;frame=requestAnimationFrame(()=>{frame=0;setWindow({top:scroll.scrollTop,height:scroll.clientHeight})})}
    const observer=new ResizeObserver(update);observer.observe(scroll);scroll.addEventListener('scroll',update,{passive:true});update()
    return()=>{observer.disconnect();scroll.removeEventListener('scroll',update);cancelAnimationFrame(frame)}
  },[count,parentScroll])
  const range=virtualRange(count,window.height,window.top,rowHeight)
  const reveal=(index:number)=>{
    const scroll=parentScroll?ref.current?.parentElement:ref.current
    if(scroll){scroll.scrollTop=Math.max(0,index*rowHeight-scroll.clientHeight/2);setWindow({top:scroll.scrollTop,height:scroll.clientHeight})}
  }
  return {ref,...range,top:range.start*rowHeight,bottom:Math.max(0,(count-range.end)*rowHeight),reveal}
}
