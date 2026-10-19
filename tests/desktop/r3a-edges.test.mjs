import assert from 'node:assert/strict'
import {test} from 'node:test'
import {readFile} from 'node:fs/promises'
import {prepareCanvasEdge,prepareCanvasEdges,canvasEdgeAnchor,canvasEdgeHit,samePreparedEdge,patchCanvasEdge,reverseCanvasEdge,deleteCanvasEdge,connectCanvasNodes,canvasEdgeEditable,EDGE_SIDES} from '../../apps/desktop/src/canvas-edges.ts'
import {parseCanvasDocument} from '../../apps/desktop/src/canvas-document.ts'
const a={id:'a',type:'text',x:0,y:0,width:100,height:100,text:'A'},b={...a,id:'b',x:300,text:'Б'},c={...a,id:'c',x:600},edge={id:'e',fromNode:'a',toNode:'b'},document={nodes:[a,b,c],edges:[edge],future:{keep:9}}

test('standard omitted ends mean none at start and arrow at end; automatic facing anchors never mutate source',()=>{
  const before=structuredClone(edge),prepared=prepareCanvasEdge(edge,a,b);assert.deepEqual(prepared.segments,[[100,50,300,50]]);assert.equal(prepared.heads.length,2);assert.ok(prepared.heads.every(s=>s[2]===300&&s[3]===50));assert.deepEqual(edge,before)
  assert.equal(prepareCanvasEdge({...edge,toEnd:'none'},a,b).heads.length,0);assert.equal(prepareCanvasEdge({...edge,fromEnd:'arrow'},a,b).heads.length,4)
})
test('all explicit cardinal side combinations share exact finite endpoint geometry',()=>{
  for(const from of EDGE_SIDES)for(const to of EDGE_SIDES){const p=prepareCanvasEdge({...edge,fromSide:from.id,toSide:to.id},a,b),s=p.segments[0],fromPoint=canvasEdgeAnchor(a,from.id),toPoint=canvasEdgeAnchor(b,to.id);assert.deepEqual(s,[fromPoint.x,fromPoint.y,toPoint.x,toPoint.y]);assert.ok(p.width>0&&p.height>0);assert.ok(p.heads.flat().every(Number.isFinite))}
})
test('triangle and divider anchors use actual shared outlines rather than empty bounding-box corners',()=>{
  const triangle={...a,brandon:{version:1,primitive:'triangle'}},divider={...a,brandon:{version:1,primitive:'divider'}};assert.deepEqual(canvasEdgeAnchor(triangle,'left'),{x:25,y:50});assert.deepEqual(canvasEdgeAnchor(triangle,'right'),{x:75,y:50});assert.deepEqual(canvasEdgeAnchor(divider,'top'),{x:50,y:50})
})
test('coincident endpoints produce a bounded visible loop and finite arrow wings',()=>{
  const p=prepareCanvasEdge({...edge,toNode:'a'},a,a);assert.equal(p.segments.length,4);assert.ok(p.width>40&&p.height>40);assert.ok(p.segments.every(s=>Math.hypot(s[2]-s[0],s[3]-s[1])>0));assert.ok(p.heads.flat().every(Number.isFinite))
})
test('hit testing uses line distance, arrow wings and label bounds, not the entire diagonal bounding box',()=>{
  const p=prepareCanvasEdge(edge,a,{...b,y:300});assert.equal(canvasEdgeHit(p,{x:200,y:200},2),true);assert.equal(canvasEdgeHit(p,{x:90,y:250},2),false)
  const labelled=prepareCanvasEdge({...edge,label:'Русская подпись'},a,b),r=labelled.labelRect;assert.equal(canvasEdgeHit(labelled,{x:r.x+1,y:r.y+1},1),true)
})
test('long plain labels have bounded two-line display while original Unicode source remains unchanged',()=>{
  const label='Длинная 🧠 подпись <script>never execute</script> '.repeat(100),input={...edge,label},before=structuredClone(input),p=prepareCanvasEdge(input,a,b);assert.equal(p.label.split('\n').length,2);assert.ok(Array.from(p.label).length<=50);assert.ok(p.label.endsWith('…'));assert.ok(p.labelRect.width<=240);assert.deepEqual(input,before)
})
test('edge styling preserves standard and foreign data; a reset removes only the requested standard field',()=>{
  const source={...document,edges:[{...edge,color:'6',foreign:{keep:2},brandon:{version:1,future:{keep:3}}}]},before=structuredClone(source),next=patchCanvasEdge(source,'e',{fromSide:'top',toSide:'bottom',fromEnd:'arrow',toEnd:'none',label:'Подпись',extension:{dash:'dashed',strokeWidth:4},color:undefined}),roundtrip=parseCanvasDocument(JSON.stringify(next));assert.equal(roundtrip.error,null);const e=roundtrip.document.edges[0];assert.equal(e.color,undefined);assert.equal(e.brandon.strokeWidth,4);assert.deepEqual(e.foreign,{keep:2});assert.deepEqual(e.brandon.future,{keep:3});assert.deepEqual(next.nodes,source.nodes);assert.deepEqual(source,before)
})
test('reversal swaps endpoints and semantic default arrow ends in one pure transform',()=>{
  const next=reverseCanvasEdge(document,'e');assert.equal(next.edges[0].fromNode,'b');assert.equal(next.edges[0].toNode,'a');assert.equal(next.edges[0].fromEnd,'arrow');assert.equal(next.edges[0].toEnd,'none');assert.ok(samePreparedEdge(prepareCanvasEdges(document)[0],prepareCanvasEdges(reverseCanvasEdge(next,'e'))[0]));assert.deepEqual(next.future,document.future)
})
test('creating a valid connection adds one standard edge and never copies or mutates either canonical node',()=>{
  const before=structuredClone(document),next=connectCanvasNodes(document,'a','c','new');assert.equal(next.edges.length,2);assert.deepEqual(next.edges[1],{id:'new',fromNode:'a',toNode:'c'});assert.equal(next.nodes,document.nodes);assert.deepEqual(document,before);assert.deepEqual(next.future,document.future)
})
test('reattachment keeps identity/metadata and deletion removes only the targeted edge',()=>{
  const source={...document,edges:[{...edge,foreign:{keep:7}},{...edge,id:'other',toNode:'c'}]},next=patchCanvasEdge(source,'e',{toNode:'c'});assert.equal(next.edges[0].id,'e');assert.equal(next.edges[0].toNode,'c');assert.deepEqual(next.edges[0].foreign,{keep:7});assert.equal(deleteCanvasEdge(next,'e').edges[0].id,'other');assert.deepEqual(source.nodes,next.nodes)
})
test('locked endpoint and locked ancestor cannot be bypassed by connection create/edit/delete/reverse',()=>{
  const locked={...document,nodes:[a,{...b,brandon:{version:1,locked:true}},c]},before=structuredClone(locked);assert.equal(canvasEdgeEditable(locked,'e'),false)
  for(const action of [()=>patchCanvasEdge(locked,'e',{label:'bad'}),()=>deleteCanvasEdge(locked,'e'),()=>reverseCanvasEdge(locked,'e'),()=>connectCanvasNodes(locked,'c','b','new')])assert.throws(action,/заблокирован/)
  assert.deepEqual(locked,before);const group={id:'g',type:'group',x:290,y:-10,width:120,height:120,brandon:{version:1,locked:true}};assert.equal(canvasEdgeEditable({...document,nodes:[...document.nodes,group]},'e'),false)
})
test('invalid known fields fail closed while dangling references and foreign data are retained, not silently discarded',()=>{
  for(const patch of [{fromSide:'middle'},{toEnd:'circle'},{label:8},{color:9},{brandon:{version:1,strokeWidth:100}},{brandon:{version:1,dash:'script'}}])assert.ok(parseCanvasDocument(JSON.stringify({...document,edges:[{...edge,...patch}]})).error)
  const dangling={...document,edges:[{...edge,toNode:'missing',foreign:3}]};assert.equal(parseCanvasDocument(JSON.stringify(dangling)).error,null);assert.deepEqual(prepareCanvasEdges(dangling),[]);assert.throws(()=>patchCanvasEdge(document,'e',{toNode:'missing'}));assert.throws(()=>connectCanvasNodes(document,'a','b','e'))
})
test('prepared-edge deltas change only incident geometry and ignore unrelated source fields',()=>{
  const source={...document,edges:[edge,{id:'second',fromNode:'b',toNode:'c'}]},before=prepareCanvasEdges(source),after=prepareCanvasEdges({...source,nodes:[{...a,x:-20},b,c]});assert.equal(samePreparedEdge(before[0],after[0]),false);assert.equal(samePreparedEdge(before[1],after[1]),true)
  const metadata=prepareCanvasEdges({...source,edges:source.edges.map(e=>({...e,foreign:{keep:5}}))});assert.ok(metadata.every((e,i)=>samePreparedEdge(before[i],e)))
})
test('shared geometry golden fixtures match the exact DTO consumed by native Rust validation',async()=>{
  const fixtures=JSON.parse(await readFile(new URL('./fixtures/r3a-edge-geometry.json',import.meta.url),'utf8'));for(const fixture of fixtures)assert.deepEqual(prepareCanvasEdges(fixture.document),fixture.prepared,fixture.name)
})
