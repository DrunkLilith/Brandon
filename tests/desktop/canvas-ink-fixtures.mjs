import {inkNode} from '../../apps/desktop/src/canvas-ink.ts'
export function inkFixture(large=false){
  const ink=(id,points,tool,size,color,pressure=false)=>inkNode(id,points.map(([x,y,p=1])=>({x,y,pressure:p})),tool,size,color,pressure)
  return{nodes:[
    {id:'lower-text',type:'text',x:-380,y:-190,width:300,height:140,text:'LOWER TEXT',foreign:{keep:1}},
    ink('blue-under',[[-350,-95,1],[-285,-125,.7],[-200,-100,.3],[-100,-135,0]],'pen',14,'#4da8ff',true),
    {id:'tint',type:'group',x:-420,y:-220,width:390,height:220,label:'TINT OVER INK / TEXT',color:'#ff0000'},
    ink('green-over',[[-350,-45,0],[-275,-65,.5],[-190,-30,1],[-100,-40,.5]],'pen',14,'#72ef91',true),
    ink('repeat',[[-360,110],[-120,110],[-360,110],[-120,110]],'highlighter',32,'#ffff00'),
    ink('single',[[70,110],[310,110]],'highlighter',32,'#ffff00'),
    {id:'image',type:'file',file:'Attachments/Photo.png',x:50,y:-200,width:300,height:200},
    ink('on-photo',[[75,-105],[325,-105]],'highlighter',32,'#72ef91'),
    {id:'upper-text',type:'text',x:80,y:-150,width:260,height:80,text:'TEXT ABOVE PHOTO',brandon:{version:1,primitive:'text',fontSize:16}},
    ...(large?Array.from({length:300},(_,i)=>({id:'far'+i,type:'text',x:10000+i*300,y:10000,width:200,height:100,text:'Far'})):[])
  ],edges:[],foreign:{keep:73}}
}
