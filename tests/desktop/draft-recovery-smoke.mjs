// Synthetic vault only. Real native IPC, watcher, keyboard and copy dialogs.
import assert from 'node:assert/strict'
import { chmod, mkdir, mkdtemp, readFile, rename, writeFile } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { fileURLToPath } from 'node:url'
import { spawn } from 'node:child_process'
import { connect } from './cdp-test-client.mjs'

const { socket, evaluate, send, click, key, waitFor, delay } = await connect()
const command = (name, args = {}) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)}, ${JSON.stringify(args)})`)
const original = await command('restore_vault'); assert.ok(original?.root)
assert.equal(await evaluate("Boolean(document.querySelector('.save-button:not(:disabled)') || document.querySelector('.conflict-dialog'))"), false)
const scratch = await mkdtemp(join(tmpdir(), 'brandon-recovery-'))
const passes = [], pass = text => { passes.push(text); console.log(`PASS ${text}`) }
let restarted = null, windowClosed = false
const read = path => readFile(join(scratch, path), 'utf8')
async function inputCopyName(text) {
  await evaluate("document.querySelector('.dialog input').focus()")
  await key('a', 'KeyA', 2); await send('Input.insertText', { text })
}
async function type(text) {
  await evaluate("document.querySelector('.cm-content').focus()")
  await key('End', 'End', 2); await send('Input.insertText', { text })
  await waitFor("Boolean(document.querySelector('.title-tab.active i'))")
}
try {
  await writeFile(join(scratch, 'Original.md'), '# Original\n\nSynthetic base.\n')
  await writeFile(join(scratch, 'Other.md'), '# Other\n\nKeep this file intact.\n')
  const board = { nodes: [{ id: 'a', type: 'text', text: 'Base card', x: -120, y: -60, width: 240, height: 120, future: 'keep-node' }], edges: [], future: { keep: true } }
  await writeFile(join(scratch, 'Board.canvas'), JSON.stringify(board))
  await command('open_vault', { path: scratch }); await send('Page.reload')
  await waitFor("Boolean(document.querySelector('.cm-content'))")
  await type('\nUNSAVED_RECOVERY_MARKER')
  await rename(join(scratch, 'Original.md'), join(scratch, 'Renamed.md'))
  await waitFor("Boolean(document.querySelector('.source-recovery'))")
  await delay(1200)
  assert.ok(await evaluate("document.querySelector('.cm-content')?.textContent.includes('UNSAVED_RECOVERY_MARKER')"))
  assert.equal((await read('Renamed.md')).includes('UNSAVED_RECOVERY_MARKER'), false)
  assert.equal(await evaluate("Boolean(document.querySelector('.title-tab.active i'))"), true)
  assert.ok(await evaluate("document.querySelector('.source-recovery').getBoundingClientRect().height < 160"), 'Wide recovery banner must not consume the editor')
  if (process.env.BRANDON_QA_SCREENSHOT) {
    const screenshot = await send('Page.captureScreenshot', { format: 'png' })
    await writeFile(process.env.BRANDON_QA_SCREENSHOT, Buffer.from(screenshot.data, 'base64'))
  }
  await send('Emulation.setDeviceMetricsOverride', { width: 900, height: 600, deviceScaleFactor: 1, mobile: false })
  await delay(100)
  assert.ok(await evaluate("document.querySelector('.source-recovery').getBoundingClientRect().height < 220 && document.querySelector('.source-recovery').scrollWidth <= document.querySelector('.source-recovery').clientWidth + 1"), 'Compact recovery banner wraps without horizontal overflow')
  if (process.env.BRANDON_QA_SCREENSHOT) {
    const screenshot = await send('Page.captureScreenshot', { format: 'png' })
    await writeFile(process.env.BRANDON_QA_SCREENSHOT.replace(/\.png$/, '-compact.png'), Buffer.from(screenshot.data, 'base64'))
  }
  await send('Emulation.clearDeviceMetricsOverride'); await delay(100)
  pass('External rename before autosave preserves the unsaved editor and dirty tab instead of clearing them')

  await key('r', 'KeyR', 2); await waitFor("Boolean(document.querySelector('.dialog'))")
  assert.ok(await evaluate("document.querySelector('.cm-content')?.textContent.includes('UNSAVED_RECOVERY_MARKER')"))
  await click('.dialog button[type="button"]')
  await click('.tree-row[title="Other.md"]'); await waitFor("Boolean(document.querySelector('.dialog'))")
  assert.equal(await evaluate("document.querySelector('.breadcrumbs strong')?.textContent"), 'Original')
  await click('.dialog button[type="button"]')
  await click('.window-controls .close-window'); await waitFor("Boolean(document.querySelector('.dialog'))")
  assert.ok(await evaluate("Boolean(document.querySelector('.source-recovery'))"))
  await click('.dialog button[type="button"]')
  pass('Navigation, Ctrl+R and a real window close request stop safely and offer recovery without extra window permissions')

  await click('.source-recovery-save'); await inputCopyName('Other')
  await click('.dialog button[type="submit"]'); await waitFor("document.querySelector('.notice.error')?.textContent.includes('уже существует')")
  assert.ok((await read('Other.md')).includes('Keep this file intact'))
  assert.ok(await evaluate("document.querySelector('.cm-content')?.textContent.includes('UNSAVED_RECOVERY_MARKER')"))
  pass('A recovery filename collision never overwrites an existing file and keeps the original draft')

  // Delay only the local copy request, not the result or the application permissions.
  // This exercises actual native creation while allowing deterministic later typing.
  await evaluate(`(() => { const fetch = window.fetch; window.__recoveryFetch = fetch; window.__copyStarted = false; window.fetch = async (input, options) => {
    const url = typeof input === 'string' ? input : input.url;
    if (url.includes('/create_resource') && !window.__copyStarted) { window.__copyStarted = true; await new Promise(resolve => { window.__releaseCopy = resolve }); }
    return fetch(input, options);
  }; })()`)
  await inputCopyName('Recovered'); await click('.dialog button[type="submit"]')
  await waitFor('window.__copyStarted')
  await type('\nTYPED_DURING_RECOVERY')
  await evaluate('window.__releaseCopy()')
  await waitFor("!document.querySelector('.dialog') && document.querySelector('.breadcrumbs strong')?.textContent === 'Recovered'")
  await waitFor("!document.querySelector('.source-recovery')")
  assert.ok(await evaluate("document.querySelector('.cm-content')?.textContent.includes('TYPED_DURING_RECOVERY')"))
  await key('s', 'KeyS', 2)
  await waitFor("document.querySelector('.save-button')?.disabled && document.querySelector('.save-button')?.textContent.includes('Сохранено')")
  assert.ok((await read('Recovered.md')).includes('UNSAVED_RECOVERY_MARKER'))
  assert.ok((await read('Recovered.md')).includes('TYPED_DURING_RECOVERY'))
  assert.equal((await read('Renamed.md')).includes('RECOVERY_MARKER'), false)
  await evaluate('window.fetch = window.__recoveryFetch')
  pass('Save Copy keeps typing made during native creation, then saves it to the new file only')

  await type('\nRESTORED_PATH_DRAFT')
  await rename(join(scratch, 'Recovered.md'), join(scratch, 'Recovered-external.md'))
  await waitFor("Boolean(document.querySelector('.source-recovery'))")
  await writeFile(join(scratch, 'Recovered.md'), '# External replacement\n\nDo not silently overwrite.\n')
  await delay(1100)
  assert.ok((await read('Recovered.md')).includes('External replacement'))
  await click('.source-recovery-check'); await waitFor("Boolean(document.querySelector('.conflict-dialog'))")
  assert.ok(await evaluate("document.querySelector('.conflict-preview')?.textContent.includes('External replacement')"))
  await click('.conflict-dialog button:first-child')
  assert.ok(await evaluate("document.querySelector('.cm-content')?.textContent.includes('RESTORED_PATH_DRAFT')"))
  await click('.source-recovery-save'); await inputCopyName('Second recovery'); await click('.dialog button[type="submit"]')
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'Second recovery' && !document.querySelector('.dialog')")
  assert.ok((await read('Second recovery.md')).includes('RESTORED_PATH_DRAFT'))
  assert.ok((await read('Recovered.md')).includes('External replacement'))
  pass('A reappeared source does not restart autosave over external contents; comparison and a separate copy preserve both versions')

  await click('.tree-row[title="Board.canvas"]'); await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'wgpu'")
  await evaluate("(() => { const view=document.querySelector('.canvas-view'), r=view.getBoundingClientRect(); view.dispatchEvent(new MouseEvent('dblclick',{bubbles:true,clientX:r.left+r.width/2,clientY:r.top+r.height/2})); })()")
  await waitFor("Boolean(document.querySelector('.canvas-node textarea'))")
  await evaluate("document.querySelector('.canvas-node textarea').focus()")
  await key('a', 'KeyA', 2); await send('Input.insertText', { text: 'UNSAVED_BOARD_RECOVERY' })
  await rename(join(scratch, 'Board.canvas'), join(scratch, 'Board-external.canvas'))
  await waitFor("Boolean(document.querySelector('.source-recovery'))")
  await click('.source-recovery-save'); await inputCopyName('Recovered board'); await click('.dialog button[type="submit"]')
  await waitFor("document.querySelector('.tree-row.active')?.title === 'Recovered board.canvas' && !document.querySelector('.dialog')")
  const recovered = JSON.parse(await read('Recovered board.canvas'))
  assert.equal(recovered.nodes[0].text, 'UNSAVED_BOARD_RECOVERY'); assert.deepEqual(recovered.future, board.future); assert.equal(recovered.nodes[0].future, 'keep-node')
  assert.equal(JSON.parse(await read('Board-external.canvas')).nodes[0].text, 'Base card')
  await send('Page.reload'); await waitFor("Boolean(document.querySelector('.tree-row[title=\"Recovered board.canvas\"]'))")
  await click('.tree-row[title="Recovered board.canvas"]'); await waitFor("document.querySelector('.canvas-view')?.dataset.renderer === 'wgpu'")
  pass('An unsaved GPU board recovers into its own Canvas file with extension fields intact and survives reload')

  await writeFile(join(scratch, 'Resume.md'), '# Resume\n\nBase.\n')
  await waitFor("Boolean(document.querySelector('.tree-row[title=\"Resume.md\"]'))")
  await click('.tree-row[title="Resume.md"]'); await waitFor("document.querySelector('.breadcrumbs strong')?.textContent === 'Resume'")
  await type('\nRESUME_DRAFT_MARKER')
  await rename(join(scratch, 'Resume.md'), join(scratch, 'Resume-external.md'))
  await waitFor("Boolean(document.querySelector('.source-recovery'))")
  await writeFile(join(scratch, 'Resume.md'), '# External replacement for explicit comparison')
  await click('.source-recovery-check'); await waitFor("Boolean(document.querySelector('.conflict-dialog'))")
  await click('.conflict-dialog button.primary')
  await waitFor("!document.querySelector('.source-recovery') && document.querySelector('.save-button')?.textContent.includes('Сохранено')")
  assert.ok((await read('Resume.md')).includes('RESUME_DRAFT_MARKER'))
  await type('\nAUTOSAVE_RESUMED_MARKER')
  await waitFor("document.querySelector('.save-button')?.disabled && document.querySelector('.save-button')?.textContent.includes('Сохранено')")
  assert.ok((await read('Resume.md')).includes('AUTOSAVE_RESUMED_MARKER'))
  pass('Explicitly approving a reappeared source uses the conflict fingerprint and resumes ordinary autosave')

  await evaluate(`(() => { const fetch=window.fetch; window.__recoveryFetch=fetch; window.__pendingSave=false; window.fetch=async (input,options) => {
    const url=typeof input==='string'?input:input.url;
    if(url.includes('/save_note')&&!window.__pendingSave){window.__pendingSave=true;await new Promise(resolve=>{window.__releasePendingSave=resolve})}
    return fetch(input,options);
  }; })()`)
  await type('\nPENDING_SAVE_DRAFT')
  await key('s', 'KeyS', 2); await waitFor('window.__pendingSave')
  await rename(join(scratch, 'Resume.md'), join(scratch, 'Pending-external.md'))
  await waitFor("Boolean(document.querySelector('.source-recovery'))")
  await evaluate('window.__releasePendingSave()')
  await waitFor("document.querySelector('.save-button')?.textContent.includes('Сохранить')")
  await evaluate('window.fetch=window.__recoveryFetch')
  assert.ok(await evaluate("document.querySelector('.cm-content')?.textContent.includes('PENDING_SAVE_DRAFT')"))
  await click('.source-recovery-save'); await inputCopyName('Pending recovered'); await click('.dialog button[type="submit"]')
  await waitFor("document.querySelector('.breadcrumbs strong')?.textContent==='Pending recovered' && !document.querySelector('.dialog')")
  assert.ok((await read('Pending recovered.md')).includes('PENDING_SAVE_DRAFT'))
  assert.equal((await read('Pending-external.md')).includes('PENDING_SAVE_DRAFT'), false)
  pass('A delayed native save failure after external rename cannot erase the draft or prevent Save Copy recovery')

  await type('\nDISCARD_ONLY_SYNTHETIC_DRAFT')
  await rename(join(scratch, 'Pending recovered.md'), join(scratch, 'Discard-external.md'))
  await waitFor("Boolean(document.querySelector('.source-recovery'))")
  await click('.source-recovery-discard'); await waitFor("Boolean(document.querySelector('.discard-draft-dialog'))")
  await click('.discard-draft-dialog button.secondary')
  assert.ok(await evaluate("document.querySelector('.cm-content')?.textContent.includes('DISCARD_ONLY_SYNTHETIC_DRAFT')"))
  await click('.source-recovery-discard'); await waitFor("Boolean(document.querySelector('.discard-draft-dialog'))")
  await click('.discard-draft-dialog button.primary')
  await waitFor("!document.querySelector('.source-recovery') && !document.querySelector('.cm-content')")
  assert.equal((await read('Discard-external.md')).includes('DISCARD_ONLY_SYNTHETIC_DRAFT'), false)
  assert.ok((await read('Discard-external.md')).includes('PENDING_SAVE_DRAFT'))
  pass('Discard requires real confirmation, Cancel preserves the draft, and confirming changes no disk file')

  await writeFile(join(scratch, 'Readonly.md'), '# Readonly\n\nKeep disk version.\n')
  await waitFor("Boolean(document.querySelector('.tree-row[title=\"Readonly.md\"]'))")
  await click('.tree-row[title="Readonly.md"]'); await waitFor("document.querySelector('.breadcrumbs strong')?.textContent==='Readonly'")
  await chmod(join(scratch, 'Readonly.md'), 0o444)
  try {
    await type('\nREADONLY_DRAFT_MARKER'); await key('s', 'KeyS', 2)
    await waitFor("Boolean(document.querySelector('.source-recovery'))")
    assert.equal((await read('Readonly.md')).includes('READONLY_DRAFT_MARKER'), false)
    await click('.source-recovery-save'); await inputCopyName('Readonly recovered'); await click('.dialog button[type="submit"]')
    await waitFor("document.querySelector('.breadcrumbs strong')?.textContent==='Readonly recovered' && !document.querySelector('.dialog')")
    assert.ok((await read('Readonly recovered.md')).includes('READONLY_DRAFT_MARKER'))
    pass('A real Windows read-only save failure retains the draft, stops retry loops and offers a non-overwriting copy')
  } finally { await chmod(join(scratch, 'Readonly.md'), 0o666) }

  await type('\nCLOSE_FLUSH_DRAFT_MARKER')
  await click('.window-controls .close-window')
  for (let attempt = 0; attempt < 100 && socket.readyState !== WebSocket.CLOSED; attempt++) await delay(50)
  assert.equal(socket.readyState, WebSocket.CLOSED, 'Ordinary dirty window must save and actually close')
  windowClosed = true
  assert.ok((await read('Readonly recovered.md')).includes('CLOSE_FLUSH_DRAFT_MARKER'))
  const child = spawn(fileURLToPath(new URL('../../target/release/knowledge-desktop.exe', import.meta.url)), [], {
    cwd: fileURLToPath(new URL('../../', import.meta.url)), detached: true, windowsHide: true, stdio: 'ignore',
    env: { ...process.env, WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: '--remote-debugging-port=9222' },
  })
  child.unref()
  restarted = await connect()
  pass('An ordinary dirty native window flushes the last keystrokes before real close, then relaunches without extra permissions')

  const report = fileURLToPath(new URL('../../target/qa/draft-recovery.json', import.meta.url))
  await mkdir(join(report, '..'), { recursive: true })
  await writeFile(report, JSON.stringify({ date: new Date().toISOString(), passes, scratch, notes: 'Synthetic fixtures, actual native IPC and window close request. Only local copy fetch is paused for the typing race. No crash/restart draft durability is claimed.' }, null, 2))
  console.log(JSON.stringify({ passed: passes.length, report }))
} finally {
  if (restarted) {
    await restarted.evaluate(`window.__TAURI_INTERNALS__.invoke('open_vault', ${JSON.stringify({ path: original.root })})`)
    await restarted.send('Page.reload'); restarted.socket.close()
  } else if (!windowClosed) {
    await evaluate('window.__releaseCopy?.(); window.__releasePendingSave?.(); if (window.__recoveryFetch) window.fetch=window.__recoveryFetch')
    await command('open_vault', { path: original.root }); await send('Page.reload'); socket.close()
  }
}
