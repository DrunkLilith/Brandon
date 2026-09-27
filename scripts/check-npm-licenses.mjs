import { spawnSync } from 'node:child_process'
import { pathToFileURL } from 'node:url'

const approved = new Set(['MIT'])

export function validateLicenseReport(report) {
  if (!report || typeof report !== 'object' || Array.isArray(report)) throw new Error('license report is malformed')
  const groups = Object.entries(report)
  if (groups.length === 0) throw new Error('license report contains no production dependencies')
  for (const [groupLicense, dependencies] of groups) {
    if (!approved.has(groupLicense)) throw new Error(`production license is not approved: ${groupLicense || '<missing>'}`)
    if (!Array.isArray(dependencies) || dependencies.length === 0) throw new Error(`license group is malformed: ${groupLicense}`)
    for (const dependency of dependencies) {
      if (!dependency || typeof dependency !== 'object' || typeof dependency.name !== 'string') throw new Error('license entry is malformed')
      if (typeof dependency.license !== 'string' || dependency.license.length === 0) throw new Error(`dependency ${dependency.name} has a missing license`)
      if (dependency.license !== groupLicense) throw new Error(`dependency ${dependency.name} license does not match its group`)
      if (!approved.has(dependency.license)) throw new Error(`production license is not approved: ${dependency.license}`)
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
