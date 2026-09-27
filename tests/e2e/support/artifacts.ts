import { randomUUID } from 'node:crypto'
import { rmSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const ARTIFACT_ENV = 'KP_E2E_ARTIFACT_RUN'
const runPattern = /^[a-f0-9-]{36}$/

export function artifactDirectory() {
  let run = process.env[ARTIFACT_ENV]
  if (!run) {
    run = randomUUID()
    process.env[ARTIFACT_ENV] = run
  }
  if (!runPattern.test(run)) throw new Error('refusing unsafe Playwright artifact directory')
  return path.join(os.tmpdir(), 'knowledge-platform-playwright', run)
}

export function cleanupArtifacts() {
  const target = artifactDirectory()
  const parent = path.join(os.tmpdir(), 'knowledge-platform-playwright')
  if (path.dirname(path.resolve(target)) !== path.resolve(parent)) {
    throw new Error('refusing Playwright artifact cleanup outside the test root')
  }
  rmSync(target, { recursive: true, force: true })
}
