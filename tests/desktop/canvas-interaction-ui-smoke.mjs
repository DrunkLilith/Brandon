import assert from 'node:assert/strict'
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { connect } from './cdp-test-client.mjs'

const { socket, send, evaluate, waitFor, click, key, delay } = await connect()
const command = (name, args) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)})`)
const original = await command('restore_vault')
assert.ok(original?.root)
assert.equal(await evaluate("Boolean(document.querySelector('.save-button:not(:disabled), .conflict-dialog'))"), false, 'Do not replace an unsaved user workspace')
const scratch = await mkdtemp(join(tmpdir(), 'brandon-canvas-ui-'))
const passed = [], pass = message => { passed.push(message); console.log(`PASS ${message}`) }
const board = { nodes: [
  { id: 'one', type: 'text', x: -300, y: -70, width: 250, height: 160, text: 'Карточка для проверки\nВыделение только в редакторе' },
  { id: 'two', type: 'text', x: 100, y: -70, width: 250, height: 160, text: 'Вторая карточка\nДоска без отладочной плашки' },
], edges: [] }
const mouse = (type, p, extra = {}) => send('Input.dispatchMouseEvent', { type, ...p, ...extra })
async function drag(from, to) {
  await mouse('mousePressed', from, { button: 'left', buttons: 1, clickCount: 1 })
  for (let step = 1; step <= 12; step++) await mouse('mouseMoved', { x: from.x + (to.x - from.x) * step / 12, y: from.y + (to.y - from.y) * step / 12 }, { button: 'left', buttons: 1 })
  await mouse('mouseReleased', to, { button: 'left', buttons: 0, clickCount: 1 })
}
const point = (x, y) => evaluate(`(() => { const r=document.querySelector('.canvas-view').getBoundingClientRect(); return {x:r.left+r.width/2+${x}, y:r.top+r.height/2+${y}} })()`)
const rect = selector => evaluate(`(() => { const r=document.querySelector(${JSON.stringify(selector)}).getBoundingClientRect(); return {x:r.x,y:r.y,width:r.width,height:r.height} })()`)
const noTextSelection = () => evaluate("String(window.getSelection()) === ''")
try {
  await writeFile(join(scratch, 'Board.canvas'), JSON.stringify(board))
  await writeFile(join(scratch, 'Note.md'), '# Обычная заметка\n\nТекст по-прежнему можно выделять.\n')
  await command('open_vault', { path: scratch }); await send('Page.reload')
  await waitFor("Boolean(document.querySelector('.tree-row[title=\"Board.canvas\"]'))")
  await click('.tree-row[title="Board.canvas"]')
  await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'wgpu'")
  assert.equal(await evaluate("Boolean(document.querySelector('.canvas-render-status'))"), false)
  assert.equal(await evaluate("getComputedStyle(document.querySelector('.canvas-toolbar button')).userSelect"), 'none')
  assert.equal(await evaluate("getComputedStyle(document.querySelector('.canvas-view')).userSelect"), 'none')
  pass('Canvas and toolbar suppress document text selection; the wgpu/count debug badge is absent')
  const toolbar = await rect('.canvas-toolbar')
  await evaluate('window.getSelection().removeAllRanges()')
  await drag({ x: toolbar.x + 235, y: toolbar.y + toolbar.height / 2 }, { x: toolbar.x + toolbar.width - 30, y: toolbar.y + toolbar.height + 65 })
  assert.ok(await noTextSelection())
  pass('A real browser pointer drag across toolbar labels does not leave a text selection')
  await key('Escape', 'Escape')
  await drag(await point(-355, 150), await point(390, -100))
  await waitFor("document.querySelector('.canvas-selection-actions span')?.textContent === '2 объекта'")
  assert.ok(await noTextSelection())
  await key('a', 'KeyA', 2)
  assert.ok(await noTextSelection())
  pass('Marquee and Ctrl+A select board objects without selecting UI text')
  await key('Escape', 'Escape')
  const first = await point(-190, -20)
  await mouse('mousePressed', first, { button: 'left', buttons: 1, clickCount: 2 })
  await mouse('mouseReleased', first, { button: 'left', buttons: 0, clickCount: 2 })
  await waitFor("document.activeElement?.tagName === 'TEXTAREA'")
  assert.equal(await evaluate('getComputedStyle(document.activeElement).userSelect'), 'text')
  const editor = await rect('.canvas-node textarea')
  await drag({ x: editor.x + 15, y: editor.y + 23 }, { x: editor.x + 155, y: editor.y + 23 })
  assert.equal(await evaluate('document.activeElement.selectionEnd > document.activeElement.selectionStart'), true, 'Dragging inside the editor must still select text')
  // Native textarea shortcuts need a Windows virtual key, unlike JS-only board shortcuts.
  await send('Input.dispatchKeyEvent', { type: 'rawKeyDown', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65, nativeVirtualKeyCode: 65, modifiers: 2 })
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key: 'a', code: 'KeyA', windowsVirtualKeyCode: 65, nativeVirtualKeyCode: 65, modifiers: 2 })
  assert.equal(await evaluate('document.activeElement.selectionStart === 0 && document.activeElement.selectionEnd === document.activeElement.value.length'), true)
  await send('Input.insertText', { text: 'Редактирование и выделение работают' })
  await waitFor("document.querySelector('.canvas-node textarea')?.value === 'Редактирование и выделение работают'")
  await waitFor("Boolean(document.querySelector('.save-button:not(:disabled)'))")
  await key('Escape', 'Escape'); await key('s', 'KeyS', 2)
  await waitFor("document.querySelector('.save-button span')?.textContent === 'Сохранено'")
  assert.equal(JSON.parse(await readFile(join(scratch, 'Board.canvas'), 'utf8')).nodes[0].text, 'Редактирование и выделение работают')
  pass('Card text can still be selected, replaced and saved in its focused editor')
  await command('canvas_gpu_stop')
  await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'dom'")
  assert.equal(await evaluate("Boolean(document.querySelector('.canvas-render-status'))"), false)
  const grip = await rect('.canvas-node-grip')
  await drag({ x: grip.x + 60, y: grip.y + grip.height / 2 }, { x: grip.x + 100, y: grip.y + grip.height / 2 + 22 })
  assert.ok(await noTextSelection())
  await key('s', 'KeyS', 2); await waitFor("document.querySelector('.save-button span')?.textContent === 'Сохранено'")
  assert.equal(JSON.parse(await readFile(join(scratch, 'Board.canvas'), 'utf8')).nodes[0].x, -260)
  pass('Ordinary fallback also drags cards without selecting labels or showing diagnostics')
  await click('.tree-row[title="Note.md"]')
  await waitFor("Boolean(document.querySelector('.cm-content'))")
  assert.notEqual(await evaluate("getComputedStyle(document.querySelector('.cm-content')).userSelect"), 'none')
  pass('Selection suppression is scoped to the board and does not affect the note editor')
  if (process.env.BRANDON_VISUAL_QA === '1') {
    await click('.tree-row[title="Board.canvas"]')
    await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'wgpu'")
    console.log(`VISUAL QA: ${scratch}`); await delay(45000)
  }
  const reportPath = fileURLToPath(new URL('../../target/qa/canvas-interaction-ui.json', import.meta.url))
  await mkdir(join(reportPath, '..'), { recursive: true })
  await writeFile(reportPath, JSON.stringify({ date: new Date().toISOString(), passed, scratch }, null, 2))
  console.log(JSON.stringify({ passed: passed.length, report: reportPath }))
} finally {
  await command('open_vault', { path: original.root }); await send('Page.reload'); socket.close()
  // Preserve QA fixtures; never remove or overwrite the user's vault.
}
