// Real native app + Rust property writer + watcher + portable Base files.
import assert from 'node:assert/strict'
import {mkdtemp,mkdir,writeFile,readFile} from 'node:fs/promises'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {connect} from './cdp-test-client.mjs'
const c=await connect(),{evaluate,send,click,key,waitFor,delay}=c
const command=(name,args={})=>evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)})`)
const original=await command('restore_vault'),settings=await evaluate("localStorage.getItem('brandon.settings.v1')")
assert.ok(original?.root)
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i,.side-save:not(:disabled),.conflict-dialog,.source-recovery,.workspace-dialog,.property-dialog'))"),false,'Never run over user drafts')
const scratch=await mkdtemp(join(tmpdir(),'brandon-r2b-')),passes=[],pass=message=>{passes.push(message);console.log('PASS '+message)}
let root
const input=async(selector,value)=>{await evaluate(`document.querySelector(${JSON.stringify(selector)}).focus()`);await key('a','KeyA',2);await send('Input.insertText',{text:value})}
const file=async(path)=>{await click('button[aria-label="Файлы"]');await click(`.tree-row[title=${JSON.stringify(path)}]`)}
const base='filters: file.inFolder("Projects")\nproperties:\n  note.rank:\n    displayName: Рейтинг\nfuture: {keep: true}\nviews:\n  - type: table\n    name: Работа\n    order: [file.name, note.status, note.rank]\n    sort: [{property: note.rank, direction: DESC}]\n    limit: 1\n    filters: status != "done"\n  - type: table\n    name: Все\n    order: [file.name, note.status]\n'
const source='\uFEFF---\r\n# preserve before\r\nstatus: todo # preserve inline\r\nrank: 20\r\nnested:\r\n  object: {keep: true}\r\n---\r\n# Unique heading\r\n\r\n## Target\r\n\r\nBody\r\n'
try{
  await mkdir(join(scratch,'Projects'))
  await writeFile(join(scratch,'Projects','Alpha.md'),source)
  await writeFile(join(scratch,'Projects','Beta.md'),'---\nstatus: todo\nrank: 2\n---\n# Beta heading\n')
  await writeFile(join(scratch,'Outside.md'),'# Outside\n')
  await writeFile(join(scratch,'View.base'),base)
  await writeFile(join(scratch,'Board.canvas'),JSON.stringify({nodes:[{id:'alpha',type:'file',file:'Projects/Alpha.md',x:0,y:0,width:380,height:260}],edges:[]}))
  const opened=await command('open_vault',{path:scratch});root=opened.root
  await evaluate("localStorage.setItem('brandon.settings.v1',JSON.stringify({...JSON.parse(localStorage.getItem('brandon.settings.v1')??'{}'),version:1,propertiesMode:'visible',editorMode:'visual'}))")
  await send('Page.reload');await waitFor("document.querySelector('.metadata-status')?.dataset.pending==='0'")
  await file('View.base');await waitFor("document.querySelector('.base-table-wrap')?.dataset.totalRows==='1'")
  assert.deepEqual(await evaluate("Array.from(document.querySelectorAll('.base-table th')).map(x=>x.textContent.replace(/[↓↑]/g,''))"),['Имя','note.status','Рейтинг'])
  assert.ok(await evaluate("document.querySelector('.base-table tr[data-path=\"Projects/Alpha.md\"]')?.textContent.includes('20')"))
  assert.equal(await readFile(join(scratch,'View.base'),'utf8'),base)
  pass('Actual .base global/view filters, columns, labels, numeric sort and limit select the right note without rewriting unknown sections')
  await evaluate("{const select=document.querySelector('[aria-label=\"Представление базы\"]');select.value='1';select.dispatchEvent(new Event('change',{bubbles:true}))}")
  await waitFor("document.querySelector('.base-table-wrap')?.dataset.totalRows==='2'")
  assert.equal(await evaluate("Boolean(document.querySelector('.base-table tr[data-path=\"Outside.md\"]'))"),false)
  await evaluate("{const select=document.querySelector('[aria-label=\"Представление базы\"]');select.value='0';select.dispatchEvent(new Event('change',{bubbles:true}))}")
  await waitFor("document.querySelector('.base-table-wrap')?.dataset.totalRows==='1'")
  pass('Switching saved views applies local conditions while retaining the global filter')
  await click('.base-cell-edit[data-property="note.status"]');await waitFor("Boolean(document.querySelector('.property-dialog'))")
  await waitFor("document.activeElement?.getAttribute('aria-label')==='Значение свойства'")
  await input('.property-dialog [aria-label="Значение свойства"]','active');await click('.property-apply');await waitFor("!document.querySelector('.property-dialog')")
  await waitFor("document.querySelector('.base-cell-edit[data-property=\"note.status\"]')?.textContent==='active'")
  assert.equal(await readFile(join(scratch,'Projects','Alpha.md'),'utf8'),source.replace('status: todo','"status": "active"'))
  pass('Editing a table cell changes only that YAML field and propagates through common metadata')
  await click('.base-cell-edit[data-property="note.status"]');await waitFor("Boolean(document.querySelector('.property-dialog'))")
  const external=(await readFile(join(scratch,'Projects','Alpha.md'),'utf8')).replace('Body','EXTERNAL BODY')
  await writeFile(join(scratch,'Projects','Alpha.md'),external)
  await input('.property-dialog [aria-label="Значение свойства"]','must-not-win');await click('.property-apply')
  await waitFor("document.querySelector('.property-dialog [role=alert]')?.textContent.includes('изменён извне')")
  assert.equal(await readFile(join(scratch,'Projects','Alpha.md'),'utf8'),external)
  await evaluate("Array.from(document.querySelectorAll('.property-dialog button')).find(button=>button.textContent==='Отмена').click()")
  pass('A stale cell editor reports an external conflict and leaves newer source bytes intact')
  const document=await command('read_note',{relativePath:'Projects/Alpha.md'})
  const rootError=await evaluate(`window.__TAURI_INTERNALS__.invoke('patch_note_property',${JSON.stringify({root:root+'-wrong',relativePath:document.path,expectedFingerprint:document.fingerprint,patch:{key:'status',kind:'text',value:'wrong',remove:false}})}).then(()=>'',error=>String(error))`)
  assert.match(rootError,/Хранилище сменилось/)
  pass('Property writes are scoped to both the vault root and the inspected fingerprint')
  await file('Projects/Alpha.md');await waitFor("Array.from(document.querySelectorAll('.document .property-field')).some(button=>button.textContent.includes('status'))")
  assert.equal(await evaluate("document.querySelector('.document .cm-content')?.textContent.includes('preserve before')"),false)
  assert.ok(await evaluate("Array.from(document.querySelectorAll('.document .property-field')).some(button=>button.disabled&&button.textContent.includes('nested'))"))
  await evaluate("Array.from(document.querySelectorAll('.document .property-field')).find(button=>button.textContent.includes('status')).click()")
  await input('.property-dialog [aria-label="Значение свойства"]','visible-edit');await click('.property-apply');await waitFor("!document.querySelector('.property-dialog')")
  await waitFor("Array.from(document.querySelectorAll('.document .property-field')).some(button=>button.textContent.includes('visible-edit'))")
  for(let i=0;i<60;i++){if((await readFile(join(scratch,'Projects','Alpha.md'),'utf8')).includes('visible-edit'))break;await delay(50)}
  assert.ok((await readFile(join(scratch,'Projects','Alpha.md'),'utf8')).includes('visible-edit'))
  pass('Visible typed properties hide raw YAML without losing body text, nested fields or autosave')
  const propertyMode=async mode=>{await click('button[aria-label="Настройки"]');await waitFor("Boolean(document.querySelector('[aria-label=\"Свойства в документе\"]'))");await evaluate(`{const select=document.querySelector('[aria-label="Свойства в документе"]');select.value=${JSON.stringify(mode)};select.dispatchEvent(new Event('change',{bubbles:true}))}`);await click('button[aria-label="Закрыть настройки"]')}
  await propertyMode('hidden');await waitFor("!document.querySelector('.document .note-properties')")
  assert.equal(await evaluate("document.querySelector('.document .cm-content')?.textContent.includes('preserve before')"),false)
  await propertyMode('source');await waitFor("document.querySelector('.document .cm-content')?.textContent.includes('preserve before')")
  await propertyMode('visible');await waitFor("Boolean(document.querySelector('.document .property-fields'))&& !document.querySelector('.document .cm-content')?.textContent.includes('preserve before')")
  pass('Visible/hidden/source preferences reconfigure the actual editor without deleting frontmatter or note text')
  await file('Board.canvas');await waitFor("document.querySelector('.canvas-file-properties')?.textContent.includes('visible-edit')")
  pass('The linked Canvas note card reflects the same property value')
  await file('View.base');await waitFor("Boolean(document.querySelector('.base-config-toggle'))");await click('.base-config-toggle');await input('.base-config textarea',base+'# saved intact\n');await click('.base-config-apply')
  await waitFor("!document.querySelector('.base-config')");await key('s','KeyS',2);await delay(150)
  assert.equal(await readFile(join(scratch,'View.base'),'utf8'),base+'# saved intact\n')
  await send('Page.reload');await waitFor("document.querySelector('.base-table-wrap')?.dataset.totalRows==='1'")
  pass('Editing and reopening the Base definition retains comments and unknown YAML sections')
  await click('.base-config-toggle');await input('.base-config textarea',base.replace('status != "done"','fetch("https://invalid.example")'));await click('.base-config-apply')
  await waitFor("document.querySelector('.base-view [role=alert]')?.textContent.includes('не поддерживается')")
  assert.equal(await evaluate("document.querySelector('.base-table-wrap')?.dataset.totalRows"),'0')
  pass('Unsupported expressions show an explicit error and fail closed, never execute JavaScript')
  await click('.base-config-toggle');await input('.base-config textarea',base);await click('.base-config-apply');await waitFor("!document.querySelector('.base-config')&&document.querySelector('.base-table-wrap')?.dataset.totalRows==='1'");await key('s','KeyS',2)
  for(let i=0;i<60;i++){if(await readFile(join(scratch,'View.base'),'utf8')===base)break;await delay(50)}
  assert.equal(await readFile(join(scratch,'View.base'),'utf8'),base)
  await waitFor("!document.querySelector('.title-tab.active i')");await delay(400)
  console.log(`PASS ${passes.length} R2B property/Base UI checks; fixture ${scratch}`)
}catch(error){console.error(await evaluate("({active:document.querySelector('.title-tab.active')?.textContent,fields:document.querySelector('.note-properties')?.textContent,alerts:Array.from(document.querySelectorAll('[role=alert]')).map(element=>element.textContent)})").catch(()=>null));throw error}finally{
  await command('open_vault',{path:original.root})
  await evaluate(settings===null?"localStorage.removeItem('brandon.settings.v1')":`localStorage.setItem('brandon.settings.v1',${JSON.stringify(settings)})`)
  await send('Page.reload');await waitFor('Boolean(document.querySelector(".metadata-status"))');c.socket.close()
}
