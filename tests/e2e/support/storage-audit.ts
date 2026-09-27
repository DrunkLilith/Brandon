import { spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { chmodSync, mkdirSync, rmSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'

const postgresImage = 'postgres:18.6-alpine'
const volumePattern = /^kp-[A-Za-z0-9_.-]+$/

export function assertPhysicalPostgresStorageHasNoPlaintext(volume: string, plaintext: string[]) {
  if (!volumePattern.test(volume)) throw new Error('refusing to scan an unsafe PostgreSQL volume name')
  if (plaintext.length === 0 || plaintext.some((value) => !value || /[\r\n\0]/.test(value))) {
    throw new Error('physical PostgreSQL plaintext probes are malformed')
  }
  const temporary = path.join(os.tmpdir(), `kp-physical-audit-${randomBytes(6).toString('hex')}`)
  const needles = path.join(temporary, 'needles')
  mkdirSync(temporary, { mode: 0o700 })
  writeFileSync(needles, `${plaintext.join('\n')}\n`, { mode: 0o644 })
  chmodSync(needles, 0o644)
  try {
    const result = spawnSync('docker', [
      'run', '--rm', '--network', 'none', '--read-only',
      '--user', '70:70', '--cap-drop', 'ALL', '--security-opt', 'no-new-privileges:true',
      '--volume', `${volume}:/var/lib/postgresql:ro`,
      '--volume', `${needles}:/run/needles:ro`,
      '--entrypoint', '/bin/sh', postgresImage, '-ec',
      'set +e; grep -a -r -F -f /run/needles /var/lib/postgresql >/dev/null 2>&1; status=$?; set -e; if [ "$status" -eq 0 ]; then exit 42; fi; if [ "$status" -eq 1 ]; then exit 0; fi; exit "$status"',
    ], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
      timeout: 5 * 60_000,
    })
    if (result.error) throw new Error(`physical PostgreSQL storage scan could not run: ${result.error.name}`)
    if (result.status === 42) throw new Error('physical PostgreSQL storage contains forbidden plaintext')
    if (result.status !== 0) throw new Error(`physical PostgreSQL storage scan failed with exit code ${result.status ?? 'unknown'}`)
  } finally {
    rmSync(temporary, { recursive: true, force: true })
  }
}
