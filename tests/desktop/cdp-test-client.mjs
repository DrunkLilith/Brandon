export async function connect() {
  let target
  for (let attempt = 0; attempt < 100 && !target; attempt++) {
    try { const targets = await fetch(process.env.BRANDON_CDP ?? 'http://127.0.0.1:9222/json').then(r => r.json()); target = targets.find(entry => entry.type === 'page' && entry.title === 'Brandon') } catch { /* app may still be starting */ }
    if (!target) await new Promise(resolve => setTimeout(resolve, 100))
  }
  if (!target) throw new Error('Launch the standalone Brandon build with WebView2 remote debugging on localhost:9222')
  const socket = new WebSocket(target.webSocketDebuggerUrl)
  await new Promise((resolve, reject) => { socket.addEventListener('open', resolve, { once: true }); socket.addEventListener('error', reject, { once: true }) })
  let sequence = 0
  const pending = new Map()
  socket.addEventListener('message', ({ data }) => {
    const result = JSON.parse(data), entry = pending.get(result.id)
    if (!entry) return
    pending.delete(result.id); clearTimeout(entry.timer)
    if (result.error) entry.reject(new Error(result.error.message)); else entry.resolve(result.result)
  })
  const send = (method, params = {}) => new Promise((resolve, reject) => {
    const id = ++sequence, timer = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)) }, 30000)
    pending.set(id, { resolve, reject, timer }); socket.send(JSON.stringify({ id, method, params }))
  })
  const evaluate = async expression => {
    const result = await send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true })
    if (result.exceptionDetails) throw new Error(result.exceptionDetails.exception?.description ?? result.exceptionDetails.text)
    return result.result.value
  }
  const delay = milliseconds => new Promise(resolve => setTimeout(resolve, milliseconds))
  const waitFor = async expression => { for (let i = 0; i < 240; i++) { if (await evaluate(expression)) return; await delay(50) } throw new Error(`Timed out: ${expression}`) }
  const click = selector => evaluate(`document.querySelector(${JSON.stringify(selector)}).click()`)
  const key = async (key, code, modifiers = 0) => { await send('Input.dispatchKeyEvent', { type: 'keyDown', key, code, modifiers }); await send('Input.dispatchKeyEvent', { type: 'keyUp', key, code, modifiers }) }
  return { socket, send, evaluate, waitFor, click, key, delay }
}
