import test from 'node:test'
import assert from 'node:assert/strict'
import {canvasExportScene,canvasExportWarnings} from '../../apps/desktop/src/canvas-export.ts'
import {renderCanvasNode} from '../../apps/desktop/src/canvas-render-scene.ts'
import {CANVAS_FRAME_COMMANDS} from '../../apps/desktop/src/canvas-commands.ts'
const frame={id:'frame',type:'group',x:0,y:0,width:400,height:240,label:'Сцена',brandon:{version:1,frame:{version:1,preset:'custom',notes:'PRIVATE NOTES'}}}
const card=(id,x,y,text=id)=>({id,type:'text',x,y,width:120,height:80,text})
const document=()=>({nodes:[frame,card('a',10,70),card('outside',800,0,'SECRET'),card('partial',390,200)],edges:[],foreign:{keep:73}})
test('export shares the native node adapter and preserves source paint order without modifying the document',()=>{
  const d=document(),before=JSON.stringify(d),s=canvasExportScene(before,'frame',1,false)
  assert.deepEqual(s.nodes.map(n=>n.id),['frame','a','partial']);assert.deepEqual(s.nodes,d.nodes.filter(n=>n.id!=='outside').map(n=>renderCanvasNode(n)))
  assert.equal(JSON.stringify(d),before);assert.ok(!JSON.stringify(s).includes('PRIVATE NOTES'));assert.ok(!JSON.stringify(s).includes('SECRET'));assert.deepEqual(s.bounds,{x:0,y:0,width:400,height:240});assert.equal(s.version,1)
})
test('crossing edges are included even when both endpoints lie outside, not merely incident edges',()=>{
  const d={nodes:[frame,card('left',-500,80),card('right',700,80),card('far',700,900)],edges:[{id:'crossing',fromNode:'left',toNode:'right',label:'Переход'},{id:'far',fromNode:'right',toNode:'far'}]}
  const s=canvasExportScene(JSON.stringify(d),'frame',2,true);assert.deepEqual(s.nodes.map(n=>n.id),['frame']);assert.deepEqual(s.edges.map(e=>e.id),['crossing']);assert.equal(s.scale,2);assert.equal(s.transparent,true);assert.ok(s.edges[0].segments.length);assert.equal(s.edges[0].label,'Переход')
})
test('unsupported versions and bad frame IDs fail without normalizing the original',()=>{
  const d=document();d.nodes[0]={...frame,brandon:{version:999}};const source=JSON.stringify(d);assert.throws(()=>canvasExportScene(source,'frame',1,false));assert.equal(JSON.stringify(d),source)
  assert.throws(()=>canvasExportScene(JSON.stringify(document()),'missing',1,false),/существует/)
  assert.throws(()=>canvasExportScene('{bad','frame',1,false))
})
test('output size, source bytes, node, text and raster counts are refused rather than silently reduced',()=>{
  const source=JSON.stringify(document());assert.throws(()=>canvasExportScene(source,'frame',3,false),/параметры/)
  const large=document();large.nodes[0]={...frame,width:3000};assert.throws(()=>canvasExportScene(JSON.stringify(large),'frame',2,false),/4096/)
  const many=document();many.nodes=Array.from({length:2000},(_,i)=>card('n'+i,0,0));many.nodes.unshift(frame);assert.throws(()=>canvasExportScene(JSON.stringify(many),'frame',1,false),/2000/)
  const text=document();text.nodes[1].text='я'.repeat(32769);assert.throws(()=>canvasExportScene(JSON.stringify(text),'frame',1,false),/64 КиБ/)
  const images=document();images.nodes=[frame,...Array.from({length:65},(_,i)=>({id:'image'+i,type:'file',x:0,y:0,width:120,height:80,file:`a${i}.png`}))];assert.throws(()=>canvasExportScene(JSON.stringify(images),'frame',1,false),/бюджет/)
  assert.throws(()=>canvasExportScene(source+' '.repeat(16*1024*1024),'frame',1,false),/16 МиБ/)
})
test('missing media and glyphs, clipping, resampling and tasks produce actionable warnings',()=>{
  const report={render:{clippedText:['a'],missingGlyphs:3,missingRasters:['b']},rasters:[{resampled:true},{resampled:false}],missingTasks:['t']}
  assert.equal(canvasExportWarnings(report).length,5)
  assert.deepEqual(canvasExportWarnings({render:{clippedText:[],missingGlyphs:0,missingRasters:[]},rasters:[],missingTasks:[]}),[])
  assert.ok(CANVAS_FRAME_COMMANDS.some(c=>c.id==='canvas-export-png'))
})
