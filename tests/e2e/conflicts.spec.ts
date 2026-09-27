import { expect, test } from '@playwright/test'

import { browserContextFor, createPage, disposeWorkspace, seedWorkspace, updatePage, memberPassword, type SeededWorkspace } from './support/fixtures'
import { readRuntimeState } from './support/deployment'

test('a different account never receives another identitys unpublished browser draft', async ({ browser }) => {
  const workspace = await seedWorkspace()
  const created = await createPage(workspace.editor, workspace.spaceId, 'Identity draft', 'published text')
  const runtime = readRuntimeState()
  const context = await browserContextFor(browser, workspace.editor)
  await context.clearCookies()
  expect((await context.request.post(`${runtime.baseUrl}/api/v1/auth/login`, { headers: { Origin: runtime.baseUrl }, data: { email: runtime.seededUsers!.editor.email, password: memberPassword } })).status()).toBe(200)
  try {
    const page = await context.newPage()
    await page.goto(`${runtime.baseUrl}/app/spaces/${workspace.spaceId}/pages/${created.id}`)
    await page.getByRole('button', { name: 'English' }).click()
    await page.getByRole('button', { name: 'Raw Markdown' }).click()
    await page.getByLabel('Markdown').fill('private unpublished draft')
    expect((await context.request.post(`${runtime.baseUrl}/api/v1/auth/login`, {
      headers: { Origin: runtime.baseUrl }, data: { email: runtime.seededUsers!.viewer.email, password: memberPassword },
    })).status()).toBe(200)
    await page.reload()
    await page.getByRole('button', { name: 'Raw Markdown' }).click()
    // Compare booleans to keep unpublished content out of failure output.
    expect((await page.getByLabel('Markdown').inputValue()) === 'published text').toBe(true)
  } finally {
    await context.close()
    await disposeWorkspace(workspace)
  }
})

test('drafts isolate accounts and recover independent same-context tabs across server versions', async ({ browser }) => {
  const workspace = await seedWorkspace()
  const created = await createPage(workspace.editor, workspace.spaceId, 'Draft recovery', 'server base')
  const runtime = readRuntimeState()
  const context = await browserContextFor(browser, workspace.editor)
  await context.clearCookies()
  expect((await context.request.post(`${runtime.baseUrl}/api/v1/auth/login`, { headers: { Origin: runtime.baseUrl }, data: { email: runtime.seededUsers!.editor.email, password: memberPassword } })).status()).toBe(200)
  try {
    const first = await context.newPage(), second = await context.newPage()
    const url = `${runtime.baseUrl}/app/spaces/${workspace.spaceId}/pages/${created.id}`
    await first.goto(url)
    await first.getByRole('button', { name: 'English' }).click()
    await second.goto(url)
    await first.getByRole('button', { name: 'Raw Markdown' }).click()
    await second.getByRole('button', { name: 'Raw Markdown' }).click()
    await first.getByLabel('Markdown').fill('first private draft')
    await second.getByLabel('Markdown').fill('second private draft')
    // Advancing the server must not hide either durable base-version draft.
    expect((await updatePage(workspace.editor, created.id, 1, 'Draft recovery', 'new server base')).status()).toBe(200)
    await first.reload()
    await first.getByRole('button', { name: 'Recover browser drafts' }).click()
    const recovery = first.getByRole('dialog', { name: 'Recover browser drafts' })
    await expect(recovery).toContainText('first private draft')
    await expect(recovery).toContainText('second private draft')
    await recovery.getByRole('button', { name: 'Continue editing' }).first().click()
    await first.getByRole('button', { name: 'Raw Markdown' }).click()
    await expect(first.getByLabel('Markdown')).toHaveValue(/private draft/)
    // Public login changes the cookie in the same browser profile.
    const switched = await context.request.post(`${runtime.baseUrl}/api/v1/auth/login`, {
      headers: { Origin: runtime.baseUrl }, data: { email: runtime.seededUsers!.viewer.email, password: memberPassword },
    })
    expect(switched.status()).toBe(200)
    await first.reload()
    await first.getByRole('button', { name: 'Raw Markdown' }).click()
    await expect(first.getByLabel('Markdown')).toHaveValue('new server base')
    await expect(first.getByRole('button', { name: 'Recover browser drafts' })).toHaveCount(0)
  } finally {
    await context.close()
    await disposeWorkspace(workspace)
  }
})

test('pagehide persists pending rich editor content before debounce', async ({ browser }) => {
  const workspace = await seedWorkspace()
  const created = await createPage(workspace.editor, workspace.spaceId, 'Lifecycle draft', 'base')
  const context = await browserContextFor(browser, workspace.editor)
  try {
    const page = await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}/pages/${created.id}`)
    await page.getByRole('button', { name: 'English' }).click()
    const rich = page.getByRole('textbox', { name: 'Visual Markdown' })
    await rich.fill('pending lifecycle draft')
    await page.evaluate(() => window.dispatchEvent(new Event('pagehide')))
    const persisted = await page.evaluate(() => Object.values(localStorage).some((entry) => entry.includes('pending lifecycle draft')))
    expect(persisted).toBe(true)
  } finally {
    await context.close()
    await disposeWorkspace(workspace)
  }
})

test('two browser contexts preserve server and submitted text through a usable conflict flow', async ({ browser }) => {
  const workspace: SeededWorkspace = await seedWorkspace()
  const created = await createPage(workspace.editor, workspace.spaceId, 'Conflict page', 'base text')
  const firstText = `first tab ${crypto.randomUUID()}`
  const secondText = `second tab ${crypto.randomUUID()}`
  const firstContext = await browserContextFor(browser, workspace.editor)
  const secondContext = await browserContextFor(browser, workspace.editor)
  try {
    const [first, second] = await Promise.all([firstContext.newPage(), secondContext.newPage()])
    const url = `${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}/pages/${created.id}`
    await Promise.all([first.goto(url), second.goto(url)])
    await first.getByRole('button', { name: 'English' }).click()
    await second.getByRole('button', { name: 'English' }).click()
    await first.getByRole('button', { name: 'Raw Markdown' }).click()
    await second.getByRole('button', { name: 'Raw Markdown' }).click()
    await first.getByLabel('Markdown').fill(firstText)
    await second.getByLabel('Markdown').fill(secondText)

    await first.getByRole('button', { name: 'Save revision' }).click()
    await expect(first.getByText('Revision saved to server', { exact: true })).toBeVisible()
    await second.getByRole('button', { name: 'Save revision' }).click()

    const conflict = second.getByRole('dialog', { name: 'Version conflict' })
    await expect(conflict).toContainText(secondText)
    const server = await workspace.editor.context.get(`/api/v1/pages/${created.id}`)
    expect((await server.json() as { markdown: string }).markdown).toBe(firstText)
    await conflict.getByRole('button', { name: 'Continue manual merge' }).click()
    await expect(second.getByLabel('Markdown')).toHaveValue(secondText)
    await second.getByRole('button', { name: 'Save revision' }).click()
    await expect(second.getByText('Revision saved to server', { exact: true })).toBeVisible()
    const resolved = await workspace.editor.context.get(`/api/v1/pages/${created.id}`)
    expect((await resolved.json() as { markdown: string }).markdown).toBe(secondText)
  } finally {
    await Promise.all([firstContext.close(), secondContext.close(), disposeWorkspace(workspace)])
  }
})
