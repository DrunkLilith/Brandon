import { cleanup, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { App } from '../app'

function json(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': status >= 400 ? 'application/problem+json' : 'application/json' } })
}

function requestOf(input: RequestInfo | URL, init?: RequestInit) {
  return input instanceof Request ? input : new Request(new URL(String(input), window.location.origin), init)
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  window.history.replaceState({}, '', '/')
})

describe('groups administration', () => {
  it('searches active users and performs idempotent member add and remove operations', async () => {
    window.history.replaceState({}, '', '/admin/groups')
    const mutations: string[] = []
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = requestOf(input, init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') return json({ sessionId: 's', userId: 'admin', workspaceId: 'w', workspaceAdmin: true })
      if (path === '/api/v1/groups' && request.method === 'GET') return json([{ id: 'group-1', name: 'Редакторы', description: 'Команда', memberIds: ['user-1'], archivedAt: null }])
      if (path === '/api/v1/users') return json([
        { id: 'user-1', email: 'ivan@example.com', displayName: 'Иван Иванов', workspaceRole: 'member', status: 'active' },
        { id: 'user-2', email: 'anna@example.com', displayName: 'Анна Смирнова', workspaceRole: 'member', status: 'active' },
        { id: 'user-3', email: 'sleep@example.com', displayName: 'Спящий пользователь', workspaceRole: 'member', status: 'suspended' },
      ])
      if (path === '/api/v1/groups/archived') return json({ items: [], nextCursor: null })
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/groups/group-1/members/user-2' && request.method === 'PUT') { mutations.push(`PUT ${path}`); return new Response(null, { status: 204 }) }
      if (path === '/api/v1/groups/group-1/members/user-1' && request.method === 'DELETE') { mutations.push(`DELETE ${path}`); return new Response(null, { status: 204 }) }
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    const user = userEvent.setup()
    render(<App />)

    const group = await screen.findByRole('region', { name: 'Группа Редакторы' })
    await user.type(within(group).getByLabelText('Поиск активного пользователя'), 'Анна')
    expect(within(group).getByRole('button', { name: 'Добавить Анна Смирнова' })).toBeVisible()
    expect(within(group).queryByText('Спящий пользователь')).not.toBeInTheDocument()
    await user.click(within(group).getByRole('button', { name: 'Добавить Анна Смирнова' }))
    await waitFor(() => expect(mutations).toContain('PUT /api/v1/groups/group-1/members/user-2'))
    await user.click(within(group).getByRole('button', { name: 'Удалить Иван Иванов' }))
    await waitFor(() => expect(mutations).toContain('DELETE /api/v1/groups/group-1/members/user-1'))
  })
})

describe('spaces administration', () => {
  it('validates and creates a typed user or group grant from discovered subjects', async () => {
    window.history.replaceState({}, '', '/admin/spaces')
    const grantBodies: unknown[] = []
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = requestOf(input, init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') return json({ sessionId: 's', userId: 'admin', workspaceId: 'w', workspaceAdmin: true })
      if (path === '/api/v1/spaces' && request.method === 'GET') return json([{ id: 'space-1', name: 'Проект', description: null, effectiveRole: 'admin', archivedAt: null }])
      if (path === '/api/v1/spaces/archived') return json({ items: [], nextCursor: null })
      if (path === '/api/v1/spaces/space-1/grants' && request.method === 'GET') return json([])
      if (path === '/api/v1/spaces/space-1/subjects') return json({ items: [
        { id: 'user-1', label: 'Иван Иванов', type: 'user', status: 'active' },
        { id: 'group-1', label: 'Редакторы', type: 'group', status: 'active' },
      ], nextCursor: null })
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/spaces/space-1/grants' && request.method === 'POST') {
        grantBodies.push(await request.json())
        return json({ id: 'grant-1', subjectId: 'group-1', subjectType: 'group', role: 'editor' }, 201)
      }
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    const user = userEvent.setup()
    render(<App />)

    await screen.findByRole('heading', { name: 'Пространства' })
    await user.click(await screen.findByRole('button', { name: 'Добавить право доступа' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Выберите пользователя или группу')
    expect(grantBodies).toHaveLength(0)

    await user.selectOptions(screen.getByLabelText('Тип субъекта'), 'group')
    await user.selectOptions(screen.getByLabelText('Пользователь или группа'), 'group-1')
    await user.selectOptions(screen.getByLabelText('Роль доступа'), 'editor')
    await user.click(screen.getByRole('button', { name: 'Добавить право доступа' }))
    await waitFor(() => expect(grantBodies).toEqual([{ subjectId: 'group-1', subjectType: 'group', role: 'editor' }]))
  })

  it('updates an active space through the typed patch operation', async () => {
    window.history.replaceState({}, '', '/admin/spaces')
    const updates: unknown[] = []
    vi.stubGlobal('fetch', vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = requestOf(input, init)
      const path = new URL(request.url).pathname
      if (path === '/api/v1/auth/session') return json({ sessionId: 's', userId: 'admin', workspaceId: 'w', workspaceAdmin: true })
      if (path === '/api/v1/spaces' && request.method === 'GET') return json([{ id: 'space-1', name: 'Проект', description: 'Старое описание', effectiveRole: 'admin', archivedAt: null }])
      if (path === '/api/v1/spaces/archived') return json({ items: [], nextCursor: null })
      if (path === '/api/v1/spaces/space-1/grants') return json([])
      if (path === '/api/v1/spaces/space-1/subjects') return json({ items: [], nextCursor: null })
      if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
      if (path === '/api/v1/spaces/space-1' && request.method === 'PATCH') {
        updates.push(await request.json())
        return json({ id: 'space-1', name: 'Новый проект', description: 'Новое описание', effectiveRole: 'admin', archivedAt: null })
      }
      throw new Error(`Unexpected request: ${request.method} ${path}`)
    }))
    const user = userEvent.setup()
    render(<App />)

    await user.click(await screen.findByText('Изменить пространство'))
    await user.clear(screen.getByLabelText('Название выбранного пространства'))
    await user.type(screen.getByLabelText('Название выбранного пространства'), 'Новый проект')
    await user.clear(screen.getByLabelText('Описание выбранного пространства'))
    await user.type(screen.getByLabelText('Описание выбранного пространства'), 'Новое описание')
    await user.click(screen.getByRole('button', { name: 'Сохранить пространство' }))
    await waitFor(() => expect(updates).toEqual([{ name: 'Новый проект', description: 'Новое описание' }]))
  })
})
