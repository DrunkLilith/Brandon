import { randomBytes, randomUUID } from 'node:crypto'
import { execFileSync, spawnSync } from 'node:child_process'
import { chmodSync, existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { createServer } from 'node:net'
import os from 'node:os'
import path from 'node:path'
import type { BrowserContext } from '@playwright/test'

export interface RuntimeState {
  project: string
  tempDirectory: string
  baseUrl: string
  adminEmail: string
  adminPassword: string
  composeEnvironment: Record<string, string>
  expectedVolumes: string[]
  ownershipVolume?: string
  ownershipToken?: string
  dockerExecutable?: string
  dockerArguments?: string[]
  seededUsers?: {
    admin: SeededUser
    editor: SeededUser
    viewer: SeededUser
  }
}

export interface StartDeploymentOptions {
  dockerExecutable?: string
  dockerArguments?: string[]
}

export interface SeededUser {
  id: string
  email: string
  storageState: Awaited<ReturnType<BrowserContext['storageState']>>
}

const STATE_ENV = 'KP_E2E_STATE_FILE'
const projectPattern = /^kp-e2e-[a-f0-9]{12}$/

function repoRoot() {
  return path.resolve(process.cwd())
}

function composeFile() {
  return path.join(repoRoot(), 'deploy', 'compose.yaml')
}

function compose(state: RuntimeState, args: string[], inherit = true) {
  const result = spawnSync(state.dockerExecutable ?? 'docker', [
    ...(state.dockerArguments ?? []), 'compose', '-f', composeFile(), '-p', state.project, ...args,
  ], {
    cwd: repoRoot(),
    env: { ...process.env, ...state.composeEnvironment },
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 30 * 60_000,
  })
  if (result.error) throw new Error(`docker compose could not start: ${result.error.name}`)
  if (result.status !== 0) {
    const nativeError = result.stderr ?? ''
    if (/address already in use|port is already allocated|bind for .* failed/i.test(nativeError)) {
      throw new Error('docker compose failed: host port is already allocated')
    }
    throw new Error(`docker compose failed with exit code ${result.status ?? 'unknown'}`)
  }
  if (inherit) {
    if (result.stdout) process.stdout.write(result.stdout)
    if (result.stderr) process.stderr.write(result.stderr)
  }
  return result.stdout ?? ''
}

function docker(state: RuntimeState, args: string[]) {
  return execFileSync(state.dockerExecutable ?? 'docker', [...(state.dockerArguments ?? []), ...args], {
    cwd: repoRoot(),
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
    timeout: 60_000,
  })
}

async function freePort() {
  return await new Promise<number>((resolve, reject) => {
    const server = createServer()
    server.once('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (!address || typeof address === 'string') {
        server.close()
        reject(new Error('could not allocate a loopback port'))
        return
      }
      const port = address.port
      server.close((error) => error ? reject(error) : resolve(port))
    })
  })
}

function writeSecret(directory: string, name: string, value: string) {
  const target = path.join(directory, name)
  writeFileSync(target, value, { encoding: 'utf8', mode: 0o644 })
  chmodSync(target, 0o644)
  return target
}

function assertUnused(state: RuntimeState) {
  const containers = docker(state, ['ps', '-a', '--filter', `label=com.docker.compose.project=${state.project}`, '--format', '{{.ID}}']).trim()
  if (containers) throw new Error(`refusing to reuse Compose project ${state.project}`)
  for (const volume of [...state.expectedVolumes, state.ownershipVolume!]) {
    try {
      docker(state, ['volume', 'inspect', volume])
      throw new Error(`refusing to reuse Compose volume ${volume}`)
    } catch (error) {
      if (error instanceof Error && error.message.startsWith('refusing')) throw error
    }
  }
}

function claimOwnership(state: RuntimeState) {
  if (!state.ownershipVolume || !state.ownershipToken) throw new Error('missing E2E ownership marker')
  docker(state, ['volume', 'create', '--label', `knowledge-platform.e2e-owner=${state.ownershipToken}`, state.ownershipVolume])
  const actual = docker(state, [
    'volume', 'inspect', '--format', '{{ index .Labels "knowledge-platform.e2e-owner" }}', state.ownershipVolume,
  ]).trim()
  if (actual !== state.ownershipToken) throw new Error('could not prove E2E resource ownership')
}

function cleanupTempDirectory(state: RuntimeState) {
  const expected = path.join(os.tmpdir(), state.project)
  if (path.resolve(state.tempDirectory) !== path.resolve(expected)) throw new Error('refusing cleanup outside the E2E temporary root')
  rmSync(state.tempDirectory, { recursive: true, force: true })
}

async function waitUntilReady(baseUrl: string) {
  const deadline = Date.now() + 8 * 60_000
  let lastFailure: unknown
  while (Date.now() < deadline) {
    try {
      const response = await fetch(`${baseUrl}/api/v1/health/ready`, { signal: AbortSignal.timeout(5_000) })
      if (response.ok && (await response.json() as { status?: string }).status === 'ready') return
      lastFailure = new Error(`readiness returned ${response.status}`)
    } catch (error) {
      lastFailure = error
    }
    await new Promise((resolve) => setTimeout(resolve, 1_000))
  }
  throw new Error(`production deployment did not become ready: ${String(lastFailure)}`)
}

