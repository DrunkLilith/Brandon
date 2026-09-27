import assert from 'node:assert/strict'
import { execFileSync, spawnSync } from 'node:child_process'
import { randomBytes } from 'node:crypto'
import { chmodSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import test from 'node:test'
import { pathToFileURL } from 'node:url'

import {
  assertDeploymentResourcesUnused,
  cleanupDeployment,
  resolveComposeResourceNames,
  startDeployment,
  type RuntimeState,
} from './deployment.ts'

const alpine = 'alpine:3.22.1@sha256:4bcff63911fcb4448bd4fdacec207030997caf25e9bea4045fa6c8c44de311d1'

function fakeDocker() {
  const root = path.join(os.tmpdir(), `kp-e2e-native-${randomBytes(6).toString('hex')}`)
  const log = path.join(root, 'calls.jsonl')
  const implementation = path.join(root, 'docker.mjs')
  const owner = path.join(root, 'owner')
  mkdirSync(root, { recursive: true })
  writeFileSync(implementation, `
    import { appendFileSync, existsSync, readFileSync, unlinkSync, writeFileSync } from 'node:fs'
    const args = process.argv.slice(2)
    appendFileSync(process.env.KP_FAKE_DOCKER_LOG, JSON.stringify(args) + '\\n')
    if (args[0] === 'ps') process.exit(0)
    if (args[0] === 'volume' && args[1] === 'inspect') {
      if (process.env.KP_FAKE_DOCKER_COLLISION === '1' && String(args.at(-1)).endsWith('_postgres_data')) {
        process.stdout.write('[{}]')
        process.exit(0)
      }
      if (String(args.at(-1)).endsWith('_e2e_owner') && existsSync(process.env.KP_FAKE_DOCKER_OWNER)) {
        if (args.includes('--format')) process.stdout.write(readFileSync(process.env.KP_FAKE_DOCKER_OWNER, 'utf8'))
        else process.stdout.write('[{}]')
        process.exit(0)
      }
      process.exit(1)
    }
    if (args[0] === 'volume' && args[1] === 'create') {
      const label = args[args.indexOf('--label') + 1]
      writeFileSync(process.env.KP_FAKE_DOCKER_OWNER, label.split('=')[1])
      process.stdout.write(String(args.at(-1)))
      process.exit(0)
    }
    if (args[0] === 'volume' && args[1] === 'rm') {
      if (existsSync(process.env.KP_FAKE_DOCKER_OWNER)) unlinkSync(process.env.KP_FAKE_DOCKER_OWNER)
      process.exit(0)
    }
    if (args[0] === 'network' && args[1] === 'inspect') process.exit(1)
    if (args[0] === 'compose' && args.includes('config') && args.includes('--format')) {
      if (process.env.KP_FAKE_DOCKER_BAD_CONFIG === '1') {
        process.stdout.write('{}')
        process.exit(0)
      }
      const project = args[args.indexOf('-p') + 1]
      process.stdout.write(JSON.stringify({
        networks: { storage: { name: project + '_storage' } },
        volumes: { postgres_data: { name: project + '_postgres_data' } },
      }))
      process.exit(0)
    }
    if (args[0] === 'compose' && args.includes('config') && args.includes('--quiet') && process.env.KP_FAKE_DOCKER_PORT_COLLISION === '1') {
      process.stderr.write('Bind for 127.0.0.1:49152 failed: port is already allocated\\n')
      process.exit(1)
    }
    if (args[0] === 'compose' && args.includes('build') && process.env.KP_FAKE_DOCKER_LARGE_BUILD === '1') {
      process.stdout.write('x'.repeat(2 * 1024 * 1024 + 17))
      process.exit(0)
    }
    if (args[0] === 'compose' && args.includes('up') && process.env.KP_FAKE_DOCKER_LARGE_BUILD === '1') process.exit(2)
    process.exit(0)
  `)
  return { root, log, owner, executable: process.execPath, arguments: [implementation] }
}

function calls(log: string) {
  return readFileSync(log, 'utf8').trim().split(/\r?\n/).filter(Boolean).map((line) => JSON.parse(line) as string[])
}

test('preflight collision never invokes compose down for a foreign project', async () => {
  const fake = fakeDocker()
  const previous = { log: process.env.KP_FAKE_DOCKER_LOG, owner: process.env.KP_FAKE_DOCKER_OWNER, collision: process.env.KP_FAKE_DOCKER_COLLISION }
  process.env.KP_FAKE_DOCKER_LOG = fake.log
  process.env.KP_FAKE_DOCKER_OWNER = fake.owner
  process.env.KP_FAKE_DOCKER_COLLISION = '1'
  try {
    await assert.rejects(startDeployment({ dockerExecutable: fake.executable, dockerArguments: fake.arguments }), /refusing to reuse Compose volume/)
    assert.equal(calls(fake.log).filter((args) => args[0] === 'compose' && args.includes('down')).length, 0)
  } finally {
    if (previous.log == null) delete process.env.KP_FAKE_DOCKER_LOG
    else process.env.KP_FAKE_DOCKER_LOG = previous.log
    if (previous.owner == null) delete process.env.KP_FAKE_DOCKER_OWNER
    else process.env.KP_FAKE_DOCKER_OWNER = previous.owner
    if (previous.collision == null) delete process.env.KP_FAKE_DOCKER_COLLISION
    else process.env.KP_FAKE_DOCKER_COLLISION = previous.collision
    rmSync(fake.root, { recursive: true, force: true })
  }
})

test('malformed resolved Compose resources leave no attempt directory or destructive cleanup', async () => {
  const fake = fakeDocker()
  const previous = { log: process.env.KP_FAKE_DOCKER_LOG, owner: process.env.KP_FAKE_DOCKER_OWNER, bad: process.env.KP_FAKE_DOCKER_BAD_CONFIG }
  process.env.KP_FAKE_DOCKER_LOG = fake.log
  process.env.KP_FAKE_DOCKER_OWNER = fake.owner
  process.env.KP_FAKE_DOCKER_BAD_CONFIG = '1'
  let attemptDirectory = ''
  try {
    await assert.rejects(startDeployment({ dockerExecutable: fake.executable, dockerArguments: fake.arguments }), /malformed volume configuration/)
    const configCall = calls(fake.log).find((args) => args[0] === 'compose' && args.includes('--format'))
    assert.ok(configCall, 'Compose config was not invoked')
    attemptDirectory = path.join(os.tmpdir(), configCall[configCall.indexOf('-p') + 1])
    assert.equal(calls(fake.log).filter((args) => args[0] === 'compose' && args.includes('down')).length, 0)
    assert.equal(readFileSync(fake.log, 'utf8').includes('knowledge-platform.e2e-owner='), false)
    assert.equal(existsSync(attemptDirectory), false)
  } finally {
    if (previous.log == null) delete process.env.KP_FAKE_DOCKER_LOG
    else process.env.KP_FAKE_DOCKER_LOG = previous.log
    if (previous.owner == null) delete process.env.KP_FAKE_DOCKER_OWNER
    else process.env.KP_FAKE_DOCKER_OWNER = previous.owner
    if (previous.bad == null) delete process.env.KP_FAKE_DOCKER_BAD_CONFIG
    else process.env.KP_FAKE_DOCKER_BAD_CONFIG = previous.bad
    if (attemptDirectory) rmSync(attemptDirectory, { recursive: true, force: true })
    rmSync(fake.root, { recursive: true, force: true })
  }
})

test('native port-bind stderr triggers exactly three bounded attempts', async () => {
  const fake = fakeDocker()
  const previous = { log: process.env.KP_FAKE_DOCKER_LOG, owner: process.env.KP_FAKE_DOCKER_OWNER, collision: process.env.KP_FAKE_DOCKER_PORT_COLLISION }
  process.env.KP_FAKE_DOCKER_LOG = fake.log
  process.env.KP_FAKE_DOCKER_OWNER = fake.owner
  process.env.KP_FAKE_DOCKER_PORT_COLLISION = '1'
  try {
    await assert.rejects(startDeployment({ dockerExecutable: fake.executable, dockerArguments: fake.arguments }), /host port is already allocated/)
    assert.equal(calls(fake.log).filter((args) => args[0] === 'compose' && args.includes('config') && args.includes('--quiet')).length, 3)
  } finally {
    if (previous.log == null) delete process.env.KP_FAKE_DOCKER_LOG
    else process.env.KP_FAKE_DOCKER_LOG = previous.log
    if (previous.owner == null) delete process.env.KP_FAKE_DOCKER_OWNER
    else process.env.KP_FAKE_DOCKER_OWNER = previous.owner
    if (previous.collision == null) delete process.env.KP_FAKE_DOCKER_PORT_COLLISION
    else process.env.KP_FAKE_DOCKER_PORT_COLLISION = previous.collision
    rmSync(fake.root, { recursive: true, force: true })
  }
})

test('verbose Compose output larger than two MiB is not globally captured', () => {
  const fake = fakeDocker()
  const runner = `
    const { startDeployment } = await import(${JSON.stringify(pathToFileURL(path.join(process.cwd(), 'tests/e2e/support/deployment.ts')).href)})
    try {
      await startDeployment({ dockerExecutable: ${JSON.stringify(fake.executable)}, dockerArguments: ${JSON.stringify(fake.arguments)} })
      process.exit(9)
    } catch (error) {
      if (!String(error).includes('exit code 2')) {
        process.stderr.write(String(error))
        process.exit(8)
      }
    }
  `
  try {
    const result = spawnSync(process.execPath, ['--experimental-strip-types', '--input-type=module', '--eval', runner], {
      cwd: process.cwd(),
      env: {
        ...process.env,
        KP_FAKE_DOCKER_LOG: fake.log,
        KP_FAKE_DOCKER_OWNER: fake.owner,
        KP_FAKE_DOCKER_LARGE_BUILD: '1',
      },
      encoding: 'utf8',
      maxBuffer: 4 * 1024 * 1024,
    })
    assert.equal(result.status, 0, result.stderr)
    assert.ok(result.stdout.length > 2 * 1024 * 1024, 'large child output was not streamed through the deployment runner')
  } finally {
    rmSync(fake.root, { recursive: true, force: true })
  }
})

test('resolved foreign Compose network survives preflight and unowned cleanup', () => {
  const suffix = randomBytes(6).toString('hex')
  const project = `kp-e2e-${suffix}`
  const temporary = path.join(os.tmpdir(), project)
  const marker = randomBytes(16).toString('hex')
  mkdirSync(temporary, { mode: 0o700 })
  const secret = (name: string) => {
    const target = path.join(temporary, name)
    writeFileSync(target, 'test-only', { mode: 0o644 })
    chmodSync(target, 0o644)
    return target
  }
  const state = {
    project,
    tempDirectory: temporary,
    baseUrl: 'http://127.0.0.1',
    adminEmail: 'unused@example.test',
    adminPassword: 'unused',
    composeEnvironment: {
      KNOWLEDGE_MASTER_KEY_FILE: secret('master_key'),
      KNOWLEDGE_DATABASE_PASSWORD_FILE: secret('database_password'),
      KNOWLEDGE_OBJECT_ACCESS_KEY_FILE: secret('object_access_key'),
      KNOWLEDGE_OBJECT_SECRET_KEY_FILE: secret('object_secret_key'),
      KNOWLEDGE_SITE_ADDRESS: 'http://localhost',
      KNOWLEDGE_PUBLIC_ORIGIN: 'http://localhost',
    },
    expectedVolumes: [],
    expectedNetworks: [],
  } as RuntimeState
  let network = ''
  try {
    const resources = resolveComposeResourceNames(state)
    assert.ok(resources.networks.includes(`${project}_storage`), 'actual Compose storage network name was not resolved')
    state.expectedVolumes = resources.volumes
    state.expectedNetworks = resources.networks
    network = `${project}_storage`
    execFileSync('docker', ['network', 'create', '--label', `knowledge-platform.foreign-marker=${marker}`, network])
    assert.throws(() => assertDeploymentResourcesUnused(state), /refusing to reuse Compose network/)
    cleanupDeployment(state)
    const identity = execFileSync('docker', [
      'network', 'inspect', '--format', '{{.Id}}|{{ index .Labels "knowledge-platform.foreign-marker" }}', network,
    ], { encoding: 'utf8' }).trim()
    assert.match(identity, new RegExp(`^[0-9a-f]+\\|${marker}$`))
  } finally {
    if (network) execFileSync('docker', ['network', 'rm', network], { stdio: 'ignore' })
    rmSync(temporary, { recursive: true, force: true })
  }
})

test('cleanup without a matching ownership marker preserves a foreign Compose volume and bytes', () => {
  const suffix = randomBytes(6).toString('hex')
  const project = `kp-e2e-${suffix}`
  const volume = `${project}_postgres_data`
  const temporary = path.join(os.tmpdir(), project)
  mkdirSync(temporary, { mode: 0o700 })
  for (const name of ['master_key', 'database_password', 'object_access_key', 'object_secret_key']) {
    writeFileSync(path.join(temporary, name), 'test-only', { mode: 0o600 })
  }
  execFileSync('docker', ['volume', 'create',
    '--label', `com.docker.compose.project=${project}`,
    '--label', 'com.docker.compose.volume=postgres_data', volume,
  ])
  execFileSync('docker', ['run', '--rm', '--volume', `${volume}:/data`, alpine, '/bin/sh', '-ec', 'printf preserved >/data/marker'])
  const state = {
    project,
    tempDirectory: temporary,
    baseUrl: 'http://127.0.0.1',
    adminEmail: 'unused@example.test',
    adminPassword: 'unused',
    composeEnvironment: {
      KNOWLEDGE_MASTER_KEY_FILE: path.join(temporary, 'master_key'),
      KNOWLEDGE_DATABASE_PASSWORD_FILE: path.join(temporary, 'database_password'),
      KNOWLEDGE_OBJECT_ACCESS_KEY_FILE: path.join(temporary, 'object_access_key'),
      KNOWLEDGE_OBJECT_SECRET_KEY_FILE: path.join(temporary, 'object_secret_key'),
      KNOWLEDGE_SITE_ADDRESS: 'http://localhost', KNOWLEDGE_PUBLIC_ORIGIN: 'http://localhost',
    },
    expectedVolumes: [volume],
    expectedNetworks: [],
  } as RuntimeState
  try {
    cleanupDeployment(state)
    execFileSync('docker', ['volume', 'inspect', volume])
    const marker = execFileSync('docker', ['run', '--rm', '--volume', `${volume}:/data:ro`, alpine, 'cat', '/data/marker'], { encoding: 'utf8' })
    assert.equal(marker, 'preserved')
  } finally {
    execFileSync('docker', ['volume', 'rm', '--force', volume], { stdio: 'ignore' })
    rmSync(temporary, { recursive: true, force: true })
  }
})
