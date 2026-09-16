import { afterEach, describe, expect, it, vi } from 'vitest'

import { ApiClient, ApiError } from './client'

afterEach(() => vi.unstubAllGlobals())

describe('ApiClient', () => {
  it('uses same-origin cookies and obtains CSRF before a mutation', async () => {
    const fetcher = vi.fn()
      .mockResolvedValueOnce(new Response(JSON.stringify({ token: 'csrf-value' }), { status: 200, headers: { 'content-type': 'application/json' } }))
      .mockResolvedValueOnce(new Response(JSON.stringify({ ok: true }), { status: 200, headers: { 'content-type': 'application/json' } }))
    vi.stubGlobal('fetch', fetcher)
    const client = new ApiClient()

    await client.request('/api/v1/admin/settings', { method: 'PATCH', json: { attachmentRecoveryDays: 30 } })

    expect(fetcher).toHaveBeenNthCalledWith(1, '/api/v1/auth/csrf', expect.objectContaining({ credentials: 'include' }))
    expect(fetcher).toHaveBeenNthCalledWith(2, '/api/v1/admin/settings', expect.objectContaining({ credentials: 'include' }))
    expect(new Headers(fetcher.mock.calls[1][1].headers).get('X-CSRF-Token')).toBe('csrf-value')
  })

  it('normalizes problem details without retaining an arbitrary server body', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
      type: 'https://knowledge.local/problems/authentication-required', title: 'Authentication required',
      status: 401, detail: 'The supplied credentials or session are invalid', code: 'authentication_required',
      secretDebug: 'must not escape',
    }), { status: 401, headers: { 'content-type': 'application/problem+json' } })))

    const failure = await new ApiClient().request('/api/v1/auth/session').catch((error: unknown) => error)
    expect(failure).toBeInstanceOf(ApiError)
    expect(failure).toMatchObject({ status: 401, code: 'authentication_required' })
    expect(JSON.stringify(failure)).not.toContain('secretDebug')
  })
})
