import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react'
import { afterEach, expect, it, vi } from 'vitest'
import {
  Editor,
  EditorStatus,
  editorViewCtx,
  serializerCtx,
} from '@milkdown/core'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { Link, MemoryRouter, Route, Routes } from 'react-router-dom'
import { PageView } from '../pages/page-view'
import { pageKey } from '../pages/queries'
import { apiClient } from '../api/client'
import { MilkdownAdapter } from './milkdown-adapter'

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
  localStorage.clear()
})

it('does not report an authoritative external replacement as a new user edit', async () => {
  const editors = vi.spyOn(Editor, 'make')
  const change = vi.fn()
  const view = render(<MilkdownAdapter value="old" onChange={change} />)
  await waitFor(() =>
    expect(editors.mock.results[0]?.value.status).toBe(EditorStatus.Created),
  )
  view.rerender(<MilkdownAdapter value="server version" onChange={change} />)
  expect(await screen.findByText('server version')).toBeVisible()
  expect(change).not.toHaveBeenCalled()
})

it.each(['raw', 'preview', 'save', 'navigation', 'unmount'] as const)(
  'preserves the latest real rich-editor transaction on immediate %s',
  async (boundary) => {
    // Call-through observation only: transactions, serializer, adapter,
    // PageEditor and draft persistence all remain real.
    const editors = vi.spyOn(Editor, 'make')
    const page = {
      id: 'p',
      spaceId: 's',
      parentId: null,
      stableKey: 'key',
      title: 'Page',
      markdown: 'old',
      properties: {},
      restricted: false,
      version: 1,
      capabilities: { edit: true, managePermissions: false },
    }
    const cache = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    })
    cache.setQueryData(pageKey('p'), page)
    let submitted: { markdown: string } | undefined
    apiClient.invalidateCsrf()
    vi.stubGlobal('fetch', async (input: Request | string) => {
      const request =
        typeof input === 'string'
          ? new Request(new URL(input, location.origin))
          : input
      const path = new URL(request.url).pathname
      const body = path.endsWith('/csrf')
        ? { token: 'csrf' }
        : request.method === 'PATCH'
          ? ((submitted = await request.json()), { id: 'p', version: 2 })
          : []
      return new Response(JSON.stringify(body), {
        headers: { 'content-type': 'application/json' },
      })
    })
    const rendered = render(
      <QueryClientProvider client={cache}>
        <MemoryRouter>
          <Link to="/away">Leave page</Link>
          <Routes>
            <Route
              path="/"
              element={<PageView pageId="p" spaceId="s" workspaceId="w" />}
            />
            <Route path="/away" element={<p>Away</p>} />
          </Routes>
        </MemoryRouter>
      </QueryClientProvider>,
    )
    await waitFor(() =>
      expect(editors.mock.results[0]?.value.status).toBe(EditorStatus.Created),
    )
    const instance: Editor = editors.mock.results[0].value
    const serialize = instance.action((ctx) => {
      const original = ctx.get(serializerCtx)
      const observed = vi.fn(original)
      ctx.set(serializerCtx, observed)
      return observed
    })
    const persist = vi.spyOn(Storage.prototype, 'setItem')
    act(() => {
      instance.action((ctx) => {
        const view = ctx.get(editorViewCtx)
        view.dispatch(view.state.tr.insertText(' NEW', 4))
      })
    })
    expect(serialize).not.toHaveBeenCalled()
    expect(persist).not.toHaveBeenCalled()
    // No debounce delay between the real edit and the next action.
    if (boundary === 'raw') {
      fireEvent.click(screen.getByRole('button', { name: 'Исходный Markdown' }))
      expect(screen.getByLabelText('Markdown')).toHaveValue('old NEW\n')
    } else if (boundary === 'preview') {
      fireEvent.click(screen.getByRole('button', { name: 'Предпросмотр' }))
      expect(await screen.findByText('old NEW')).toBeVisible()
    } else if (boundary === 'save') {
      fireEvent.click(screen.getByRole('button', { name: 'Сохранить версию' }))
      await waitFor(() => expect(submitted?.markdown).toBe('old NEW\n'))
    } else {
      if (boundary === 'navigation')
        fireEvent.click(screen.getByRole('link', { name: 'Leave page' }))
      else rendered.unmount()
      expect(
        JSON.parse(localStorage.getItem('w:p:1') ?? 'null')?.markdown,
      ).toBe('old NEW\n')
    }
    expect(serialize).toHaveBeenCalledTimes(1)
    expect(persist).toHaveBeenCalledTimes(1)
  },
)

