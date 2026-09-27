import { expect, test } from '@playwright/test'

import {
  browserContextFor,
  createPage,
  disposeWorkspace,
  expectProblem,
  mutate,
  pagePermission,
  seedWorkspace,
  updatePage,
} from './support/fixtures'
import { readRuntimeState } from './support/deployment'

test('restricted subtree and related resources are non-enumerating across every surface', async () => {
  const workspace = await seedWorkspace()
  const marker = `restricted-${crypto.randomUUID()}`
  try {
    const restricted = await createPage(workspace.admin, workspace.spaceId, marker, `secret ${marker}`, { restricted: true })
    await pagePermission(workspace.admin, restricted.id, 'group', workspace.groupId, 'editor')
    const child = await createPage(workspace.editor, workspace.spaceId, `child ${marker}`, `child secret ${marker}`, { parentId: restricted.id })
    const payload = Buffer.from(`attachment ${marker}`, 'utf8')
    const upload = await mutate(workspace.editor, 'post', `/api/v1/pages/${restricted.id}/attachments?size=${payload.length}`, {
      headers: { 'Idempotency-Key': crypto.randomUUID() },
      multipart: { file: { name: `private-${marker}.txt`, mimeType: 'text/plain', buffer: payload } },
    })
    expect(upload.status(), await upload.text()).toBe(201)
    const attachment = await upload.json() as Record<string, unknown> & { id: string }
    expect(attachment).not.toHaveProperty('objectKey')

    const exportResponse = await mutate(workspace.admin, 'post', '/api/v1/exports', {
      headers: { 'Idempotency-Key': crypto.randomUUID() },
      data: { spaceId: workspace.spaceId },
    })
    expect(exportResponse.status(), await exportResponse.text()).toBe(202)
    const exportId = (await exportResponse.json() as { id: string }).id
    await expect.poll(async () => {
      const response = await workspace.admin.context.get('/api/v1/exports?limit=100')
      if (!response.ok()) return 'request-failed'
      const item = (await response.json() as { exports: { id: string; status: string }[] }).exports.find((candidate) => candidate.id === exportId)
      return item?.status
    }, { timeout: 30_000 }).toBe('ready')

    for (const path of [
      `/api/v1/pages/${restricted.id}`,
      `/api/v1/pages/${restricted.id}/revisions`,
      `/api/v1/pages/${restricted.id}/revisions/1`,
      `/api/v1/pages/${child.id}`,
      `/api/v1/attachments/${attachment.id}`,
      `/api/v1/exports/${exportId}/download`,
    ]) {
      await expectProblem(await workspace.viewer.context.get(path), 404, 'not_found')
    }
    const search = await workspace.viewer.context.get(`/api/v1/search?q=${encodeURIComponent(marker)}`)
    expect(search.status()).toBe(200)
    expect(await search.json()).toEqual([])
    const tree = await workspace.viewer.context.get(`/api/v1/pages?spaceId=${workspace.spaceId}`)
    expect((await tree.json() as { id: string }[]).some((page) => page.id === restricted.id || page.id === child.id)).toBe(false)

    const raw = await workspace.viewer.context.get(`/knowledge/${attachment.id}`)
    expect(raw.headers()['content-type']).toContain('text/html')
    expect(await raw.text()).not.toContain(marker)
  } finally {
    await disposeWorkspace(workspace)
  }
})

test('CSRF and stale revisions fail without side effects', async () => {
  const workspace = await seedWorkspace()
  const page = await createPage(workspace.editor, workspace.spaceId, 'Concurrency', 'unchanged')
  try {
    const csrfDenied = await workspace.editor.context.patch(`/api/v1/pages/${page.id}`, {
      data: { expectedVersion: page.version, title: 'CSRF overwrite', markdown: 'csrf overwrite', properties: {}, restricted: false },
    })
    await expectProblem(csrfDenied, 403, 'csrf_rejected')
    let current = await workspace.editor.context.get(`/api/v1/pages/${page.id}`)
    expect((await current.json() as { markdown: string; version: number }).markdown).toBe('unchanged')

    const accepted = await updatePage(workspace.editor, page.id, page.version, 'Concurrency', 'first accepted')
    expect(accepted.status()).toBe(200)
    const stale = await updatePage(workspace.editor, page.id, page.version, 'Concurrency', 'stale overwrite')
    await expectProblem(stale, 409, 'version_conflict')
    current = await workspace.editor.context.get(`/api/v1/pages/${page.id}`)
    const body = await current.json() as { markdown: string; version: number }
    expect(body.markdown).toBe('first accepted')
    expect(body.version).toBe(page.version + 1)
  } finally {
    await disposeWorkspace(workspace)
  }
})

test('untrusted Markdown cannot execute in the production browser', async ({ browser }) => {
  const workspace = await seedWorkspace()
  const marker = `xss-${crypto.randomUUID()}`
  const pageRecord = await createPage(
    workspace.editor,
    workspace.spaceId,
    `Unsafe ${marker}`,
    `<script>window.__knowledgeOwned = '${marker}'</script>\n\n<a href="javascript:window.__knowledgeOwned='link'">unsafe</a>`,
  )
  const context = await browserContextFor(browser, workspace.viewer)
  try {
    const page = await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}/pages/${pageRecord.id}`)
    await page.getByRole('button', { name: 'English' }).click()
    const visualMarkdown = page.getByLabel('Visual Markdown')
    await expect(visualMarkdown).toBeVisible()
    expect(await page.evaluate(() => (window as typeof window & { __knowledgeOwned?: string }).__knowledgeOwned)).toBeUndefined()
    await expect(visualMarkdown.locator('script')).toHaveCount(0)
    await expect(visualMarkdown.locator('a[href^="javascript:"]')).toHaveCount(0)
  } finally {
    await Promise.all([context.close(), disposeWorkspace(workspace)])
  }
})
