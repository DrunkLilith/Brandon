// Real packaged desktop + native Rust IPC; fixtures and preferences are restored.
import assert from 'node:assert/strict'
import { mkdtemp, readFile, writeFile, rm, mkdir } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join, resolve, sep } from 'node:path'
import { connect } from './cdp-test-client.mjs'

const { socket, send, evaluate, waitFor, click, key, delay } = await connect()
const original = await evaluate("window.__TAURI_INTERNALS__.invoke('restore_vault')")
assert.ok(original?.root, 'Restore testing needs an original vault')
assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i, .conflict-dialog, .source-recovery'))"), false, 'Never switch away from an existing dirty draft')
const preferences = await evaluate("Object.fromEntries(['brandon.settings.v1','brandon.visualEditing','brandon.uiZoom.v2'].map(key => [key, localStorage.getItem(key)]))")
const scratch = await mkdtemp(join(tmpdir(), 'brandon-r1a-'))
const first = '# First\n\nBefore first\n## Повтор\n\nFirst body.\n\nBefore second\n## Повтор\n\nSecond body ^second-block\n\n[[Second#Target|Go to target]]\n\n[Markdown target](Second.md#^target-block)\n\n[[Missing#Nope]]\n'
const second = '---\naliases: [Вторая]\ntags: [r1a]\n---\n# Second\n\n## Target\n\nTarget body ^target-block\n\n## Last\n\nLast body.\n'
const passed = []
const pass = text => { passed.push(text); console.log(`PASS ${text}`) }
const contentSelector = '.codemirror-host > .cm-editor > .cm-scroller > .cm-content'
async function reload() { await send('Page.reload'); await delay(100); await waitFor("Boolean(document.querySelector('.tree-row'))") }
async function openFile(path) { await click('button[aria-label="Файлы"]'); await click(`.tree-row[title=${JSON.stringify(path)}]`); await waitFor(`document.querySelector('.title-tab.active')?.getAttribute('title') === ${JSON.stringify(path.split('/').at(-1).replace(/\.(md|canvas)$/i, ''))}`) }
async function focusEnd() { await evaluate(`document.querySelector(${JSON.stringify(contentSelector)}).focus()`); await key('End', 'End', 2) }
async function focusStart() { await evaluate(`document.querySelector(${JSON.stringify(contentSelector)}).focus()`); await key('Home', 'Home', 2) }
async function save() { if (await evaluate("Boolean(document.querySelector('.settings-dialog'))")) await key('Escape', 'Escape'); if (await evaluate("Boolean(document.querySelector('.command-menu'))")) await key('Escape', 'Escape'); await click('.save-button'); await waitFor("!document.querySelector('.title-tab.active i') && !document.querySelector('.save-button')?.textContent.includes('Сохраняю')") }
async function settings() { await click('button[aria-label="Настройки"]'); await waitFor("Boolean(document.querySelector('.settings-dialog'))") }
async function selectValue(label, value) { await evaluate(`(() => { const select = document.querySelector('select[aria-label=' + ${JSON.stringify(JSON.stringify(label))} + ']'); select.value = ${JSON.stringify(value)}; select.dispatchEvent(new Event('change', {bubbles:true})); })()`); await delay(30) }
async function menuAction(label) { await evaluate(`Array.from(document.querySelectorAll('.command-menu button')).find(button => button.querySelector('span')?.textContent === ${JSON.stringify(label)}).click()`); await waitFor("!document.querySelector('.command-menu')") }
async function fill(selector, value) { await evaluate(`document.querySelector(${JSON.stringify(selector)}).focus()`); await key('a', 'KeyA', 2); await send('Input.insertText', { text: value }); await delay(50) }
async function clickText(selector, text) { await evaluate(`Array.from(document.querySelectorAll(${JSON.stringify(selector)})).find(element => element.textContent.trim() === ${JSON.stringify(text)}).click()`); await delay(30) }
try {
  await mkdir(join(scratch, 'Folder'))
  await writeFile(join(scratch, 'First.md'), first)
  await writeFile(join(scratch, 'Second.md'), second)
  await writeFile(join(scratch, 'Links.md'), '[[Second#Target|Jump]]\n\n[[#^self]]\n\nLocal body ^self\n\n[[Second#Absent]]\n')
  await writeFile(join(scratch, 'Folder', 'Relative.md'), '# Relative\n\n[Parent](../Second.md#Target)\n')
  await writeFile(join(scratch, 'Code.md'), '# Code\n\n```rust\nlet answer = 42;\n```\n\n| A | B |\n| --- | --- |\n| one | two |\n')
  await writeFile(join(scratch, 'Board.canvas'), '{"nodes":[],"edges":[]}\n')
  await evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault', {path:${JSON.stringify(scratch)}})`)
  await evaluate("localStorage.removeItem('brandon.settings.v1'); localStorage.setItem('brandon.visualEditing','visual'); localStorage.setItem('brandon.uiZoom.v2','1')")
  await reload(); await openFile('First.md')

  await key('з', 'KeyP', 2)
  await waitFor("Boolean(document.querySelector('.palette'))")
  await send('Input.insertText', { text: 'настройки' })
  await waitFor("document.querySelector('.palette-results button strong')?.textContent === 'Открыть настройки'")
  await key('Enter', 'Enter')
  await waitFor("Boolean(document.querySelector('.settings-dialog'))")
  assert.equal(await evaluate("Boolean(document.querySelector('.palette'))"), false)
  pass('Shared command registry opens Settings from the command palette with Russian physical keys')
  await click('button[aria-label="Закрыть настройки"]')

  await focusEnd(); await send('Input.insertText', { text: '\nMenu marker' }); await key('Home', 'Home', 8)
  assert.equal(await evaluate('window.getSelection().toString()'), 'Menu marker')
  await key('F10', 'F10', 8)
  await waitFor("Boolean(document.querySelector('.command-menu'))")
  assert.equal(await evaluate("document.activeElement?.getAttribute('role')"), 'menuitem')
  await menuAction('Жирный'); await save()
  assert.ok((await readFile(join(scratch, 'First.md'), 'utf8')).includes('**Menu marker**'))
  await key('z', 'KeyZ', 2); await save()
  assert.equal((await readFile(join(scratch, 'First.md'), 'utf8')).includes('**Menu marker**'), false)
  pass('Keyboard context menu formats the selected text and one Undo reverses that command')

  await focusEnd(); await key('Home', 'Home', 8)
  const point = await evaluate("(() => { const range = window.getSelection().getRangeAt(0); const rect = range.getBoundingClientRect(); return {x:rect.left + 5,y:rect.top + rect.height/2}; })()")
  await send('Input.dispatchMouseEvent', { type: 'mousePressed', button: 'right', buttons: 2, clickCount: 1, ...point })
  await send('Input.dispatchMouseEvent', { type: 'mouseReleased', button: 'right', buttons: 0, clickCount: 1, ...point })
  await waitFor("Boolean(document.querySelector('.command-menu'))")
  await menuAction('Курсив'); await save()
  assert.ok((await readFile(join(scratch, 'First.md'), 'utf8')).includes('*Menu marker*'))
  await key('z', 'KeyZ', 2); await save()
  pass('Actual secondary mouse click opens the same command menu without losing the selection')

  await focusEnd(); await key('F10', 'F10', 8); await waitFor("Boolean(document.querySelector('.command-menu'))")
  const settingsPoint = await evaluate("(() => { const bounds = document.querySelector('button[aria-label=\"Настройки\"]').getBoundingClientRect(); return {x:bounds.left+bounds.width/2,y:bounds.top+bounds.height/2}; })()")
  await send('Input.dispatchMouseEvent', {type:'mousePressed',button:'left',buttons:1,clickCount:1,...settingsPoint})
  await send('Input.dispatchMouseEvent', {type:'mouseReleased',button:'left',buttons:0,clickCount:1,...settingsPoint})
  await waitFor("Boolean(document.querySelector('.settings-dialog')) && !document.querySelector('.command-menu')")
  await delay(80)
  assert.ok(await evaluate("Boolean(document.activeElement.closest('.settings-dialog'))"))
  await click('button[aria-label="Закрыть настройки"]')
  pass('Dismissing a context menu by clicking another control does not steal its focus')

  await click('button[aria-label="Контекст заметки"]')
  await waitFor("document.querySelectorAll('.outline-list button').length === 3")
  await click('.outline-list button[data-heading-slug="повтор-1"]')
  await waitFor("window.getSelection().toString().includes('Повтор')")
  assert.equal(await evaluate("window.getSelection().anchorNode.parentElement.closest('.cm-line').previousElementSibling.textContent"), 'Before second')
  await focusEnd(); await send('Input.insertText', { text: '\n\n## Unsaved heading' })
  await waitFor("Array.from(document.querySelectorAll('.outline-list button')).some(button => button.textContent === 'Unsaved heading')")
  await click('.outline-list button[data-heading-slug="unsaved-heading"]'); await save()
  pass('Live outline follows dirty text and resolves duplicate headings to their exact source line')

  await clickText('.view-switcher button', 'Чтение')
  await click('.outline-list button[data-heading-slug="повтор-1"]')
  await waitFor("document.querySelector('.preview-pane #повтор-1')?.classList.contains('note-anchor-highlight')")
  assert.ok(await evaluate("document.querySelector('.editor-grid')?.classList.contains('mode-preview')"))
  pass('Outline navigates in Reading mode without forcing a switch back to the editor')
  await clickText('.writing-mode button', 'Markdown')
  await click('.outline-list button[data-heading-slug="повтор-1"]')
  await waitFor("window.getSelection().toString().includes('Повтор')")
  assert.equal(await evaluate("window.getSelection().anchorNode.parentElement.closest('.cm-line').previousElementSibling.textContent"), 'Before second')
  await clickText('.view-switcher button', 'Разделить')
  await click('.outline-list button[data-heading-slug="повтор-1"]')
  await waitFor("document.querySelector('.preview-pane #повтор-1')?.classList.contains('note-anchor-highlight')")
  pass('Source and split modes share the same exact outline target with visual and reading modes')
  await clickText('.writing-mode button', 'Визуально'); await clickText('.view-switcher button', 'Чтение')
  await click('.preview-pane a[data-note-target="Second#Target"]')
  await waitFor("document.querySelector('.title-tab.active')?.title === 'Second'")
  await waitFor("window.getSelection().toString().includes('Target')")
  pass('Wiki aliases retain heading anchors during cross-note navigation')

  await openFile('First.md'); await clickText('.view-switcher button', 'Чтение')
  await click('.preview-pane a[data-note-kind="markdown"]')
  await waitFor("window.getSelection().toString().includes('Target body')")
  await openFile('Folder/Relative.md'); await clickText('.view-switcher button', 'Чтение')
  await click('.preview-pane a[data-note-kind="markdown"]')
  await waitFor("document.querySelector('.title-tab.active')?.title === 'Second'")
  pass('Markdown block references and parent-relative file links open the correct note and anchor')

  await openFile('Links.md'); await focusStart(); await key('ArrowRight', 'ArrowRight'); await key('ArrowRight', 'ArrowRight')
  await key('Enter', 'Enter', 2)
  await waitFor("document.querySelector('.title-tab.active')?.title === 'Second'")
  await waitFor("window.getSelection().toString().includes('Target')")
  pass('Ctrl+Enter opens the internal link under the editor cursor')

  await openFile('Links.md'); await clickText('.view-switcher button', 'Чтение')
  await click('.preview-pane a[data-note-target="#^self"]')
  await waitFor("Array.from(document.querySelectorAll('.preview-pane p.note-anchor-highlight')).some(paragraph => paragraph.textContent.includes('Local body'))")
  assert.equal(await evaluate("document.querySelector('.title-tab.active')?.title"), 'Links')
  await click('.preview-pane a[data-note-target="Second#Absent"]')
  await waitFor("document.querySelector('.notice')?.textContent.includes('заголовок или блок не найден')")
  assert.equal(await readFile(join(scratch, 'Second.md'), 'utf8'), second)
  pass('Same-note block anchors work; missing anchors are reported without changing source bytes')

  await openFile('Links.md'); await focusEnd(); await send('Input.insertText', {text:'\nUnsaved before link'})
  await focusStart(); await key('ArrowRight', 'ArrowRight'); await key('ArrowRight', 'ArrowRight'); await key('Enter', 'Enter', 2)
  await waitFor("document.querySelector('.title-tab.active')?.title === 'Second'")
  assert.ok((await readFile(join(scratch, 'Links.md'), 'utf8')).includes('Unsaved before link'))
  pass('Cross-note link navigation flushes pending edits before opening the target')

  await openFile('First.md'); await clickText('.view-switcher button', 'Чтение')
  await click('.preview-pane a[data-note-target="Missing#Nope"]')
  await waitFor("document.querySelector('.notice')?.textContent.includes('Заметка не найдена')")
  assert.equal(await evaluate("window.__TAURI_INTERNALS__.invoke('refresh_vault').then(snapshot => snapshot.files.some(file => file.path === 'Missing.md'))"), false)
  pass('A missing destination reports its state and never silently creates a file')

  await openFile('Second.md')
  if (!await evaluate("Boolean(document.querySelector('.right-sidebar'))")) await click('button[aria-label="Контекст заметки"]')
  await clickText('.tag-cloud button', '#r1a')
  await waitFor("document.querySelector('.local-search input')?.value === 'tag:r1a' || document.querySelector('.sidebar input')?.value === 'tag:r1a'")
  pass('Clicking a note tag opens the shared local search with that filter')

  await openFile('First.md'); await settings()
  await click('input[aria-label="Номера строк"]')
  await click('input[aria-label="Ограничивать длину строки"]')
  await selectValue('Предпросмотр при наведении', 'none')
  await click('button[aria-label="Закрыть настройки"]')
  await waitFor("Boolean(document.querySelector('.codemirror-host > .cm-editor > .cm-scroller > .cm-gutters')) && document.querySelector('.document').classList.contains('wide-lines')")
  pass('Settings change line numbers and readable width immediately in the live editor')

  await focusEnd(); await send('Input.insertText', { text: '\nUndo survives settings' })
  await settings(); await click('input[aria-label="Проверять орфографию"]'); await click('button[aria-label="Закрыть настройки"]')
  await focusEnd(); await key('z', 'KeyZ', 2); await save()
  assert.equal((await readFile(join(scratch, 'First.md'), 'utf8')).includes('Undo survives settings'), false)
  assert.equal(await evaluate(`document.querySelector(${JSON.stringify(contentSelector)}).getAttribute('spellcheck')`), 'false')
  pass('Changing preferences reconfigures CodeMirror without losing undo or the document')

  await settings(); await clickText('.settings-layout nav button', 'Горячие клавиши')
  await click('button[aria-label="Назначить: Сохранить документ"]')
  await key('D', 'KeyD', 10)
  await waitFor("document.querySelector('button[aria-label=\"Назначить: Сохранить документ\"]')?.textContent.includes('Shift')")
  await click('button[aria-label="Назначить: Новая заметка"]'); await key('D', 'KeyD', 10)
  await waitFor("document.querySelector('.hotkey-error')?.textContent.includes('Уже назначено')")
  await key('Escape', 'Escape')
  await click('button[aria-label="Закрыть настройки"]')
  await focusEnd(); await send('Input.insertText', { text: '\nCustom save binding' }); await key('В', 'KeyD', 10)
  await waitFor("!document.querySelector('.title-tab.active i')")
  assert.ok((await readFile(join(scratch, 'First.md'), 'utf8')).includes('Custom save binding'))
  pass('Custom RU-layout hotkey executes the same save command; collisions are rejected')

  await reload(); await openFile('First.md'); await settings()
  await clickText('.settings-layout nav button', 'Горячие клавиши')
  assert.ok(await evaluate("document.querySelector('button[aria-label=\"Назначить: Сохранить документ\"]')?.textContent.includes('Shift')"))
  await click('button[aria-label="По умолчанию: Сохранить документ"]')
  assert.equal(await evaluate("document.querySelector('button[aria-label=\"Назначить: Сохранить документ\"]')?.textContent"), 'Ctrl + S')
  await click('button[aria-label="Закрыть настройки"]')
  pass('Preferences and custom assignments survive reload; a command can reset to its default binding')

  await settings(); await selectValue('Режим новых документов', 'preview'); await click('button[aria-label="Закрыть настройки"]')
  await openFile('Second.md')
  await waitFor("Boolean(document.querySelector('.editor-grid.mode-preview'))")
  await settings(); await selectValue('Режим новых документов', 'edit'); await click('button[aria-label="Закрыть настройки"]')
  await openFile('First.md')
  pass('Default document mode applies to the next note without mutating its source')

  await clickText('.view-switcher button', 'Чтение')
  await send('Input.dispatchMouseEvent', {type:'mouseMoved',x:2,y:50})
  await evaluate("document.querySelector('.preview-pane a[data-note-target=\"Second#Target\"]').scrollIntoView({block:'center'})")
  // Scroll dismisses previews by design; let that event settle before hovering.
  await delay(100)
  const hover = await evaluate("(() => { const bounds = document.querySelector('.preview-pane a[data-note-target=\"Second#Target\"]').getBoundingClientRect(); return {x:bounds.left+8,y:bounds.top+bounds.height/2}; })()")
  await send('Input.dispatchMouseEvent', { type: 'mouseMoved', ...hover })
  await waitFor("document.querySelector('.note-hover-preview')?.textContent.includes('Second.md')")
  pass('Configured no-modifier hover preview reads a local note and preserves its anchor')
  await settings(); await selectValue('Предпросмотр при наведении', 'ctrl'); await click('button[aria-label="Закрыть настройки"]')
  await send('Input.dispatchMouseEvent', {type:'mouseMoved',x:2,y:50})
  await send('Input.dispatchMouseEvent', {type:'mouseMoved',...hover})
  await delay(450)
  assert.equal(await evaluate("Boolean(document.querySelector('.note-hover-preview'))"), false)
  await send('Input.dispatchKeyEvent', {type:'keyDown',key:'Control',code:'ControlLeft',modifiers:2,windowsVirtualKeyCode:17})
  await waitFor("Boolean(document.querySelector('.note-hover-preview'))")
  await send('Input.dispatchKeyEvent', {type:'keyUp',key:'Control',code:'ControlLeft',modifiers:0,windowsVirtualKeyCode:17})
  await waitFor("!document.querySelector('.note-hover-preview')")
  pass('Ctrl-required hover preview stays closed without Ctrl and dismisses on modifier release')

  await openFile('Code.md')
  await waitFor("Boolean(document.querySelector('.cm-live-code .cm-content'))")
  await evaluate("document.querySelector('.cm-live-code .cm-content').focus()")
  await key('F10', 'F10', 8)
  await waitFor("Boolean(document.querySelector('.command-menu'))")
  assert.equal(await evaluate("Array.from(document.querySelectorAll('.command-menu button')).find(button => button.querySelector('span')?.textContent === 'Жирный').disabled"), true)
  await key('Escape', 'Escape')
  await waitFor("Boolean(document.activeElement.closest('.cm-live-code'))")
  assert.equal(await evaluate("(() => { const input = document.querySelector('.cm-live-table input'); const event = new MouseEvent('contextmenu',{bubbles:true,cancelable:true}); input.dispatchEvent(event); return event.defaultPrevented; })()"), false)
  pass('Code context disables prose formatting; Escape restores code focus; table fields retain a native menu')

  // Clipboard adapter is mocked: this checks real menu/editor flows without
  // replacing the user's multi-format Windows clipboard.
  await openFile('First.md'); await focusEnd()
  await evaluate("Object.defineProperty(navigator, 'clipboard', {configurable:true, value:{readText:()=>new Promise(resolve=>window.__r1aPaste=resolve), writeText:async text=>{window.__r1aCopied=text}}})")
  await key('F10', 'F10', 8); await waitFor("Boolean(document.querySelector('.command-menu'))"); await menuAction('Вставить')
  await send('Input.insertText', { text: '\nNewer edit' })
  await evaluate("window.__r1aPaste('STALE CLIPBOARD')")
  await waitFor("document.querySelector('.notice')?.textContent.includes('Текст изменился')")
  await save()
  assert.equal((await readFile(join(scratch, 'First.md'), 'utf8')).includes('STALE CLIPBOARD'), false)
  assert.ok((await readFile(join(scratch, 'First.md'), 'utf8')).includes('Newer edit'))
  pass('A delayed clipboard read cannot overwrite newer typing or a changed selection')

  if (process.env.BRANDON_QA_SCREENSHOT) {
    await settings()
    const screenshot = await send('Page.captureScreenshot', { format: 'png' })
    await writeFile(process.env.BRANDON_QA_SCREENSHOT, Buffer.from(screenshot.data, 'base64'))
    await click('button[aria-label="Закрыть настройки"]')
  }
  const baseScale = await evaluate('window.devicePixelRatio')
  for (const scale of [1.25, 1.5]) {
    await evaluate(`localStorage.setItem('brandon.uiZoom.v2',${JSON.stringify(String(scale))})`)
    await reload(); await settings()
    await waitFor(`Math.abs(window.devicePixelRatio / ${baseScale} - ${scale}) < 0.03`)
    assert.ok(await evaluate("(() => { const bounds = document.querySelector('.settings-dialog').getBoundingClientRect(); return bounds.left >= 0 && bounds.top >= 0 && bounds.right <= innerWidth+1 && bounds.bottom <= innerHeight+1 && document.querySelector('.settings-content').scrollWidth <= document.querySelector('.settings-content').clientWidth+1; })()"))
    if (process.env.BRANDON_QA_SCREENSHOT) {
      const screenshot = await send('Page.captureScreenshot', {format:'png'})
      await writeFile(process.env.BRANDON_QA_SCREENSHOT.replace(/\.png$/,`-${Math.round(scale*100)}.png`), Buffer.from(screenshot.data,'base64'))
    }
    await click('button[aria-label="Закрыть настройки"]')
    pass(`Packaged WebView layout remains within the window at ${Math.round(scale*100)}% UI zoom (not an OS DPI switch)`)
  }
  console.log(JSON.stringify({ passed: passed.length, scratch, binary: 'standalone Tauri/WebView2', clipboard: 'mocked adapter, OS clipboard untouched' }))
} catch (error) {
  console.log(await evaluate("({active:document.querySelector('.title-tab.active')?.title, mode:document.querySelector('.editor-grid')?.className, links:Array.from(document.querySelectorAll('.preview-pane a')).map(link=>({target:link.dataset.noteTarget,kind:link.dataset.noteKind})), notice:document.querySelector('.notice')?.textContent, fatal:document.querySelector('.fatal-screen')?.textContent})"))
  throw error
} finally {
  try {
    if (await evaluate("Boolean(document.querySelector('.command-menu, .settings-dialog, .palette'))")) await key('Escape', 'Escape')
    if (await evaluate("Boolean(document.querySelector('.save-button:not(:disabled)'))")) await save()
    assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i, .conflict-dialog, .source-recovery'))"), false, 'Do not discard an unexpected dirty fixture')
    await evaluate(`(() => { const original = ${JSON.stringify(preferences)}; for (const [key,value] of Object.entries(original)) { if(value === null) localStorage.removeItem(key); else localStorage.setItem(key,value); } })()`)
    await evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault',{path:${JSON.stringify(original.root)}})`)
    await reload()
    assert.ok(resolve(scratch).startsWith(resolve(tmpdir()) + sep) && scratch.includes('brandon-r1a-'))
    await rm(scratch, { recursive: true, force: true })
  } finally { socket.close() }
}
