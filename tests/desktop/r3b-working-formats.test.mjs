import test from 'node:test'
import assert from 'node:assert/strict'
import {compileTaskFormat,createTaskFormat,isWorkingFormat,civilOrdinal,civilDate,civilSchedule,shiftTaskSchedule,resizeTaskDuration,timelineTask} from '../../apps/desktop/src/working-formats.ts'
import {queryTasks} from '../../apps/desktop/src/task-query.ts'
import {virtualRange} from '../../apps/desktop/src/virtual-rows.ts'
const task=(id,patch={})=>({id,title:id,managed:true,completed:false,status:'todo',project:'Project',path:`Tasks/${id}.md`,revision:'rev-'+id,scheduled:'2026-10-19 09:30',due:'2026-10-25',durationMinutes:60,tags:[],taskTags:[],priority:null,...patch})
test('three saved layouts share one filter/query and never store task rows or revisions',()=>{
  for(const layout of ['table','kanban','timeline']){
    const source=createTaskFormat('Project',layout),raw=JSON.parse(source),before=JSON.stringify(raw),definition=compileTaskFormat(raw)
    assert.equal(isWorkingFormat(raw),true);assert.equal(definition.views[0].layout,layout);assert.equal(definition.views.length,3);assert.equal(definition.filter.project,'Project');assert.deepEqual(definition.warnings,[])
    const tasks=[task('a'),task('other',{project:'Other'}),task('b',{status:'doing'})]
    for(const view of definition.views)assert.deepEqual(queryTasks(tasks,definition.filter,view.sort).map(t=>t.id),['a','b'])
    assert.equal(JSON.stringify(raw),before);assert.equal(definition.raw,raw);assert.ok(!source.includes('revision'));assert.ok(!source.includes('Tasks/'))
  }
})
test('unknown v1 fields survive inspection and are reported instead of executed',()=>{
  const raw=JSON.parse(createTaskFormat());raw.foreign={keep:73};raw.brandon.foreign={command:'not executed'};raw.views[0].foreign={url:'not fetched'}
  const before=JSON.stringify(raw),definition=compileTaskFormat(raw);assert.equal(definition.warnings.length,3);assert.equal(JSON.stringify(raw),before);assert.equal(definition.raw,raw)
})
test('a large unknown-field set cannot create an unbounded warning DOM and is not pruned from source',()=>{
  const raw=JSON.parse(createTaskFormat());for(let i=0;i<1000;i++)raw['foreign'+i]={keep:i};const before=JSON.stringify(raw),d=compileTaskFormat(raw)
  assert.equal(d.warnings.length,129);assert.ok(d.warnings.at(-1).includes('872'));assert.equal(JSON.stringify(raw),before);assert.equal(d.raw.foreign999.keep,999)
})
test('future versions, unknown types including prototype names, duplicate IDs and unsafe expressions fail closed',()=>{
  for(const type of ['foreign','__proto__','constructor','toString']){const raw=JSON.parse(createTaskFormat());raw.views[0].type=type;assert.throws(()=>compileTaskFormat(raw),/Неизвестный тип/)}
  const raw=JSON.parse(createTaskFormat());raw.brandon.version=2;assert.throws(()=>compileTaskFormat(raw),/версия|верси/)
  raw.brandon.version=1;raw.views[1].id=raw.views[0].id;assert.throws(()=>compileTaskFormat(raw),/повторяющееся/)
  const expression=JSON.parse(createTaskFormat());expression.brandon.filter.expression='fetch("https://example.invalid")';assert.throws(()=>compileTaskFormat(expression))
  assert.equal(isWorkingFormat({views:[]}),false);assert.equal(isWorkingFormat({brandon:null}),true);assert.throws(()=>compileTaskFormat({brandon:null}))
})
test('known filters, sort, windows and view count have explicit bounded contracts',()=>{
  for(const field of ['filter','sort','window','count']){const raw=JSON.parse(createTaskFormat());if(field==='filter')raw.brandon.filter.unknown='x';if(field==='sort')raw.views[0].sort={property:'constructor',direction:'ASC'};if(field==='window')raw.views[0].windowDays=365;if(field==='count')raw.views=Array.from({length:17},(_,i)=>({...raw.views[0],id:String(i)}));assert.throws(()=>compileTaskFormat(raw))}
  for(const days of [7,14,30,60]){const raw=JSON.parse(createTaskFormat());raw.views[2].windowDays=days;assert.equal(compileTaskFormat(raw).views[2].windowDays,days)}
})
test('civil arithmetic is calendar-based across leap days, DST boundaries and years 0001..9999',()=>{
  for(const date of ['0001-01-01','0099-02-03','2024-02-29','2026-03-08','2026-11-01','9999-12-31'])assert.equal(civilDate(civilOrdinal(date)),date)
  assert.equal(civilDate(civilOrdinal('2024-02-28')+1),'2024-02-29');assert.equal(civilDate(civilOrdinal('2024-02-28')+2),'2024-03-01')
  assert.equal(civilOrdinal('2026-03-09')-civilOrdinal('2026-03-08'),1)
  for(const date of ['0000-01-01','2025-02-29','2026-13-01','2026-00-03','2026-04-31','2026-1-01'])assert.throws(()=>civilOrdinal(date))
  assert.throws(()=>civilDate(civilOrdinal('9999-12-31')+1))
})
test('a timeline move changes scheduled only, preserving date-only and exact optional clock separator',()=>{
  for(const scheduled of ['2026-03-08','2026-03-08 02:30','2026-03-08T02:30']){
    const t=task('a',{scheduled}),before=JSON.stringify(t),patch=shiftTaskSchedule(t,1)
    assert.deepEqual(patch,{scheduled:'2026-03-09'+scheduled.slice(10)});assert.equal(JSON.stringify(t),before);assert.equal(Object.hasOwn(patch,'due'),false);assert.equal(Object.hasOwn(patch,'durationMinutes'),false)
  }
  assert.throws(()=>shiftTaskSchedule({scheduled:null},1),/плановую/);assert.throws(()=>shiftTaskSchedule({scheduled:'2026-10-19'},.5))
  for(const date of ['2026-10-19 24:00','2026-10-19 12:60','2026-10-19T12:00Z'])assert.throws(()=>civilSchedule(date))
})
test('resize changes duration only and never invents an appointment for a date-only plan',()=>{
  const t=task('a'),before=JSON.stringify(t);assert.deepEqual(resizeTaskDuration(t,15),{durationMinutes:75});assert.equal(JSON.stringify(t),before)
  assert.deepEqual(resizeTaskDuration(task('a',{durationMinutes:null}),-15),{durationMinutes:45})
  assert.throws(()=>resizeTaskDuration(task('a',{scheduled:'2026-10-19'}),15),/полночь/)
  for(const delta of [1,.5,Infinity,-60,525600])assert.throws(()=>resizeTaskDuration(t,delta))
})
test('timeline shows separate schedule interval and due marker, clipping without mutating sources',()=>{
  const t=task('a',{scheduled:'2026-10-18 12:00',durationMinutes:1440,due:'2026-10-22 12:00'}),before=JSON.stringify(t),v=timelineTask(t,'2026-10-19',7)
  assert.equal(v.bar.left,0);assert.ok(Math.abs(v.bar.width-.5/7*100)<1e-8);assert.equal(v.bar.clipped,true);assert.equal(v.bar.dateOnly,false);assert.equal(v.due,50);assert.equal(JSON.stringify(t),before)
  const day=timelineTask(task('day',{scheduled:'2026-10-19',durationMinutes:60,due:'2026-10-20'}),'2026-10-19',7);assert.equal(day.bar.dateOnly,true);assert.ok(Math.abs(day.bar.width-100/7)<1e-8)
})
test('unscheduled/out-of-window tasks retain distinct reasons and can still have a visible deadline',()=>{
  const u=timelineTask(task('a',{scheduled:null,due:'2026-10-20'}),'2026-10-19',7);assert.equal(u.bar,null);assert.equal(u.reason,'unscheduled');assert.ok(Math.abs(u.due-100/7)<1e-8)
  const outside=timelineTask(task('a',{scheduled:'2027-01-01',due:'2026-10-21'}),'2026-10-19',7);assert.equal(outside.reason,'outside');assert.equal(outside.bar,null);assert.ok(Math.abs(outside.due-200/7)<1e-8)
})
test('3000 tasks remain in query results while each layout can bound its visible row range',()=>{
  const definition=compileTaskFormat(JSON.parse(createTaskFormat('Project'))),tasks=Array.from({length:3000},(_,i)=>task('n'+String(i).padStart(4,'0')))
  for(const view of definition.views){const rows=queryTasks(tasks,definition.filter,view.sort);assert.equal(rows.length,3000);const height=view.layout==='kanban'?126:view.layout==='timeline'?64:54,r=virtualRange(rows.length,500,(rows.length-3)*height,height);assert.ok(r.end-r.start<50);assert.equal(r.end,3000);assert.equal(rows[r.end-1].id,'n2999')}
})
