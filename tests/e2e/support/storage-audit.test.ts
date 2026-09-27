import assert from 'node:assert/strict'
import { execFileSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { chmodSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'

import { assertPhysicalPostgresStorageHasNoPlaintext } from './storage-audit.ts'

const alpine = 'alpine:3.22.1@sha256:4bcff63911fcb4448bd4fdacec207030997caf25e9bea4045fa6c8c44de311d1'

test('physical PostgreSQL volume scanner detects a marker without echoing it', () => {
  const suffix = randomBytes(6).toString('hex')
  const volume = `kp-physical-scan-${suffix}`
  const temporary = path.join(os.tmpdir(), volume)
  const source = path.join(temporary, 'source')
  const marker = `physical-marker-${randomBytes(16).toString('hex')}`
  mkdirSync(temporary, { mode: 0o700 })
  writeFileSync(source, marker, { mode: 0o644 })
  chmodSync(source, 0o644)
  try {
    execFileSync('docker', ['volume', 'create', '--label', 'knowledge-platform.physical-scan=true', volume], { stdio: 'ignore' })
    execFileSync('docker', [
      'run', '--rm', '--network', 'none',
      '--volume', `${volume}:/var/lib/postgresql`,
      '--volume', `${source}:/source:ro`,
      alpine, '/bin/sh', '-ec', 'cp /source /var/lib/postgresql/marker',
    ], { stdio: 'ignore' })
    assert.throws(
      () => assertPhysicalPostgresStorageHasNoPlaintext(volume, [marker]),
      (error: unknown) => error instanceof Error
        && error.message === 'physical PostgreSQL storage contains forbidden plaintext'
        && !error.message.includes(marker),
    )
  } finally {
    execFileSync('docker', ['volume', 'rm', '--force', volume], { stdio: 'ignore' })
    rmSync(temporary, { recursive: true, force: true })
  }
})
