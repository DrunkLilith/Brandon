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
