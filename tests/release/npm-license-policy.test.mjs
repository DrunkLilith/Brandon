import assert from 'node:assert/strict'
import test from 'node:test'

import { validateLicenseReport } from '../../scripts/check-npm-licenses.mjs'

const dependency = (license = 'MIT') => ({ name: 'example', versions: ['1.0.0'], license })

test('accepts the explicitly approved production license', () => {
  assert.doesNotThrow(() => validateLicenseReport({ MIT: [dependency()] }))
})

test('fails closed on unknown, missing, mismatched, and disallowed licenses', () => {
  assert.throws(() => validateLicenseReport({ UNKNOWN: [dependency('UNKNOWN')] }), /not approved/)
  assert.throws(() => validateLicenseReport({ MIT: [{ name: 'missing', versions: ['1.0.0'] }] }), /missing license/)
  assert.throws(() => validateLicenseReport({ MIT: [dependency('ISC')] }), /does not match/)
  assert.throws(() => validateLicenseReport({ 'GPL-3.0': [dependency('GPL-3.0')] }), /not approved/)
  assert.throws(() => validateLicenseReport({}), /no production dependencies/)
})

test('fails closed on empty identities and malformed version inventories', () => {
  for (const malformed of [
    { name: '', versions: ['1.0.0'], license: 'MIT' },
    { name: '   ', versions: ['1.0.0'], license: 'MIT' },
    { name: 'missing-versions', license: 'MIT' },
    { name: 'empty-versions', versions: [], license: 'MIT' },
    { name: 'string-versions', versions: '1.0.0', license: 'MIT' },
    { name: 'empty-version', versions: [''], license: 'MIT' },
    { name: 'whitespace-version', versions: ['   '], license: 'MIT' },
    { name: 'non-string-version', versions: [1], license: 'MIT' },
  ]) {
    assert.throws(() => validateLicenseReport({ MIT: [malformed] }), /license entry is malformed/)
  }
})
