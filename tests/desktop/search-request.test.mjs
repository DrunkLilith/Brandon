import assert from 'node:assert/strict'
import { setImmediate } from 'node:timers/promises'
import { test } from 'node:test'
import { observeSearch } from '../../apps/desktop/src/search-request.ts'

test('a delayed old response cannot overwrite the newer result', async () => {
  let finishOld
  const results = [], errors = [], completions = []
  const stop = observeSearch(new Promise(resolve => { finishOld = resolve }), {
    result: value => results.push(value), error: error => errors.push(error), complete: () => completions.push('old'),
  })
  stop()
  observeSearch(Promise.resolve('new'), {
    result: value => results.push(value), error: error => errors.push(error), complete: () => completions.push('new'),
  })
  await setImmediate()
  finishOld('old'); await setImmediate()
  assert.deepEqual(results, ['new'])
  assert.deepEqual(errors, [])
  assert.deepEqual(completions, ['new'])
})

test('a canceled rejection cannot replace current status with an error', async () => {
  let rejectOld
  const calls = []
  const stop = observeSearch(new Promise((_, reject) => { rejectOld = reject }), {
    result: () => calls.push('result'), error: () => calls.push('error'), complete: () => calls.push('complete'),
  })
  stop(); stop(); rejectOld(new Error('superseded request')); await setImmediate()
  assert.deepEqual(calls, [])
})

test('a current request reports failure and completion without a false result', async () => {
  const calls = []
  observeSearch(Promise.reject(new Error('index unavailable')), {
    result: () => calls.push('result'), error: error => calls.push(error.message), complete: () => calls.push('complete'),
  })
  await setImmediate()
  assert.deepEqual(calls, ['index unavailable', 'complete'])
})

test('cancel before an already resolved microtask prevents all UI updates', async () => {
  const calls = []
  observeSearch(Promise.resolve('ready'), {
    result: () => calls.push('result'), error: () => calls.push('error'), complete: () => calls.push('complete'),
  })()
  await setImmediate()
  assert.deepEqual(calls, [])
})
