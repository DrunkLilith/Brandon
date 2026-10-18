// Synthetic fixtures; actual local native search and editor navigation.
import assert from 'node:assert/strict'
import { mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { createHash } from 'node:crypto'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { connect } from './cdp-test-client.mjs'
const { socket, send, evaluate, waitFor, click, key, delay } = await connect()
const command = (name, args = {}) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)})`)
const original = await command('restore_vault'); assert.ok(original?.root)
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i') || document.querySelector('.source-recovery') || document.querySelector('.conflict-dialog'))"), false)
const scratch = await mkdtemp(join(tmpdir(), 'brandon-search-ranking-'))
const passes = [], pass = message => { passes.push(message); console.log(`PASS ${message}`) }
const hashTasks = async () => createHash('sha256').update(await readFile(join(scratch, '.brandon/tasks.sqlite3'))).digest('hex')
const chooseKind = kind => evaluate(`(()=>{const select=document.querySelector('select[aria-label="Тип результатов"]');select.value=${JSON.stringify(kind)};select.dispatchEvent(new Event('change',{bubbles:true}));})()`)
const complete = () => waitFor("document.querySelector('.local-search .sidebar-content')?.dataset.completedQuery==='rankingneedle' && document.querySelector('.local-search .sidebar-content')?.getAttribute('aria-busy')==='false'")
try {
  await writeFile(join(scratch, 'Note.md'), '# First\n\nrankingneedle\n\n## Second\n\nrankingneedle\n')
  await writeFile(join(scratch, 'Board.canvas'), JSON.stringify({ nodes: [{ id: 'a', type: 'text', x: 0, y: 0, width: 300, height: 150, text: 'rankingneedle one' }, { id: 'b', type: 'text', x: 400, y: 0, width: 300, height: 150, text: 'rankingneedle two' }], edges: [] }))
  await command('open_vault', { path: scratch }); await send('Page.reload')
  await waitFor('Boolean(document.querySelector(\'.tree-row[title="Note.md"]\'))')
  await command('create_task', { title: 'AAA crowded' })
  await writeFile(join(scratch, 'Tasks/AAA crowded.md'), Array.from({ length: 100 }, (_, id) => `## Section ${id}\n\nrankingneedle\n\n`).join(''))
  for (let id = 0; id < 9; id++) {
    await command('create_task', { title: `ZZZ task ${id}` })
    await writeFile(join(scratch, `Tasks/ZZZ task ${id}.md`), `# ZZZ task ${id}\n\nrankingneedle with extra context words\n`)
  }
  const before = await hashTasks()
  await key('k', 'KeyK', 2); await waitFor('Boolean(document.querySelector(\'input[aria-label="Поиск по vault"]\'))')
  await chooseKind('task')
  await evaluate('document.querySelector(\'input[aria-label="Поиск по vault"]\').focus()')
  await send('Input.insertText', { text: 'rankingneedle' }); await complete()
  // The first response can precede the background indexing of the externally
  // written task descriptions. Wait for their watcher-driven result, not just
  // for the initial query acknowledgement; the same count is asserted below.
  await waitFor("document.querySelectorAll('.search-results button').length === 10")
  assert.equal(await evaluate("document.querySelectorAll('.search-results button').length"), 10)
  assert.equal(await evaluate("new Set([...document.querySelectorAll('.search-results button')].map(button=>button.dataset.path)).size"), 10)
  pass('Words finds all ten task notes despite one task containing one hundred matching sections')
  await click('.local-search-options button:nth-child(2)'); await complete()
  assert.equal(await evaluate("document.querySelectorAll('.search-results button').length"), 10)
  const limited = await command('search_vault', { query: 'rankingneedle type:task', mode: 'literal', limit: 3 })
  assert.equal(limited.hits.length, 3)
  pass('Literal mode and a small native result limit count distinct tasks, not description chunks')
  await chooseKind(''); await complete()
  assert.equal(await evaluate("document.querySelectorAll('.search-results button').length"), 14)
  assert.equal(await evaluate("document.querySelectorAll('.search-results button[data-path=\"Note.md\"]').length"), 2)
  assert.equal(await evaluate("document.querySelectorAll('.search-results button[data-node-id]').length"), 2)
  pass('Mixed search retains both real note sections and both Canvas card anchors alongside all tasks')
  await click('.search-results button[data-path="Tasks/ZZZ task 8.md"]')
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'ZZZ task 8' && Boolean(document.querySelector('.cm-content'))")
  assert.ok(await evaluate("window.getSelection()?.toString().includes('rankingneedle')"))
  assert.equal(await hashTasks(), before)
  pass('Previously starved task opens its exact saved matching line; canonical task storage is unchanged')
  await writeFile(join(scratch, 'Large.canvas'), JSON.stringify({ nodes: Array.from({ length: 20_000 }, (_, id) => ({ id: `load-${id}`, type: 'text', x: id % 200 * 320, y: Math.floor(id / 200) * 160, width: 300, height: 150, text: `canvasloadneedle ${id}` })), edges: [] }))
  await delay(500)
  // The active UI query may already have indexed this file after its watcher
  // event. Do not describe the first broad request as a cold-index benchmark.
  const first = await command('search_vault', { query: 'canvasloadneedle', mode: 'words', limit: 80 })
  assert.equal(first.hits.length, 80)
  const broadWarmMs = []
  for (let sample = 0; sample < 5; sample++) {
    const warm = await command('search_vault', { query: 'canvasloadneedle', mode: 'words', limit: 80 })
    assert.equal(warm.hits.length, 80)
    assert.equal(warm.changedSources, 0)
    assert.equal(new Set(warm.hits.map(hit => hit.nodeId)).size, 80)
    broadWarmMs.push(warm.elapsedMs)
  }
  assert.equal(await hashTasks(), before)
  pass('A 20,000-match Canvas query returns eighty distinct anchors without rewriting canonical task storage')
  console.log(JSON.stringify({ firstBroadMs: first.elapsedMs, firstChangedSources: first.changedSources, broadWarmMs, notes: 'Local release-build diagnostic samples, not an old/new comparative benchmark or UI frame-time measure.' }))
  await writeFile(new URL('../../target/qa/search-ranking.json', import.meta.url), JSON.stringify({ date: new Date().toISOString(), passes, scratch, firstBroadMs: first.elapsedMs, firstChangedSources: first.changedSources, broadWarmMs, notes: 'All fixtures synthetic; native IPC and real keyboard/UI navigation. No user content captured. Timings are local release-build diagnostics, not an old/new comparative benchmark.' }, null, 2))
  console.log(JSON.stringify({ passed: passes.length }))
} finally {
  await command('open_vault', { path: original.root }); await send('Page.reload'); socket.close()
}
