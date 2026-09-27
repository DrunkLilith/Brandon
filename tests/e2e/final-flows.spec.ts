import { expect, test } from '@playwright/test'
import { apiSession, browserContextFor, createMember, createPage, disposeWorkspace, grantSpace, loginPage, memberPassword, mutate, seedWorkspace } from './support/fixtures'
import { readRuntimeState } from './support/deployment'

test('space owner manages grants and downloads a completed export through the website', async ({ browser }) => {
  const workspace = await seedWorkspace()
  await grantSpace(workspace.admin, workspace.spaceId, 'user', workspace.editorId, 'owner')
  await createPage(workspace.editor, workspace.spaceId, 'Portable page', 'portable markdown')
  const context = await browserContextFor(browser, workspace.editor)
  try {
    const page = await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}`)
    await page.getByRole('button', { name: 'English' }).click()
    await page.getByRole('button', { name: 'Manage space' }).click({timeout:15_000})
    await expect(page.getByRole('region', { name: /Access grants/ })).toBeVisible()
    await page.getByText('Edit space',{exact:true}).click()
    await page.getByLabel('Selected space description').fill('Owner-managed settings')
    await page.getByRole('button',{name:'Save space',exact:true}).click()
    await expect.poll(async()=>((await (await workspace.editor.context.get(`/api/v1/spaces/${workspace.spaceId}`)).json()) as {description:string}).description).toBe('Owner-managed settings')
    const subjects=await (await workspace.editor.context.get(`/api/v1/spaces/${workspace.spaceId}/subjects`)).json() as {items:{id:string;label:string}[]}
    const viewer=subjects.items.find(subject=>subject.id===workspace.viewerId)!
    await page.getByRole('combobox',{name:new RegExp(`^Role for ${viewer.label}`)}).selectOption('editor',{timeout:15_000})
    await expect.poll(async()=>((await (await workspace.editor.context.get(`/api/v1/spaces/${workspace.spaceId}/grants`)).json()) as {subjectId:string;role:string}[]).find(grant=>grant.subjectId===workspace.viewerId)?.role).toBe('editor')
    await page.getByRole('button', { name: 'Create export' }).click({timeout:15_000})
    await expect(page.getByRole('button', { name: 'Download export' })).toBeVisible({ timeout: 30_000 })
    const downloaded = page.waitForEvent('download')
    await page.getByRole('button', { name: 'Download export' }).click()
    const stream = await (await downloaded).createReadStream()
    const chunks: Buffer[] = []
    for await (const chunk of stream!) chunks.push(Buffer.from(chunk))
    expect(Buffer.concat(chunks).includes(Buffer.from('portable markdown'))).toBe(true)
    const denied = await context.request.get(`${readRuntimeState().baseUrl}/api/v1/users`)
    expect(denied.status()).toBe(403)
    expect((await mutate(workspace.editor,'delete',`/api/v1/spaces/${workspace.spaceId}`)).status()).toBe(204)
    await page.goto(`${readRuntimeState().baseUrl}/app`)
    await page.getByRole('button',{name:'Space archive',exact:true}).click()
    await page.getByRole('region',{name:'Space archive',exact:true}).getByRole('button',{name:'Restore',exact:true}).click()
    await expect.poll(async()=>(await workspace.editor.context.get(`/api/v1/spaces/${workspace.spaceId}`)).status()).toBe(200)
  } finally { await context.close(); await disposeWorkspace(workspace) }
})

test('confirmed member deletion revokes a live session and keeps administrator access',async({browser})=>{
  const workspace=await seedWorkspace()
  const member=await createMember(workspace.admin,'Member for deletion')
  const session=await apiSession(member.email,member.password)
  const context=await browserContextFor(browser,workspace.admin)
  try {
    const page=await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/admin/users`)
    await page.getByRole('button',{name:'English'}).click()
    await page.getByRole('button',{name:'Delete member Member for deletion',exact:true}).click()
    await page.getByRole('dialog').getByRole('button',{name:'Cancel',exact:true}).click()
    expect((await session.context.get('/api/v1/auth/session')).status()).toBe(200)
    await page.getByRole('button',{name:'Delete member Member for deletion',exact:true}).click()
    const deleted=page.waitForResponse(response=>response.request().method()==='DELETE'&&response.url().endsWith(`/api/v1/users/${member.id}`))
    await page.getByRole('dialog').getByRole('button',{name:'Confirm member deletion',exact:true}).click()
    expect((await deleted).status()).toBe(204)
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(page.getByRole('button',{name:'Delete member Member for deletion',exact:true})).toHaveCount(0)
    expect((await session.context.get('/api/v1/auth/session')).status()).toBe(401)
    expect((await workspace.admin.context.get('/api/v1/users')).status()).toBe(200)
  } finally {await context.close();await session.context.dispose();await disposeWorkspace(workspace)}
})

