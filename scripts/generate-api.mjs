import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'
import openapiTS, { astToString } from 'openapi-typescript'

const root = resolve(import.meta.dirname, '..')
const raw = execFileSync(
  'cargo',
  ['run', '--quiet', '-p', 'knowledge-server', '--bin', 'openapi'],
  { cwd: root, encoding: 'utf8', windowsHide: true },
)
const document = JSON.parse(raw)
const ast = await openapiTS(document, {
  alphabetize: true,
  exportType: true,
  immutable: true,
})
const output = `${astToString(ast).replaceAll('\r\n', '\n').trimEnd()}\n`
const target = resolve(root, 'apps/web/src/api/generated.ts')

if (process.argv.includes('--check')) {
  const current = readFileSync(target, 'utf8').replaceAll('\r\n', '\n')
  if (current !== output) {
    console.error('Generated API client is stale. Run `pnpm api:generate`.')
    process.exitCode = 1
  }
} else {
  mkdirSync(dirname(target), { recursive: true })
  writeFileSync(target, output, 'utf8')
}
