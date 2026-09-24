// @vitest-environment node

import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiError, createApiClient, unwrap } from './client'

afterEach(() => vi.unstubAllGlobals())

describe('typed API client', () => {
  it.each(['csrf', 'upload'])('aborts attachment %s transport when its signal is cancelled', async (phase) => {
    const abort = new AbortController()
    let transportSignal: AbortSignal | undefined
    let reached!: () => void
    const ready = new Promise<void>(resolve => { reached = resolve })
    vi.stubGlobal('fetch', vi.fn((input: Request | string, init?: RequestInit) => {
      if (typeof input === 'string' && phase === 'upload') return Promise.resolve(new Response(JSON.stringify({ token: 'csrf' })))
      transportSignal = typeof input === 'string' ? init?.signal ?? undefined : input.signal
      reached()
      return new Promise<Response>((_, reject) => transportSignal?.addEventListener('abort', () => reject(new DOMException('Aborted', 'AbortError'))))
    }))
    const upload = createApiClient().uploadAttachment({ pageId: 'p', file: new File(['abc'], 'a.txt'), signal: abort.signal })
    await ready
    abort.abort()
    expect(transportSignal?.aborted).toBe(true)
    await expect(upload).rejects.toMatchObject({ name: 'AbortError' })
  })
  it('uses same-origin cookies and obtains CSRF before a mutation', async () => {
    const fetcher = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ token: 'csrf-value' }), { status: 200, headers: { 'content-type': 'application/json' } }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true }), { status: 200, headers: { 'content-type': 'application/json' } }))
    vi.stubGlobal('fetch', fetcher)
    const client = createApiClient()

    await unwrap(await client.PATCH('/api/v1/admin/settings', {
      body: { attachmentRecoveryDays: 30, exportRetentionHours: 24 },
    }))

    expect(fetcher).toHaveBeenNthCalledWith(1, '/api/v1/auth/csrf', expect.objectContaining({ credentials: 'include' }))
    const request = fetcher.mock.calls[1][0] as Request
    expect(new URL(request.url).pathname).toBe('/api/v1/admin/settings')
    expect(request.credentials).toBe('include')
    expect(request.headers.get('X-CSRF-Token')).toBe('csrf-value')
  })

  it('normalizes problem details without retaining an arbitrary server body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      type: 'https://knowledge.local/problems/authentication-required', title: 'Authentication required',
      status: 401, detail: 'The supplied credentials or session are invalid', code: 'authentication_required',
      secretDebug: 'must not escape',
    }), { status: 401, headers: { 'content-type': 'application/problem+json' } })))

    const client = createApiClient()
    const failure = await client.GET('/api/v1/auth/session').then(unwrap).catch((error: unknown) => error)
    expect(failure).toBeInstanceOf(ApiError)
    expect(failure).toMatchObject({ status: 401, code: 'authentication_required' })
    expect(JSON.stringify(failure)).not.toContain('secretDebug')
  })

  it('serializes attachment uploads as multipart with a browser-generated boundary', async () => {
    const fetcher = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ token: 'csrf-value' }), { status: 200, headers: { 'content-type': 'application/json' } }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        id: 'attachment-id', pageId: 'page-id', name: 'notes.txt', mime: 'text/plain', size: 5, archived: false,
      }), { status: 201, headers: { 'content-type': 'application/json' } }))
    vi.stubGlobal('fetch', fetcher)
    const client = createApiClient()
    const file = new File(['hello'], 'notes.txt', { type: 'text/plain' })

    const attachment = await client.uploadAttachment({
      pageId: 'page-id',
      file,
      idempotencyKey: '00000000-0000-0000-0000-000000000001',
    })

    expect(attachment.name).toBe('notes.txt')
    const request = fetcher.mock.calls[1][0] as Request
    expect(new URL(request.url).searchParams.get('size')).toBe('5')
    expect(request.headers.get('Idempotency-Key')).toBe('00000000-0000-0000-0000-000000000001')
    expect(request.headers.get('Content-Type')).toMatch(/^multipart\/form-data; boundary=/)
    const sent = await request.clone().formData()
    const sentFile = sent.get('file')
    expect(sentFile).toBeInstanceOf(File)
    expect((sentFile as File).name).toBe('notes.txt')
    expect(await (sentFile as File).text()).toBe('hello')
  })

  it('accepts prebuilt FormData and derives the declared size from its file field', async () => {
    const fetcher = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ token: 'csrf-value' }), { status: 200, headers: { 'content-type': 'application/json' } }))
      .mockResolvedValueOnce(new Response(JSON.stringify({
        id: 'attachment-id', pageId: 'page-id', name: 'ready.txt', mime: 'text/plain', size: 5, archived: false,
      }), { status: 201, headers: { 'content-type': 'application/json' } }))
    vi.stubGlobal('fetch', fetcher)
    const form = new FormData()
    form.append('file', new File(['ready'], 'ready.txt', { type: 'text/plain' }))

    await createApiClient().uploadAttachment({ pageId: 'page-id', file: form })

    const request = fetcher.mock.calls[1][0] as Request
    expect(new URL(request.url).searchParams.get('size')).toBe('5')
    expect(request.headers.get('Content-Type')).toMatch(/^multipart\/form-data; boundary=/)
    expect((await request.clone().formData()).get('file')).toBeInstanceOf(File)
  })

  it('parses attachment and export downloads as blobs without caller casts', async () => {
    const fetcher = vi.fn()
      .mockResolvedValueOnce(new Response(new Uint8Array([1, 2, 3]), { status: 206, headers: { 'content-type': 'application/octet-stream' } }))
      .mockResolvedValueOnce(new Response(new Uint8Array([4, 5]), { status: 200, headers: { 'content-type': 'application/x-tar' } }))
    vi.stubGlobal('fetch', fetcher)
    const client = createApiClient()

    const attachment = await client.downloadAttachment({ attachmentId: 'attachment-id', range: 'bytes=0-2' })
    const archive = await client.downloadExport({ exportId: 'export-id' })

    expect(attachment).toBeInstanceOf(Blob)
    expect([...new Uint8Array(await attachment.arrayBuffer())]).toEqual([1, 2, 3])
    expect(archive).toBeInstanceOf(Blob)
    expect([...new Uint8Array(await archive.arrayBuffer())]).toEqual([4, 5])
    const attachmentRequest = fetcher.mock.calls[0][0] as Request
    expect(attachmentRequest.headers.get('Range')).toBe('bytes=0-2')
  })
})
