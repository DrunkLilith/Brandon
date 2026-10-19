import assert from 'node:assert/strict'
import {test} from 'node:test'
import {CANVAS_FRAME_PRESETS,createCanvasFrame,canvasFrame,canvasPresentation,orderedCanvasFrames,editCanvasFrame,reorderCanvasFrames,fitCanvasFrame,canvasFrameScreenRect,FRAME_LIMIT,FRAME_ORDER_LIMIT,FRAME_NOTES_LIMIT} from '../../apps/desktop/src/canvas-frames.ts'
import {parseCanvasDocument} from '../../apps/desktop/src/canvas-document.ts'
import {assertCanvasFramePresentable,FRAME_OBJECT_LIMIT} from '../../apps/desktop/src/canvas-frames.ts'
import {CANVAS_FRAME_COMMANDS} from '../../apps/desktop/src/canvas-commands.ts'
import {validShortcut} from '../../apps/desktop/src/commands.ts'
import {moveCanvasNodes,toggleCanvasLock,deleteCanvasSelection,ungroupCanvasSelection,groupCanvasSelection} from '../../apps/desktop/src/canvas-operations.ts'
const selection=(...ids)=>new Set(ids),empty=()=>({nodes:[],edges:[],foreign:{keep:7},metadata:{version:'1.0-1.0',startNode:'foreign-start'}}),card=(id,x,y)=>({id,type:'text',x,y,width:120,height:80,text:id,foreign:{keep:id}})
const create=(d=empty(),id='frame',selected=new Set(),preset='16:9',center={x:0,y:0})=>createCanvasFrame(d,selected,id,preset,'Фрейм',center)
const source=d=>parseCanvasDocument(JSON.stringify(d)),frame=d=>d.nodes.find(n=>canvasFrame(n))
test('frame commands expose valid shared bindings and a complete presentation lifecycle',()=>{for(const c of CANVAS_FRAME_COMMANDS)if(c.defaultShortcut)assert.equal(validShortcut(c.defaultShortcut),true);assert.ok(['canvas-add-frame','canvas-frame-details','canvas-frame-remove','canvas-present','canvas-present-end','canvas-present-next','canvas-present-prev'].every(id=>CANVAS_FRAME_COMMANDS.some(c=>c.id===id)))})
test('presentation object budget refuses rather than trimming the frame or rewriting source',()=>{const d=create(),f=frame(d),full={...d,nodes:[f,...Array.from({length:FRAME_OBJECT_LIMIT},(_,i)=>card('n'+i,0,0))]},before=JSON.stringify(full);assert.throws(()=>assertCanvasFramePresentable(full,f.id),/2000/);assert.equal(JSON.stringify(full),before);assert.equal(assertCanvasFramePresentable({...full,nodes:full.nodes.slice(0,FRAME_OBJECT_LIMIT)},f.id),f);assert.throws(()=>assertCanvasFramePresentable(d,'missing'),/существует/)})

