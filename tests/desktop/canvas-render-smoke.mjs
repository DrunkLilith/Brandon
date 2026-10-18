// Real Canvas files in an isolated temporary vault. Restore the user's vault in finally.
import assert from 'node:assert/strict'
import { mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'
import { renderFixture } from './canvas-fixtures.mjs'

const targets = await fetch(process.env.BRANDON_CDP ?? 'http://127.0.0.1:9222/json').then((r) => r.json())
const target = targets.find((entry) => entry.type === 'page' && entry.title === 'Brandon')
assert.ok(target, 'Launch Brandon with WebView2 remote debugging enabled')
const socket = new WebSocket(target.webSocketDebuggerUrl)
await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }) })
let sequence = 0
const pending = new Map()
socket.addEventListener('message', ({ data }) => {
  const result = JSON.parse(data), handler = pending.get(result.id)
  if (!handler) return
  pending.delete(result.id); clearTimeout(handler.timeout)
  if (result.error) handler.reject(new Error(result.error.message))
  else handler.resolve(result.result)
})
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++sequence, timeout = setTimeout(() => { pending.delete(id); reject(new Error(`Timeout: ${method}`)) }, 25000)
    pending.set(id, { resolve, reject, timeout }); socket.send(JSON.stringify({ id, method, params }))
  })
}
async function evaluate(expression) {
  const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text)
  return result.result.value
}
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms))
async function waitFor(expression) {
  for (let attempt = 0; attempt < 200; attempt++) { if (await evaluate(expression)) return; await delay(50) }
  throw new Error(`Timed out: ${expression}`)
}
const click = (selector) => evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`)
const toolbar = (title) => click(`.canvas-toolbar button[title=${JSON.stringify(title)}]`)
async function open(path, gpu = true) {
  await click(`.tree-row[title=${JSON.stringify(path)}]`)
  await waitFor(`document.querySelector('.breadcrumbs strong')?.textContent === ${JSON.stringify(path.replace(/\.(md|canvas)$/i, ''))}`)
  if (gpu) await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'wgpu'")
  else await waitFor("!document.documentElement.classList.contains('native-canvas-active')")
}
async function point(dx, dy) {
  return evaluate(`(() => { const r = document.querySelector('.canvas-view').getBoundingClientRect(); return { x: r.left + r.width / 2 + ${dx}, y: r.top + r.height / 2 + ${dy} }; })()`)
}
async function mouse(type, p, extras = {}) { await send('Input.dispatchMouseEvent', { type, ...p, ...extras }) }
async function press(p, modifiers = 0, clickCount = 1) {
  await mouse('mousePressed', p, { button: 'left', buttons: 1, modifiers, clickCount })
  await mouse('mouseReleased', p, { button: 'left', buttons: 0, modifiers, clickCount })
}
async function key(key, code, modifiers = 0) {
  await send('Input.dispatchKeyEvent', { type: 'keyDown', key, code, modifiers })
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, modifiers })
}
async function save() {
  await key('s', 'KeyS', 2)
  await waitFor("document.querySelector('.save-button')?.disabled === true")
  await delay(60)
}
const passed = [], benchmarks = []
const pass = (message) => { passed.push(message); console.log(`PASS ${message}`) }
const original = await evaluate("window.__TAURI_INTERNALS__.invoke('restore_vault')")
assert.ok(original?.root)
const scratch = await mkdtemp(join(tmpdir(), 'brandon-main-canvas-'))
const readBoard = async () => JSON.parse(await readFile(join(scratch, 'Board.canvas'), 'utf8'))
try {
  for (const [file, count] of [['Board.canvas', 6], ['Large.canvas', 10000], ['Huge.canvas', 20000]]) await writeFile(join(scratch, file), JSON.stringify(renderFixture(count)))
  await writeFile(join(scratch, 'Empty.canvas'), '{"nodes":[],"edges":[],"unknown":"keep"}\n')
  await writeFile(join(scratch, 'Broken.canvas'), '{broken JSON — DO NOT OVERWRITE')
  await writeFile(join(scratch, 'Note.md'), '# Note\n\nCanvas navigation target.\n')
  await evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault', { path: ${JSON.stringify(scratch)} })`)
  await send('Page.reload'); await waitFor("Boolean(document.querySelector('.tree-row[title=\"Board.canvas\"]'))")
  await open('Board.canvas')
  assert.equal(await evaluate("Boolean(document.querySelector('button[aria-label=\"Лаборатория GPU-рендера\"], .render-lab'))"), false)
  assert.equal(await evaluate("document.querySelectorAll('.canvas-node').length"), 0)
  pass('A real .canvas automatically uses GPU, with no lab or renderer toggle')
  const first = await point(-220, -66)
  await press(first); await press(await point(220, 66), 2)
  await waitFor("document.querySelector('.canvas-selection-actions span')?.textContent === '2 объекта'")
  await click('.canvas-selection-actions .primary'); await save()
  assert.equal((await readBoard()).edges.length, 5)
  pass('Ctrl multi-selection links two native hit targets and saves the edge')
  await mouse('mousePressed', first, { button: 'left', buttons: 1, clickCount: 1 })
  await mouse('mouseMoved', { x: first.x + 32, y: first.y + 24 }, { button: 'left', buttons: 1 })
  await mouse('mouseReleased', { x: first.x + 32, y: first.y + 24 }, { button: 'left', buttons: 0, clickCount: 1 })
  await save(); assert.equal((await readBoard()).nodes[0].x, renderFixture(6).nodes[0].x + 32)
  await evaluate("document.querySelector('.canvas-view').focus()")
  await key('я', 'KeyZ', 2); await save()
  assert.equal((await readBoard()).nodes[0].x, renderFixture(6).nodes[0].x)
  pass('Dragging saves once; undo works with the Russian keyboard layout')
  await press(first, 0, 2)
  await waitFor("document.activeElement?.tagName === 'TEXTAREA'")
  await evaluate("document.activeElement.select()")
  await send('Input.insertText', { text: 'Русский текст · GPU 123\nПравка без потери данных' })
  await key('Escape', 'Escape'); await save()
  const board = await readBoard()
  assert.equal(board.nodes[0].text, 'Русский текст · GPU 123\nПравка без потери данных')
  assert.equal(board.nodes[0].color, '6'); assert.equal(board.nodes[0].extensionFixture.preserved, true)
  assert.equal(board.brandonFixture, true); assert.equal(board.edges[0].extensionFixture, 'edge-kept')
  pass('Focused editing, Escape, Russian text, colors and unknown fields survive saving')

  await open('Note.md', false); await open('Board.canvas')
  await press(first, 0, 2); await waitFor("document.activeElement?.tagName === 'TEXTAREA'")
  assert.equal(await evaluate("document.activeElement.value"), board.nodes[0].text)
  await key('Escape', 'Escape')
  pass('Leaving the board restores opaque UI; reopening restores saved content and native rendering')
  await open('Empty.canvas')
  await evaluate("Array.from(document.querySelectorAll('.canvas-toolbar button')).find(b => b.textContent.includes('Карточка')).click()")
  await waitFor("document.activeElement?.tagName === 'TEXTAREA'")
  await evaluate("document.activeElement.select()")
  await send('Input.insertText', { text: 'Новая карточка сразу редактируется' })
  await key('Escape', 'Escape'); await save()
  const empty = JSON.parse(await readFile(join(scratch, 'Empty.canvas'), 'utf8'))
  assert.equal(empty.nodes[0].text, 'Новая карточка сразу редактируется'); assert.equal(empty.unknown, 'keep')
  pass('Adding a card immediately opens its own text editor')

  for (const [path, count] of [['Large.canvas', 10000], ['Huge.canvas', 20000]]) {
    await open(path); await toolbar('Показать весь холст'); await delay(400)
    const result = await evaluate(`(async () => {
      const element = document.querySelector('.canvas-view'), samples = [], cpu = [];
      const initial = Number(element.dataset.zoom); let prevented = true;
      for (let i = 0; i < 70; i++) {
        const rect = element.getBoundingClientRect(), before = element.dataset.nativeFrames;
        const event = new WheelEvent('wheel', { bubbles: true, cancelable: true, ctrlKey: true, deltaY: i % 2 ? -20 : 20, clientX: rect.left + rect.width / 2, clientY: rect.top + rect.height / 2 });
        const start = performance.now(); element.dispatchEvent(event); prevented &&= event.defaultPrevented;
        await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame);
        while (element.dataset.nativeFrames === before && performance.now() - start < 1500) await new Promise(r => setTimeout(r, 1));
        if (i >= 10) { samples.push(performance.now() - start); cpu.push(Number(element.dataset.frameCpuMs)); }
      }
      const quantiles = values => { values.sort((a,b) => a-b); return { p50: values[Math.floor(values.length * .5)], p95: values[Math.floor(values.length * .95)], max: values.at(-1) }; };
      return { total: Number(element.dataset.totalNodes), visible: Number(element.dataset.visibleNodes), mode: element.dataset.renderer, domCards: document.querySelectorAll('.canvas-node').length, inputAcknowledgementMs: quantiles(samples), nativeCpuMs: quantiles(cpu), preventedBrowserZoom: prevented, dpr: devicePixelRatio, initialZoom: initial, finalZoom: Number(element.dataset.zoom), adapter: element.dataset.renderAdapter, backend: element.dataset.renderBackend };
    })()`)
    assert.equal(result.mode, 'wgpu'); assert.equal(result.total, count); assert.ok(result.visible >= 2000)
    assert.equal(result.domCards, 0); assert.ok(result.preventedBrowserZoom)
    benchmarks.push(result); console.log(JSON.stringify(result))
  }
  pass('10k/20k ordinary boards complete Ctrl-wheel stress without DOM explosion')
  await open('Broken.canvas', false)
  await waitFor("Boolean(document.querySelector('.canvas-invalid[role=alert]'))")
  assert.equal(await readFile(join(scratch, 'Broken.canvas'), 'utf8'), '{broken JSON — DO NOT OVERWRITE')
  assert.equal(await evaluate("Boolean(document.querySelector('.canvas-toolbar'))"), false)
  pass('Malformed Canvas stays untouched and cannot be edited as an empty board')

  await open('Board.canvas')
  // Simulate an unavailable native renderer through the real IPC boundary.
  // WebView2 may use postMessage instead of fetch, so fetch interception is not reliable.
  await evaluate("window.__TAURI_INTERNALS__.invoke('canvas_gpu_stop')")
  try {
    await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'dom'")
    await waitFor("!document.documentElement.classList.contains('native-canvas-active')")
    assert.equal(await evaluate("document.querySelectorAll('.canvas-node').length"), 6)
    assert.equal(await evaluate("document.querySelector('.canvas-node textarea').value"), board.nodes[0].text)
    pass('A render failure restores the same editable board in the automatic fallback')
  } finally { /* The normal component lifecycle owns renderer recovery. */ }
  await open('Note.md', false); await open('Board.canvas')
  pass('GPU restarts successfully on the next board opening after a failure')
  const output = resolve(process.env.BRANDON_RENDER_REPORT ?? fileURLToPath(new URL('../../target/qa/canvas-render.json', import.meta.url)))
  await mkdir(resolve(output, '..'), { recursive: true })
  await writeFile(output, `${JSON.stringify({ date: new Date().toISOString(), passed, benchmarks, scratch, note: 'Native CPU time includes preparation/present submission, not GPU execution time. Input acknowledgement includes two RAF callbacks and IPC and is not measured display FPS. Ten warm-up iterations excluded; native text LOD is disabled below 50% zoom.' }, null, 2)}\n`)
  console.log(JSON.stringify({ passed: passed.length, report: output }))
} finally {
  try {
    await evaluate("if (window.__canvasQaFetch) { window.fetch = window.__canvasQaFetch; delete window.__canvasQaFetch; }")
    await evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault', { path: ${JSON.stringify(original.root)} })`)
    await send('Page.reload'); await delay(250)
    // Keep the isolated fixture directory for failure investigation; never delete a user's vault.
    console.log(`QA scratch: ${scratch}`)
  } finally { socket.close() }
}
