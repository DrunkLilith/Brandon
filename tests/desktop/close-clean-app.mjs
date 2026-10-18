// Developer-only lifecycle helper: never close a window containing an unsaved draft.
const targets = await fetch('http://127.0.0.1:9222/json').then(r => r.json())
const target = targets.find(entry => entry.type === 'page' && entry.title === 'Brandon')
if (!target) throw new Error('No Brandon QA window found')
const socket = new WebSocket(target.webSocketDebuggerUrl)
await new Promise((resolve, reject) => { socket.addEventListener('open', resolve); socket.addEventListener('error', reject) })
socket.send(JSON.stringify({ id: 1, method: 'Runtime.evaluate', params: { expression: "({dirty:Boolean(document.querySelector('.title-tab.active i') || document.querySelector('.save-button:not(:disabled)') || document.querySelector('.source-recovery') || document.querySelector('.save-button')?.textContent.includes('Сохраняю')), conflict:Boolean(document.querySelector('.conflict-dialog'))})", returnByValue: true } }))
socket.addEventListener('message', ({data}) => {
  const result = JSON.parse(data)
  if (result.id !== 1) return
  const state = result.result?.result?.value
  if (!state || state.dirty || state.conflict) { console.error('Refusing to close: unsaved work or unknown state'); process.exitCode = 1; socket.close(); return }
  socket.send(JSON.stringify({ id: 2, method: 'Runtime.evaluate', params: { expression: "window.__TAURI_INTERNALS__.invoke('plugin:window|close', {label:'main'})", awaitPromise: true } }))
  setTimeout(() => socket.close(), 500)
})
await new Promise(resolve => socket.addEventListener('close', resolve))
if (!process.exitCode) {
  let closed = false
  for (let attempt = 0; attempt < 30; attempt++) {
    try {
      const remaining = await fetch('http://127.0.0.1:9222/json').then(response => response.json())
      if (!remaining.some(entry => entry.id === target.id)) { closed = true; break }
    } catch { closed = true; break }
    await new Promise(resolve => setTimeout(resolve, 100))
  }
  if (!closed) throw new Error('Clean QA window did not actually close')
  console.log('PASS clean QA window closed')
}
