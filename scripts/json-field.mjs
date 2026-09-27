// Restore-drill JSON only; never include the input (which can be confidential)
// in diagnostics. Standard input keeps fields out of process arguments.
import { readFileSync } from 'node:fs'
try {
  const value=JSON.parse(readFileSync(0,'utf8'))[process.argv[2]]
  if (typeof value !== 'string') throw new Error('missing field')
  process.stdout.write(value)
} catch {
  process.stderr.write('JSON field is missing or invalid\n')
  process.exitCode=1
}
