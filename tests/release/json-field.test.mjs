import { spawnSync } from 'node:child_process'
import assert from 'node:assert/strict'
import test from 'node:test'

test('restore JSON extraction preserves escapes and Unicode and rejects missing fields', () => {
  const expected = 'line one\n"quoted" \\ Cyrillic: проверка'
  const result = spawnSync(process.execPath, ['scripts/json-field.mjs', 'markdown'], {input:JSON.stringify({markdown:expected}),encoding:'utf8'})
  assert.equal(result.status,0)
  assert.equal(result.stdout,expected)
  for (const input of ['{', '{}', '{"markdown":null}']) {
    const invalid=spawnSync(process.execPath,['scripts/json-field.mjs','markdown'],{input,encoding:'utf8'})
    assert.notEqual(invalid.status,0)
    assert.equal(invalid.stdout,'')
  }
})
