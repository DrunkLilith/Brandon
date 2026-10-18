import { spawnSync } from 'node:child_process'
import { pathToFileURL } from 'node:url'

const approved = new Set(['MIT'])
// Exact package/version approvals, not a new blanket license allowance.
// See docs/dependency-licenses.md; new versions must be reviewed again.
const packageApprovals = new Map([
  ['@tauri-apps/api', { version: '2.12.3', license: 'Apache-2.0 OR MIT' }],
  ['@tauri-apps/plugin-dialog', { version: '2.8.2', license: 'MIT OR Apache-2.0' }],
  ['@ungap/structured-clone', { version: '1.4.0', license: 'ISC' }],
])
const scopedLicenses = new Set([...packageApprovals.values()].map(entry => entry.license))

export function validateLicenseReport(report) {
  if (!report || typeof report !== 'object' || Array.isArray(report)) throw new Error('license report is malformed')
  const groups = Object.entries(report)
  if (groups.length === 0) throw new Error('license report contains no production dependencies')
  for (const [groupLicense, dependencies] of groups) {
    if (!approved.has(groupLicense) && !scopedLicenses.has(groupLicense)) throw new Error(`production license is not approved: ${groupLicense || '<missing>'}`)
    if (!Array.isArray(dependencies) || dependencies.length === 0) throw new Error(`license group is malformed: ${groupLicense}`)
    for (const dependency of dependencies) {
      if (
        !dependency
        || typeof dependency !== 'object'
        || Array.isArray(dependency)
        || typeof dependency.name !== 'string'
        || dependency.name.trim().length === 0
        || !Array.isArray(dependency.versions)
        || dependency.versions.length === 0
        || dependency.versions.some((version) => typeof version !== 'string' || version.trim().length === 0)
      ) throw new Error('license entry is malformed')
      if (typeof dependency.license !== 'string' || dependency.license.length === 0) throw new Error(`dependency ${dependency.name} has a missing license`)
      if (dependency.license !== groupLicense) throw new Error(`dependency ${dependency.name} license does not match its group`)
      const exception = packageApprovals.get(dependency.name)
      const approvedException = exception?.license === dependency.license
        && dependency.versions.every(version => version === exception.version)
      if (!approved.has(dependency.license) && !approvedException) throw new Error(`production license is not approved: ${dependency.license} for ${dependency.name}`)
    }
  }
}

function main() {
  const result = process.platform === 'win32'
    ? spawnSync(process.env.ComSpec ?? 'cmd.exe', ['/d', '/s', '/c', 'pnpm licenses list --prod --json'], { encoding: 'utf8' })
    : spawnSync('pnpm', ['licenses', 'list', '--prod', '--json'], { encoding: 'utf8' })
  if (result.error || result.status !== 0) throw new Error(`pnpm license inventory failed with exit code ${result.status ?? 'unknown'}`)
  let report
  try { report = JSON.parse(result.stdout) } catch { throw new Error('pnpm license inventory was not valid JSON') }
  validateLicenseReport(report)
  process.stdout.write('Production npm license policy PASS\n')
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  try { main() } catch (error) {
    process.stderr.write(`Production npm license policy FAILED: ${error instanceof Error ? error.message : 'unknown error'}\n`)
    process.exitCode = 1
  }
}
