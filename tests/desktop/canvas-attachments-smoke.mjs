import assert from 'node:assert/strict'
import { copyFile, mkdir, mkdtemp, readFile, readdir, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { crc32, deflateSync } from 'node:zlib'
import { connect } from './cdp-test-client.mjs'

function png() {
  const width = 640, height = 360, raw = Buffer.alloc(height * (1 + width * 3))
  for (let y = 0; y < height; y++) for (let x = 0; x < width; x++) {
    const offset = y * (1 + width * 3) + 1 + x * 3
    raw[offset] = Math.round(x / width * 210 + 30); raw[offset + 1] = Math.round(y / height * 150 + 60); raw[offset + 2] = 160
  }
  const chunk = (type, data) => { const name = Buffer.from(type), head = Buffer.alloc(4), tail = Buffer.alloc(4); head.writeUInt32BE(data.length); tail.writeUInt32BE(crc32(Buffer.concat([name, data]))); return Buffer.concat([head, name, data, tail]) }
  const header = Buffer.alloc(13); header.writeUInt32BE(width); header.writeUInt32BE(height, 4); header[8] = 8; header[9] = 2
  return Buffer.concat([Buffer.from('89504e470d0a1a0a', 'hex'), chunk('IHDR', header), chunk('IDAT', deflateSync(raw)), chunk('IEND', Buffer.alloc(0))])
}
const { socket, send, evaluate, waitFor, click, key, delay } = await connect()
const original = await evaluate("window.__TAURI_INTERNALS__.invoke('restore_vault')")
const scratch = await mkdtemp(join(tmpdir(), 'brandon-attachments-'))
const source = await mkdtemp(join(tmpdir(), 'brandon-attachment-source-'))
const passes = [], pass = text => { passes.push(text); console.log(`PASS ${text}`) }
const command = (name, args) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)})`)
const openBoard = async () => { await click('.tree-row[title="Board.canvas"]'); await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'wgpu'") }
const save = async () => { await key('s', 'KeyS', 2); await waitFor("document.querySelector('.save-button')?.disabled === true"); await delay(100) }
const board = async () => JSON.parse(await readFile(join(scratch, 'Board.canvas'), 'utf8'))
async function drop(paths, x = 0, y = 0) {
  await evaluate(`(() => { const r = document.querySelector('.canvas-view').getBoundingClientRect(); return window.__TAURI_INTERNALS__.invoke('plugin:event|emit', {event:'tauri://drag-drop',payload:{paths:${JSON.stringify(paths)},position:{x:(r.left+r.width/2+${x})*devicePixelRatio,y:(r.top+r.height/2+${y})*devicePixelRatio}}}) })()`)
  await delay(100)
  await waitFor("!document.querySelector('.canvas-import-status progress')")
}
try {
  await writeFile(join(scratch, 'Board.canvas'), JSON.stringify({ nodes: [], edges: [], unknown: 'keep' }))
  await writeFile(join(scratch, 'Note.md'), '# Reference\n\nExisting local note.\n')
  await writeFile(join(source, 'Изображение.png'), png())
  await writeFile(join(source, 'Документ.pdf'), '%PDF-synthetic-smoke-not-a-real-document')
  await writeFile(join(source, 'token.txt'), 'SYNTHETIC_TEST_NOT_A_SECRET')
  await command('open_vault', { path: scratch }); await send('Page.reload')
  await waitFor("Boolean(document.querySelector('.tree-row[title=\"Board.canvas\"]'))")
  await openBoard(); await delay(200)
  await drop([join(source, 'Изображение.png')], -200, -50)
  await waitFor("document.querySelector('.canvas-image-preview')?.naturalWidth > 0")
  await save()
  let document = await board()
  assert.equal(document.nodes.length, 1); assert.equal(document.nodes[0].file, 'Attachments/Изображение.png'); assert.equal(document.unknown, 'keep')
  assert.deepEqual(await readFile(join(scratch, document.nodes[0].file)), await readFile(join(source, 'Изображение.png')))
  assert.ok(JSON.stringify(document).length < 1000)
  pass('Native drop copies the original once and saves a portable relative Canvas reference, not base64')
  const preview = await evaluate("({width:document.querySelector('.canvas-image-preview').naturalWidth,height:document.querySelector('.canvas-image-preview').naturalHeight,mask:getComputedStyle(document.querySelector('.has-image-preview')).maskImage})")
  assert.ok(preview.width <= 640 && preview.height <= 400); assert.ok(preview.mask.includes('clip-'))
  pass('Raster preview uses bounded binary IPC, decodes successfully and has an occlusion mask')
  await drop([join(source, 'Документ.pdf')], 220, 40); await save()
  document = await board(); assert.equal(document.nodes.length, 2)
  await evaluate("(() => { const r = document.querySelector('.canvas-view').getBoundingClientRect(); const e = new MouseEvent('dblclick',{bubbles:true,clientX:r.left+r.width/2+220,clientY:r.top+r.height/2+40}); document.querySelector('.canvas-view').dispatchEvent(e); })()")
  await waitFor("document.querySelector('.canvas-attachment-details h3')?.textContent === 'Документ.pdf'")
  await waitFor("document.querySelector('.canvas-attachment-details')?.textContent.includes('PDF')")
  await click('.canvas-attachment-details header button')
  pass('Non-image file opens a local detail peek instead of the Markdown editor')
  await drop([join(source, 'Изображение.png')], 100, -110); await save()
  assert.equal((await readdir(join(scratch, 'Attachments'))).length, 2)
  assert.equal((await board()).nodes.length, 3)
  pass('Repeated same-name same-content import reuses bytes while adding another board reference')
  await drop([join(source, 'token.txt')]); await waitFor("Boolean(document.querySelector('.canvas-import-status[role=alert]'))")
  assert.equal((await board()).nodes.length, 3); assert.equal((await readdir(join(scratch, 'Attachments'))).length, 2)
  await evaluate("document.querySelector('.canvas-import-status button').click()")
  pass('Credential-shaped file import is rejected without a card or a copied file')
  await evaluate("(() => {const data = new DataTransfer(); data.setData('application/x-brandon-vault-path','Note.md'); const r=document.querySelector('.canvas-view').getBoundingClientRect(); document.querySelector('.canvas-view').dispatchEvent(new DragEvent('drop',{bubbles:true,cancelable:true,dataTransfer:data,clientX:r.left+r.width/2,clientY:r.top+r.height/2+200}));})()")
  await waitFor("Boolean(document.querySelector('.save-button:not(:disabled)'))"); await save()
  assert.equal((await board()).nodes.at(-1).file, 'Note.md'); assert.equal((await readdir(join(scratch, 'Attachments'))).length, 2)
  pass('Dragging an existing vault file adds a reference without copying it')
  await send('Page.reload'); await waitFor("Boolean(document.querySelector('.tree-row[title=\"Board.canvas\"]'))"); await openBoard()
  await waitFor("document.querySelector('.canvas-image-preview')?.naturalWidth > 0")
  assert.equal((await board()).nodes.length, 4)
  pass('Saved mixed board and image previews survive application reload')
  if (process.env.BRANDON_VISUAL_QA === '1') {
    const current = await command('read_resource', { relativePath: 'Board.canvas' }), visual = JSON.parse(current.content)
    visual.nodes.push({ id: 'occlusion-check', type: 'text', x: -260, y: -90, width: 300, height: 80, text: 'Поверх изображения\nНижняя картинка не просвечивает.' })
    await command('save_resource', { relativePath: 'Board.canvas', contents: JSON.stringify(visual), expectedFingerprint: current.fingerprint })
    await waitFor("document.querySelector('.canvas-view')?.dataset.totalNodes === '5'")
    console.log(`VISUAL QA: ${scratch}`); await delay(45000)
  }
  const reportPath = fileURLToPath(new URL('../../target/qa/canvas-attachments.json', import.meta.url))
  await mkdir(join(reportPath, '..'), { recursive: true })
  await writeFile(reportPath, JSON.stringify({ date: new Date().toISOString(), passes, scratch, source, preview, note: 'Tauri native drop event and HTML internal drop are injected through CDP; actual Explorer/Finder gestures require separate interactive QA.' }, null, 2))
  console.log(JSON.stringify({ passed: passes.length, report: reportPath }))
} finally {
  await command('open_vault', { path: original.root }); await send('Page.reload'); socket.close()
  // Preserve isolated fixtures for visual/failure inspection; never remove a user vault.
}
