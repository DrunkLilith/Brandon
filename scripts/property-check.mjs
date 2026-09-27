import { spawnSync } from 'node:child_process'

// Fixed seed 0x51eed; 128 AEAD mutations, 64 real database tree operations,
// 32 hostile Markdown documents. DATABASE_URL must point at an isolated DB.
for (const [command, args] of [
  ['cargo', ['test', '-p', 'knowledge-crypto', '--test', 'bounded_properties']],
  ['cargo', ['test', '-p', 'knowledge-server', '--test', 'api_pages', 'bounded_properties']],
  ['pnpm', ['--filter', '@knowledge/web', 'test', '--run', 'src/editor/bounded-properties.test.tsx']],
]) {
  // Only static, repository-owned commands; no interpolated user input.
  const result = process.platform === 'win32' && command === 'pnpm'
    ? spawnSync(process.env.ComSpec ?? 'cmd.exe', ['/d', '/s', '/c', [command, ...args].join(' ')], { stdio:'inherit' })
    : spawnSync(command, args, { stdio: 'inherit' })
  if (result.error || result.status !== 0) process.exit(result.status || 1)
}
