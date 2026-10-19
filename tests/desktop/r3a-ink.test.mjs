import assert from 'node:assert/strict'
import {test} from 'node:test'
import {inkNode,canvasInk,prepareCanvasInk,inkPath,inkHit,inkSweptHit,INK_POINT_LIMIT,validateInkBudget} from '../../apps/desktop/src/canvas-ink.ts'
import {parseCanvasDocument} from '../../apps/desktop/src/canvas-document.ts'
import {canvasNodeHit} from '../../apps/desktop/src/canvas-primitives.ts'
import {insertCanvasNodes,groupCanvasSelection,moveCanvasNodes,toggleCanvasLock,deleteCanvasSelection} from '../../apps/desktop/src/canvas-operations.ts'
import {CANVAS_INK_COMMANDS} from '../../apps/desktop/src/canvas-commands.ts'
import {createCommandRegistry,validShortcut} from '../../apps/desktop/src/commands.ts'
const samples=[{x:0,y:0,pressure:1},{x:100,y:60,pressure:0}],stroke=()=>inkNode('ink',samples,'pen',8,'#123456',true),doc=n=>({nodes:n,edges:[],foreign:{keep:1}})
test('drawing tools use valid shared-registry modifier bindings and disabled overrides do not revive defaults',()=>{for(const command of CANVAS_INK_COMMANDS){assert.equal(validShortcut(command.defaultShortcut),true);const registry=createCommandRegistry([{...command,run:()=>{}}],{[command.id]:null});assert.equal(registry.forEvent({code:command.defaultShortcut.split('+').at(-1),key:'p',altKey:true,ctrlKey:false,shiftKey:false,metaKey:false,isComposing:false,repeat:false}),undefined)}})
test('ink writer stores bounded integer data and readable standard fallback, not renderer paths',()=>{
  const n=stroke(),s=canvasInk(n);assert.equal(n.type,'text');assert.equal(n.text,'Штрих пера');assert.equal(s.points.length,2);assert.equal(s.basis,Math.min(n.width,n.height));assert.equal(s.pressure,true);assert.ok(s.points.every(p=>p.every(Number.isInteger)));assert.equal(n.path,undefined);assert.deepEqual(parseCanvasDocument(JSON.stringify(doc([n]))).document,doc([n]));assert.throws(()=>inkNode('x',Array.from({length:INK_POINT_LIMIT+1},()=>samples[0]),'pen',4,'#123456',false),/длинный/)
})
test('one dot and pressure widths share finite local geometry and one union path',()=>{
  const n=stroke(),p=prepareCanvasInk(n,'#123456');assert.equal(p.segments.length,1);assert.equal(p.segments[0][4],4);assert.equal(p.segments[0][5],1);assert.equal(p.opacity,1);assert.ok(inkPath(p).includes('A'));const dot=inkNode('dot',[samples[0]],'pen',8,'#123456',false),d=prepareCanvasInk(dot);assert.equal(d.segments.length,1);assert.equal(d.segments[0][0],d.segments[0][2]);assert.equal(inkHit(dot,{x:0,y:0}),true);assert.equal(inkHit(dot,{x:8,y:0}),false)
  const marker=inkNode('h',samples,'highlighter',16,'#123456',true),m=prepareCanvasInk(marker);assert.equal(canvasInk(marker).pressure,false);assert.equal(m.opacity,.24);assert.equal(m.segments[0][4],m.segments[0][5]);assert.ok(m.segments.flat().every(Number.isFinite))
})
test('stroke hit regions exclude bounding-box corners and swept erasing cannot tunnel through a sparse move',()=>{
  const n=stroke();assert.equal(inkHit(n,{x:50,y:30}),true);assert.equal(inkHit(n,{x:0,y:60}),false);assert.equal(canvasNodeHit(n,{x:0,y:60},1),false);assert.equal(inkSweptHit(n,{x:50,y:-100},{x:50,y:200},2),true);assert.equal(inkSweptHit(n,{x:-100,y:100},{x:200,y:100},1),false)
})
test('moving/grouping/copying retain normalized points; resizing scales width by the minimum bound',()=>{
  const n=stroke(),before=structuredClone(n),d=groupCanvasSelection(doc([n,{...n,id:'copy',x:200}]),new Set(['ink','copy']),'g'),m=moveCanvasNodes(d,new Set(['g']),12,30);assert.deepEqual(canvasInk(m.nodes.find(n=>n.id==='ink')),canvasInk(before));assert.equal(inkHit(m.nodes.find(n=>n.id==='ink'),{x:62,y:60}),true);assert.equal(prepareCanvasInk({...n,width:n.width*2,height:n.height*2}).segments[0][4],8);const locked=toggleCanvasLock(m,new Set(['g']));assert.equal(deleteCanvasSelection(locked,new Set(['ink'])),locked);assert.throws(()=>insertCanvasNodes(locked,[{...n,id:'new',x:n.x+12,y:n.y+30}]),/заблокирован/)
})
test('malformed/future ink and incompatible node meanings refuse read-write rather than truncate',()=>{
  const n=stroke();for(const ink of [{...canvasInk(n),version:2},{...canvasInk(n),points:[]},{...canvasInk(n),points:[[1,2,2048]]},{...canvasInk(n),points:[[NaN,2,1]]},{...canvasInk(n),points:[[1,2.5,1]]},{...canvasInk(n),size:65},{...canvasInk(n),basis:0}])assert.ok(parseCanvasDocument(JSON.stringify(doc([{...n,brandon:{version:1,ink}}]))).error)
  for(const patch of [{type:'file',file:'Note.md'},{brandon:{...n.brandon,primitive:'text'}}])assert.ok(parseCanvasDocument(JSON.stringify(doc([{...n,...patch}]))).error)
  const extended={...n,brandon:{...n.brandon,ink:{...canvasInk(n),future:{keep:7}}}};assert.deepEqual(parseCanvasDocument(JSON.stringify(doc([extended]))).document,doc([extended]))
})
test('whole-board point/stroke budgets are explicit refusal, not data trimming',()=>{
  const n=stroke();assert.throws(()=>validateInkBudget(doc(Array.from({length:4097},(_,i)=>({...n,id:String(i)})))),/4096/);assert.throws(()=>validateInkBudget(doc(Array.from({length:50},(_,i)=>({...n,id:String(i),brandon:{version:1,ink:{...canvasInk(n),points:Array.from({length:2048},()=>[1,1,1])}}})))),/100000/)
})
