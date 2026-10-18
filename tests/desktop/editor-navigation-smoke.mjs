// Run against a release launched with WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222.
// The runner creates an isolated temporary vault and restores the previous vault afterwards.
import assert from 'node:assert/strict'
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, dirname, basename, resolve } from 'node:path'

const targets = await fetch(process.env.BRANDON_CDP ?? 'http://127.0.0.1:9222/json').then((response) => response.json())
const target = targets.find((item) => item.type === 'page' && item.title === 'Brandon')
assert.ok(target, 'Launch the desktop release with remote debugging enabled')
const socket = new WebSocket(target.webSocketDebuggerUrl)
await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }) })
let sequence = 0
const pending = new Map()
socket.addEventListener('message', ({ data }) => {
  const message = JSON.parse(data)
  const callback = pending.get(message.id)
  if (!callback) return
  pending.delete(message.id)
  clearTimeout(callback.timeout)
  if (message.error) callback.reject(new Error(message.error.message))
  else callback.resolve(message.result)
})
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const id = ++sequence
    const timeout = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)) }, 12000)
    pending.set(id, { resolve, reject, timeout })
    socket.send(JSON.stringify({ id, method, params }))
  })
}
async function evaluate(expression) {
  const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text)
  return result.result.value
}
const delay = (time) => new Promise((resolve) => setTimeout(resolve, time))
async function waitFor(expression, label = expression) {
  for (let attempt = 0; attempt < 100; attempt++) {
    if (await evaluate(expression)) return
    await delay(50)
  }
  throw new Error(`Timed out: ${label}`)
}
const click = (selector) => evaluate(`(() => { const target = document.querySelector(${JSON.stringify(selector)}); if (!target) throw new Error('Missing target: ' + ${JSON.stringify(selector)}); target.click(); })()`)
async function key(key, code, modifiers = 0) {
  await send('Input.dispatchKeyEvent', { type: 'keyDown', key, code, modifiers })
  await send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, modifiers })
}
const active = () => evaluate("document.querySelector('.breadcrumbs strong')?.textContent")
async function openFile(path) {
  await click(`.tree-row[title=${JSON.stringify(path)}]`)
  await waitFor(`document.querySelector('.breadcrumbs strong')?.textContent === ${JSON.stringify(path.replace(/\.(md|canvas)$/i, ''))}`)
}
async function focusEnd(selector = '.codemirror-host > .cm-editor > .cm-scroller > .cm-content', index = 0) {
  await evaluate(`document.querySelectorAll(${JSON.stringify(selector)})[${index}].focus()`)
  await key('End', 'End', 2)
}
async function reload() {
  await send('Page.reload')
  await delay(150)
  await waitFor("Boolean(document.querySelector('.tree-row'))")
}
const passed = []
const pass = (message) => { passed.push(message); console.log(`PASS ${message}`) }
const original = await evaluate("window.__TAURI_INTERNALS__.invoke('restore_vault')")
assert.ok(original?.root, 'An existing vault is required so it can be restored after the smoke test')
const preferences = await evaluate("({ visual: localStorage.getItem('brandon.visualEditing') })")
const scratch = await mkdtemp(join(tmpdir(), 'brandon-navigation-'))
const firstPath = join(scratch, 'First.md')
try {
  await writeFile(firstPath, '# First\n\nExisting `` code.\n\n```rust\nfn main() {\n  let answer = 42;\n  // [[Second]] stays literal\n  println!("{}", answer);\n}\n```\n\nTail.\n')
  await writeFile(join(scratch, 'Second.md'), '# Second\n\nUnchanged second note.\n')
  await writeFile(join(scratch, 'Board.canvas'), '{"nodes":[],"edges":[]}\n')
  await evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault', { path: ${JSON.stringify(scratch)} })`)
  await evaluate("localStorage.setItem('brandon.visualEditing', 'visual')")
  await reload()
  await openFile('First.md')
  await waitFor("Boolean(document.querySelector('.cm-live-code .tok-keyword'))", 'Rust grammar loaded offline')
  assert.equal(await evaluate("document.querySelector('.cm-live-code-language').value"), 'rust')
  assert.ok(await evaluate("Boolean(document.querySelector('.cm-inline-code-placeholder'))"))
  pass('Rust block is highlighted and legacy empty inline code has an editable placeholder')

  await click('.cm-inline-code-placeholder')
  await send('Input.insertText', { text: 'cargo run' })
  await waitFor("document.querySelector('.cm-inline-code')?.textContent === 'cargo run'")
  pass('Empty inline code becomes formatted editable code without visible backticks')
  if (process.env.BRANDON_QA_SCREENSHOT) {
    const clip = await evaluate("(() => { const r = document.querySelector('.document').getBoundingClientRect(); return { x: r.x, y: r.y, width: r.width, height: r.height, scale: 1 }; })()")
    const screenshot = await send('Page.captureScreenshot', { format: 'png', clip })
    await writeFile(process.env.BRANDON_QA_SCREENSHOT, Buffer.from(screenshot.data, 'base64'))
  }

  await click('.title-new-tab')
  await waitFor("Boolean(document.querySelector('.empty-workspace'))")
  assert.equal(await evaluate("Boolean(document.querySelector('.dialog, .save-button, button[aria-label=\"Контекст заметки\"]'))"), false)
  assert.ok((await readFile(firstPath, 'utf8')).includes('`cargo run`'))
  pass('New tab is empty, hides document actions, and flushes pending edits')
  await key('е', 'KeyT', 2)
  await waitFor("document.querySelectorAll('.title-tab').length === 3")
  pass('Ctrl+T works with the Russian keyboard layout')

  await openFile('Second.md')
  await key('ArrowLeft', 'ArrowLeft', 1)
  await waitFor("Boolean(document.querySelector('.empty-workspace'))")
  await key('ArrowRight', 'ArrowRight', 1)
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'Second'")
  await key('ArrowLeft', 'ArrowLeft', 1)
  await waitFor("Boolean(document.querySelector('.empty-workspace'))")
  await openFile('Board.canvas')
  assert.equal(await evaluate("document.querySelector('button[aria-label=\"Вперёд\"]').disabled"), true)
  pass('Back/forward history works and opening another file starts a new history branch')

  await key('w', 'KeyW', 2)
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent !== 'Board'")
  await key('T', 'KeyT', 10)
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'Board'")
  await key('Tab', 'Tab', 2)
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent !== 'Board'")
  await key('Tab', 'Tab', 10)
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'Board'")
  pass('Close, reopen, next tab, and previous tab shortcuts work')

  await click('button[aria-label="Открыть граф"]')
  await waitFor("Boolean(document.querySelector('.graph-view'))")
  await click('button[aria-label="Задачи и календарь"]')
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'Задачи'")
  await openFile('First.md')
  pass('Navigation includes graph, planner, canvas, and notes')

  await evaluate(`(() => {
    const fetch = window.fetch;
    window.__testReadStarted = false;
    window.__testReadCompleted = false;
    window.fetch = async (input, options) => {
      const url = typeof input === 'string' ? input : input.url;
      const delayed = url.includes('/read_note') && String(options?.body).includes('Second.md') && !window.__testReadStarted;
      if (delayed) {
        window.__testReadStarted = true;
        await new Promise(resolve => { window.__testReleaseRead = resolve });
      }
      const result = await fetch(input, options);
      if (delayed) window.__testReadCompleted = true;
      return result;
    };
  })()`)
  await click('.tree-row[title="Second.md"]')
  await waitFor('window.__testReadStarted')
  await click('.title-tab[title="First"]')
  await evaluate('window.__testReleaseRead()')
  await waitFor('window.__testReadCompleted')
  await delay(100)
  assert.equal(await active(), 'First')
  pass('Selecting the current tab cancels a slower pending navigation')

  await evaluate(`(() => {
    const fetch = window.fetch;
    window.__testSaveStarted = false;
    window.__testReleaseSave = null;
    window.fetch = async (input, options) => {
      const url = typeof input === 'string' ? input : input.url;
      if (url.includes('/save_note') && !window.__testSaveStarted) {
        window.__testSaveStarted = true;
        await new Promise(resolve => { window.__testReleaseSave = resolve });
      }
      return fetch(input, options);
    };
    window.__testWrapperInstalled = window.fetch !== fetch;
  })()`)
  assert.equal(await evaluate('window.__testWrapperInstalled'), true, 'IPC delay wrapper must be installed')
  await focusEnd()
  await send('Input.insertText', { text: '\nFirst edit.' })
  await key('s', 'KeyS', 2)
  try { await waitFor('window.__testSaveStarted') } catch (error) {
    console.log(await evaluate(`({ active: document.querySelector('.breadcrumbs strong')?.textContent, text: document.querySelector('.codemirror-host')?.textContent, focus: document.activeElement?.outerHTML?.slice(0, 300), dirty: Boolean(document.querySelector('.title-tab.active i')), saveStarted: window.__testSaveStarted, errors: document.querySelector('.notice')?.textContent })`))
    throw error
  }
  await send('Input.insertText', { text: '\nTyped while saving.' })
  await click('.tree-row[title="Second.md"]')
  await delay(100)
  assert.equal(await active(), 'First')
  await evaluate('window.__testReleaseSave()')
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'Second'")
  const saved = await readFile(firstPath, 'utf8')
  assert.ok(saved.includes('First edit.') && saved.includes('Typed while saving.'))
  assert.equal(await readFile(join(scratch, 'Second.md'), 'utf8'), '# Second\n\nUnchanged second note.\n')
  pass('Typing during an in-flight save is flushed before navigation, without changing the destination file')

  await openFile('First.md')
  await focusEnd()
  await click('.quick-insert-button')
  await evaluate("Array.from(document.querySelectorAll('.quick-insert-grid button')).find(button => button.querySelector('strong')?.textContent === 'Код в строке').click()")
  await delay(100)
  await send('Input.insertText', { text: 'npm test' })
  await key('s', 'KeyS', 2)
  await waitFor("Array.from(document.querySelectorAll('.cm-inline-code')).some(node => node.textContent === 'npm test')")
  await delay(120)
  assert.ok((await readFile(firstPath, 'utf8')).includes('`npm test`'))
  pass('Quick insert inline code selects an editable placeholder and round-trips to Markdown')

  await focusEnd()
  await send('Input.insertText', { text: '\nconst answer = 42;' })
  await key('Home', 'Home')
  await key('End', 'End', 8)
  await click('.quick-insert-button')
  await evaluate("Array.from(document.querySelectorAll('.quick-insert-grid button')).find(button => button.querySelector('strong')?.textContent === 'Код в строке').click()")
  await click('.title-new-tab')
  await waitFor("Boolean(document.querySelector('.empty-workspace'))")
  assert.ok((await readFile(firstPath, 'utf8')).includes('`const answer = 42;`'))
  pass('Quick insert wraps selected text instead of deleting it')

  await openFile('First.md')
  await focusEnd('.cm-live-code-editor .cm-content')
  await send('Input.insertText', { text: '\n// edited in highlighted block' })
  await click('.title-new-tab')
  await waitFor("Boolean(document.querySelector('.empty-workspace'))")
  assert.ok((await readFile(firstPath, 'utf8')).includes('// edited in highlighted block\n```'))
  await openFile('First.md')
  await click('.view-switcher button[title="Чтение"]')
  await waitFor("Boolean(document.querySelector('.preview-code .tok-keyword'))")
  assert.ok(await evaluate("document.querySelector('.preview-code code').textContent.includes('[[Second]] stays literal')"))
  pass('Highlighted block edits preserve Markdown fences and reading mode shares syntax colors')
  await click('.view-switcher button[title="Редактор"]')
  await waitFor("Boolean(document.querySelector('.codemirror-host .cm-content'))")
  await focusEnd()
  await click('.quick-insert-button')
  await evaluate("Array.from(document.querySelectorAll('.quick-insert-grid button')).find(button => button.querySelector('strong')?.textContent === 'Блок кода').click()")
  await waitFor("document.querySelectorAll('.cm-live-code').length === 2 && Boolean(document.activeElement?.closest('.cm-live-code'))")
  await send('Input.insertText', { text: 'const greeting = "hello";\n// highlighted comment\nconsole.log(greeting);' })
  await evaluate(`(() => {
    const input = document.querySelectorAll('.cm-live-code-language')[1];
    input.value = 'javascript';
    input.dispatchEvent(new Event('input', { bubbles: true }));
  })()`)
  await waitFor("Boolean(document.querySelectorAll('.cm-live-code')[1]?.querySelector('.tok-keyword'))")
  const colors = await evaluate("Array.from(document.querySelectorAll('.cm-live-code')[1].querySelectorAll('.tok-keyword, .tok-string, .tok-comment')).map(node => getComputedStyle(node).color)")
  assert.ok(new Set(colors).size >= 3, 'Keywords, strings, and comments must have different visible colors')
  await focusEnd('.cm-live-code-editor .cm-content', 1)
  await send('Input.insertText', { text: '\n// undo check' })
  await key('z', 'KeyZ', 2)
  await waitFor("!document.querySelectorAll('.cm-live-code')[1].textContent.includes('// undo check')")
  await key('y', 'KeyY', 2)
  await waitFor("document.querySelectorAll('.cm-live-code')[1].textContent.includes('// undo check')")
  pass('Undo and redo work inside the highlighted editor')
  await click('.title-new-tab')
  await waitFor("Boolean(document.querySelector('.empty-workspace'))")
  assert.ok((await readFile(firstPath, 'utf8')).includes('```javascript\nconst greeting = "hello";'))
  pass('Quick-inserted block focuses the code editor, accepts a language, and displays distinct syntax colors')
  await openFile('First.md')
  await focusEnd('.cm-live-code-editor .cm-content')
  await send('Input.insertText', { text: '\n// ``` inside a code block' })
  await click('.title-new-tab')
  await waitFor("Boolean(document.querySelector('.empty-workspace'))")
  assert.ok((await readFile(firstPath, 'utf8')).includes('````rust\n'))
  await openFile('First.md')
  await waitFor("document.querySelectorAll('.cm-live-code').length === 2")
  pass('Backticks inside a code block safely expand its Markdown fence and survive reopening')
  await focusEnd()
  await send('Input.insertText', { text: '\nLocal conflicting edit.' })
  await writeFile(firstPath, '# First\n\nExternal version.\n')
  await click('.title-new-tab')
  await waitFor("Boolean(document.querySelector('.conflict-dialog'))")
  assert.equal(await active(), 'First')
  assert.ok(await evaluate("document.querySelector('.codemirror-host').textContent.includes('Local conflicting edit.')"))
  assert.equal(await readFile(firstPath, 'utf8'), '# First\n\nExternal version.\n')
  pass('External-write conflicts block navigation and preserve both the local draft and disk version')
  await evaluate("Array.from(document.querySelectorAll('.conflict-dialog button')).find(button => button.textContent === 'Загрузить с диска').click()")
  await waitFor("!document.querySelector('.conflict-dialog')")

  for (let attempt = 0; attempt < 30; attempt++) {
    if (!await evaluate("Boolean(document.querySelector('.title-tab .tab-close'))")) break
    await click('.title-tab.active .tab-close')
    await delay(80)
  }
  await waitFor("Boolean(document.querySelector('.title-tab-empty'))")
  assert.equal(await evaluate("Boolean(document.querySelector('.save-button, button[aria-label=\"Контекст заметки\"]'))"), false)
  pass('Closing the final real tab returns to a clean centered workspace')
  console.log(JSON.stringify({ passed: passed.length, failures: 0 }))
} finally {
  try {
    await evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault', { path: ${JSON.stringify(original.root)} })`)
    await evaluate(preferences.visual === null ? "localStorage.removeItem('brandon.visualEditing')" : `localStorage.setItem('brandon.visualEditing', ${JSON.stringify(preferences.visual)})`)
    await reload()
    // Resolve and verify the exact generated target before recursive cleanup.
    assert.equal(dirname(resolve(scratch)), resolve(tmpdir()))
    assert.ok(basename(scratch).startsWith('brandon-navigation-'))
    await rm(scratch, { recursive: true, force: true })
  } finally { socket.close() }
}
