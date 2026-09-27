import { expect, test } from '@playwright/test'
import { execFileSync } from 'node:child_process'
import { mkdirSync, readFileSync, readdirSync, rmSync, statSync } from 'node:fs'
import path from 'node:path'

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
import { assertPhysicalPostgresStorageHasNoPlaintext } from './support/storage-audit'

test('restricted subtree and related resources are non-enumerating across every surface', async ({ browser }) => {
  const workspace = await seedWorkspace()
  const marker = `restricted-${crypto.randomUUID()}`
  try {
    const visibleTitle = `visible-${marker}`
    await createPage(workspace.editor, workspace.spaceId, visibleTitle, `allowed ${marker}`)
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
    const searchResults = await search.json() as { pageId: string }[]
    expect(searchResults.some((result) => result.pageId === restricted.id || result.pageId === child.id)).toBe(false)
    expect(searchResults.some((result) => result.pageId !== restricted.id && result.pageId !== child.id)).toBe(true)
    const tree = await workspace.viewer.context.get(`/api/v1/pages?spaceId=${workspace.spaceId}`)
    expect((await tree.json() as { id: string }[]).some((page) => page.id === restricted.id || page.id === child.id)).toBe(false)

    const viewerContext = await browserContextFor(browser, workspace.viewer)
    try {
      const viewerPage = await viewerContext.newPage()
      await viewerPage.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}`)
      await viewerPage.getByRole('button', { name: 'English' }).click()
      await expect(viewerPage.getByRole('treeitem', { name: visibleTitle, exact: true })).toBeVisible()
      await expect(viewerPage.getByRole('treeitem', { name: marker, exact: true })).toHaveCount(0)
      await expect(viewerPage.getByRole('treeitem', { name: `child ${marker}`, exact: true })).toHaveCount(0)
      await viewerPage.getByRole('button', { name: 'Search knowledge' }).click()
      await viewerPage.getByLabel('Search query').fill(marker)
      await viewerPage.getByRole('button', { name: 'Search', exact: true }).click()
      await expect(viewerPage.getByRole('button', { name: visibleTitle, exact: true })).toBeVisible()
      await expect(viewerPage.getByRole('button', { name: marker, exact: true })).toHaveCount(0)
      await expect(viewerPage.getByRole('button', { name: `child ${marker}`, exact: true })).toHaveCount(0)
    } finally {
      await viewerContext.close()
    }

    const runtime = readRuntimeState()
    const compose = (args: string[], encoding: BufferEncoding | null = 'utf8') => execFileSync('docker', [
      'compose', '-f', 'deploy/compose.yaml', '-p', runtime.project, ...args,
    ], { encoding, env: { ...process.env, ...runtime.composeEnvironment } })
    const minioContainer = String(compose(['ps', '-q', 'minio'], 'utf8')).trim()
    const minioPorts = execFileSync('docker', ['inspect', minioContainer, '--format', '{{json .NetworkSettings.Ports}}'], { encoding: 'utf8' })
    expect((JSON.parse(minioPorts) as Record<string, unknown>)['9000/tcp'] ?? null).toBeNull()
    expect(String(compose([
      'exec', '-T', 'backend', 'curl', '--silent', '--output', '/dev/null', '--write-out', '%{http_code}',
      'http://minio:9000/knowledge/',
    ], 'utf8')).trim()).toBe('403')

    const databaseBytes = compose([
      'exec', '-T', 'postgres', '/bin/sh', '-ec',
      'export PGPASSWORD="$(cat /run/secrets/database_password)"; exec pg_dump --host=127.0.0.1 --username=knowledge --dbname=knowledge --format=plain --no-owner --no-privileges --no-comments',
    ], null) as Buffer
    const objectBytes = compose(['exec', '-T', 'minio', 'tar', '-C', '/data', '-cf', '-', '.'], null) as Buffer
    const plaintextMarkers = [marker, `child ${marker}`, `private-${marker}.txt`]
    for (const plaintext of plaintextMarkers) {
      expect(databaseBytes.includes(Buffer.from(plaintext))).toBe(false)
      expect(objectBytes.includes(Buffer.from(plaintext))).toBe(false)
    }

    const postgresVolume = runtime.expectedVolumes.find((volume) => volume.endsWith('_postgres_data'))
    expect(postgresVolume, 'resolved PostgreSQL volume is missing').toBeTruthy()
    compose(['stop', 'postgres'])
    try {
      assertPhysicalPostgresStorageHasNoPlaintext(postgresVolume!, plaintextMarkers)
    } finally {
      compose(['start', 'postgres'])
      await expect.poll(async () => {
        const response = await workspace.admin.context.get('/api/v1/health/ready')
        return response.status()
      }, { timeout: 60_000 }).toBe(200)
    }

    const backupDirectory = path.join(runtime.tempDirectory, 'plaintext-audit-backup')
    mkdirSync(backupDirectory, { mode: 0o700 })
    execFileSync('pwsh', [
      '-NoProfile', '-File', 'scripts/backup.ps1',
      '-ProjectName', runtime.project, '-TargetDirectory', backupDirectory,
    ], { env: { ...process.env, ...runtime.composeEnvironment }, stdio: 'pipe' })
    const files = (directory: string): string[] => readdirSync(directory).flatMap((name) => {
      const candidate = path.join(directory, name)
      return statSync(candidate).isDirectory() ? files(candidate) : [candidate]
    })
    try {
      for (const file of files(backupDirectory)) {
        const bytes = readFileSync(file)
        for (const plaintext of plaintextMarkers) {
          expect(bytes.includes(Buffer.from(plaintext)), `${path.basename(file)} contains plaintext`).toBe(false)
        }
      }
    } finally {
      rmSync(backupDirectory, { recursive: true, force: true })
    }
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
