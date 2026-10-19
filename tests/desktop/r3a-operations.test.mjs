import assert from 'node:assert/strict'
import {test} from 'node:test'
import {readFile} from 'node:fs/promises'
import {CANVAS_COMMANDS} from '../../apps/desktop/src/canvas-commands.ts'
import {createCommandRegistry} from '../../apps/desktop/src/commands.ts'
import {parseCanvasDocument,canvasColors} from '../../apps/desktop/src/canvas-document.ts'
import {expandCanvasSelection,canvasSelectionEditable,canvasSelectionRoots,moveCanvasNodes,patchCanvasSelection,toggleCanvasLock,deleteCanvasSelection,groupCanvasSelection,ungroupCanvasSelection,reorderCanvasSelection,layoutCanvasSelection} from '../../apps/desktop/src/canvas-operations.ts'
const node=(id,x,y=0,width=100,height=80)=>({id,type:'text',x,y,width,height,text:id,unknown:{keep:id}})
const document=()=>({nodes:[node('a',0),node('b',180,30),node('c',500,50)],edges:[{id:'ab',fromNode:'a',toNode:'b',unknown:4}],foreign:{keep:true}})
const selected=(...ids)=>new Set(ids)
test('standard grouping inserts a container behind members and preserves all unknown data',()=>{
  const before=document(),after=groupCanvasSelection(before,selected('a','b'),'g')
  assert.equal(after.nodes[0].type,'group');assert.deepEqual(after.foreign,before.foreign);assert.equal(after.nodes[1],before.nodes[0]);assert.equal(after.edges,before.edges)
  assert.deepEqual([...expandCanvasSelection(after,selected('g'))],['g','a','b'])
  assert.throws(()=>groupCanvasSelection(after,selected('a','b'),'g'),/ID/)
})
test('nested groups and explicit child selection translate descendants exactly once',()=>{
  let d=groupCanvasSelection(document(),selected('a','b'),'inner');d=groupCanvasSelection(d,selected('inner','c'),'outer')
  const moved=moveCanvasNodes(d,selected('outer','inner','a'),12.4,-5.2)
  for(const n of d.nodes){const next=moved.nodes.find(x=>x.id===n.id);assert.equal(next.x,Math.round(n.x+12.4));assert.equal(next.y,Math.round(n.y-5.2));assert.deepEqual(next.unknown,n.unknown)}
  assert.equal(moved.edges,d.edges)
})
test('equal bounds and overlaps cannot create recursive geometric parent cycles',()=>{
  const a={...node('g1',0,0,200,200),type:'group'},b={...a,id:'g2'},d={nodes:[a,b,node('c',20,20)],edges:[]}
  assert.deepEqual([...expandCanvasSelection(d,selected('g1'))],['g1','c'])
})
test('locked groups protect descendants while selectable copy/unlock remains possible',()=>{
  let d=groupCanvasSelection(document(),selected('a','b'),'g');d=toggleCanvasLock(d,selected('g'))
  assert.equal(canvasSelectionEditable(d,selected('a')),false);assert.equal(moveCanvasNodes(d,selected('a'),50,20),d);assert.equal(deleteCanvasSelection(d,selected('g')),d);assert.equal(patchCanvasSelection(d,selected('a'),{color:'3'}),d)
  assert.equal(toggleCanvasLock(d,selected('a')),d)
  const unlocked=toggleCanvasLock(d,selected('g'));assert.equal(canvasSelectionEditable(unlocked,selected('a')),true)
})
test('a group with a locked child refuses movement/layout/deletion as one operation',()=>{
  let d=groupCanvasSelection(document(),selected('a','b'),'g');d=toggleCanvasLock(d,selected('a'))
  assert.equal(moveCanvasNodes(d,selected('g'),20,30),d);assert.equal(layoutCanvasSelection(d,selected('g','c'),'left'),d);assert.equal(deleteCanvasSelection(d,selected('g')),d)
})
test('ungroup removes only selected containers and their own edges, not linked content',()=>{
  const before=document(),d=groupCanvasSelection(before,selected('a','b'),'g');d.edges=[...d.edges,{id:'gc',fromNode:'g',toNode:'c'}]
  const after=ungroupCanvasSelection(d,selected('g'));assert.deepEqual(after.nodes,before.nodes);assert.deepEqual(after.edges,before.edges)
})
test('deletion removes group descendants and incident edges but not unrelated objects',()=>{
  const d=groupCanvasSelection(document(),selected('a','b'),'g'),after=deleteCanvasSelection(d,selected('g'))
  assert.deepEqual(after.nodes,[d.nodes.at(-1)]);assert.deepEqual(after.edges,[]);assert.deepEqual(after.foreign,d.foreign)
})
test('z-order changes preserve paint order inside the selection and all node identities',()=>{
  const d=document(),front=reorderCanvasSelection(d,selected('a','c'),true)
  assert.deepEqual(front.nodes.map(n=>n.id),['b','a','c']);assert.equal(front.nodes[1],d.nodes[0]);assert.equal(reorderCanvasSelection(front,selected('a','c'),true),front)
  assert.deepEqual(reorderCanvasSelection(d,selected('b'),false).nodes.map(n=>n.id),['b','a','c'])
})
test('alignment and distribution preserve endpoints and produce equal gaps',()=>{
  const d=document(),aligned=layoutCanvasSelection(d,selected('a','b','c'),'left');assert.deepEqual(aligned.nodes.map(n=>n.x),[0,0,0])
  const distributed=layoutCanvasSelection(d,selected('a','b','c'),'distribute-x');assert.deepEqual(distributed.nodes.map(n=>n.x),[0,250,500]);assert.equal(distributed.nodes[0],d.nodes[0]);assert.equal(distributed.nodes[2],d.nodes[2])
  assert.equal(layoutCanvasSelection(d,selected('a','b'),'distribute-x'),d)
})
test('layout carries descendants, while stack/grid are explicit bounded transforms',()=>{
  const d=groupCanvasSelection(document(),selected('a','b'),'g'),after=layoutCanvasSelection(d,selected('g','a','c'),'left'),g=after.nodes.find(n=>n.id==='g')
  assert.equal(after.nodes.find(n=>n.id==='a').x-g.x,24);assert.equal(after.nodes.find(n=>n.id==='c').x,g.x)
  assert.deepEqual(layoutCanvasSelection(document(),selected('a','b','c'),'stack').nodes.map(n=>[n.x,n.y]),[[0,0],[0,104],[0,208]])
  assert.deepEqual(layoutCanvasSelection(document(),selected('a','b','c'),'grid').nodes.map(n=>[n.x,n.y]),[[0,0],[124,0],[0,104]])
})
test('extension updates preserve unknown namespace fields and future versions fail read-only',()=>{
  const d=document();d.nodes[0].brandon={version:1,future:{keep:7}}
  const styled=patchCanvasSelection(d,selected('a'),{color:'3',extension:{fontSize:24}})
  assert.deepEqual(styled.nodes[0].brandon,{version:1,future:{keep:7},fontSize:24});assert.equal(styled.nodes[1],d.nodes[1])
  for(const ext of [{version:2},'foreign',{version:1,locked:'yes'},{version:1,primitive:'unknown'}]){const source=JSON.stringify({...d,nodes:[{...d.nodes[0],brandon:ext}]});assert.ok(parseCanvasDocument(source).error)}
  assert.equal(parseCanvasDocument(JSON.stringify(styled)).error,null)
})
test('invalid movements refuse without mutating the input or producing an undo payload',()=>{
  const d=document(),original=structuredClone(d)
  assert.throws(()=>moveCanvasNodes(d,selected('a'),Infinity,0));assert.throws(()=>moveCanvasNodes(d,selected('a'),1e8,0));assert.deepEqual(d,original);assert.equal(moveCanvasNodes(d,selected('missing'),20,0),d);assert.equal(moveCanvasNodes(d,selected('a'),0,0),d)
})
test('ambiguous overlapping group layouts refuse rather than moving shared children twice',()=>{
  const d={nodes:[{...node('g1',0,0,300,300),type:'group'},{...node('g2',100,0,300,300),type:'group'},node('child',150,20,50,50)],edges:[]}
  assert.throws(()=>layoutCanvasSelection(d,selected('g1','g2'),'left'),/Пересекающиеся/)
})
test('pathological group collections hit a bounded refusal instead of recursive or quadratic unbounded work',()=>{
  const d={nodes:Array.from({length:1500},(_,i)=>({...node('g'+i,0,0,200,200),type:'group'})),edges:[]},ids=new Set(d.nodes.map(n=>n.id))
  assert.throws(()=>expandCanvasSelection(d,ids),/Слишком сложная/)
  assert.throws(()=>canvasSelectionRoots(d,ids),/Слишком сложная/)
})
test('actual Obsidian 1.14.4 save snapshot retains namespaces, geometric groups and standard edge defaults',async()=>{
  const source=await readFile(new URL('./fixtures/r3a-obsidian-1.14.4.canvas',import.meta.url),'utf8'),{document:d,error}=parseCanvasDocument(source)
  assert.equal(error,null);assert.deepEqual(d.foreignDocument,{keep:5});assert.deepEqual(d.nodes[0].foreignGroup,{keep:1});assert.equal(d.nodes.find(n=>n.id==='beta').x,100)
  assert.deepEqual(d.nodes.find(n=>n.id==='alpha').brandon,{version:1,primitive:'sticky',locked:true,future:{keep:2}})
  assert.deepEqual(d.edges[0].foreignEdge,{keep:4});assert.deepEqual(d.edges[0].brandon,{version:1,dash:'dashed'});assert.equal(d.edges[0].toEnd,undefined)
  assert.equal(canvasSelectionEditable(d,selected('alpha')),false)
  const unlocked=toggleCanvasLock(d,selected('alpha')),after=moveCanvasNodes(unlocked,selected('group'),20,30)
  assert.equal(after.nodes.find(n=>n.id==='beta').x,120);assert.deepEqual(after.foreignDocument,d.foreignDocument);assert.equal(after.edges,d.edges)
})
test('locking a mixed selection locks all selected roots, not redundant child locks',()=>{
  let d=toggleCanvasLock(document(),selected('a'));d=toggleCanvasLock(d,selected('a','b'));assert.equal(d.nodes[0].brandon.locked,true);assert.equal(d.nodes[1].brandon.locked,true)
  let grouped=groupCanvasSelection(document(),selected('a','b'),'g');grouped=toggleCanvasLock(grouped,selected('g','a'));assert.equal(grouped.nodes[0].brandon.locked,true);assert.equal(grouped.nodes[1].brandon,undefined)
  const unlocked=toggleCanvasLock(grouped,selected('g','a'));assert.equal(canvasSelectionEditable(unlocked,selected('a')),true)
})
test('an unrelated edit cannot round a foreign 64-bit ID, precise decimal, underflow or infinity into different JSON data',()=>{
  for(const token of ['9007199254740993','9007199254740992','0.123456789012345678901','1e-9999','1e9999']){
    const source=JSON.stringify(document()).replace('"foreign":{"keep":true}',`"foreign":{"keep":${token}}`);assert.match(parseCanvasDocument(source).error,/точности/)
  }
  for(const token of ['1.5000','1.25e-3','0e9999','1e3','9007199254740991']){
    const source=JSON.stringify(document()).replace('"foreign":{"keep":true}',`"foreign":{"keep":${token}}`);assert.equal(parseCanvasDocument(source).error,null)
  }
  const d=document();d.nodes[0].text='A string with 9007199254740993 and escaped "quotes"';assert.equal(parseCanvasDocument(JSON.stringify(d)).error,null)
})
test('the global Canvas catalog refuses corrupted cross-context binding collisions',()=>{
  const commands=[...CANVAS_COMMANDS.map(c=>({...c,run:()=>{}})),{id:'new-note',label:'New note',run:()=>{}}]
  const registry=createCommandRegistry(commands,{'canvas-group':'Mod+Shift+KeyG','new-note':'Mod+Shift+KeyG'})
  assert.equal(registry.forEvent({key:'П',code:'KeyG',ctrlKey:true,metaKey:false,shiftKey:true,altKey:false,isComposing:false,repeat:false}),undefined)
})
test('group backgrounds share an explicit translucent RGBA fill across DOM, overview and native DTO',()=>{
  assert.deepEqual(canvasColors({type:'group',color:'6'}),{fill:'#322b3e33',border:'#9381ba'})
  assert.equal(canvasColors({type:'group',color:'#FF0000'}).fill.length,9)
  assert.equal(canvasColors({type:'text',color:'6'}).fill,'#322b3e')
})