it('coalesces a burst of real transactions without synchronous serialization or persistence', async () => {
  const editors = vi.spyOn(Editor, 'make')
  const change = vi.fn((markdown: string) =>
    localStorage.setItem('draft', markdown),
  )
  const rendered = render(<MilkdownAdapter value="old" onChange={change} />)
  await waitFor(() =>
    expect(editors.mock.results[0]?.value.status).toBe(EditorStatus.Created),
  )
  const instance: Editor = editors.mock.results[0].value
  const serialize = instance.action((ctx) => {
    const observed = vi.fn(ctx.get(serializerCtx))
    ctx.set(serializerCtx, observed)
    return observed
  })
  const persist = vi.spyOn(Storage.prototype, 'setItem')
  vi.useFakeTimers()
  act(() => {
    instance.action((ctx) => {
      const view = ctx.get(editorViewCtx)
      for (const letter of [' A', ' B', ' C'])
        view.dispatch(
          view.state.tr.insertText(letter, view.state.doc.content.size - 1),
        )
    })
  })
  expect(serialize).not.toHaveBeenCalled()
  expect(change).not.toHaveBeenCalled()
  expect(persist).not.toHaveBeenCalled()
  act(() => vi.advanceTimersByTime(1000))
  expect(serialize).toHaveBeenCalledTimes(1)
  expect(change).toHaveBeenCalledExactlyOnceWith('old A B C\n')
  expect(localStorage.getItem('draft')).toBe('old A B C\n')
  rendered.unmount()
  act(() => vi.advanceTimersByTime(1000))
  expect(serialize).toHaveBeenCalledTimes(1)
  expect(change).toHaveBeenCalledTimes(1)
})

it('flushes pending text once on cleanup without a late React change notification', async () => {
  const editors = vi.spyOn(Editor, 'make')
  const change = vi.fn()
  const persist = vi.fn((markdown: string) =>
    localStorage.setItem('leaving-draft', markdown),
  )
  const errors = vi.spyOn(console, 'error')
  const rendered = render(
    <MilkdownAdapter value="old" onChange={change} onUnmount={persist} />,
  )
  await waitFor(() =>
    expect(editors.mock.results[0]?.value.status).toBe(EditorStatus.Created),
  )
  const instance: Editor = editors.mock.results[0].value
  const serialize = instance.action((ctx) => {
    const observed = vi.fn(ctx.get(serializerCtx))
    ctx.set(serializerCtx, observed)
    return observed
  })
  vi.useFakeTimers()
  act(() =>
    instance.action((ctx) => {
      const view = ctx.get(editorViewCtx)
      view.dispatch(view.state.tr.insertText(' NEW', 4))
    }),
  )
  rendered.unmount()
  expect(localStorage.getItem('leaving-draft')).toBe('old NEW\n')
  act(() => vi.advanceTimersByTime(1000))
  expect(serialize).toHaveBeenCalledTimes(1)
  expect(persist).toHaveBeenCalledExactlyOnceWith('old NEW\n')
  expect(change).not.toHaveBeenCalled()
  expect(errors).not.toHaveBeenCalled()
})

