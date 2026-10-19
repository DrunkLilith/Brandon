import assert from 'node:assert/strict'
import {test} from 'node:test'
import {taskReferenceNode,canvasTaskId,canvasTaskBinding,canvasTaskLabel,validTaskFallback} from '../../apps/desktop/src/canvas-tasks.ts'
import {parseCanvasDocument} from '../../apps/desktop/src/canvas-document.ts'
import {groupCanvasSelection,moveCanvasNodes,deleteCanvasSelection,toggleCanvasLock,canvasSelectionEditable,insertCanvasNodes} from '../../apps/desktop/src/canvas-operations.ts'
const task={id:'task-one',managed:true,revision:'r1',path:'Tasks/Учёба.md',title:'Подготовить проект',status:'todo',completed:false,project:'Учёба',scheduled:'2040-01-01',due:'2040-01-02',tags:[],taskTags:[],line:1,blockId:null,durationMinutes:30,priority:'high'}
const map=new Map([[task.id,task]]),node=()=>taskReferenceNode('view',task,{x:0,y:0})
test('a task reference is portable Markdown plus stable identity, never a cloned row or renderer label',()=>{
  const n=node();assert.deepEqual(n.brandon,{version:1,taskId:task.id});assert.equal(n.type,'file');assert.equal(n.file,task.path);for(const key of ['title','status','revision','text','scheduled','due'])assert.equal(Object.hasOwn(n,key),false);assert.equal(parseCanvasDocument(JSON.stringify({nodes:[n],edges:[]})).error,null);assert.equal(canvasTaskId(n),task.id)
  assert.throws(()=>taskReferenceNode('x',{...task,managed:false},{x:0,y:0}));assert.throws(()=>taskReferenceNode('x',{...task,path:'../outside.md'},{x:0,y:0}))
})
test('unknown or malformed task identity/fallback is read-only, not silently rebound',()=>{
  for(const patch of [{type:'text'},{file:'../outside.md'},{file:'C:/file.md'},{file:'.private/file.md'},{file:'Tasks\\file.md'},{file:'/absolute.md'},{file:'file.png'},{brandon:{version:1,taskId:42}},{brandon:{version:1,taskId:''}},{brandon:{version:1,taskId:'bad/id'}}])assert.ok(parseCanvasDocument(JSON.stringify({nodes:[{...node(),...patch}],edges:[]})).error)
  assert.equal(validTaskFallback('Задачи/Обычная.md'),true);assert.equal(validTaskFallback('file\u0000.md'),false)
})
test('bindings resolve managed TaskId, not a matching filename, and distinguish loading/missing/moved',()=>{
  assert.equal(canvasTaskBinding(node(),map).task,task);assert.equal(canvasTaskBinding(node(),map).error,null);assert.match(canvasTaskBinding(node(),map,false).error,/загружаются/);assert.match(canvasTaskBinding(node(),new Map([['different',{...task,id:'different'}]])).error,/недоступна/);assert.match(canvasTaskBinding(node(),new Map([[task.id,{...task,path:'Tasks/New.md'}]])).error,/изменился/);assert.match(canvasTaskBinding(node(),new Map([[task.id,{...task,managed:false}]])).error,/каноническая/);assert.equal(canvasTaskBinding({...node(),brandon:undefined},map),null)
})
test('two card identities share fresh canonical display while source and ordinary notes stay untouched',()=>{
  const a=node(),b={...a,id:'copy',x:200},before=JSON.stringify([a,b]),updated=new Map([[task.id,{...task,status:'done',completed:true,revision:'r2'}]])
  assert.match(canvasTaskLabel(a,map),/К выполнению/);assert.match(canvasTaskLabel(a,updated),/☑.*Подготовить проект/);assert.equal(canvasTaskLabel(a,updated),canvasTaskLabel(b,updated));assert.equal(JSON.stringify([a,b]),before);assert.equal(canvasTaskLabel({...a,brandon:undefined},map),undefined)
})
test('copy/group/move/delete and locks affect representation, not task identity or canonical metadata',()=>{
  const a=node(),copy={...a,id:'copy',x:300,unknown:{keep:true}},before=JSON.stringify(task),d={nodes:[a,copy],edges:[],future:{keep:1}},g=groupCanvasSelection(d,new Set(['view','copy']),'g'),m=moveCanvasNodes(g,new Set(['g']),10,20)
  assert.deepEqual(m.nodes.filter(n=>n.type==='file').map(canvasTaskId),[task.id,task.id]);const locked=toggleCanvasLock(m,new Set(['g']));assert.equal(canvasSelectionEditable(locked,new Set(['view'])),false);assert.equal(deleteCanvasSelection(locked,new Set(['g'])),locked);assert.throws(()=>insertCanvasNodes(locked,[{...node(),id:'blocked',x:a.x+10,y:a.y+20}]),/заблокирован/i);assert.equal(deleteCanvasSelection(m,new Set(['g'])).nodes.length,0);assert.equal(JSON.stringify(task),before);assert.deepEqual(m.future,{keep:1})
})
