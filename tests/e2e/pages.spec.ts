import { expect, test } from '@playwright/test'

import {
  browserContextFor,
  createPage,
  disposeWorkspace,
  seedWorkspace,
  updatePage,
  type SeededWorkspace,
} from './support/fixtures'
import { readRuntimeState } from './support/deployment'

let workspace: SeededWorkspace
let pageId: string
const marker = `history-${crypto.randomUUID()}`

test.beforeAll(async () => {
  workspace = await seedWorkspace()
  const page = await createPage(workspace.editor, workspace.spaceId, `History ${marker}`, `original ${marker}`)
  pageId = page.id
  const updated = await updatePage(workspace.editor, page.id, page.version, `History ${marker}`, `updated ${marker}`)
  expect(updated.status()).toBe(200)
})

test.afterAll(async () => disposeWorkspace(workspace))

test('editor compares and restores immutable history while viewer can only read it', async ({ browser }) => {
  const editorContext = await browserContextFor(browser, workspace.editor)
  const viewerContext = await browserContextFor(browser, workspace.viewer)
  try {
    const editor = await editorContext.newPage()
    await editor.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}/pages/${pageId}`)
    await editor.getByRole('button', { name: 'English' }).click()
    await editor.getByRole('button', { name: 'History' }).click()
    await editor.getByRole('button', { name: 'Compare revision 1' }).click()
    await expect(editor.getByLabel('Compare with current text')).toContainText(`original ${marker}`)
    await editor.getByRole('button', { name: 'Restore revision 1' }).click()
    await editor.getByRole('button', { name: 'Confirm restore' }).click()
    await expect(editor.getByText('Revision saved to server', { exact: true })).toBeVisible()
    await editor.getByRole('button', { name: 'Raw Markdown' }).click()
    await expect(editor.getByLabel('Markdown')).toHaveValue(`original ${marker}`)

    const viewer = await viewerContext.newPage()
    await viewer.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}/pages/${pageId}`)
    await viewer.getByRole('button', { name: 'English' }).click()
    await viewer.getByRole('button', { name: 'History' }).click()
    await expect(viewer.getByRole('button', { name: 'Compare revision 2' })).toBeVisible()
    await expect(viewer.getByRole('button', { name: /Restore revision/ })).toHaveCount(0)
  } finally {
    await Promise.all([editorContext.close(), viewerContext.close()])
  }
})

test('mobile width supports reading, search and a basic editor save', async ({ browser }) => {
  const context = await browserContextFor(browser, workspace.editor, { width: 390, height: 844 })
  try {
    const page = await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}/pages/${pageId}`)
    await page.getByRole('button', { name: 'English' }).click()
    await expect(page.getByLabel('Page title')).toBeVisible()
    await page.getByRole('button', { name: /Search knowledge/ }).click()
    await page.getByLabel('Search query').fill(marker)
    await page.getByRole('button', { name: 'Search', exact: true }).click()
    await expect(page.getByRole('dialog').getByRole('button', { name: `History ${marker}`, exact: true })).toBeVisible()
    await page.getByRole('button', { name: 'Close' }).click()
    await page.getByRole('button', { name: 'Raw Markdown' }).click()
    await page.getByLabel('Markdown').fill(`mobile edit ${marker}`)
    await page.getByRole('button', { name: 'Save revision' }).click()
    await expect(page.getByText('Revision saved to server', { exact: true })).toBeVisible()
  } finally {
    await context.close()
  }
})