it('marks rich edits dirty before debounce so a newer background read cannot replace them', async () => {
  const editors = vi.spyOn(Editor, 'make')
  const page = {
    id: 'p',
    spaceId: 's',
    parentId: null,
    stableKey: 'key',
    title: 'Page',
    markdown: 'old',
    properties: {},
    restricted: false,
    version: 1,
    capabilities: { edit: true, managePermissions: false },
  }
  const cache = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  })
  cache.setQueryData(pageKey('p'), page)
  vi.stubGlobal(
    'fetch',
    async () =>
      new Response('[]', { headers: { 'content-type': 'application/json' } }),
  )
  render(
    <QueryClientProvider client={cache}>
      <PageView pageId="p" spaceId="s" workspaceId="w" />
    </QueryClientProvider>,
  )
  await waitFor(() =>
    expect(editors.mock.results[0]?.value.status).toBe(EditorStatus.Created),
  )
  const instance: Editor = editors.mock.results[0].value
  vi.useFakeTimers()
  act(() =>
    instance.action((ctx) => {
      const view = ctx.get(editorViewCtx)
      view.dispatch(view.state.tr.insertText(' NEW', 4))
    }),
  )
  act(() => {
    cache.setQueryData(pageKey('p'), {
      ...page,
      version: 2,
      markdown: 'remote',
    })
    vi.advanceTimersByTime(1)
  })
  fireEvent.click(screen.getByRole('button', { name: 'Исходный Markdown' }))
  expect(screen.getByLabelText('Markdown')).toHaveValue('old NEW\n')
  expect(JSON.parse(localStorage.getItem('w:p:1') ?? 'null')?.markdown).toBe(
    'old NEW\n',
  )
})

it('keeps the mounted in-memory draft through a background 503 and retry when storage fails', async () => {
  const page = {
    id: 'p',
    spaceId: 's',
    parentId: null,
    stableKey: 'key',
    title: 'Page',
    markdown: 'old',
    properties: {},
    restricted: false,
    version: 1,
    capabilities: { edit: true, managePermissions: false },
  }
  const cache = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  })
  cache.setQueryData(pageKey('p'), page)
  let failed = true
  vi.stubGlobal('fetch', async (request: Request) => {
    if (new URL(request.url).pathname === '/api/v1/pages/p')
      return new Response(
        JSON.stringify(
          failed
            ? {
                status: 503,
                code: 'unavailable',
                type: 'about:blank',
                title: 'PRIVATE ERROR',
              }
            : { ...page, version: 2 },
        ),
        {
          status: failed ? 503 : 200,
          headers: {
            'content-type': failed
              ? 'application/problem+json'
              : 'application/json',
          },
        },
      )
    return new Response('[]', {
      headers: { 'content-type': 'application/json' },
    })
  })
  render(
    <QueryClientProvider client={cache}>
      <PageView pageId="p" spaceId="s" workspaceId="w" />
    </QueryClientProvider>,
  )
  const title = screen.getByLabelText('Название страницы')
  vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
    throw new Error('quota')
  })
  fireEvent.change(title, { target: { value: 'Unsaved in memory' } })
  expect(
    screen.getByText(
      'Не удалось сохранить черновик в браузере. Скопируйте текст.',
    ),
  ).toBeVisible()
  await act(() => cache.refetchQueries({ queryKey: pageKey('p') }))
  expect(
    await screen.findByText('Не удалось выполнить запрос. Попробуйте ещё раз.'),
  ).toBeVisible()
  expect(screen.queryByText('PRIVATE ERROR')).not.toBeInTheDocument()
  expect(title).toBeInTheDocument()
  expect(title).toHaveValue('Unsaved in memory')
  failed = false
  await act(() => cache.refetchQueries({ queryKey: pageKey('p') }))
  await waitFor(() =>
    expect(
      screen.queryByText('Не удалось выполнить запрос. Попробуйте ещё раз.'),
    ).not.toBeInTheDocument(),
  )
  expect(screen.getByLabelText('Название страницы')).toBe(title)
  expect(title).toHaveValue('Unsaved in memory')
})
