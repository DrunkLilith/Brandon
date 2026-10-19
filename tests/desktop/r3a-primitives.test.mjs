import assert from 'node:assert/strict'
import {test} from 'node:test'
import {CANVAS_PRIMITIVES,primitiveNode,canvasAppearance,canvasNodeHit,canvasShapePath,canvasTopHit} from '../../apps/desktop/src/canvas-primitives.ts'
import {parseCanvasDocument} from '../../apps/desktop/src/canvas-document.ts'
import {insertCanvasNodes,patchCanvasSelection} from '../../apps/desktop/src/canvas-operations.ts'
test('every primitive has a valid portable text/color/geometry fallback and versioned namespace',()=>{
  for(const {id} of CANVAS_PRIMITIVES){const node=primitiveNode(id,id,{x:0,y:0});assert.equal(node.type,'text');assert.equal(node.brandon.version,1);assert.equal(node.brandon.primitive,id);assert.ok(node.text);assert.equal(parseCanvasDocument(JSON.stringify({nodes:[node],edges:[]})).error,null)}
  assert.throws(()=>primitiveNode('x','script',{x:0,y:0}));assert.throws(()=>primitiveNode('x','sticky',{x:Infinity,y:0}))
})
test('ellipse/diamond/triangle hit tests exclude bounding-box corners and include true boundaries',()=>{
  for(const id of ['ellipse','diamond','triangle']){
    const node={...primitiveNode(id,id,{x:0,y:0}),x:0,y:0,width:200,height:100}
    assert.equal(canvasNodeHit(node,{x:0,y:0}),false);assert.equal(canvasNodeHit(node,{x:100,y:50}),true);assert.equal(canvasNodeHit(node,{x:100,y:0}),true);assert.equal(canvasNodeHit(node,{x:201,y:50}),false)
  }
})
test('divider hit tolerance is explicit and text boxes remain selectable without an opaque fill',()=>{
  const divider={...primitiveNode('d','divider',{x:0,y:0}),x:0,y:0};assert.equal(canvasNodeHit(divider,{x:100,y:1},5),false);assert.equal(canvasNodeHit(divider,{x:100,y:20},5),true)
  const text=primitiveNode('t','text',{x:0,y:0});assert.equal(canvasAppearance(text).fill,'#00000000');assert.equal(canvasAppearance(text).shape,6);assert.equal(canvasNodeHit(text,{x:text.x,y:text.y}),true)
})
test('common relative text bounds stay inside every filled primitive at corners',()=>{
  for(const id of ['rectangle','ellipse','diamond','triangle','sticky']){
    const node=primitiveNode(id,id,{x:0,y:0}),{bounds}=canvasAppearance(node)
    for(const x of [bounds.x,bounds.x+bounds.width])for(const y of [bounds.y,bounds.y+bounds.height])assert.equal(canvasNodeHit(node,{x:node.x+x,y:node.y+y}),true,id)
  }
})
test('paint/typography values and unknown extension data survive roundtrip; invalid paint fails closed',()=>{
  const node=primitiveNode('x','ellipse',{x:0,y:0});Object.assign(node.brandon,{fillColor:'transparent',strokeColor:'#abcdef',textColor:'#FFEEDD',fontSize:32,textAlign:'right',strokeWidth:4,future:{keep:8}})
  const parsed=parseCanvasDocument(JSON.stringify({nodes:[node],edges:[],other:7}));assert.equal(parsed.error,null);assert.deepEqual(parsed.document.nodes[0].brandon,node.brandon)
  assert.deepEqual({...canvasAppearance(node),clipPath:undefined},{...canvasAppearance(parsed.document.nodes[0]),clipPath:undefined});assert.equal(canvasAppearance(node).fill,'#00000000');assert.equal(canvasAppearance(node).border,'#abcdef');assert.equal(canvasAppearance(node).fontSize,32)
  for(const field of ['fillColor','strokeColor','textColor']){const bad=structuredClone(node);bad.brandon[field]='url(https://invalid)';assert.ok(parseCanvasDocument(JSON.stringify({nodes:[bad],edges:[]})).error)}
})
test('shape paths are bounded local geometry and cannot include source text or executable content',()=>{
  for(const {id}of CANVAS_PRIMITIVES){const node=primitiveNode(id,id,{x:0,y:0});node.text='<script>no</script>';const path=canvasShapePath(node);assert.ok(path.startsWith('M '));assert.ok(!path.includes('script'));assert.ok(!path.includes('NaN'))}
})
test('style command rejects invalid active fields and cannot turn a file/group into a malformed primitive',()=>{
  const node=primitiveNode('x','ellipse',{x:0,y:0}),d={nodes:[node],edges:[]},before=structuredClone(d)
  assert.throws(()=>patchCanvasSelection(d,new Set(['x']),{extension:{fontSize:999}}));assert.deepEqual(d,before)
  const file={...node,type:'file',file:'Source.md',brandon:undefined};assert.throws(()=>patchCanvasSelection({nodes:[file],edges:[]},new Set(['x']),{extension:{primitive:'ellipse'}}))
})

test('fresh tools refuse locked geometric containment atomically and reject duplicate IDs',()=>{
  const group={id:'g',type:'group',x:-500,y:-500,width:1000,height:1000,brandon:{version:1,locked:true}},document={nodes:[group],edges:[],foreign:{keep:4}},before=structuredClone(document)
  assert.throws(()=>insertCanvasNodes(document,[primitiveNode('s','sticky',{x:0,y:0})]),/заблокированной группы/);assert.deepEqual(document,before)
  assert.throws(()=>insertCanvasNodes(document,[primitiveNode('g','text',{x:1000,y:0})]),/ID/)
  const next=insertCanvasNodes(document,[primitiveNode('s','sticky',{x:1000,y:0})]);assert.equal(next.nodes.length,2);assert.deepEqual(next.foreign,document.foreign)
})

test('explicit paint reset removes only that override and retains future extension fields',()=>{
  const node=primitiveNode('x','ellipse',{x:0,y:0});Object.assign(node.brandon,{fillColor:'#112233',strokeColor:'#445566',future:{keep:9}})
  const document={nodes:[node],edges:[]},next=patchCanvasSelection(document,new Set(['x']),{extension:{fillColor:undefined}})
  const roundtrip=parseCanvasDocument(JSON.stringify(next));assert.equal(roundtrip.error,null);assert.equal(roundtrip.document.nodes[0].brandon.fillColor,undefined);assert.equal(roundtrip.document.nodes[0].brandon.strokeColor,'#445566');assert.deepEqual(roundtrip.document.nodes[0].brandon.future,{keep:9});assert.equal(node.brandon.fillColor,'#112233')
  assert.equal(canvasAppearance(node).centerText,true);assert.equal(canvasAppearance(node).textAlign,'center')
})
test('transparent foreground groups preserve child image/text picking, retain header priority and empty-body selection',()=>{
  const child={id:'photo',type:'file',file:'Photo.png',x:10,y:10,width:200,height:190},group={id:'g',type:'group',x:0,y:0,width:300,height:300},point={x:50,y:140};assert.equal(canvasTopHit([child,group],point).id,'photo');assert.equal(canvasTopHit([child,group],{x:50,y:20}).id,'g');assert.equal(canvasTopHit([group],point).id,'g');assert.equal(canvasTopHit([group,child],point).id,'photo')
})
