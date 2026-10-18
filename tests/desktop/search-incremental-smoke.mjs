// Native IPC; no mocked search results. Only retained synthetic vaults.
import assert from 'node:assert/strict'
import { mkdtemp, mkdir, writeFile, rename, unlink } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { connect } from './cdp-test-client.mjs'
const { socket, evaluate, delay } = await connect()
const command = (name, args = {}) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(name)},${JSON.stringify(args)}).catch(error=>{throw new Error(String(error))})`)
const original = await command('restore_vault')
assert.ok(original?.root)
const root = await mkdtemp(join(tmpdir(), 'brandon-incremental-'))
const other = await mkdtemp(join(tmpdir(), 'brandon-incremental-other-'))
const passes = [], pass = value => { passes.push(value); console.log(`PASS ${value}`) }
let selectedRoot
async function search(query, selected = selectedRoot) { return command('search_vault', { root: selected, query, mode: 'words', limit: 80 }) }
async function until(query, predicate, selected = selectedRoot) {
  for (let i = 0; i < 300; i++) {
    const result = await search(query, selected)
    if (i === 0 || i === 299) console.log({ query, indexed: result.indexedSources, syncing: result.syncing, pending: result.pendingSources, hits: result.hits.length })
    if (predicate(result)) return result
    await delay(50)
  }
  throw new Error(`Index did not converge for ${query}`)
}
try {
  await mkdir(join(root, 'Folder'))
  await Promise.all(Array.from({ length: 1500 }, (_, n) => writeFile(join(root, 'Folder', `Note-${n}.md`), `# Note ${n}\n\nsharedincremental marker${n}\n`)))
  await writeFile(join(other, 'Other.md'), '# Other\n\notherrootneedle\n')
  selectedRoot = (await command('open_vault', { path: root })).root
  const initial = await until('sharedincremental', r => r.indexedSources === 1500 && !r.syncing)
  assert.equal(initial.hits.length, 80)
  pass('Batched initial indexing publishes all 1500 synthetic notes and settles')
  const warm = await search('marker1499')
  assert.equal(warm.scannedSources, 0); assert.equal(warm.changedSources, 0); assert.equal(warm.hits.length, 1)
  pass('A settled query scans zero sources and performs no reindexing')
  await writeFile(join(root, 'Folder/Note-1499.md'), '# Changed\n\nnewincrementalneedle\n')
  assert.equal((await search('marker1499')).hits.length, 0)
  await until('newincrementalneedle', r => r.hits.length === 1)
  pass('External edits remove old snippets immediately and publish the new match')
  await rename(join(root, 'Folder'), join(root, 'Moved'))
  const moved = await until('newincrementalneedle', r => r.hits[0]?.path === 'Moved/Note-1499.md')
  assert.ok(moved.hits.every(hit => !hit.path.startsWith('Folder/')))
  pass('Directory relocation converges to the new relative paths')
  await unlink(join(root, 'Moved/Note-1499.md'))
  assert.equal((await search('newincrementalneedle')).hits.length, 0)
  pass('Deleted sources never return their cached text')
  await writeFile(join(root, 'token.txt'), 'neverpublicincremental')
  assert.equal((await search('neverpublicincremental')).hits.length, 0)
  await command('reconcile_search_index', { root: selectedRoot })
  await until('sharedincremental', r => !r.syncing && r.indexedSources === 1499)
  pass('Explicit reconciliation settles with the correct count and excludes credentials')
  const otherRoot = (await command('open_vault', { path: other })).root
  await assert.rejects(search('sharedincremental'), /Vault изменился/)
  assert.equal((await until('otherrootneedle', r => r.hits.length === 1 && !r.syncing, otherRoot)).hits[0].path, 'Other.md')
  assert.equal((await search('sharedincremental', otherRoot)).hits.length, 0)
  pass('Expected-root validation rejects stale requests and isolates the next vault')
  console.log(JSON.stringify({ passes, root, other, warmQueryMs: warm.elapsedMs, retainedSynthetic: true }))
} finally { await command('open_vault', { path: original.root }); socket.close() }
