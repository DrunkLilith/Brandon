import { expect, test, type Page } from '@playwright/test'
import { execFileSync } from 'node:child_process'

import { memberPassword } from './support/fixtures'
import { readRuntimeState, writeRuntimeState } from './support/deployment'

async function invite(page: Page, email: string) {
  await page.goto(`${readRuntimeState().baseUrl}/admin/users`)
  await page.getByRole('button', { name: 'Invite user' }).click()
  await page.getByLabel('Invitee email').fill(email)
  await page.getByRole('button', { name: 'Create invitation' }).click()
  const token = await page.locator('.token-result code').textContent()
  expect(token).toBeTruthy()
  await page.getByRole('button', { name: 'Close' }).click()
  return token!
}

async function accept(page: Page, token: string, displayName: string) {
  const runtime = readRuntimeState()
  const response = await page.goto(`${runtime.baseUrl}/invite/${token}`)
  expect(response?.headers()['referrer-policy']).toBe('no-referrer')
  await page.getByRole('button', { name: 'English' }).click()
  await expect(page.getByRole('heading', { name: 'Accept invitation' })).toBeVisible()
  await expect(page.locator('main')).not.toContainText(token)
  await page.getByLabel('Display name').fill(displayName)
  await page.getByLabel('Password', { exact: true }).fill(memberPassword)
  await page.getByLabel('Confirm password').fill(memberPassword)
  await page.getByRole('button', { name: 'Create account' }).click()
  await expect(page).toHaveURL(`${runtime.baseUrl}/login`)
  await expect(page.getByRole('status')).toContainText('account is ready')
}

test('bootstrap admin provisions editor and viewer through the complete browser flow', async ({ browser }) => {
  const runtime = readRuntimeState()
  const suffix = crypto.randomUUID().slice(0, 8)
  const editorEmail = `browser-editor-${suffix}@example.test`
  const viewerEmail = `browser-viewer-${suffix}@example.test`
  const editorName = `Browser Editor ${suffix}`
  const viewerName = `Browser Viewer ${suffix}`
  const groupName = `Browser Editors ${suffix}`
  const spaceName = `Browser Space ${suffix}`
  const pageTitle = `Browser Page ${suffix}`
  const markdown = `# Browser flow ${suffix}\n\nViewer-visible text.`

  const adminContext = await browser.newContext()
  const editorContext = await browser.newContext()
  const viewerContext = await browser.newContext()
  try {
    const admin = await adminContext.newPage()
    await admin.goto(`${runtime.baseUrl}/login`)
    await admin.getByRole('button', { name: 'English' }).click()
    await admin.getByLabel('Email address').fill(runtime.adminEmail)
    await admin.getByLabel('Password').fill(runtime.adminPassword)
    await admin.getByRole('button', { name: 'Sign in' }).click()
    await expect(admin).toHaveURL(/\/app$/)

    const editorToken = await invite(admin, editorEmail)
    const viewerToken = await invite(admin, viewerEmail)
    await accept(await editorContext.newPage(), editorToken, editorName)
    await accept(await viewerContext.newPage(), viewerToken, viewerName)
    const webLogs = execFileSync('docker', [
      'compose', '-f', 'deploy/compose.yaml', '-p', runtime.project,
      'logs', '--no-color', 'web',
    ], { encoding: 'utf8', env: { ...process.env, ...runtime.composeEnvironment } })
    expect(webLogs).not.toContain(editorToken)
    expect(webLogs).not.toContain(viewerToken)

    await admin.goto(`${runtime.baseUrl}/admin/groups`)
    await admin.getByLabel('Group name').fill(groupName)
    await admin.getByRole('button', { name: 'Create', exact: true }).click()
    const group = admin.getByRole('region', { name: `Group ${groupName}` })
    await expect(group).toBeVisible()
    await group.getByLabel('Search active users').fill(editorEmail)
    await group.getByRole('button', { name: `Add ${editorName}` }).click()
    await expect(group.getByText(editorName, { exact: true })).toBeVisible()

    await admin.goto(`${runtime.baseUrl}/admin/spaces`)
    await admin.getByLabel('Space name').fill(spaceName)
    await admin.getByRole('button', { name: 'Create', exact: true }).click()
    const grants = admin.getByRole('region', { name: `Access grants ${spaceName}` })
    await expect(grants).toBeVisible()
    await grants.getByLabel('Subject type').selectOption('group')
    await grants.getByLabel('User or group').selectOption({ label: groupName })
    await grants.getByLabel('Access role').selectOption('editor')
    await grants.getByRole('button', { name: 'Add access grant' }).click()
    await expect(grants.getByText(new RegExp(`${groupName}.*Group`))).toBeVisible()
    await grants.getByLabel('Subject type').selectOption('user')
    await grants.getByLabel('User or group').selectOption({ label: viewerName })
    await grants.getByLabel('Access role').selectOption('viewer')
    await grants.getByRole('button', { name: 'Add access grant' }).click()
    await expect(grants.getByText(new RegExp(`${viewerName}.*User`))).toBeVisible()

    const editor = editorContext.pages()[0]
    await editor.getByLabel('Email address').fill(editorEmail)
    await editor.getByLabel('Password').fill(memberPassword)
    await editor.getByRole('button', { name: 'Sign in' }).click()
    await editor.getByRole('link', { name: spaceName }).click()
    await editor.getByRole('button', { name: 'Create page' }).click()
    await editor.getByLabel('Page title').fill(pageTitle)
    await editor.getByRole('button', { name: 'Create page' }).click()
    await expect(editor.getByLabel('Page title')).toHaveValue(pageTitle)
    await editor.getByRole('button', { name: 'Raw Markdown' }).click()
    await editor.getByLabel('Markdown').fill(markdown)
    await editor.getByRole('button', { name: 'Save revision' }).click()
    await expect(editor.getByText('Revision saved to server', { exact: true })).toBeVisible()

    const viewer = viewerContext.pages()[0]
    await viewer.getByLabel('Email address').fill(viewerEmail)
    await viewer.getByLabel('Password').fill(memberPassword)
    await viewer.getByRole('button', { name: 'Sign in' }).click()
    await viewer.getByRole('link', { name: spaceName }).click()
    await viewer.getByRole('treeitem', { name: pageTitle }).click()
    await viewer.getByRole('button', { name: 'Raw Markdown' }).click()
    await expect(viewer.getByLabel('Markdown')).toHaveValue(markdown)
    await expect(viewer.getByLabel('Markdown')).toHaveAttribute('readonly')
    await expect(viewer.getByRole('button', { name: 'Save revision' })).toHaveCount(0)

    const [adminSession, editorSession, viewerSession] = await Promise.all([
      adminContext.request.get(`${runtime.baseUrl}/api/v1/auth/session`).then((response) => response.json() as Promise<{ userId: string }>),
      editorContext.request.get(`${runtime.baseUrl}/api/v1/auth/session`).then((response) => response.json() as Promise<{ userId: string }>),
      viewerContext.request.get(`${runtime.baseUrl}/api/v1/auth/session`).then((response) => response.json() as Promise<{ userId: string }>),
    ])
    runtime.seededUsers = {
      admin: { id: adminSession.userId, email: runtime.adminEmail, storageState: await adminContext.storageState() },
      editor: { id: editorSession.userId, email: editorEmail, storageState: await editorContext.storageState() },
      viewer: { id: viewerSession.userId, email: viewerEmail, storageState: await viewerContext.storageState() },
    }
    writeRuntimeState(runtime)
  } finally {
    await Promise.all([adminContext.close(), editorContext.close(), viewerContext.close()])
  }
})
