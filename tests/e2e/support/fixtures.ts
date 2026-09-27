import { expect, request as playwrightRequest, type APIRequestContext } from '@playwright/test'

import { endpoint } from './api'
import { readRuntimeState } from './deployment'

export const memberPassword = 'Correct horse battery staple 42!'

export interface ApiSession {
  context: APIRequestContext
  csrf: string
}

export interface SeededWorkspace {
  admin: ApiSession
  editor: ApiSession
  viewer: ApiSession
  editorId: string
  viewerId: string
  groupId: string
  spaceId: string
}

async function json<T>(response: Awaited<ReturnType<APIRequestContext['get']>>, status: number): Promise<T> {
  expect(response.status(), await response.text()).toBe(status)
  return await response.json() as T
}

export async function apiSession(email: string, password: string): Promise<ApiSession> {
  const runtime = readRuntimeState()
  const context = await playwrightRequest.newContext({
    baseURL: runtime.baseUrl,
    extraHTTPHeaders: { Origin: runtime.baseUrl },
  })
  const login = await context.post('/api/v1/auth/login', { data: { email, password } })
  expect(login.status(), await login.text()).toBe(200)
  const csrfResponse = await context.get('/api/v1/auth/csrf')
  const csrf = await json<{ token: string }>(csrfResponse, 200)
  return { context, csrf: csrf.token }
}

export async function mutate(
  session: ApiSession,
  method: 'post' | 'put' | 'patch' | 'delete',
  path: string,
  options: Parameters<APIRequestContext[typeof method]>[1] = {},
) {
  return await session.context[method](path, {
    ...options,
    headers: { 'X-CSRF-Token': session.csrf, ...(options.headers ?? {}) },
  })
}

export async function createMember(admin: ApiSession, label: string) {
  const suffix = crypto.randomUUID()
  const email = `${label}-${suffix}@example.test`
  const invitation = await mutate(admin, 'post', '/api/v1/invitations', {
    headers: { 'Idempotency-Key': crypto.randomUUID() },
    data: { email, workspaceRole: 'member', expiresInHours: 24 },
  })
  const created = await json<{ token: string }>(invitation, 201)
  const anonymous = await playwrightRequest.newContext({
    baseURL: readRuntimeState().baseUrl,
    extraHTTPHeaders: { Origin: readRuntimeState().baseUrl },
  })
  try {
    const accepted = await anonymous.post(`/api/v1/invitations/${created.token}/accept`, {
      data: { displayName: label, password: memberPassword },
    })
    const user = await json<{ id: string }>(accepted, 201)
    return { id: user.id, email, password: memberPassword }
  } finally {
    await anonymous.dispose()
  }
}

export async function createGroup(admin: ApiSession, name = `group-${crypto.randomUUID()}`) {
  const response = await mutate(admin, 'post', '/api/v1/groups', { data: { name, description: 'E2E group' } })
  return await json<{ id: string; name: string }>(response, 201)
}

export async function addGroupMember(admin: ApiSession, groupId: string, userId: string) {
  const response = await mutate(admin, 'put', `/api/v1/groups/${groupId}/members/${userId}`)
  expect(response.status(), await response.text()).toBe(204)
}

export async function createSpace(admin: ApiSession, name = `space-${crypto.randomUUID()}`) {
  const response = await mutate(admin, 'post', '/api/v1/spaces', { data: { name, description: 'E2E space' } })
  return await json<{ id: string; name: string }>(response, 201)
}

export async function grantSpace(admin: ApiSession, spaceId: string, subjectType: 'user' | 'group', subjectId: string, role: 'owner' | 'editor' | 'viewer') {
  const response = await mutate(admin, 'post', `/api/v1/spaces/${spaceId}/grants`, { data: { subjectType, subjectId, role } })
  expect(response.status(), await response.text()).toBe(201)
}

