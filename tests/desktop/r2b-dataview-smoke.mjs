// Real native renderer and Rust/watcher metadata; no plugin runtime or writes
// through query results. All files are synthetic and the original root is restored.
import assert from 'node:assert/strict'
import {mkdtemp,mkdir,writeFile,readFile} from 'node:fs/promises'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {connect} from './cdp-test-client.mjs'
const c=await connect(),{evaluate,send,click,key,waitFor,delay}=c
const command=(name,args={})=>evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)})`)
await waitFor("Boolean(document.querySelector('.metadata-status'))")
const original=await command('restore_vault');assert.ok(original?.root)
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i,.side-save:not(:disabled),.conflict-dialog,.source-recovery,.workspace-dialog'))"),false,'Preserve user drafts')
const scratch=await mkdtemp(join(tmpdir(),'brandon-r2b-dql-')),passes=[],pass=message=>{passes.push(message);console.log('PASS '+message)}
const block=source=>'```dataview\n'+source+'\n```\n'
const dashboard='# Dashboard\n\n'+block('TABLE status AS "State", rank\nFROM "Projects"\nWHERE status = "open"\nSORT rank DESC\nLIMIT 2')+'\n'+block('LIST rank\nFROM #study\nWHERE status = "open"')+'\n'+block('TASK\nFROM "Projects"\nWHERE !completed')+'\n'+block('TABLE rank\nGROUP BY status')+'\n```dataviewjs\nglobalThis.__dqlExecuted = true; fetch("https://invalid.example");\n```\n'
const alpha='---\nstatus: open\nrank: 20\ntags: [study/sub]\n---\n# Alpha\n\n- [ ] Source task ^source-task\n'
const file=async path=>{await key('o','KeyO',2);await waitFor("Boolean(document.querySelector('.palette[aria-label=\"Быстрый переход\"] input'))");await send('Input.insertText',{text:path});await waitFor("document.querySelectorAll('.palette-results button').length===1");await key('Enter','Enter');await waitFor(`document.querySelector('.title-tab.active')?.title===${JSON.stringify(path.split('/').at(-1).replace(/\.md$/i,''))}`)}
const reading=async()=>{if(!await evaluate("Boolean(document.querySelector('.preview-pane'))"))await evaluate("Array.from(document.querySelectorAll('.view-switcher button')).find(button=>button.textContent==='Чтение').click()");await waitFor("Boolean(document.querySelector('.preview-pane'))")}
let root
try{
  await mkdir(join(scratch,'Projects'));await mkdir(join(scratch,'Scale'))
  await writeFile(join(scratch,'Projects/Alpha.md'),alpha);await writeFile(join(scratch,'Projects/Beta.md'),'---\nstatus: open\nrank: 2\ntags: [study]\n---\n# Beta\n\n- [x] Done source task ^source-task\n');await writeFile(join(scratch,'Outside.md'),'---\nstatus: open\nrank: 99\n---\n# Outside\n')
  await writeFile(join(scratch,'Dashboard.md'),dashboard);await writeFile(join(scratch,'Scale.md'),'# Scale\n\n'+block('TABLE rank\nFROM "Scale"\nSORT rank ASC'))
  await Promise.all(Array.from({length:450},(_,i)=>writeFile(join(scratch,'Scale',`${String(i).padStart(4,'0')}.md`),`---\nrank: ${i}\n---\n# Row ${i}\n`)))
  root=(await command('open_vault',{path:scratch})).root;await send('Page.reload');await waitFor("document.querySelector('.metadata-status')?.dataset.pending==='0'")
  await evaluate("window.__dqlExecuted=false;window.__dqlFullIndex=[];window.__dqlFetch=window.fetch;window.fetch=(input,options)=>{const url=String(typeof input==='string'?input:input.url);if(url.includes('/get_vault_index')||url.includes('/refresh_vault'))window.__dqlFullIndex.push(url);return window.__dqlFetch(input,options)}")
  await file('Dashboard.md');await reading();await waitFor("document.querySelector('[data-dql-kind=TABLE] .dql-scroll')?.dataset.totalRows==='2'")
  assert.deepEqual(await evaluate("Array.from(document.querySelectorAll('[data-dql-kind=TABLE] th')).map(x=>x.textContent)"),['File','State','rank'])
  assert.deepEqual(await evaluate("Array.from(document.querySelectorAll('[data-dql-kind=TABLE] tr[data-dql-path]')).map(x=>x.dataset.dqlPath)"),['Projects/Alpha.md','Projects/Beta.md'])
  pass('A real dataview fenced block renders TABLE with folder filter, aliases, numeric sort and limit')
  assert.equal(await evaluate("document.querySelector('[data-dql-kind=LIST] .dql-scroll').dataset.totalRows"),'2')
  assert.equal(await evaluate("document.querySelector('[data-dql-kind=TASK] .dql-scroll').dataset.totalRows"),'1')
  assert.equal(await evaluate("document.querySelector('[data-dql-kind=TASK] input').disabled"),true)
  assert.equal(await evaluate("document.querySelector('[data-dql-kind=TASK] input').checked"),false)
  pass('LIST uses nested tag membership; TASK shows only source Markdown checkboxes as read-only')
  assert.ok(await evaluate("Array.from(document.querySelectorAll('.dql-error')).some(x=>x.textContent.includes('GROUP'))"))
  assert.ok(await evaluate("Array.from(document.querySelectorAll('.dql-error')).some(x=>x.textContent.includes('DataviewJS не исполняется'))"));assert.equal(await evaluate('window.__dqlExecuted'),false)
  assert.equal(await readFile(join(scratch,'Dashboard.md'),'utf8'),dashboard)
  pass('Unsupported DQL and DataviewJS are visible errors with source retained; no JavaScript executes')
  await click('[data-dql-kind=TABLE] tr[data-dql-path="Projects/Alpha.md"] button');await waitFor("document.querySelector('.title-tab.active')?.title==='Alpha'")
  await file('Dashboard.md');await reading();const before=await command('metadata_status',{root})
  await writeFile(join(scratch,'Projects/Alpha.md'),alpha.replace('rank: 20','rank: 1'));await waitFor("document.querySelector('[data-dql-kind=TABLE] tr[data-dql-path]')?.dataset.dqlPath==='Projects/Beta.md'")
  const after=await command('metadata_status',{root});assert.equal(after.stats.parsedSources-before.stats.parsedSources,1);assert.equal(after.stats.fullScans,before.stats.fullScans);assert.deepEqual(await evaluate('window.__dqlFullIndex'),[])
  pass('Source links navigate and one external edit updates the query through one parse, without a full index rebuild')
  await file('Scale.md');await reading();await waitFor("document.querySelector('.dql-scroll')?.dataset.totalRows==='450'")
  assert.ok(await evaluate("document.querySelectorAll('.dql-table tr[data-dql-path]').length<100"))
  const geometry=await evaluate("({body:document.querySelector('.dql-table tbody').getBoundingClientRect().height,row:document.querySelector('.dql-table tr[data-dql-path]').getBoundingClientRect().height})")
  assert.equal(geometry.row,44);assert.ok(Math.abs(geometry.body-450*44)<2,JSON.stringify(geometry))
  await evaluate("document.querySelector('.dql-scroll').scrollTop=document.querySelector('.dql-scroll').scrollHeight");await waitFor("Boolean(document.querySelector('tr[data-dql-path=\"Scale/0449.md\"]'))")
  assert.equal(await readFile(join(scratch,'Projects/Beta.md'),'utf8'),'---\nstatus: open\nrank: 2\ntags: [study]\n---\n# Beta\n\n- [x] Done source task ^source-task\n')
  pass('Large DQL results virtualize at exact 44px geometry and keep the final source reachable')
  await file('Dashboard.md');await reading();await click('[data-dql-kind=TASK] li button');await waitFor("document.querySelector('.title-tab.active')?.title==='Alpha'");await reading();await waitFor("Array.from(document.querySelectorAll('.note-anchor-highlight')).some(x=>x.textContent.includes('Source task'))")
  pass('Source TASK navigation retains its block anchor and highlights the original checkbox')
  await file('Dashboard.md');await reading();await click('button[aria-label="Разделить рабочее пространство вправо"]');await waitFor("Boolean(document.querySelector('.secondary-pane .pane-toolbar'))");await evaluate("Array.from(document.querySelectorAll('.secondary-pane .pane-toolbar button')).find(button=>button.textContent==='Чтение').click()");await waitFor("document.querySelector('.secondary-pane [data-dql-kind=TABLE] .dql-scroll')?.dataset.totalRows==='2'")
  await click('.secondary-pane [data-dql-kind=TABLE] tr[data-dql-path="Projects/Beta.md"] button');await waitFor("document.querySelector('.secondary-pane .pane-toolbar > span')?.textContent==='Projects/Beta.md'");assert.equal(await evaluate("document.querySelector('.title-tab.active')?.title"),'Dashboard');await click('[aria-label="Закрыть вторую панель"]');await waitFor("!document.querySelector('.secondary-pane')")
  pass('The shared DQL renderer works in a split pane and its source link stays in that pane')
  console.log(JSON.stringify({passed:passes.length,fixture:scratch,geometry,picker:'not used',scope:'Real Windows/Tauri, native metadata/watcher, read-only DQL subset'}))
}catch(error){console.error(await evaluate("({active:document.querySelector('.title-tab.active')?.title,preview:document.querySelector('.preview-pane')?.textContent.slice(0,2500),alerts:Array.from(document.querySelectorAll('[role=alert]')).map(x=>x.textContent)})").catch(()=>null));throw error}finally{
  await evaluate('if(window.__dqlFetch)window.fetch=window.__dqlFetch').catch(()=>{})
  await command('open_vault',{path:original.root});await send('Page.reload');await delay(200);c.socket.close()
}
