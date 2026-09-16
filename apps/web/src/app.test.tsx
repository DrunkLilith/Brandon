import { cleanup, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'

import { App } from './app'

function response(body: unknown, status = 200) {
  return new Response(JSON.stringify(body), { status, headers: { 'content-type': status >= 400 ? 'application/problem+json' : 'application/json' } })
}

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  window.history.replaceState({}, '', '/')
})

describe('authentication and route guards', () => {
  it('redirects an anonymous /app visit to login', async () => {
    window.history.replaceState({}, '', '/app')
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response({ code: 'authentication_required', type: 'x', title: 'Authentication required', status: 401, detail: 'invalid' }, 401)))
    render(<App />)
    expect(await screen.findByRole('heading', { name: 'Вход' })).toBeVisible()
    expect(window.location.pathname).toBe('/login')
  })

  it('shows a generic localized error and redirects a valid login to /app', async () => {
    window.history.replaceState({}, '', '/login')
    const fetcher = vi.fn()
      .mockResolvedValueOnce(response({ code: 'authentication_required', type: 'x', title: 'Authentication required', status: 401, detail: 'server-specific detail' }, 401))
      .mockResolvedValueOnce(response({ code: 'authentication_required', type: 'x', title: 'Authentication required', status: 401, detail: 'server-specific detail' }, 401))
      .mockResolvedValueOnce(response({ sessionId: 's', userId: 'u', workspaceId: 'w', workspaceAdmin: true }))
    vi.stubGlobal('fetch', fetcher)
    const user = userEvent.setup()
    render(<App />)
    await user.type(await screen.findByLabelText('Электронная почта'), 'admin@example.com')
    await user.type(screen.getByLabelText('Пароль'), 'wrong password')
    await user.click(screen.getByRole('button', { name: 'Войти' }))
    expect(await screen.findByRole('alert')).toHaveTextContent('Неверная почта или пароль')
    expect(screen.queryByText('server-specific detail')).not.toBeInTheDocument()
    await user.clear(screen.getByLabelText('Пароль'))
    await user.type(screen.getByLabelText('Пароль'), 'correct password')
    await user.click(screen.getByRole('button', { name: 'Войти' }))
    await waitFor(() => expect(window.location.pathname).toBe('/app'))
    expect(await screen.findByRole('navigation', { name: 'Основная навигация' })).toBeVisible()
  })

  it('redirects a signed-in non-admin away from /admin', async () => {
    window.history.replaceState({}, '', '/admin')
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response({ sessionId: 's', userId: 'u', workspaceId: 'w', workspaceAdmin: false })))
    render(<App />)
    await waitFor(() => expect(window.location.pathname).toBe('/app'))
  })
})

describe('application shell', () => {
  it('exposes landmarks and moves focus to search with the global shortcut', async () => {
    window.history.replaceState({}, '', '/app')
    vi.stubGlobal('fetch', vi.fn()
      .mockResolvedValueOnce(response({ sessionId: 's', userId: 'u', workspaceId: 'w', workspaceAdmin: true }))
      .mockResolvedValueOnce(response({ token: 'csrf' }))
      .mockResolvedValueOnce(new Response(null, { status: 204 })))
    const user = userEvent.setup()
    render(<App />)
    expect(await screen.findByRole('navigation', { name: 'Основная навигация' })).toBeVisible()
    expect(screen.getByRole('combobox', { name: 'Пространство' })).toBeVisible()
    expect(screen.getByRole('region', { name: 'Дерево страниц' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Открыть поиск' })).toBeVisible()
    expect(screen.getByRole('button', { name: 'Профиль' })).toBeVisible()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
    expect(screen.getByRole('button', { name: 'Открыть поиск' })).toHaveFocus()
    const input = document.createElement('input')
    document.body.append(input)
    input.focus()
    input.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
    expect(input).toHaveFocus()
    input.remove()
    await user.click(screen.getByRole('button', { name: 'Профиль' }))
    await user.click(screen.getByRole('button', { name: 'Выйти' }))
    await waitFor(() => expect(window.location.pathname).toBe('/login'))
  })
})
