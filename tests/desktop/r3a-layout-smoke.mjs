// Real Windows/Tauri document IO and both native GPU / secondary DOM surfaces.
import assert from 'node:assert/strict'
import {mkdtemp,writeFile,readFile} from 'node:fs/promises'
import {tmpdir} from 'node:os'
import {join} from 'node:path'
import {connect} from './cdp-test-client.mjs'
const c=await connect(),{evaluate,send,click,key,waitFor,delay}=c
const command=(name,args={})=>evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)}).catch(error=>{throw Error(String(error))})`)
await waitFor("Boolean(document.querySelector('.metadata-status'))")
const original=await command('restore_vault');assert.ok(original?.root)
const settingsBefore=await evaluate("localStorage.getItem('brandon.settings.v1')")
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i,.side-save:not(:disabled),.workspace-dialog,.source-recovery,.conflict-dialog'))"),false)
const scratch=await mkdtemp(join(tmpdir(),'brandon-r3a-layout-')),passes=[],pass=message=>{passes.push(message);console.log('PASS '+message)}
const fixture={nodes:[{id:'a',type:'text',x:-210,y:-140,width:140,height:100,text:'Alpha',foreign:{keep:1}},{id:'b',type:'text',x:-30,y:10,width:140,height:100,text:'Beta',foreign:{keep:2}},{id:'c',type:'file',x:150,y:70,width:140,height:100,file:'Note.md'}],edges:[{id:'ab',fromNode:'a',toNode:'b',foreign:{keep:3}}],foreign:{keep:4}}
const read=async file=>JSON.parse(await readFile(join(scratch,file),'utf8'))
const point=async(selector,dx=0,dy=0)=>evaluate(`(()=>{const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect();return {x:r.left+r.width/2+${dx},y:r.top+r.height/2+${dy}}})()`)
const mouse=async(type,p,extra={})=>send('Input.dispatchMouseEvent',{type,...p,...extra})
const press=async(selector,modifiers=0)=>{const p=await point(selector);await mouse('mouseMoved',p);await mouse('mousePressed',p,{button:'left',buttons:1,clickCount:1,modifiers});await mouse('mouseReleased',p,{button:'left',buttons:0,clickCount:1,modifiers})}
const focus=async pane=>evaluate(`document.querySelector(${JSON.stringify(pane+' .canvas-view')}).focus()`)
const selection=async(pane,...ids)=>{await focus(pane);await key('Escape','Escape');for(let i=0;i<ids.length;i++){const selector=`${pane} [data-node-id=${JSON.stringify(ids[i])}]`,group=await evaluate(`document.querySelector(${JSON.stringify(selector)}).classList.contains('type-group')`);await press(selector+(group?' .canvas-group-label':''),i?2:0)}}
const settled=async()=>{await key('s','KeyS',2);await waitFor("!document.querySelector('.title-tab.active i,.side-save:not(:disabled)')");await delay(100)}
const action=async(pane,label)=>{await click(`${pane} [aria-label="Упорядочить объекты"]`);await waitFor("Boolean(document.querySelector('.command-menu'))");await evaluate(`{const button=Array.from(document.querySelectorAll('.command-menu button')).find(b=>b.textContent.includes(${JSON.stringify(label)}));if(!button||button.disabled)throw Error('Action missing/disabled: '+${JSON.stringify(label)});button.click()}`)}
const palette=async label=>{await key('p','KeyP',2);await waitFor("document.activeElement===document.querySelector('.palette input')");await send('Input.insertText',{text:label});await waitFor(`document.querySelectorAll('.palette-results button').length===1&&document.querySelector('.palette-results button.selected')?.textContent.includes(${JSON.stringify(label)})`);await key('Enter','Enter');await waitFor("!document.querySelector('.palette')")}
try{
  for(const file of ['Native.canvas','Fallback.canvas'])await writeFile(join(scratch,file),JSON.stringify(fixture))
  await writeFile(join(scratch,'Note.md'),'# Note\n\nCanonical linked note.\n')
  await command('open_vault',{path:scratch});await send('Page.reload');await waitFor("Boolean(document.querySelector('.tree-row[title=\"Native.canvas\"]'))")
  await click('.tree-row[title="Native.canvas"]');await waitFor("document.querySelector('.canvas-view')?.dataset.renderer==='wgpu'")
  const main='.content-shell[data-pane="main"]'
  for(const [pane,file,kind]of [[main,'Native.canvas','native'],['.secondary-pane','Fallback.canvas','fallback']]){
    if(kind==='fallback'){
      await click('[aria-label="Разделить рабочее пространство вправо"]');await waitFor("Boolean(document.querySelector('.secondary-pane .canvas-view'))")
      await focus('.secondary-pane');await click('.tree-row[title="Fallback.canvas"]');await waitFor("document.querySelector('.secondary-pane .pane-toolbar > span')?.textContent==='Fallback.canvas'")
      assert.equal(await evaluate("document.querySelector('.secondary-pane .canvas-view').dataset.renderer"),'dom')
    }
    await selection(pane,'a','b');await key('G','KeyG',2|8);await settled()
    let board=await read(file),group=board.nodes.find(n=>n.type==='group');assert.ok(group);assert.equal(board.nodes.length,4);assert.deepEqual(board.foreign,{keep:4});assert.equal(board.nodes[0].id,group.id)
    const contextPoint=await point(`${pane} [data-node-id=${JSON.stringify(group.id)}] .canvas-group-label`);await mouse('mousePressed',contextPoint,{button:'right',buttons:2,clickCount:1});await mouse('mouseReleased',contextPoint,{button:'right',buttons:0,clickCount:1});await waitFor("Boolean(document.querySelector('.command-menu'))");assert.equal(await evaluate("Array.from(document.querySelectorAll('.command-menu button')).find(b=>b.textContent.trim()==='Открыть').disabled"),true);await key('Escape','Escape')
    pass(`${kind}: physical Ctrl+Shift+G creates one standard group behind selected members without losing foreign fields`)
    await focus(pane);await key('z','KeyZ',2);await settled();assert.equal((await read(file)).nodes.length,3)
    await key('Z','KeyZ',2|8);await settled();assert.equal((await read(file)).nodes.length,4)
    pass(`${kind}: one Undo/Redo reverses and restores the whole grouping, with no duplicate command dispatch`)
    await selection(pane,group.id)
    const grip=`${pane} [data-node-id=${JSON.stringify(group.id)}] .canvas-group-label`,p=await point(grip),delta={x:40,y:24}
    await mouse('mousePressed',p,{button:'left',buttons:1,clickCount:1});await mouse('mouseMoved',{x:p.x+delta.x,y:p.y+delta.y},{button:'left',buttons:1});await mouse('mouseReleased',{x:p.x+delta.x,y:p.y+delta.y},{button:'left',buttons:0,clickCount:1});await settled()
    board=await read(file);for(const id of ['a','b']){assert.equal(board.nodes.find(n=>n.id===id).x,fixture.nodes.find(n=>n.id===id).x+40);assert.equal(board.nodes.find(n=>n.id===id).y,fixture.nodes.find(n=>n.id===id).y+24)}
    assert.equal(board.nodes.find(n=>n.id==='c').x,fixture.nodes[2].x)
    pass(`${kind}: a real group drag translates enclosed members exactly once and leaves the unrelated note unchanged`)
    await focus(pane);await key('c','KeyC',2);await key('v','KeyV',2);await settled();board=await read(file);assert.equal(board.nodes.length,7);assert.equal(board.edges.length,2)
    const cloned=board.nodes.filter(n=>!fixture.nodes.some(old=>old.id===n.id)&&n.id!==group.id);assert.equal(cloned.length,3);assert.ok(board.edges.some(e=>cloned.some(n=>n.id===e.fromNode)&&cloned.some(n=>n.id===e.toNode)))
    await focus(pane);await palette('Отменить');await focus(pane);await settled();assert.equal((await read(file)).nodes.length,4);await selection(pane,group.id)
    pass(`${kind}: Ctrl+C/V copies a complete geometric group and remaps its internal edge; global palette Undo removes only the copy`)
    await action(pane,'Заблокировать');await settled();const locked=await readFile(join(scratch,file),'utf8');assert.equal((await read(file)).nodes.find(n=>n.id===group.id).brandon.locked,true)
    await focus(pane);await key('Delete','Delete');await settled();assert.equal(await readFile(join(scratch,file),'utf8'),locked)
    await selection(pane,'a');assert.equal(await evaluate(`document.querySelector(${JSON.stringify(pane+' [data-node-id="a"]')}).classList.contains('is-locked')`),true)
    await key('Delete','Delete');await settled();assert.equal(await readFile(join(scratch,file),'utf8'),locked)
    const child=await point(`${pane} [data-node-id="a"]`);await mouse('mousePressed',child,{button:'left',buttons:1,clickCount:1});await mouse('mouseMoved',{x:child.x+30,y:child.y+20},{button:'left',buttons:1});await mouse('mouseReleased',{x:child.x+30,y:child.y+20},{button:'left',buttons:0,clickCount:1});await settled();assert.equal(await readFile(join(scratch,file),'utf8'),locked)
    pass(`${kind}: locked group and descendant reject Delete and drag without changing any source byte`)
    await selection(pane,group.id);await action(pane,'Разблокировать');await action(pane,'Разгруппировать');await settled();board=await read(file);assert.equal(board.nodes.length,3);assert.equal(board.edges.length,1)
    pass(`${kind}: unlocking and ungrouping retain children, linked note and its edge`)
    await selection(pane,'a','b','c');await action(pane,'Выровнять по левому краю');await settled();board=await read(file);assert.equal(new Set(board.nodes.map(n=>n.x)).size,1)
    await focus(pane);await key('z','KeyZ',2);await settled();await selection(pane,'a','b','c')
    await palette('Холст: Разложить сеткой');await focus(pane);await settled();board=await read(file)
    assert.equal(board.nodes.find(n=>n.id==='a').x,board.nodes.find(n=>n.id==='c').x);assert.ok(board.nodes.find(n=>n.id==='b').x>board.nodes.find(n=>n.id==='a').x)
    pass(`${kind}: toolbar alignment and app palette grid command target the same active board, each with one undo effect`)
    await selection(pane,'a');await action(pane,'На передний план');await settled();assert.equal((await read(file)).nodes.at(-1).id,'a')
    await focus(pane);await key('z','KeyZ',2);await settled();assert.equal((await read(file)).nodes[0].id,'a')
    pass(`${kind}: z-order follows standard array order and one Undo preserves source data`)
    await click('[aria-label="Настройки"]');await waitFor("Boolean(document.querySelector('.settings-dialog'))");await evaluate("Array.from(document.querySelectorAll('.settings-layout nav button')).find(b=>b.textContent==='Горячие клавиши').click()")
    await click('[aria-label="Назначить: Холст: Сгруппировать"]');await waitFor("document.activeElement?.classList.contains('hotkey-capture')");await key('Г','KeyU',2|8);await waitFor("document.querySelector('[aria-label=\"Назначить: Холст: Сгруппировать\"]')?.textContent.includes('U')");await click('[aria-label="Закрыть настройки"]')
    await selection(pane,'a','b');await key('G','KeyG',2|8);await settled();assert.equal((await read(file)).nodes.length,3)
    await key('Г','KeyU',2|8);await settled();assert.equal((await read(file)).nodes.length,4);await focus(pane);await key('z','KeyZ',2);await settled();assert.equal((await read(file)).nodes.length,3)
    await click('[aria-label="Настройки"]');await waitFor("Boolean(document.querySelector('.settings-dialog'))");await evaluate("Array.from(document.querySelectorAll('.settings-layout nav button')).find(b=>b.textContent==='Горячие клавиши').click()");await click('[aria-label="По умолчанию: Холст: Сгруппировать"]');await click('[aria-label="Закрыть настройки"]')
    pass(`${kind}: a custom RU-layout shortcut replaces the default in both app and local board dispatch without double execution`)
  }
  await click('[aria-label="Закрыть вторую панель"]');await waitFor("!document.querySelector('.secondary-pane')")
  await click('.tree-row[title="Note.md"]');await click('.tree-row[title="Native.canvas"]');await waitFor("document.querySelector('.canvas-view')?.dataset.renderer==='wgpu'")
  assert.equal((await read('Native.canvas')).nodes.length,3);assert.equal(await readFile(join(scratch,'Note.md'),'utf8'),'# Note\n\nCanonical linked note.\n')
  pass('Saved arrangement reopens on the GPU board without rewriting the linked Markdown source')
  const roundtrip=await readFile(new URL('./fixtures/r3a-obsidian-1.14.4.canvas',import.meta.url),'utf8');await writeFile(join(scratch,'Roundtrip.canvas'),roundtrip);await waitFor("Boolean(document.querySelector('.tree-row[title=\"Roundtrip.canvas\"]'))");await click('.tree-row[title="Roundtrip.canvas"]');await waitFor("Boolean(document.querySelector('[data-node-id=alpha].is-locked'))");assert.ok(await evaluate("document.querySelector('.canvas-compatibility')?.textContent.includes('Расширенный')"));await selection(main,'alpha');await key('Delete','Delete');await settled();assert.equal(await readFile(join(scratch,'Roundtrip.canvas'),'utf8'),roundtrip)
  pass('The actual Obsidian-save snapshot reopens in Brandon, advertises extended compatibility and re-enforces its preserved node lock')
  const gestureSource=JSON.stringify(fixture);await writeFile(join(scratch,'Gestures.canvas'),gestureSource);await waitFor("Boolean(document.querySelector('.tree-row[title=\"Gestures.canvas\"]'))");await click('.tree-row[title="Gestures.canvas"]');await waitFor("Boolean(document.querySelector('[data-node-id=a]'))")
  let gesturePoint=await point(`${main} [data-node-id=a]`)
  await mouse('mousePressed',gesturePoint,{button:'left',buttons:1,clickCount:1});await mouse('mouseMoved',{x:gesturePoint.x+30,y:gesturePoint.y+20},{button:'left',buttons:1});await evaluate("document.querySelector('.canvas-view').dispatchEvent(new PointerEvent('pointercancel',{bubbles:true,pointerId:1}))");await mouse('mouseReleased',{x:gesturePoint.x+30,y:gesturePoint.y+20},{button:'left',buttons:0,clickCount:1});await settled();assert.equal(await readFile(join(scratch,'Gestures.canvas'),'utf8'),gestureSource)
  pass('Pointer cancellation rolls a transient gesture back without publishing a source edit or undo item')
  gesturePoint=await point(`${main} [data-node-id=a]`);await mouse('mousePressed',gesturePoint,{button:'left',buttons:1,clickCount:1});await mouse('mouseMoved',{x:gesturePoint.x+30,y:gesturePoint.y+20},{button:'left',buttons:1})
  const external=JSON.stringify({...fixture,nodes:fixture.nodes.map(n=>n.id==='a'?{...n,text:'EXTERNAL DURING GESTURE'}:n),newForeign:{keep:true}});await writeFile(join(scratch,'Gestures.canvas'),external);await waitFor("document.querySelector('[data-node-id=a]')?.textContent.includes('EXTERNAL DURING GESTURE')");await mouse('mouseReleased',{x:gesturePoint.x+30,y:gesturePoint.y+20},{button:'left',buttons:0,clickCount:1});await settled();assert.equal(await readFile(join(scratch,'Gestures.canvas'),'utf8'),external)
  pass('An external source replacement cancels an in-progress gesture; releasing the old pointer cannot overwrite the new disk revision')
  const future=JSON.stringify({...fixture,nodes:[{...fixture.nodes[0],brandon:{version:999,keep:true}}]});await writeFile(join(scratch,'Future.canvas'),future);await waitFor("Boolean(document.querySelector('.tree-row[title=\"Future.canvas\"]'))");await click('.tree-row[title="Future.canvas"]');await waitFor("Boolean(document.querySelector('.canvas-invalid'))");await key('s','KeyS',2);assert.equal(await readFile(join(scratch,'Future.canvas'),'utf8'),future)
  pass('An unknown extension version is visibly read-only and cannot be laundered into an empty writable board')
  console.log(JSON.stringify({passed:passes.length,fixture:scratch,scope:'Packaged Windows native GPU + secondary DOM; real keyboard/pointer, filesystem, autosave and palette'}))
}catch(error){console.error(await evaluate("({text:document.querySelector('.canvas-operation-error')?.textContent,menu:document.querySelector('.command-menu')?.textContent,alerts:Array.from(document.querySelectorAll('[role=alert]')).map(x=>x.textContent)})").catch(()=>null));throw error}finally{await evaluate(`{const saved=${JSON.stringify(settingsBefore)};if(saved===null)localStorage.removeItem('brandon.settings.v1');else localStorage.setItem('brandon.settings.v1',saved)}`);await command('open_vault',{path:original.root});await send('Page.reload');await waitFor("Boolean(document.querySelector('.metadata-status'))");c.socket.close()}
