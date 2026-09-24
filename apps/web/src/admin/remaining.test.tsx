import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { App } from '../app'

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': status >= 400 ? 'application/problem+json' : 'application/json' } })
}

function requestOf(input: RequestInfo | URL, init?: RequestInit) {
  return input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
}

const session = { sessionId: 's', userId: 'admin', workspaceId: 'w', workspaceAdmin: true }

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  vi.restoreAllMocks()
  window.history.replaceState({}, '', '/')
})

describe('audit administration', () => {
  it('keeps filters and exclusive cursor in the URL and renders metadata only as text', async () => {
    const firstCursor = '11111111-1111-4111-8111-111111111111'
    const nextCursor = '22222222-2222-4222-8222-222222222222'
    window.history.replaceState({}, '', `/admin/audit?type=user.suspended&after=${firstCursor}`)
    const auditUrls: URL[] = []
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = requestOf(input, init)
      const url = new URL(request.url)
      if (url.pathname === '/api/v1/auth/session') return json(session)
      if (url.pathname === '/api/v1/audit-events') {
        auditUrls.push(url)
        return json({ events: [{ id: 'event-1', actorUserId: 'admin', createdAt: '2026-09-24T12:00:00Z', metadata: { note: '<img src=x onerror=alert(1)>' }, requestId: 'request-1', sessionId: 's', targetId: 'user-1', targetType: 'user', type: url.searchParams.get('type') ?? 'unknown' }], nextCursor })
      }
      throw new Error(`Unexpected request: ${request.method} ${url.pathname}`)
    }))
    const user = userEvent.setup()
    render(<App />)

    expect(await screen.findByText('<img src=x onerror=alert(1)>', { exact: false })).toBeVisible()
    expect(document.querySelector('img')).toBeNull()
    expect(auditUrls[0].searchParams.get('type')).toBe('user.suspended')
    expect(auditUrls[0].searchParams.get('after')).toBe(firstCursor)

    await user.clear(screen.getByLabelText('Тип события'))
    await user.type(screen.getByLabelText('Тип события'), 'invitation.created')
    await user.type(screen.getByLabelText('С даты'), '2026-09-01T00:00:00Z')
    await user.click(screen.getByRole('button', { name: 'Применить фильтры' }))
    await waitFor(() => expect(window.location.search).toContain('type=invitation.created'))
    expect(new URLSearchParams(window.location.search).has('after')).toBe(false)
    expect(new URLSearchParams(window.location.search).get('from')).toBe('2026-09-01T00:00:00Z')

    await user.click(await screen.findByRole('button', { name: 'Следующая страница' }))
    await waitFor(() => expect(new URLSearchParams(window.location.search).get('after')).toBe(nextCursor))
    expect(new URLSearchParams(window.location.search).get('type')).toBe('invitation.created')
  })
})

describe('settings administration', () => {
  it('validates limits, retains values after a safe Problem conflict, and sends typed settings', async () => {
    window.history.replaceState({}, '', '/admin/settings')
    const updates: unknown[] = []
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = requestOf(input, init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') return json(session)
      if (path === '/api/v1/admin/settings' && request.method === 'GET') return json({ attachmentRecoveryDays: 30, exportRetentionHours: 24 })
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/admin/settings' && request.method === 'PATCH') {
        updates.push(await request.json())
        return json({ type: 'x', title: 'Conflict', status: 409, code: 'conflict', detail: 'postgres://secret@db/internal' }, 409)
      }
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    const user = userEvent.setup()
    render(<App />)

    const recovery = await screen.findByLabelText('Срок восстановления вложений, дней')
    const retention = screen.getByLabelText('Срок хранения экспорта, часов')
    await user.clear(recovery)
    await user.type(recovery, '0')
    await user.clear(retention)
    await user.type(retention, '721')
    await user.click(screen.getByRole('button', { name: 'Сохранить настройки' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('от 1 до 3650')
    expect(updates).toHaveLength(0)

    await user.clear(recovery)
    await user.type(recovery, '45')
    await user.clear(retention)
    await user.type(retention, '48')
    await user.click(screen.getByRole('button', { name: 'Сохранить настройки' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('конфликтует с текущим состоянием')
    expect(screen.queryByText('postgres://secret@db/internal')).not.toBeInTheDocument()
    expect(recovery).toHaveValue(45)
    expect(retention).toHaveValue(48)
    expect(updates).toEqual([{ attachmentRecoveryDays: 45, exportRetentionHours: 48 }])
  })
})

describe('admin dashboard', () => {
  it('shows provider-neutral readiness and jobs while hiding raw failure details without console warnings', async () => {
    window.history.replaceState({}, '', '/admin')
    const consoleError = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    const consoleWarn = vi.spyOn(console, 'warn').mockImplementation(() => undefined)
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = requestOf(input, init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') return json(session)
      if (path === '/api/v1/admin/status') return json({ activeExports: 1, components: { database: true, masterKey: true, migrations: true, objectStorage: false }, migrationState: 'current', queuedJobs: 2, version: '1.0.0' })
      if (path === '/api/v1/health/ready') return json({ type: 'x', title: 'Unavailable', status: 503, code: 'not_ready', detail: 's3://bucket/private database-url=secret' }, 503)
      if (path === '/api/v1/exports') return json({ exports: [{ id: 'export-1', spaceId: 'space-1', status: 'ready', expiresAt: '2026-09-25T00:00:00Z', createdAt: '2026-09-24T00:00:00Z' }], nextCursor: null })
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    render(<App />)

    expect(await screen.findByText('Хранилище объектов: недоступно')).toBeVisible()
    expect(screen.getByText('Задач в очереди: 2')).toBeVisible()
    expect(screen.getByText('Активных экспортов: 1')).toBeVisible()
    expect(screen.getByText('Готовность сервиса недоступна.')).toBeVisible()
    expect(screen.queryByText(/s3:\/\/bucket|database-url=secret/)).not.toBeInTheDocument()
    expect(consoleError).not.toHaveBeenCalled()
    expect(consoleWarn).not.toHaveBeenCalled()
  })
})
