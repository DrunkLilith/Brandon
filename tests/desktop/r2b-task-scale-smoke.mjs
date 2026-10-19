import assert from 'node:assert/strict'
import {mkdtemp,mkdir,writeFile} from 'node:fs/promises'
import {resolve,join} from 'node:path'
import {DatabaseSync} from 'node:sqlite'
import {performance} from 'node:perf_hooks'
import {connect} from './cdp-test-client.mjs'
const c=await connect(),{evaluate,send,click,delay}=c
const command=(name,args={})=>evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)}).catch(error=>{throw new Error(String(error))})`)
async function until(expression,timeout=90000){const at=performance.now();while(performance.now()-at<timeout){if(await evaluate(expression))return;await delay(60)}throw new Error(`Task-scale timeout: ${expression}`)}
await until("Boolean(document.querySelector('.metadata-status'))")
const original=await command('restore_vault');assert.ok(original?.root);assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i,.side-save:not(:disabled),.conflict-dialog,.source-recovery,.workspace-dialog'))"),false)
const rootPath=await mkdtemp(join(resolve('target/qa'),'tasks-3000-')),count=3000
const passes=[],pass=message=>{passes.push(message);console.log('PASS '+message)}
try{
  await mkdir(join(rootPath,'Tasks'));await mkdir(join(rootPath,'.brandon'))
  const db=new DatabaseSync(join(rootPath,'.brandon/tasks.sqlite3'))
  db.exec("CREATE TABLE tasks(id TEXT PRIMARY KEY,note_path TEXT NOT NULL UNIQUE,title TEXT NOT NULL,completed INTEGER NOT NULL DEFAULT 0,scheduled TEXT,due TEXT,duration_minutes INTEGER,priority TEXT,created_unix_ms INTEGER NOT NULL,updated_unix_ms INTEGER NOT NULL,status TEXT NOT NULL DEFAULT 'todo',project TEXT,tags_json TEXT NOT NULL DEFAULT '[]');PRAGMA user_version=1;BEGIN;")
  const insert=db.prepare('INSERT INTO tasks VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?)'),now=Date.now()
  for(let i=0;i<count;i++){const id=`scale${String(i).padStart(5,'0')}`,status=['todo','doing','done'][i%3];insert.run(id,`Tasks/${id}.md`,`Task ${i}`,Number(status==='done'),null,null,30,null,now,now,status,i%2?'Home':'Study','[]')}
  db.exec('COMMIT');db.close();let next=0
  await Promise.all(Array.from({length:32},async()=>{while(next<count){const i=next++,id=`scale${String(i).padStart(5,'0')}`;await writeFile(join(rootPath,'Tasks',`${id}.md`),`# Task ${i}\n\nDescription ${i}.\n`)}}))
  const opened=await command('open_vault',{path:rootPath}),root=opened.root;await send('Page.reload')
  await until(`document.querySelector('.metadata-status')?.dataset.pending==='0'&&window.__TAURI_INTERNALS__.invoke('metadata_status',{root:${JSON.stringify(root)}}).then(delta=>delta.stats.documents===${count})`)
  await click('button[aria-label="Задачи и календарь"]');await until("Boolean(document.querySelector('.planner-view'))")
  await evaluate("Array.from(document.querySelectorAll('.planner-mode-switch button')).find(button=>button.textContent==='Список').click()")
  await until(`document.querySelector('.task-list-wrap')?.dataset.totalRows==='${count}'`)
  assert.ok(await evaluate("document.querySelectorAll('.task-list-table tr[data-task-id]').length<100"))
  const geometry=await evaluate("({body:document.querySelector('.task-list-table tbody').getBoundingClientRect().height,row:document.querySelector('.task-list-table tr[data-task-id]').getBoundingClientRect().height})")
  assert.equal(geometry.row,48);assert.ok(Math.abs(geometry.body-count*48)<2,JSON.stringify(geometry))
  await evaluate("document.querySelector('.task-list-wrap').scrollTop=document.querySelector('.task-list-wrap').scrollHeight")
  await until("Boolean(document.querySelector('.task-list-table [data-task-id=scale02999]'))")
  pass('Three thousand canonical tasks use a bounded List DOM, exact row geometry and a reachable final row')
  await evaluate("Array.from(document.querySelectorAll('.planner-mode-switch button')).find(button=>button.textContent==='Kanban').click()")
  await until("document.querySelector('.planner-view')?.dataset.mode==='kanban'")
  assert.ok(await evaluate("document.querySelectorAll('.planner-task-stack [data-task-id]').length<200"))
  const column=await evaluate("(()=>{const column=document.querySelector('[data-planner-drop=kanban-todo] .planner-task-list'),stack=column.querySelector('.planner-task-stack');return {height:stack.getBoundingClientRect().height,total:Number(stack.dataset.totalRows),row:stack.querySelector('.planner-task-slot').getBoundingClientRect().height}})()")
  assert.equal(column.total,1000);assert.equal(column.row,124);assert.ok(Math.abs(column.height-1000*124)<2)
  await evaluate("document.querySelector('[data-planner-drop=kanban-todo] .planner-task-list').scrollTop=9999999")
  await until("Boolean(document.querySelector('[data-planner-drop=kanban-todo] [data-task-id=scale02997]'))")
  pass('Kanban virtualizes each status column with exact height and a reachable final card')
  const before=await command('metadata_delta',{root,generation:null,after:null}),task=before.tasks.find(task=>task.id==='scale02997'),at=performance.now()
  const changed=await command('patch_managed_task',{root,taskId:task.id,expectedRevision:task.revision,patch:{status:'doing'}})
  assert.ok(JSON.stringify(changed).length<4096);assert.equal(Object.hasOwn(changed,'index'),false)
  await until("document.querySelector('[data-planner-drop=kanban-todo] .planner-task-stack')?.dataset.totalRows==='999'&&document.querySelector('[data-planner-drop=kanban-doing] .planner-task-stack')?.dataset.totalRows==='1001'")
  const elapsedMs=performance.now()-at,delta=await command('metadata_delta',{root,generation:before.generation,after:before.revision})
  assert.equal(delta.stats.parsedSources,before.stats.parsedSources);assert.equal(delta.stats.fullScans,before.stats.fullScans);assert.equal(delta.documents.length,0);assert.equal(delta.tasks.length,1);assert.equal(delta.tasks[0].id,task.id)
  pass('One status change returns a small command result and one task delta, with zero Markdown reparses/full scans')
  console.log(JSON.stringify({passed:passes.length,count,fixture:rootPath,listGeometry:geometry,kanbanGeometry:column,mutationToProjectionMs:elapsedMs,mutationWireBytes:JSON.stringify(changed).length,deltaTaskCount:delta.tasks.length,notes:'Synthetic Windows/Tauri run; includes CDP polling. Not a universal latency guarantee.'}))
}finally{await command('open_vault',{path:original.root});await send('Page.reload');await until("Boolean(document.querySelector('.metadata-status'))");c.socket.close()}