export async function createPage(
  session: ApiSession,
  spaceId: string,
  title: string,
  markdown: string,
  options: { parentId?: string | null; restricted?: boolean } = {},
) {
  const response = await mutate(session, 'post', '/api/v1/pages', {
    data: {
      spaceId,
      parentId: options.parentId ?? null,
      title,
      markdown,
      properties: {},
      restricted: options.restricted ?? false,
    },
  })
  return await json<{ id: string; version: number; stableKey: string }>(response, 201)
}

export async function updatePage(session: ApiSession, pageId: string, expectedVersion: number, title: string, markdown: string, restricted = false) {
  return await mutate(session, 'patch', `/api/v1/pages/${pageId}`, {
    data: { expectedVersion, title, markdown, properties: {}, restricted },
  })
}

export async function pagePermission(admin: ApiSession, pageId: string, subjectType: 'user' | 'group', subjectId: string, role: 'editor' | 'viewer') {
  const response = await mutate(admin, 'put', `/api/v1/pages/${pageId}/permissions/${subjectType}:${subjectId}`, { data: { role } })
  expect(response.status(), await response.text()).toBe(200)
}

export async function seedWorkspace(): Promise<SeededWorkspace> {
  const runtime = readRuntimeState()
  if (!runtime.seededUsers) throw new Error('auth dependency did not persist reusable browser sessions')
  const fromSeed = async (role: keyof typeof runtime.seededUsers): Promise<ApiSession> => {
    const context = await playwrightRequest.newContext({
      baseURL: runtime.baseUrl,
      extraHTTPHeaders: { Origin: runtime.baseUrl },
      storageState: runtime.seededUsers![role].storageState,
    })
    const csrfResponse = await context.get('/api/v1/auth/csrf')
    const csrfValue = await json<{ token: string }>(csrfResponse, 200)
    return { context, csrf: csrfValue.token }
  }
  const admin = await fromSeed('admin')
  const group = await createGroup(admin)
  await addGroupMember(admin, group.id, runtime.seededUsers.editor.id)
  const space = await createSpace(admin)
  await grantSpace(admin, space.id, 'group', group.id, 'editor')
  await grantSpace(admin, space.id, 'user', runtime.seededUsers.viewer.id, 'viewer')
  return {
    admin,
    editor: await fromSeed('editor'),
    viewer: await fromSeed('viewer'),
    editorId: runtime.seededUsers.editor.id,
    viewerId: runtime.seededUsers.viewer.id,
    groupId: group.id,
    spaceId: space.id,
  }
}

export async function disposeWorkspace(workspace: SeededWorkspace) {
  await Promise.all([workspace.admin, workspace.editor, workspace.viewer].map((session) => session.context.dispose()))
}

export async function loginPage(page: import('@playwright/test').Page, email: string, password: string) {
  const runtime = readRuntimeState()
  await page.goto(`${runtime.baseUrl}/login`)
  await page.getByRole('button', { name: 'English' }).click()
  await page.getByLabel('Email address').fill(email)
  await page.getByLabel('Password').fill(password)
  await page.getByRole('button', { name: 'Sign in' }).click()
  await expect(page).toHaveURL(/\/app(?:\/|$)/)
}

export async function browserContextFor(browser: import('@playwright/test').Browser, session: ApiSession, viewport?: { width: number; height: number }) {
  return await browser.newContext({ storageState: await session.context.storageState(), viewport })
}

export async function expectProblem(response: Awaited<ReturnType<APIRequestContext['get']>>, status: number, code?: string) {
  const body = await json<{ code: string; status: number; detail?: string }>(response, status)
  expect(body.status).toBe(status)
  if (code) expect(body.code).toBe(code)
  const serialized = JSON.stringify(body)
  expect(serialized).not.toMatch(/sql|postgres|stack|secret|bearer|password/i)
  expect(serialized).not.toMatch(/[A-Za-z0-9_-]{64,}/)
  return body
}

export { endpoint, json }