test('owner discovers and restores archived pages and attachments through the website', async ({ browser }) => {
  const workspace = await seedWorkspace()
  await grantSpace(workspace.admin, workspace.spaceId, 'user', workspace.editorId, 'owner')
  const created = await createPage(workspace.editor, workspace.spaceId, 'Recovered page', 'retained text')
  expect((await mutate(workspace.editor,'delete',`/api/v1/pages/${created.id}`)).status()).toBe(204)
  const context = await browserContextFor(browser, workspace.editor)
  try {
    const page = await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}`)
    await page.getByRole('button', { name: 'English' }).click()
    await page.getByRole('button', { name: 'Page archive' }).click({timeout:15_000})
    await expect(page.getByText('Recovered page',{ exact: true })).toBeVisible()
    await page.getByRole('button', { name: 'Restore page' }).click()
    await page.getByRole('treeitem', { name: 'Recovered page',exact:true }).click()
    await page.getByLabel('File',{ exact:true }).setInputFiles({name:'retained.txt',mimeType:'text/plain',buffer:Buffer.from('retained attachment')})
    await page.getByRole('button',{name:'Upload file',exact:true}).click()
    await expect(page.getByText('File uploaded')).toBeVisible()
    await page.getByRole('button',{name:'Archive attachment'}).click()
    await page.getByRole('button',{name:'Attachment archive'}).click()
    await page.getByRole('button',{name:'Restore attachment'}).click()
    await expect(page.getByRole('button',{name:'Download retained.txt'})).toBeVisible()
  } finally { await context.close(); await disposeWorkspace(workspace) }
})

test('mobile login discovers a space and its page without a deep link', async ({ browser }) => {
  const workspace = await seedWorkspace()
  await createPage(workspace.editor, workspace.spaceId, 'Mobile discovery', 'readable mobile text')
  const space = await (await workspace.editor.context.get(`/api/v1/spaces/${workspace.spaceId}`)).json() as {name:string}
  const context = await browser.newContext({viewport:{width:390,height:844}})
  try {
    const page=await context.newPage()
    await loginPage(page,readRuntimeState().seededUsers!.editor.email,memberPassword)
    await page.getByRole('link',{name:space.name,exact:true}).click()
    await page.getByRole('treeitem',{name:'Mobile discovery',exact:true}).click({timeout:15_000})
    await page.getByRole('button',{name:'Raw Markdown'}).click()
    await expect(page.getByLabel('Markdown')).toHaveValue('readable mobile text')
  } finally { await context.close(); await disposeWorkspace(workspace) }
})

test('attachment traversal reaches a new upload after the oldest hundred', async ({ browser }) => {
  const workspace=await seedWorkspace()
  const created=await createPage(workspace.editor,workspace.spaceId,'Many attachments','files')
  for(let index=0;index<100;index++) {
    const response=await mutate(index<50?workspace.admin:workspace.editor,'post',`/api/v1/pages/${created.id}/attachments?size=1`,{headers:{'Idempotency-Key':crypto.randomUUID()},multipart:{file:{name:`file-${index}.txt`,mimeType:'text/plain',buffer:Buffer.from('x')}}})
    expect(response.status()).toBe(201)
  }
  const context=await browserContextFor(browser,workspace.editor)
  try {
    const page=await context.newPage()
    await page.goto(`${readRuntimeState().baseUrl}/app/spaces/${workspace.spaceId}/pages/${created.id}`)
    await page.getByRole('button',{name:'English'}).click()
    await page.getByLabel('File',{exact:true}).setInputFiles({name:'latest.txt',mimeType:'text/plain',buffer:Buffer.from('latest')})
    await page.getByRole('button',{name:'Upload file',exact:true}).click()
    await expect(page.getByText('File uploaded')).toBeVisible()
    await page.getByRole('button',{name:'Load more attachments'}).click({timeout:15_000})
    await expect(page.getByRole('button',{name:'Download latest.txt'})).toBeVisible()
  } finally { await context.close(); await disposeWorkspace(workspace) }
})
