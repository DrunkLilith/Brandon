import { expect, test } from '@playwright/test'

import { browserContextFor, createPage, disposeWorkspace, seedWorkspace } from './support/fixtures'
import { readRuntimeState } from './support/deployment'

test('administrator filters the real audit log', async ({ browser }) => {
  const workspace = await seedWorkspace()
  await createPage(workspace.editor, workspace.spaceId, `Audited ${crypto.randomUUID()}`, 'audited body')
  const context = await browserContextFor(browser, workspace.admin)
  try {
    const page = await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/admin/audit`)
    await page.getByRole('button', { name: 'English' }).click()
    await page.getByLabel('Event type').fill('page.created')
    await page.getByRole('button', { name: 'Apply filters' }).click()
    await expect(page).toHaveURL(/type=page.created/)
    const events = page.getByRole('list').locator('li')
    await expect(events.first()).toContainText('page.created')
    expect((await events.allTextContents()).every((text) => text.includes('page.created'))).toBe(true)
  } finally {
    await Promise.all([context.close(), disposeWorkspace(workspace)])
  }
})
