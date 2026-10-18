import assert from 'node:assert/strict'
import test from 'node:test'

import { validateLicenseReport } from '../../scripts/check-npm-licenses.mjs'

const dependency = (license = 'MIT') => ({ name: 'example', versions: ['1.0.0'], license })

test('accepts the explicitly approved production license', () => {
  assert.doesNotThrow(() => validateLicenseReport({ MIT: [dependency()] }))
})

test('accepts only exact reviewed desktop package/version/license combinations', () => {
  for (const [name, version, license] of [
    ['@tauri-apps/api', '2.12.3', 'Apache-2.0 OR MIT'],
    ['@tauri-apps/plugin-dialog', '2.8.2', 'MIT OR Apache-2.0'],
    ['@ungap/structured-clone', '1.4.0', 'ISC'],
  ]) {
    const entry = { name, versions: [version], license }
    assert.doesNotThrow(() => validateLicenseReport({ [license]: [entry] }))
    assert.throws(() => validateLicenseReport({ [license]: [{ ...entry, name: 'unreviewed-package' }] }), /not approved/)
    assert.throws(() => validateLicenseReport({ [license]: [{ ...entry, versions: ['99.0.0'] }] }), /not approved/)
    assert.throws(() => validateLicenseReport({ [license]: [{ ...entry, versions: [version, '99.0.0'] }] }), /not approved/)
  }
  assert.throws(() => validateLicenseReport({ 'Apache-2.0 AND MIT': [{ name: '@tauri-apps/api', versions: ['2.12.3'], license: 'Apache-2.0 AND MIT' }] }), /not approved/)
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
