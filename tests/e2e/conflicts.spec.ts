import { expect, test } from '@playwright/test'

import { browserContextFor, createPage, disposeWorkspace, seedWorkspace, type SeededWorkspace } from './support/fixtures'
import { readRuntimeState } from './support/deployment'

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
