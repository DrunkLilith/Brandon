import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import userEvent from '@testing-library/user-event'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { App } from '../app'
import { UsersPage } from './users'

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': status >= 400 ? 'application/problem+json' : 'application/json' },
  })
}

afterEach(() => {
  cleanup()
  localStorage.clear()
  vi.unstubAllGlobals()
  window.history.replaceState({}, '', '/')
})

describe('users and invitations administration', () => {
  it('requires an explicit confirmation before deleting a member', async () => {
    window.history.replaceState({}, '', '/admin/users')
    let deleted = false
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') return json({ sessionId: 's', userId: 'admin', workspaceId: 'w', workspaceAdmin: true })
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/invitations') return json([])
      if (path === '/api/v1/users') return json(deleted ? [] : [{ id: 'member', email: 'member@example.com', displayName: 'Участник', status: 'active', workspaceRole: 'member' }])
      if (path === '/api/v1/users/member' && request.method === 'DELETE') { deleted = true; return new Response(null, { status: 204 }) }
      throw new Error('Unexpected request')
    }))
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Удалить участника Участник' }))
    expect(deleted).toBe(false)
    await user.click(screen.getByRole('button', { name: 'Отмена' }))
    expect(deleted).toBe(false)
    await user.click(screen.getByRole('button', { name: 'Удалить участника Участник' }))
    await user.click(screen.getByRole('button', { name: 'Подтвердить удаление участника' }))
    await waitFor(() => expect(screen.queryByText('member@example.com')).not.toBeInTheDocument())
    expect(deleted).toBe(true)
  })

  it('validates an invitation and reveals its token only after a successful explicit-expiry submission', async () => {
    window.history.replaceState({}, '', '/admin/users')
    const invitationRequests: Request[] = []
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
      const url = new URL(request.url, window.location.origin)
      if (url.pathname === '/api/v1/auth/session') {
        return json({ sessionId: 'session-1', userId: 'admin-1', workspaceId: 'workspace-1', workspaceAdmin: true })
      }
      if (url.pathname === '/api/v1/users') {
        return json([{ id: 'admin-1', email: 'admin@example.com', displayName: 'Администратор', workspaceRole: 'admin', status: 'active' }])
      }
      if (url.pathname === '/api/v1/invitations' && request.method === 'GET') return json([])
      if (url.pathname === '/api/v1/auth/csrf') return json({ token: 'csrf-token' })
      if (url.pathname === '/api/v1/invitations' && request.method === 'POST') {
        invitationRequests.push(request.clone())
        return json({ id: 'invite-1', token: 'one-time-secret-token', expiresAt: '2026-10-01T00:00:00Z' }, 201)
      }
      throw new Error(`Unexpected request: ${request.method} ${url.pathname}`)
    })
    vi.stubGlobal('fetch', fetcher)
    const user = userEvent.setup()
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText } })

    render(<App />)
    const englishSwitch = screen.getByRole('button', { name: 'English' })
    const russianSwitch = screen.getByRole('button', { name: 'Русский' })
    const invitationOpener = await screen.findByRole('button', { name: 'Пригласить пользователя' })
    invitationOpener.focus()
    await user.click(invitationOpener)
    const dialog = screen.getByRole('dialog', { name: 'Новое приглашение' })
    expect(dialog).toBeVisible()
    expect(document.querySelector('.admin-shell')).toHaveAttribute('inert')
    expect(screen.getByLabelText('Электронная почта приглашённого')).toHaveFocus()

    await user.keyboard('{Shift>}{Tab}{/Shift}')
    expect(screen.getByRole('button', { name: 'Создать приглашение' })).toHaveFocus()
    await user.keyboard('{Tab}')
    expect(screen.getByLabelText('Электронная почта приглашённого')).toHaveFocus()
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog', { name: 'Новое приглашение' })).not.toBeInTheDocument()
    expect(document.querySelector('.admin-shell')).not.toHaveAttribute('inert')
    expect(invitationOpener).toHaveFocus()
    await user.click(invitationOpener)

    await user.type(screen.getByLabelText('Электронная почта приглашённого'), 'not-an-email')
    await user.click(screen.getByRole('button', { name: 'Создать приглашение' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Введите корректный адрес')
    expect(invitationRequests).toHaveLength(0)

    await user.clear(screen.getByLabelText('Электронная почта приглашённого'))
    await user.type(screen.getByLabelText('Электронная почта приглашённого'), 'member@example.com')
    await user.selectOptions(screen.getByLabelText('Роль в рабочем пространстве'), 'admin')
    await user.clear(screen.getByLabelText('Срок действия, часов'))
    await user.type(screen.getByLabelText('Срок действия, часов'), '48')
    await user.click(screen.getByRole('button', { name: 'Создать приглашение' }))

    expect(await screen.findByText('one-time-secret-token')).toBeVisible()
    expect(screen.getByRole('button', { name: 'Скопировать токен' })).toHaveFocus()
    expect(invitationRequests).toHaveLength(1)
    expect(await invitationRequests[0].json()).toEqual({
      email: 'member@example.com',
      workspaceRole: 'admin',
      expiresInHours: 48,
    })
    expect(invitationRequests[0].headers.get('Idempotency-Key')).toMatch(/^[0-9a-f-]{36}$/i)

    await user.click(screen.getByRole('button', { name: 'Скопировать токен' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('one-time-secret-token'))
    expect(screen.getByRole('status')).toHaveTextContent('Токен скопирован')
    fireEvent.click(englishSwitch)
    expect(screen.getByRole('status')).toHaveTextContent('Token copied')
    expect(screen.getByText('one-time-secret-token')).toBeVisible()
    expect(writeText).toHaveBeenCalledTimes(1)
    fireEvent.click(russianSwitch)
    expect(screen.getByRole('status')).toHaveTextContent('Токен скопирован')
    expect(screen.getByText('one-time-secret-token')).toBeVisible()
    expect(writeText).toHaveBeenCalledTimes(1)

    await user.click(screen.getByRole('button', { name: 'Закрыть' }))
    expect(screen.queryByText('one-time-secret-token')).not.toBeInTheDocument()
  })

  it('never stores a one-time invitation token in the mutation cache, including completion after dismissal', async () => {
    let releaseInvitation!: (response: Response) => void
    const invitationResponse = new Promise<Response>((resolve) => { releaseInvitation = resolve })
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/users') return json([])
      if (path === '/api/v1/invitations' && request.method === 'GET') return json([])
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/invitations' && request.method === 'POST') return invitationResponse
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } })
    const user = userEvent.setup()
    render(<QueryClientProvider client={queryClient}><MemoryRouter><UsersPage /></MemoryRouter></QueryClientProvider>)

    await user.click(await screen.findByRole('button', { name: 'Пригласить пользователя' }))
    await user.type(screen.getByLabelText('Электронная почта приглашённого'), 'dismiss@example.com')
    await user.click(screen.getByRole('button', { name: 'Создать приглашение' }))
    await waitFor(() => expect(queryClient.getMutationCache().getAll()).toHaveLength(1))
    expect(JSON.stringify(queryClient.getMutationCache().getAll().map((entry) => entry.state.data))).not.toContain('never-cache-this-token')
    await user.click(screen.getByRole('button', { name: 'Отмена' }))
    expect(JSON.stringify(queryClient.getMutationCache().getAll().map((entry) => entry.state.data))).not.toContain('never-cache-this-token')
    releaseInvitation(json({ id: 'invite', token: 'never-cache-this-token', expiresAt: '2026-10-01T00:00:00Z' }, 201))
    await waitFor(() => expect(queryClient.getMutationCache().getAll()[0]?.state.status).toBe('success'))

    expect(JSON.stringify(queryClient.getMutationCache().getAll().map((entry) => entry.state.data))).not.toContain('never-cache-this-token')
    expect(screen.queryByText('never-cache-this-token')).not.toBeInTheDocument()
  })

  it('updates users and protects session and invitation revocation with confirmation', async () => {
    window.history.replaceState({}, '', '/admin/users')
    const mutations: string[] = []
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
      const url = new URL(request.url, window.location.origin)
      if (url.pathname === '/api/v1/auth/session') {
        return json({ sessionId: 'session-1', userId: 'admin-1', workspaceId: 'workspace-1', workspaceAdmin: true })
      }
      if (url.pathname === '/api/v1/users' && request.method === 'GET') {
        return json([
          { id: 'admin-1', email: 'admin@example.com', displayName: 'Администратор', workspaceRole: 'admin', status: 'active' },
          { id: 'user-2', email: 'member@example.com', displayName: 'Участник', workspaceRole: 'member', status: 'active' },
        ])
      }
      if (url.pathname === '/api/v1/invitations' && request.method === 'GET') {
        return json([{ id: 'invite-2', email: 'pending@example.com', workspaceRole: 'member', status: 'pending', expiresAt: '2026-10-01T00:00:00Z' }])
      }
      if (url.pathname === '/api/v1/auth/csrf') return json({ token: 'csrf-token' })
      if (url.pathname === '/api/v1/users/user-2' && request.method === 'PATCH') {
        mutations.push(`${request.method} ${url.pathname} ${await request.text()}`)
        return json({ id: 'user-2', workspaceRole: 'admin', status: 'suspended' })
      }
      if (url.pathname === '/api/v1/users/user-2/revoke-sessions' && request.method === 'POST') {
        mutations.push(`${request.method} ${url.pathname}`)
        return new Response(null, { status: 204 })
      }
      if (url.pathname === '/api/v1/invitations/invite-2' && request.method === 'DELETE') {
        mutations.push(`${request.method} ${url.pathname}`)
        return new Response(null, { status: 204 })
      }
      throw new Error(`Unexpected request: ${request.method} ${url.pathname}`)
    })
    vi.stubGlobal('fetch', fetcher)
    const user = userEvent.setup()

    render(<App />)
    expect(await screen.findByRole('navigation', { name: 'Навигация администрирования' })).toBeVisible()
    expect(document.documentElement).toHaveAttribute('lang', 'ru')
    expect(screen.getByRole('link', { name: 'Пользователи' })).toHaveAttribute('aria-current', 'page')
    const userRow = (await screen.findByText('member@example.com')).closest('tr')!
    await user.selectOptions(userRow.querySelectorAll('select')[0], 'suspended')
    await user.selectOptions(userRow.querySelectorAll('select')[1], 'admin')
    await user.click(userRow.querySelector('button')!)
    await waitFor(() => expect(mutations.some((entry) => entry.includes('"status":"suspended"') && entry.includes('"workspaceRole":"admin"'))).toBe(true))

    await user.click(screen.getByRole('button', { name: 'Отозвать сеансы участника Участник' }))
    expect(screen.getByRole('dialog', { name: 'Отозвать все сеансы?' })).toBeVisible()
    await user.click(screen.getByRole('button', { name: 'Подтвердить отзыв сеансов' }))
    await waitFor(() => expect(mutations).toContain('POST /api/v1/users/user-2/revoke-sessions'))

    await user.click(screen.getByRole('button', { name: 'Отозвать приглашение pending@example.com' }))
    expect(screen.getByRole('dialog', { name: 'Отозвать приглашение?' })).toBeVisible()
    await user.click(screen.getByRole('button', { name: 'Подтвердить отзыв приглашения' }))
    await waitFor(() => expect(mutations).toContain('DELETE /api/v1/invitations/invite-2'))
  })

  it('reuses an idempotency key only while retrying the same invitation payload', async () => {
    window.history.replaceState({}, '', '/admin/users')
    const keys: string[] = []
    let attempts = 0
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') return json({ sessionId: 's', userId: 'admin', workspaceId: 'w', workspaceAdmin: true })
      if (path === '/api/v1/users') return json([])
      if (path === '/api/v1/invitations' && request.method === 'GET') return json([])
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/invitations' && request.method === 'POST') {
        keys.push(request.headers.get('Idempotency-Key') ?? '')
        attempts += 1
        if (attempts < 3) return json({ type: 'x', title: 'Unavailable', status: 503, code: 'unavailable', detail: 'internal' }, 503)
        return json({ id: 'invite', token: 'token', expiresAt: '2026-10-01T00:00:00Z' }, 201)
      }
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    const user = userEvent.setup()
    render(<App />)
    await user.click(await screen.findByRole('button', { name: 'Пригласить пользователя' }))
    await user.type(screen.getByLabelText('Электронная почта приглашённого'), 'retry@example.com')
    await user.click(screen.getByRole('button', { name: 'Создать приглашение' }))
    await screen.findByRole('alert')
    await user.click(screen.getByRole('button', { name: 'Создать приглашение' }))
    await waitFor(() => expect(keys).toHaveLength(2))
    expect(keys[1]).toBe(keys[0])

    await user.clear(screen.getByLabelText('Срок действия, часов'))
    await user.type(screen.getByLabelText('Срок действия, часов'), '48')
    await user.click(screen.getByRole('button', { name: 'Создать приглашение' }))
    await screen.findByText('token')
    expect(keys[2]).not.toBe(keys[1])
  })

  it('refreshes the session after self-demotion and leaves the admin console', async () => {
    window.history.replaceState({}, '', '/admin/users')
    let sessionRequests = 0
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') {
        sessionRequests += 1
        return json({ sessionId: 's', userId: 'admin', workspaceId: 'w', workspaceAdmin: sessionRequests === 1 })
      }
      if (path === '/api/v1/users' && request.method === 'GET') return json([{ id: 'admin', email: 'admin@example.com', displayName: 'Администратор', workspaceRole: 'admin', status: 'active' }])
      if (path === '/api/v1/invitations') return json([])
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/users/admin' && request.method === 'PATCH') return json({ id: 'admin', workspaceRole: 'member', status: 'active' })
      if (path === '/api/v1/spaces') return json([])
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    const user = userEvent.setup()
    render(<App />)

    const row = (await screen.findByText('admin@example.com')).closest('tr')!
    await user.selectOptions(row.querySelectorAll('select')[1], 'member')
    await user.click(row.querySelector('button')!)

    expect(await screen.findByText('Нет доступных пространств. Обратитесь к администратору.')).toBeVisible()
    expect(sessionRequests).toBeGreaterThanOrEqual(2)
  })

  it('refreshes the session after self-revocation and follows confirmed 401 semantics', async () => {
    window.history.replaceState({}, '', '/admin/users')
    let revoked = false
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') return revoked
        ? json({ type: 'x', title: 'Unauthorized', status: 401, code: 'unauthorized' }, 401)
        : json({ sessionId: 's', userId: 'admin', workspaceId: 'w', workspaceAdmin: true })
      if (path === '/api/v1/users') return json([{ id: 'admin', email: 'admin@example.com', displayName: 'Администратор', workspaceRole: 'admin', status: 'active' }])
      if (path === '/api/v1/invitations') return json([])
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/users/admin/revoke-sessions' && request.method === 'POST') { revoked = true; return new Response(null, { status: 204 }) }
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    const user = userEvent.setup()
    render(<App />)

    await user.click(await screen.findByRole('button', { name: 'Отозвать сеансы участника Администратор' }))
    await user.click(screen.getByRole('button', { name: 'Подтвердить отзыв сеансов' }))

    expect(await screen.findByRole('heading', { name: 'Вход' })).toBeVisible()
  })
})