async function startDeploymentAttempt(options: StartDeploymentOptions): Promise<RuntimeState> {
  const suffix = randomBytes(6).toString('hex')
  const project = `kp-e2e-${suffix}`
  if (!projectPattern.test(project)) throw new Error('generated unsafe E2E project name')
  const tempDirectory = path.join(os.tmpdir(), project)
  if (existsSync(tempDirectory)) throw new Error(`refusing to reuse E2E directory ${tempDirectory}`)
  mkdirSync(tempDirectory, { recursive: false, mode: 0o700 })
  chmodSync(tempDirectory, 0o700)

  const httpPort = await freePort()
  let httpsPort = await freePort()
  while (httpsPort === httpPort) httpsPort = await freePort()
  const adminPassword = `E2E-${randomUUID()}-Aa1!`
  const masterKey = randomBytes(32).toString('base64')
  const composeEnvironment = {
    KNOWLEDGE_MASTER_KEY_FILE: writeSecret(tempDirectory, 'master_key', masterKey),
    KNOWLEDGE_DATABASE_PASSWORD_FILE: writeSecret(tempDirectory, 'database_password', `db-${randomUUID()}`),
    KNOWLEDGE_OBJECT_ACCESS_KEY_FILE: writeSecret(tempDirectory, 'object_access_key', `kp${suffix}`),
    KNOWLEDGE_OBJECT_SECRET_KEY_FILE: writeSecret(tempDirectory, 'object_secret_key', `obj-${randomUUID()}`),
    KNOWLEDGE_SITE_ADDRESS: 'http://localhost',
    KNOWLEDGE_PUBLIC_ORIGIN: `http://localhost:${httpPort}`,
    KNOWLEDGE_HTTP_PORT: String(httpPort),
    KNOWLEDGE_HTTPS_PORT: String(httpsPort),
  }
  const state: RuntimeState = {
    project,
    tempDirectory,
    baseUrl: `http://localhost:${httpPort}`,
    adminEmail: `admin-${suffix}@example.test`,
    adminPassword,
    composeEnvironment,
    expectedVolumes: ['postgres_data', 'minio_data', 'caddy_data', 'caddy_config'].map((name) => `${project}_${name}`),
    ownershipVolume: `${project}_e2e_owner`,
    ownershipToken: randomBytes(24).toString('hex'),
    ...(options.dockerExecutable ? { dockerExecutable: options.dockerExecutable } : {}),
    ...(options.dockerArguments ? { dockerArguments: options.dockerArguments } : {}),
  }
  const stateFile = path.join(tempDirectory, 'runtime.json')
  writeFileSync(stateFile, JSON.stringify(state), { encoding: 'utf8', mode: 0o600 })
  chmodSync(stateFile, 0o600)
  process.env[STATE_ENV] = stateFile

  try {
    assertUnused(state)
  } catch (error) {
    cleanupTempDirectory(state)
    throw error
  }

  try {
    claimOwnership(state)
    compose(state, ['config', '--quiet'])
    compose(state, ['build', 'backend', 'web', 'minio', 'minio-init'])
    compose(state, ['up', '--detach', 'postgres', 'minio'])
    compose(state, ['run', '--rm', '-T', 'minio-init'])
    compose(state, ['run', '--rm', '-T', 'migrate'])
    const bootstrapPasswordFile = writeSecret(tempDirectory, 'bootstrap_password', adminPassword)
    compose(state, [
      '--profile', 'tools', 'run', '--rm', '-T',
      '--volume', `${bootstrapPasswordFile}:/run/secrets/bootstrap_password:ro`,
      '--env', 'KNOWLEDGE_BOOTSTRAP_PASSWORD_FILE=/run/secrets/bootstrap_password',
      'bootstrap', '--email', state.adminEmail, '--workspace-name', 'E2E Workspace', '--display-name', 'E2E Administrator',
    ])
    compose(state, ['up', '--detach', 'backend', 'web', 'caddy'])
    await waitUntilReady(state.baseUrl)
    return state
  } catch (error) {
    cleanupDeployment(state)
    throw error
  }
}

export async function startDeployment(options: StartDeploymentOptions = {}): Promise<RuntimeState> {
  let lastError: unknown
  for (let attempt = 1; attempt <= 3; attempt += 1) {
    try {
      return await startDeploymentAttempt(options)
    } catch (error) {
      lastError = error
      if (!String(error).includes('host port is already allocated') || attempt === 3) throw error
    }
  }
  throw lastError
}

export function readRuntimeState() {
  const stateFile = process.env[STATE_ENV]
  if (!stateFile) throw new Error(`${STATE_ENV} was not provided by global setup`)
  return JSON.parse(readFileSync(stateFile, 'utf8')) as RuntimeState
}

export function readRuntimeStateIfPresent() {
  const stateFile = process.env[STATE_ENV]
  if (!stateFile || !existsSync(stateFile)) return undefined
  return JSON.parse(readFileSync(stateFile, 'utf8')) as RuntimeState
}

export function writeRuntimeState(state: RuntimeState) {
  const stateFile = process.env[STATE_ENV]
  if (!stateFile || path.dirname(path.resolve(stateFile)) !== path.resolve(state.tempDirectory)) {
    throw new Error('refusing to write E2E state outside its temporary directory')
  }
  writeFileSync(stateFile, JSON.stringify(state), { encoding: 'utf8', mode: 0o600 })
  chmodSync(stateFile, 0o600)
}

export function cleanupDeployment(state: RuntimeState) {
  if (!projectPattern.test(state.project)) throw new Error('refusing cleanup for unsafe project name')
  let ownsResources = false
  try {
    if (state.ownershipVolume && state.ownershipToken) {
      const actual = docker(state, [
        'volume', 'inspect', '--format', '{{ index .Labels "knowledge-platform.e2e-owner" }}', state.ownershipVolume,
      ]).trim()
      ownsResources = actual === state.ownershipToken
    }
  } catch {
    ownsResources = false
  }
  try {
    if (ownsResources) {
      compose(state, ['down', '--remove-orphans', '--volumes'], false)
      docker(state, ['volume', 'rm', '--force', state.ownershipVolume!])
    }
  } finally {
    cleanupTempDirectory(state)
  }
}