test('all nine presets create standard groups at the camera centre without source copies',()=>{
  for(const p of CANVAS_FRAME_PRESETS){const before=empty(),d=create(before,'f',selection(),p.id,{x:400,y:-300}),n=frame(d);assert.equal(n.type,'group');assert.equal(n.width,p.width);assert.equal(n.height,p.height);assert.deepEqual(canvasFrame(n),{version:1,preset:p.id,notes:''});assert.deepEqual(canvasPresentation(d),{version:1,frames:['f']});assert.equal(d.metadata,before.metadata);assert.equal(d.foreign,before.foreign);assert.equal(d.edges,before.edges);assert.equal(source(d).error,null);assert.deepEqual(before,empty())}
})
test('wrap expands nested geometric groups, preserves members and puts only the new frame behind them',()=>{
  const d={...empty(),nodes:[card('a',-100,40),card('b',180,200)],edges:[{id:'ab',fromNode:'a',toNode:'b',foreign:99}]},grouped=groupCanvasSelection(d,selection('a','b'),'g'),wrapped=create(grouped,'f',selection('g'),'16:9'),n=frame(wrapped)
  assert.equal(wrapped.nodes[0],n);assert.deepEqual(wrapped.nodes.slice(1),grouped.nodes);assert.equal(wrapped.edges,grouped.edges)
  for(const child of grouped.nodes){assert.ok(n.x<=child.x-47);assert.ok(n.y<=child.y-47);assert.ok(n.x+n.width>=child.x+child.width+47);assert.ok(n.y+n.height>=child.y+child.height+47)}
  assert.ok(Math.abs(n.width/n.height-16/9)<.01)
})
test('custom wrap uses padded bounds while fixed aspect wrap enlarges only the new group',()=>{
  const d={...empty(),nodes:[card('a',10,20)]},custom=frame(create(d,'c',selection('a'),'custom')),square=frame(create(d,'s',selection('a'),'1:1'))
  assert.deepEqual([custom.x,custom.y,custom.width,custom.height],[-38,-28,216,176]);assert.equal(square.width,square.height);assert.deepEqual(d.nodes,[card('a',10,20)]);assert.equal(create(d,'s',selection('a'),'1:1').nodes[1],d.nodes[0])
})
test('presentation order differs from paint order, is stable and retains missing slots',()=>{
  let d=create(create(empty(),'a'),'b');d={...d,brandon:{...d.brandon,future:{keep:4},presentation:{...d.brandon.presentation,frames:['gone','b','a'],future:{keep:5}}}}
  assert.deepEqual(orderedCanvasFrames(d).map(n=>n.id),['b','a']);const nodes=d.nodes,edges=d.edges,reordered=reorderCanvasFrames(d,'a',-1);assert.equal(reordered.nodes,nodes);assert.equal(reordered.edges,edges);assert.deepEqual(canvasPresentation(reordered).frames,['gone','a','b']);assert.deepEqual(reordered.brandon.future,{keep:4});assert.deepEqual(canvasPresentation(reordered).future,{keep:5});assert.equal(reorderCanvasFrames(reordered,'a',-1),reordered)
})
test('unlisted/remapped copies append to navigation and get durable order only on an explicit edit',()=>{
  const d=create(empty(),'a'),copy={...d.nodes[0],id:'copy',x:2000},pasted={...d,nodes:[...d.nodes,copy]},before=JSON.stringify(pasted);assert.deepEqual(orderedCanvasFrames(pasted).map(n=>n.id),['a','copy']);assert.equal(JSON.stringify(pasted),before);const reordered=reorderCanvasFrames(pasted,'copy',-1);assert.deepEqual(canvasPresentation(reordered).frames,['copy','a']);assert.equal(reordered.nodes,pasted.nodes)
})
test('remove-frame-only retains contents and sequence slot; reintroducing the ID restores its place',()=>{
  const d=create({...empty(),nodes:[card('a',0,0)]},'frame',selection('a')),removed=ungroupCanvasSelection(d,selection('frame'));assert.deepEqual(removed.nodes,[card('a',0,0)]);assert.deepEqual(canvasPresentation(removed).frames,['frame']);assert.deepEqual(orderedCanvasFrames(removed),[]);assert.deepEqual(orderedCanvasFrames({...removed,nodes:d.nodes}).map(n=>n.id),['frame']);assert.deepEqual(deleteCanvasSelection(d,selection('frame')).nodes,[])
})
test('common move/lock semantics carry frame children once and refuse locked mutations',()=>{
  const d=create({...empty(),nodes:[card('a',0,0)]},'frame',selection('a')),moved=moveCanvasNodes(d,selection('frame','a'),40,50);assert.deepEqual(moved.nodes.map(n=>[n.x-d.nodes.find(old=>old.id===n.id).x,n.y-d.nodes.find(old=>old.id===n.id).y]),[[40,50],[40,50]])
  const locked=toggleCanvasLock(d,selection('frame'));assert.throws(()=>editCanvasFrame(locked,'frame',{notes:'No'}),/заблокирован/);assert.throws(()=>reorderCanvasFrames(locked,'frame',1),/заблокирован/);assert.equal(ungroupCanvasSelection(locked,selection('frame')),locked);assert.throws(()=>create(locked,'new',selection('a')),/заблокирован/);const largeLocked=toggleCanvasLock(create(empty(),'frame',selection(),'1:1'),selection('frame'));assert.throws(()=>create(largeLocked,'new',selection(),'mobile',{x:0,y:0}),/заблокированной/)
})
test('frame metadata edits preserve unknown data, notes are plain source and no-op returns identity',()=>{
  const d=create();d.nodes[0].brandon.frame.future={keep:8};d.nodes[0].brandon.foreign={keep:9};const text='<script>Not executed</script>\nЮникод 🦊',edited=editCanvasFrame(d,'frame',{label:'  Заголовок 🦊  ',notes:text});assert.equal(edited.nodes[0].label,'Заголовок 🦊');assert.equal(canvasFrame(edited.nodes[0]).notes,text);assert.deepEqual(canvasFrame(edited.nodes[0]).future,{keep:8});assert.deepEqual(edited.nodes[0].brandon.foreign,{keep:9});assert.equal(edited.brandon,d.brandon);assert.equal(editCanvasFrame(edited,'frame',{notes:text}),edited);assert.equal(source(edited).error,null);assert.throws(()=>editCanvasFrame(d,'frame',{notes:'x'.repeat(FRAME_NOTES_LIMIT+1)}),/фрейм/);assert.throws(()=>editCanvasFrame(d,'frame',{label:' '}),/Название/)
})
test('future or malformed frame/document extensions fail read-only without silent normalizing',()=>{
  const d=create();for(const ext of [{version:2},[],null,{version:1,presentation:{version:2,frames:[]}},{version:1,presentation:{version:1,frames:['a','a']}},{version:1,presentation:{version:1,frames:['']}},{version:1,presentation:{version:1,frames:[3]}}])assert.ok(source({...d,brandon:ext}).error)
  for(const f of [{version:2,preset:'16:9'},{version:1,preset:'unknown'},{version:1,preset:'custom',notes:[]},null])assert.ok(source({...d,nodes:[{...d.nodes[0],brandon:{version:1,frame:f}}]}).error)
  for(const patch of [{type:'text'},{brandon:{...d.nodes[0].brandon,primitive:'rectangle'}},{brandon:{...d.nodes[0].brandon,taskId:'task-1'}}])assert.ok(source({...d,nodes:[{...d.nodes[0],...patch}]}).error)
  assert.ok(source({...d,nodes:[{...d.nodes[0],id:'x'.repeat(513)}]}).error)
})
test('frame/order/name/geometry budgets reject all-or-nothing and leave original bytes unchanged',()=>{
  const d=create(),before=JSON.stringify(d);assert.throws(()=>create(d,'frame'),/ID/);assert.throws(()=>create(d,'x',selection(),'custom',{x:1e8,y:0}),/координаты/);assert.throws(()=>createCanvasFrame(d,selection(),'x','custom','x'.repeat(513),{x:0,y:0}),/Название/);assert.ok(source({...d,nodes:Array.from({length:FRAME_LIMIT+1},(_,i)=>({...d.nodes[0],id:'f'+i}))}).error);assert.ok(source({...d,brandon:{version:1,presentation:{version:1,frames:Array.from({length:FRAME_ORDER_LIMIT+1},(_,i)=>'f'+i)}}}).error);assert.equal(JSON.stringify(d),before)
})
test('camera fit and shared screen bounds centre every preset with explicit padding without changing source',()=>{
  for(const p of CANVAS_FRAME_PRESETS){const n=frame(create(empty(),'f',selection(),p.id,{x:-4000,y:5000})),size={width:1200,height:800},camera=fitCanvasFrame(n,size),r=canvasFrameScreenRect(n,camera,size);assert.ok(r.x>=47.99&&r.y>=47.99);assert.ok(r.x+r.width<=1152.01&&r.y+r.height<=752.01);assert.ok(Math.abs(r.x+r.width/2-600)<1e-5);assert.ok(Math.abs(r.y+r.height/2-400)<1e-5)}
  const n=frame(create());assert.throws(()=>fitCanvasFrame(n,{width:0,height:800}));assert.throws(()=>fitCanvasFrame(n,{width:1000,height:800},NaN));assert.ok(Object.values(fitCanvasFrame(n,{width:1,height:1})).every(Number.isFinite))
})
