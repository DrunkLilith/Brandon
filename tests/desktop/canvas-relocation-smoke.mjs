// Real desktop IPC and UI on synthetic files only; restore the original vault.
import assert from 'node:assert/strict'
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { crc32, deflateSync } from 'node:zlib'
import { connect } from './cdp-test-client.mjs'

function png() {
  const header = Buffer.alloc(13); header.writeUInt32BE(8); header.writeUInt32BE(8, 4); header[8] = 8; header[9] = 2
  const pixels = Buffer.alloc(8 * 25, 150); for (let y = 0; y < 8; y++) pixels[y * 25] = 0
  const chunk = (type, data) => { const name = Buffer.from(type), head = Buffer.alloc(4), tail = Buffer.alloc(4); head.writeUInt32BE(data.length); tail.writeUInt32BE(crc32(Buffer.concat([name, data]))); return Buffer.concat([head, name, data, tail]) }
  return Buffer.concat([Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', header), chunk('IDAT', deflateSync(pixels)), chunk('IEND', Buffer.alloc(0))])
}

const { socket, send, evaluate, waitFor, click, key, delay } = await connect()
const command = (name, args = {}) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)})`)
const original = await command('restore_vault')
assert.ok(original?.root, 'An original vault is required for reliable restoration')
assert.equal(await evaluate("Boolean(document.querySelector('.save-button:not(:disabled)') || document.querySelector('.conflict-dialog'))"), false)
const scratch = await mkdtemp(join(tmpdir(), 'brandon-relocation-'))
const passes = [], pass = text => { passes.push(text); console.log(`PASS ${text}`) }
const read = path => readFile(join(scratch, path), 'utf8')
const board = async () => JSON.parse(await read('Board.canvas'))
const row = path => `.tree-row[title=${JSON.stringify(path)}]`
const openFile = async path => {
  await waitFor(`Boolean(document.querySelector(${JSON.stringify(row(path))}))`)
  await click(row(path))
  await waitFor(`document.querySelector('.tree-row.active')?.title === ${JSON.stringify(path)}`)
}
async function menu(path, index) {
  await evaluate(`document.querySelector(${JSON.stringify(row(path))}).dispatchEvent(new MouseEvent('contextmenu', {bubbles:true,cancelable:true,clientX:180,clientY:180}))`)
  await click(`.entry-menu button:nth-of-type(${index})`)
  await waitFor("Boolean(document.querySelector('.dialog'))")
}
async function rename(path, name, expectedPath) {
  await menu(path, 1); await evaluate("document.querySelector('.dialog input').focus()")
  await key('a', 'KeyA', 2); await send('Input.insertText', { text: name }); await click('.dialog button[type="submit"]')
  await waitFor("!document.querySelector('.dialog')")
  await waitFor(`Boolean(document.querySelector(${JSON.stringify(row(expectedPath))}))`)
}
async function move(path, folder, expectedPath) {
  await menu(path, 2)
  await evaluate(`(() => { const input=document.querySelector('.dialog select'); const setter=Object.getOwnPropertyDescriptor(HTMLSelectElement.prototype,'value').set; setter.call(input,${JSON.stringify(folder)}); input.dispatchEvent(new Event('change',{bubbles:true})); })()`)
  await click('.dialog button[type="submit"]'); await waitFor("!document.querySelector('.dialog')")
  await waitFor(`Boolean(document.querySelector(${JSON.stringify(row(expectedPath))}))`)
}
async function openBoard(path = 'Board.canvas') {
  await openFile(path); await waitFor("Boolean(document.querySelector('.canvas-view'))")
  await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'wgpu'")
}
async function doubleClickWorld(x, y) {
  await evaluate(`(() => { const view=document.querySelector('.canvas-view'), r=view.getBoundingClientRect(); view.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,clientX:r.left+r.width/2+${x},clientY:r.top+r.height/2+${y}})); })()`)
}
try {
  for (const folder of ['Attachments', 'Folder', 'Media']) await mkdir(join(scratch, folder))
  await writeFile(join(scratch, 'Old.md'), '# Old\n\nSynthetic destination marker.\n')
  await writeFile(join(scratch, 'Links.md'), 'See [[Old]].\n')
  await writeFile(join(scratch, 'Folder', 'Note.md'), '# Nested note\n\nSynthetic nested marker.\n')
  await writeFile(join(scratch, 'Attachments', 'picture.png'), png())
  const fixture = { custom: { future: 'keep', values: [1, 'two'] }, nodes: [
    { id: 'note', type: 'file', file: 'Old.md', subpath: '#Part', x: -300, y: -80, width: 280, height: 120, future: 'note-extension' },
    { id: 'image', type: 'file', file: 'Attachments/picture.png', x: 40, y: -100, width: 240, height: 180 },
    { id: 'nested', type: 'file', file: 'Folder/Note.md', x: 40, y: 150, width: 240, height: 100 },
    { id: 'text', type: 'text', text: 'Old.md is ordinary text', x: -300, y: 150, width: 280, height: 100 },
  ], edges: [{ id: 'edge', fromNode: 'note', toNode: 'image', custom: 'keep-edge' }] }
  await writeFile(join(scratch, 'Board.canvas'), JSON.stringify(fixture, null, 2))
  await writeFile(join(scratch, 'Folder', 'Inside.canvas'), JSON.stringify({ nodes: [{ id: 'inner', type: 'file', file: 'Folder/Note.md', x: -100, y: -50, width: 200, height: 100 }], edges: [], future: 'keep-inside' }))
  await command('open_vault', { path: scratch }); await send('Page.reload')
  await waitFor(`Boolean(document.querySelector(${JSON.stringify(row('Old.md'))}))`)
  await openBoard(); await waitFor("document.querySelector('.canvas-image-preview')?.naturalWidth > 0")
  const stale = await command('read_resource', { relativePath: 'Board.canvas' })
  await rename('Old.md', 'New', 'New.md')
  let current = await board()
  assert.equal(current.nodes[0].file, 'New.md'); assert.equal(current.nodes[0].subpath, '#Part')
  assert.equal(await read('Links.md'), 'See [[New]].\n')
  assert.deepEqual(current.custom, fixture.custom); assert.deepEqual(current.edges, fixture.edges)
  assert.equal(current.nodes[3].text, fixture.nodes[3].text)
  pass('UI note rename updates Canvas and wiki backlinks, preserves layout/anchors/unknown fields/non-file text')

  const conflict = await command('save_resource', { relativePath: 'Board.canvas', contents: stale.content, expectedFingerprint: stale.fingerprint })
  assert.equal(conflict.status, 'conflict'); assert.ok(conflict.disk.content.includes('New.md'))
  pass('An old board revision cannot silently save stale references over the rename result')
  await delay(500); await doubleClickWorld(-160, -20)
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'New'")
  assert.ok(await evaluate("document.querySelector('.cm-content')?.textContent.includes('Synthetic destination marker')"))
  pass('Double-clicking the renamed GPU file card opens the real new note')

  await openBoard(); await rename('Attachments/picture.png', 'renamed.png', 'Attachments/renamed.png')
  await move('Attachments', 'Media', 'Media/Attachments/renamed.png')
  current = await board(); assert.equal(current.nodes[1].file, 'Media/Attachments/renamed.png')
  await waitFor("document.querySelector('.canvas-image-preview')?.naturalWidth > 0")
  assert.deepEqual(await readFile(join(scratch, current.nodes[1].file)), png())
  pass('Attachment rename and folder move preserve original bytes and refresh the decoded board preview')

  await openBoard('Folder/Inside.canvas')
  await move('Folder', 'Media', 'Media/Folder/Inside.canvas')
  await waitFor("document.querySelector('.tree-row.active')?.title === 'Media/Folder/Inside.canvas'")
  await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'wgpu' && document.querySelector('.canvas-view')?.dataset.totalNodes === '1'")
  const inside = JSON.parse(await read('Media/Folder/Inside.canvas'))
  assert.equal(inside.nodes[0].file, 'Media/Folder/Note.md'); assert.equal(inside.future, 'keep-inside')
  assert.equal((await board()).nodes[2].file, 'Media/Folder/Note.md')
  await doubleClickWorld(0, 0)
  await waitFor("document.querySelector('.tree-row.active')?.title === 'Media/Folder/Note.md'")
  pass('Moving a folder remaps the open board tab and file references inside and outside that folder')

  await openBoard(); await doubleClickWorld(-160, 200)
  await waitFor("Boolean(document.querySelector('.canvas-node[data-node-id=\"text\"] textarea'))")
  await evaluate("document.querySelector('.canvas-node[data-node-id=\"text\"] textarea').focus()")
  await key('a', 'KeyA', 2); await send('Input.insertText', { text: 'Edited after relocation' })
  await waitFor("Boolean(document.querySelector('.save-button:not(:disabled)'))"); await key('s', 'KeyS', 2)
  await waitFor("document.querySelector('.save-button')?.disabled === true && document.querySelector('.save-button')?.textContent.includes('Сохранено')")
  await send('Page.reload'); await openBoard()
  current = await board(); assert.equal(current.nodes[3].text, 'Edited after relocation')
  assert.deepEqual(current.nodes.slice(0, 3).map(node => node.file), ['New.md', 'Media/Attachments/renamed.png', 'Media/Folder/Note.md'])
  assert.deepEqual(current.custom, fixture.custom); assert.deepEqual(current.edges, fixture.edges)
  pass('An edited mixed board saves and reloads with all relocated references and extension fields intact')

  await writeFile(join(scratch, 'Broken.canvas'), '{"nodes":[')
  await waitFor(`Boolean(document.querySelector(${JSON.stringify(row('Broken.canvas'))}))`)
  await rename('New.md', 'Final', 'Final.md')
  await waitFor("Boolean(document.querySelector('.notice.warning details'))")
  await click('.notice.warning summary')
  assert.ok((await evaluate("document.querySelector('.notice.warning')?.textContent")).includes('Broken.canvas: некорректный Canvas'))
  await delay(3200); assert.equal(await evaluate("Boolean(document.querySelector('.notice.warning'))"), true)
  assert.equal(await read('Broken.canvas'), '{"nodes":[')
  assert.equal((await board()).nodes[0].file, 'Final.md')
  if (process.env.BRANDON_QA_SCREENSHOT) {
    const screenshot = await send('Page.captureScreenshot', { format: 'png' })
    await writeFile(process.env.BRANDON_QA_SCREENSHOT, Buffer.from(screenshot.data, 'base64'))
  }
  await click('.notice.warning button[aria-label="Закрыть уведомление"]')
  pass('Partial link updates give a persistent dismissible warning with the damaged board path, without clobbering it')

  const report = fileURLToPath(new URL('../../target/qa/canvas-relocation.json', import.meta.url))
  await mkdir(join(report, '..'), { recursive: true })
  await writeFile(report, JSON.stringify({ date: new Date().toISOString(), passes, scratch, notes: 'Actual native IPC, watcher, GPU hit-testing, UI rename/move dialogs and disk save/reload. Synthetic data only; restored prior vault. Conflicts during the plan are separately deterministic Rust tests.' }, null, 2))
  console.log(JSON.stringify({ passed: passes.length, report }))
} finally {
  await command('open_vault', { path: original.root }); await send('Page.reload'); socket.close()
}
