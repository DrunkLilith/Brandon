import { expect, test } from '@playwright/test'

import { browserContextFor, createPage, disposeWorkspace, seedWorkspace } from './support/fixtures'
import { readRuntimeState } from './support/deployment'

test('editor uploads and downloads an attachment through the authorized backend', async ({ browser }) => {
  const workspace = await seedWorkspace()
  const pageRecord = await createPage(workspace.editor, workspace.spaceId, 'Attachment page', 'attachment body')
  const context = await browserContextFor(browser, workspace.editor)
  const filename = `evidence-${crypto.randomUUID()}.txt`
  const contents = Buffer.from(`encrypted attachment round trip ${crypto.randomUUID()}`, 'utf8')
  try {
    const page = await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}/pages/${pageRecord.id}`)
    await page.getByRole('button', { name: 'English' }).click()
    await page.getByLabel('File', { exact: true }).setInputFiles({ name: filename, mimeType: 'text/plain', buffer: contents })
    await page.getByRole('button', { name: 'Upload file' }).click()
    await expect(page.getByText('File uploaded', { exact: true })).toBeVisible()
    const downloadButton = page.getByRole('button', { name: `Download ${filename}`, exact: true })
    await expect(downloadButton).toBeVisible()

    const downloadPromise = page.waitForEvent('download')
    await downloadButton.click()
    const download = await downloadPromise
    expect(download.suggestedFilename()).toBe(filename)
    const stream = await download.createReadStream()
    const chunks: Buffer[] = []
    for await (const chunk of stream) chunks.push(Buffer.from(chunk))
    expect(Buffer.concat(chunks)).toEqual(contents)
  } finally {
    await Promise.all([context.close(), disposeWorkspace(workspace)])
  }
})
