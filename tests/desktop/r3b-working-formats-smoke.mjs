// Authored acceptance suite. Do not execute until the owner unlocks the session
// and an approved QA app is running. This script never launches an app.
import assert from 'node:assert/strict'
import {mkdtemp,writeFile,readFile} from 'node:fs/promises'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {connect} from './cdp-test-client.mjs'
import {createTaskFormat} from '../../apps/desktop/src/working-formats.ts'
if(process.env.BRANDON_LINKED_QA!=='1')throw Error('Opt-in BRANDON_LINKED_QA=1 and an owner-approved running QA app are required; no launcher is provided here.')
const c=await connect(),{evaluate,send,click,key,waitFor,delay}=c
const command=(name,args={})=>evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)})`)
await waitFor("Boolean(document.querySelector('.metadata-status'))")
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i,.side-save:not(:disabled),.workspace-dialog,.conflict-dialog,.source-recovery'))"),false)
const original=await command('restore_vault');assert.ok(original?.root)
const scratch=await mkdtemp(join(tmpdir(),'brandon-r3b-linked-')),passes=[],pass=s=>{passes.push(s);console.log('PASS '+s)}
const fill=async(selector,text)=>{await evaluate(`document.querySelector(${JSON.stringify(selector)}).focus()`);await key('a','KeyA',2);await send('Input.insertText',{text})}
const select=(selector,value)=>evaluate(`{const e=document.querySelector(${JSON.stringify(selector)});e.value=${JSON.stringify(value)};e.dispatchEvent(new Event('change',{bubbles:true}))}`)
const action=text=>evaluate(`{const b=Array.from(document.querySelectorAll('.workspace-dialog button,.task-format-view button')).find(b=>b.textContent.trim()===${JSON.stringify(text)});if(!b||b.disabled)throw Error('Missing or disabled action');b.click()}`)
let root,task
const current=async()=>((await command('metadata_delta',{root,generation:null,after:null})).tasks??[]).find(t=>t.id===task.id)
const taskWait=async predicate=>{for(let i=0;i<100;i++){const value=await current();if(value&&predicate(value))return value;await delay(100)}throw Error('Task mutation did not project')}
try{
  const note='# Linked document\n\nOne original.\n\n![Do not fetch](https://example.invalid/image.png)\n[Inert URL](https://example.invalid/)\n'
  const board={nodes:[{id:'doc',type:'file',file:'Document.md',x:-320,y:-120,width:280,height:240},{id:'formats',type:'file',file:'Tasks.base',x:80,y:-120,width:280,height:240}],edges:[],foreign:{keep:73}}
  await writeFile(join(scratch,'Document.md'),note);await writeFile(join(scratch,'Tasks.base'),createTaskFormat('QA'));await writeFile(join(scratch,'Board.canvas'),JSON.stringify(board))
  root=(await command('open_vault',{path:scratch})).root
  const made=await command('create_task',{root,title:'One canonical linked task',scheduled:'2026-10-19 09:30'});assert.ok(made)
  for(let i=0;i<50;i++){task=(await command('metadata_delta',{root,generation:null,after:null})).tasks.find(t=>t.managed);if(task)break;await delay(100)}assert.ok(task?.revision)
  await command('patch_managed_task',{root,taskId:task.id,expectedRevision:task.revision,patch:{project:'QA',due:'2026-10-25',durationMinutes:60}})
  const definition=await readFile(join(scratch,'Tasks.base'),'utf8'),boardBytes=await readFile(join(scratch,'Board.canvas'),'utf8'),description=await readFile(join(scratch,task.path))
  await send('Page.reload');await waitFor("Boolean(document.querySelector('.tree-row[title=\"Tasks.base\"]'))")
  await click('.tree-row[title="Tasks.base"]');await waitFor("Boolean(document.querySelector('.task-format-table [data-task-id]'))")
  await select('.task-format-table [data-task-id] select','doing');await taskWait(t=>t.status==='doing')
  await select('[aria-label="Связанный формат задач"]','kanban');await waitFor("Boolean(document.querySelector('[data-task-drop-status=doing] [data-task-id]'))")
  await select('[data-task-drop-status=doing] [data-task-id] select','done');await taskWait(t=>t.status==='done')
  assert.equal(await readFile(join(scratch,'Tasks.base'),'utf8'),definition);assert.deepEqual(await readFile(join(scratch,task.path)),description)
  pass('Table and Kanban edit one canonical TaskId without copying state to .base or rewriting its Markdown description')
  await select('[aria-label="Связанный формат задач"]','timeline');await waitFor("Boolean(document.querySelector('.task-timeline-row'))")
  await evaluate("{const e=document.querySelector('[aria-label=\"Начало Timeline\"]');Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set.call(e,'2026-10-19');e.dispatchEvent(new Event('input',{bubbles:true}));e.dispatchEvent(new Event('change',{bubbles:true}))}")
  const geometry=await evaluate("(()=>{const bar=document.querySelector('.task-timeline-move').getBoundingClientRect(),track=document.querySelector('.task-timeline-track').getBoundingClientRect();return{x:bar.x+Math.min(3,bar.width/2),y:bar.y+bar.height/2,day:track.width/14}})()")
  await send('Input.dispatchMouseEvent',{type:'mouseMoved',x:geometry.x,y:geometry.y});await send('Input.dispatchMouseEvent',{type:'mousePressed',button:'left',buttons:1,clickCount:1,x:geometry.x,y:geometry.y});await send('Input.dispatchMouseEvent',{type:'mouseMoved',buttons:1,x:geometry.x+geometry.day,y:geometry.y});await send('Input.dispatchMouseEvent',{type:'mouseReleased',button:'left',buttons:0,clickCount:1,x:geometry.x+geometry.day,y:geometry.y})
  const moved=await taskWait(t=>t.scheduled==='2026-10-20 09:30');assert.equal(moved.due,'2026-10-25');assert.equal(moved.durationMinutes,60)
  pass('Real Timeline pointer move preserves the clock, duration and independent deadline')
  await click('.task-timeline-label');await waitFor("Boolean(document.querySelector('[aria-label=\"Плановая дата связанной задачи\"]'))")
  await fill('[aria-label="Плановая дата связанной задачи"]','2026-10-21');await action('Сохранить даты');await taskWait(t=>t.scheduled==='2026-10-21')
  await waitFor("Boolean(document.querySelector('.task-timeline-interval.date-only'))");assert.equal(await evaluate("Boolean(document.querySelector('.task-timeline-resize'))"),false)
  pass('Date-only scheduling stays date-only and does not expose an invented midnight resize interval')
  await click('.tree-row[title="Board.canvas"]');await waitFor("Boolean(document.querySelector('[data-node-id=doc] .canvas-linked-peek'))");await click('[data-node-id=doc] .canvas-linked-peek');await waitFor("Boolean(document.querySelector('.linked-document-preview'))")
  assert.equal(await evaluate("document.querySelectorAll('.linked-document-preview img,.linked-document-preview a[href]').length"),0)
  await action('Markdown');await fill('[aria-label="Исходник связанного документа"]',note+'\nLocal draft.\n')
  await delay(250);const drafts=(await command('load_workspace',{root})).drafts;assert.ok(drafts.some(d=>d.path==='Document.md'&&d.pane==='linked-main'&&d.content.includes('Local draft.')))
  await action('Сохранить источник');await waitFor("document.querySelector('.linked-resource-dialog button.primary')?.disabled===true&&document.querySelector('.linked-resource-dialog button.primary')?.textContent==='Сохранить источник'")
  assert.equal(await readFile(join(scratch,'Document.md'),'utf8'),note+'\nLocal draft.\n');assert.equal(await readFile(join(scratch,'Board.canvas'),'utf8'),boardBytes)
  pass('Board Document peek edits the original through CAS, uses an isolated durable draft and never fetches linked media or changes Canvas bytes')
  await fill('[aria-label="Исходник связанного документа"]','Unsaved conflict draft')
  await writeFile(join(scratch,'Document.md'),'External replacement\n');await delay(600);await action('Сохранить источник');await waitFor("document.querySelector('.linked-resource-dialog')?.textContent.includes('Источник изменился извне')")
  assert.equal(await readFile(join(scratch,'Document.md'),'utf8'),'External replacement\n');assert.equal(await evaluate("document.querySelector('[aria-label=\"Исходник связанного документа\"]').value"),'Unsaved conflict draft')
  pass('External replacement keeps the draft and refuses a stale overwrite')
  await evaluate('window.confirm=()=>true');await action('Отбросить ввод…');await action('Закрыть')
  await click('[data-node-id=formats] .canvas-linked-peek');await waitFor("Boolean(document.querySelector('.linked-resource-dialog .task-format-view'))")
  await select('.linked-resource-dialog [aria-label="Связанный формат задач"]','table');await select('.linked-resource-dialog .task-format-table select','todo');await taskWait(t=>t.status==='todo');assert.equal(await readFile(join(scratch,'Tasks.base'),'utf8'),definition)
  pass('The board-linked Table mutates the very same canonical task previously edited in standalone Kanban/Timeline')
  await action('Закрыть');await key('p','KeyP',2);await waitFor("Boolean(document.querySelector('.palette-input input'))");await fill('.palette-input input','Создать связанные форматы задач');await click('.palette-results button');await waitFor("document.querySelector('form.dialog')?.textContent.includes('Связанные форматы задач')");await fill('form.dialog input','Created formats');await fill('[aria-label="Проект связанных форматов"]','QA');await select('[aria-label="Первый связанный формат"]','timeline');await click('form.dialog button[type=submit]');await waitFor("document.querySelector('.task-format-view')?.dataset.taskFormat==='timeline'")
  const created=JSON.parse(await readFile(join(scratch,'Created formats.base'),'utf8'));assert.equal(created.brandon.filter.project,'QA');assert.equal(created.views[0].type,'brandon-task-timeline');assert.equal((await command('metadata_delta',{root,generation:null,after:null})).tasks.filter(t=>t.managed).length,1)
  pass('The actual palette/creation dialog creates a scoped linked definition with the chosen first layout and no duplicate tasks')
  const future=JSON.parse(definition);future.brandon.version=999;await writeFile(join(scratch,'Tasks.base'),JSON.stringify(future));await click('.tree-row[title="Tasks.base"]');await waitFor("document.querySelector('.working-base')?.textContent.includes('Неизвестный формат')");assert.equal(await evaluate("document.querySelectorAll('.working-base [data-task-id]').length"),0)
  pass('Future known schema versions fail closed instead of being executed as a note table')
  console.log(JSON.stringify({passed:passes.length,scratch,note:'No app launcher, installer or physical stylus check is performed by this suite.'}))
}finally{
  await command('open_vault',{path:original.root});await send('Page.reload');await waitFor("Boolean(document.querySelector('.metadata-status'))");await c.close()
}
