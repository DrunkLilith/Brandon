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

  it.each([
    ['server failure', () => response({ code: 'unavailable', type: 'x', title: 'Unavailable', status: 503, detail: 'down' }, 503)],
    ['network failure', () => Promise.reject(new TypeError('network unavailable'))],
  ])('distinguishes a login %s from invalid credentials', async (_label, loginResult) => {
    window.history.replaceState({}, '', '/login')
    const fetcher = vi.fn()
      .mockResolvedValueOnce(response({ code: 'authentication_required', type: 'x', title: 'Authentication required', status: 401, detail: 'invalid' }, 401))
      .mockImplementationOnce(loginResult)
    vi.stubGlobal('fetch', fetcher)
    const user = userEvent.setup()
    render(<App />)
    await user.type(await screen.findByLabelText('Электронная почта'), 'admin@example.com')
    await user.type(screen.getByLabelText('Пароль'), 'correct password')
    await user.click(screen.getByRole('button', { name: 'Войти' }))

    expect(await screen.findByRole('alert')).toHaveTextContent('Сервис временно недоступен. Попробуйте ещё раз.')
    expect(screen.queryByText('Неверная почта или пароль')).not.toBeInTheDocument()
  })

  it('redirects a signed-in non-admin away from /admin', async () => {
    window.history.replaceState({}, '', '/admin')
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(response({ sessionId: 's', userId: 'u', workspaceId: 'w', workspaceAdmin: false })))
    render(<App />)
    await waitFor(() => expect(window.location.pathname).toBe('/app'))
  })

  it.each([
    ['server failure', () => response({ code: 'unavailable', type: 'x', title: 'Unavailable', status: 503, detail: 'down' }, 503)],
    ['network failure', () => Promise.reject(new TypeError('network unavailable'))],
  ])('keeps a protected route visible when session loading has a %s', async (_label, result) => {
    window.history.replaceState({}, '', '/app/recent')
    vi.stubGlobal('fetch', vi.fn().mockImplementation(result))
    render(<App />)
    expect(await screen.findByRole('alert')).toHaveTextContent('Не удалось проверить сеанс')
    expect(window.location.pathname).toBe('/app/recent')
    expect(screen.queryByRole('heading', { name: 'Вход' })).not.toBeInTheDocument()
  })

  it('returns to the requested deep link after login', async () => {
    window.history.replaceState({}, '', '/admin/audit')
    vi.stubGlobal('fetch', vi.fn()
      .mockResolvedValueOnce(response({ code: 'authentication_required', type: 'x', title: 'Authentication required', status: 401, detail: 'invalid' }, 401))
      .mockResolvedValueOnce(response({ sessionId: 's', userId: 'u', workspaceId: 'w', workspaceAdmin: true })))
    const user = userEvent.setup()
    render(<App />)
    await user.type(await screen.findByLabelText('Электронная почта'), 'admin@example.com')
    await user.type(screen.getByLabelText('Пароль'), 'correct password')
    await user.click(screen.getByRole('button', { name: 'Войти' }))
    await waitFor(() => expect(window.location.pathname).toBe('/admin/audit'))
  })
})

describe('application shell', () => {
  it('exposes landmarks, router-aware navigation, and honest future affordances', async () => {
    window.history.replaceState({}, '', '/app/recent')
    vi.stubGlobal('fetch', vi.fn()
      .mockResolvedValueOnce(response({ sessionId: 's', userId: 'u', workspaceId: 'w', workspaceAdmin: true }))
      .mockResolvedValueOnce(response({ token: 'csrf' }))
      .mockResolvedValueOnce(new Response(null, { status: 204 })))
    const user = userEvent.setup()
    render(<App />)
    expect(await screen.findByRole('navigation', { name: 'Основная навигация' })).toBeVisible()
    expect(screen.getByRole('combobox', { name: 'Пространство' })).toBeVisible()
    expect(screen.getByRole('region', { name: 'Дерево страниц' })).toBeVisible()
    expect(screen.getByRole('link', { name: 'Недавние' })).toHaveAttribute('aria-current', 'page')
    expect(screen.getByRole('link', { name: 'Обзор' })).not.toHaveAttribute('aria-current')
    const search = screen.getByRole('button', { name: 'Поиск — доступно в следующей версии' })
    expect(search).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Создать страницу — доступно в следующей версии' })).toBeDisabled()
    expect(screen.getByRole('button', { name: 'Профиль' })).toBeVisible()
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', ctrlKey: true, bubbles: true }))
    expect(search).not.toHaveFocus()
    await user.click(screen.getByRole('button', { name: 'Профиль' }))
    await user.click(screen.getByRole('button', { name: 'Выйти' }))
    await waitFor(() => expect(window.location.pathname).toBe('/login'))
  })
})
