// Real native IPC, synthetic vault only; preserve the prior vault and its files.
import assert from 'node:assert/strict'
import { mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { connect } from './cdp-test-client.mjs'

const { socket, send, evaluate, waitFor, click, key, delay } = await connect()
const command = (name, args = {}) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)})`)
const original = await command('restore_vault')
assert.ok(original?.root)
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i') || document.querySelector('.source-recovery') || document.querySelector('.conflict-dialog'))"), false)
const scratch = await mkdtemp(join(tmpdir(), 'brandon-search-recovery-'))
const privateDir = join(scratch, '.brandon'), cache = join(privateDir, 'search-v1.sqlite3')
const corrupt = 'SYNTHETIC_CORRUPT_DERIVATIVE_INDEX'
const hash = async path => createHash('sha256').update(await readFile(join(scratch, path))).digest('hex')
const passes = [], pass = message => { passes.push(message); console.log(`PASS ${message}`) }
const query = async text => {
  await evaluate('document.querySelector(\'input[aria-label="Поиск по vault"]\').focus()')
  await key('a', 'KeyA', 2)
  await send('Input.insertText', { text })
  await waitFor(`document.querySelector('.local-search .sidebar-content')?.dataset.completedQuery === ${JSON.stringify(text)} && document.querySelector('.local-search .sidebar-content')?.getAttribute('aria-busy') === 'false'`)
}
const chooseKind = kind => evaluate(`(() => { const select=document.querySelector('select[aria-label="Тип результатов"]'); select.value=${JSON.stringify(kind)}; select.dispatchEvent(new Event('change',{bubbles:true})); })()`)
const backupNames = async () => (await readdir(privateDir)).filter(name => name.startsWith('search-recovery-'))
try {
  await writeFile(join(scratch, 'Note.md'), '# Recovery\n\nrecoverneedle foo::bar(42)\n')
  await writeFile(join(scratch, 'Board.canvas'), JSON.stringify({ nodes: [{ id: 'recovery-card', type: 'text', x: 0, y: 0, width: 300, height: 150, text: 'alternatecanvas foo::bar(42)' }], edges: [] }))
  await writeFile(join(scratch, 'Media.pdf'), '%PDF-synthetic-name-only')
  await mkdir(privateDir)
  await writeFile(cache, corrupt)
  await command('open_vault', { path: scratch }); await send('Page.reload')
  await waitFor('Boolean(document.querySelector(\'.tree-row[title="Note.md"]\'))')
  await command('create_task', { title: 'Recovery task', scheduled: '2026-10-24' })
  const protectedPaths = ['Note.md', 'Board.canvas', 'Media.pdf', 'Tasks/Recovery task.md', '.brandon/tasks.sqlite3']
  const before = Object.fromEntries(await Promise.all(protectedPaths.map(async path => [path, await hash(path)])))
  await key('k', 'KeyK', 2)
  await waitFor('Boolean(document.querySelector(\'input[aria-label="Поиск по vault"]\'))')
  await click('.local-search-options button:nth-child(2)')
  await chooseKind('note')
  await query('foo::bar(42)')
  await waitFor("Boolean(document.querySelector('.local-search-recovery > button'))")
  assert.match(await evaluate("document.querySelector('.local-search-error').textContent"), /file is not a database/)
  assert.equal(await readFile(cache, 'utf8'), corrupt)
  assert.deepEqual(await backupNames(), [])
  pass('Corrupt cache has a visible recovery action and is not silently reset')
  if (process.env.BRANDON_QA_SCREENSHOT) {
    const screenshot = await send('Page.captureScreenshot', { format: 'png' })
    await writeFile(process.env.BRANDON_QA_SCREENSHOT.replace(/\.png$/, '-before.png'), Buffer.from(screenshot.data, 'base64'))
  }
  await click('.local-search-recovery > button')
  await waitFor("Boolean(document.querySelector('.local-search-recovery-message')) && document.querySelector('.local-search .sidebar-content')?.getAttribute('aria-busy') === 'false'")
  assert.equal(await evaluate("document.querySelectorAll('.search-results button').length"), 1)
  assert.equal(await evaluate("document.querySelector('.search-results button').dataset.path"), 'Note.md')
  assert.equal(await evaluate("document.querySelector('.local-search-options button:nth-child(2)').getAttribute('aria-pressed')"), 'true')
  assert.equal(await evaluate('document.querySelector(\'select[aria-label="Тип результатов"]\').value'), 'note')
  const firstBackup = (await backupNames())[0]
  assert.equal(await readFile(join(privateDir, firstBackup, 'search-v1.sqlite3'), 'utf8'), corrupt)
  for (const path of protectedPaths) assert.equal(await hash(path), before[path], path)
  pass('Explicit recovery preserves old cache bytes, original files and canonical tasks; Literal and type filters survive')

  await send('Page.reload'); await waitFor('Boolean(document.querySelector(\'.tree-row[title="Note.md"]\'))')
  await key('k', 'KeyK', 2); await waitFor('Boolean(document.querySelector(\'input[aria-label="Поиск по vault"]\'))')
  await query('recoverneedle')
  assert.equal(await evaluate("document.querySelector('.search-results button').dataset.path"), 'Note.md')
  pass('The repaired on-disk index remains usable after a WebView reload')

  await writeFile(cache, corrupt)
  await query('recoverneedle path:Note')
  await waitFor("Boolean(document.querySelector('.local-search-recovery > button'))")
  await evaluate(`(async()=>{ window.__cacheEvents=[];window.__cacheEventId=await window.__TAURI_INTERNALS__.invoke('plugin:event|listen',{event:'vault-changed',target:{kind:'Any'},handler:window.__TAURI_INTERNALS__.transformCallback(event=>window.__cacheEvents.push(event.payload))}); })()`)
  await evaluate(`(() => { const fetch=window.fetch; window.__searchRecoveryFetch=fetch; window.__rebuildStarted=false; window.__rebuildCalls=0; window.fetch=async(input,options)=>{
    const url=typeof input==='string'?input:input.url;
    if(url.includes('/rebuild_search_index')) { window.__rebuildCalls++; window.__rebuildStarted=true; await new Promise(resolve=>{window.__releaseRebuild=resolve}); }
    return fetch(input,options);
  }; })()`)
  await click('.local-search-recovery > button')
  await waitFor('window.__rebuildStarted')
  await chooseKind('canvas')
  await evaluate('document.querySelector(\'input[aria-label="Поиск по vault"]\').focus()')
  await key('a', 'KeyA', 2); await send('Input.insertText', { text: 'alternatecanvas' })
  await delay(300)
  await evaluate('window.__releaseRebuild()')
  await waitFor("document.querySelector('.local-search .sidebar-content')?.getAttribute('aria-busy') === 'false' && document.querySelector('.search-results button')?.dataset.nodeId === 'recovery-card'")
  assert.equal(await evaluate('document.querySelector(\'input[aria-label="Поиск по vault"]\').value'), 'alternatecanvas')
  assert.equal(await evaluate('window.__rebuildCalls'), 1)
  await evaluate('window.fetch=window.__searchRecoveryFetch')
  assert.equal((await backupNames()).length, 2)
  pass('Typing and filter changes during native recovery show only the latest query and do not start a second rebuild')

  // React may finish a queued latest-query refresh after the rebuild response.
  // Windows also reports parent-directory metadata changes for the quarantine.
  // Do not suppress all .brandon events: that could hide canonical task changes.
  // Observe the settled view and require no recurring events or refreshes.
  await delay(500)
  const initialEvents = await evaluate('window.__cacheEvents')
  assert.ok(initialEvents.flat().every(path => /[\\/]\.brandon$/.test(path)), JSON.stringify(initialEvents))
  await evaluate('window.__cacheEvents=[]')
  await evaluate("window.__cacheBusyChanges=0;window.__cacheObserver=new MutationObserver(()=>window.__cacheBusyChanges++);window.__cacheObserver.observe(document.querySelector('.local-search .sidebar-content'),{attributes:true,attributeFilter:['aria-busy']})")
  await delay(1500)
  assert.equal(await evaluate('window.__cacheBusyChanges'), 0, JSON.stringify(await evaluate('window.__cacheEvents')))
  assert.deepEqual(await evaluate('window.__cacheEvents'), [])
  await evaluate('window.__cacheObserver.disconnect()')
  pass('Quarantine and rebuilt SQLite files do not cause a watcher/search refresh loop')
  const cacheBefore = await hash('.brandon/search-v1.sqlite3')
  const wrongRoot = await evaluate(`window.__TAURI_INTERNALS__.invoke('rebuild_search_index', ${JSON.stringify({ root: original.root, query: 'recoverneedle', mode: 'words' })}).then(()=>({ok:true}),error=>({error:String(error)}))`)
  assert.match(wrongRoot.error, /Vault изменился/)
  assert.equal(await hash('.brandon/search-v1.sqlite3'), cacheBefore)
  pass('A rebuild request for a stale or different vault is rejected without changing the cache')
  if (process.env.BRANDON_QA_SCREENSHOT) {
    const screenshot = await send('Page.captureScreenshot', { format: 'png' })
    await writeFile(process.env.BRANDON_QA_SCREENSHOT, Buffer.from(screenshot.data, 'base64'))
  }

  await writeFile(join(privateDir, 'tasks.sqlite3'), 'SYNTHETIC_BROKEN_TASK_DB')
  await chooseKind('note'); await query('recoverneedle')
  await waitFor("document.querySelector('.local-search-error')?.textContent.includes('База задач')")
  await click('.local-search-recovery > button')
  await waitFor("document.querySelector('.local-search-recovery-error')?.textContent.includes('База задач')")
  assert.equal(await hash('.brandon/search-v1.sqlite3'), cacheBefore)
  assert.equal(await readFile(join(privateDir, 'tasks.sqlite3'), 'utf8'), 'SYNTHETIC_BROKEN_TASK_DB')
  assert.equal((await backupNames()).length, 2)
  assert.equal(await evaluate("Boolean(document.querySelector('.local-search-recovery-message'))"), false)
  pass('A corrupt task database is reported honestly and is never reset, quarantined or claimed as repaired')
  for (const path of protectedPaths.filter(path => path !== '.brandon/tasks.sqlite3')) assert.equal(await hash(path), before[path], path)
  const report = new URL('../../target/qa/search-recovery.json', import.meta.url)
  await writeFile(report, JSON.stringify({ date: new Date().toISOString(), passes, scratch, protectedPaths, notes: 'All fixture bytes synthetic. Canonical task DB is intentionally corrupted only in this temporary vault for the refusal scenario.' }, null, 2))
  console.log(JSON.stringify({ passed: passes.length, report: report.pathname }))
} finally {
  await evaluate(`(async()=>{ window.__releaseRebuild?.(); if(window.__searchRecoveryFetch) window.fetch=window.__searchRecoveryFetch;window.__cacheObserver?.disconnect();if(window.__cacheEventId!==undefined) { window.__TAURI_EVENT_PLUGIN_INTERNALS__.unregisterListener('vault-changed',window.__cacheEventId);await window.__TAURI_INTERNALS__.invoke('plugin:event|unlisten',{event:'vault-changed',eventId:window.__cacheEventId}); } })()`)
  await command('open_vault', { path: original.root }); await send('Page.reload'); socket.close()
}
