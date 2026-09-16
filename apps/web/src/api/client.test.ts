import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiError, createApiClient, unwrap } from './client'

afterEach(() => vi.unstubAllGlobals())

describe('typed API client', () => {
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
})
