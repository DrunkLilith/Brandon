import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, expect, it, vi } from 'vitest'
import { PageToolbar } from './page-toolbar'
import { treeKey } from './queries'
import { apiClient } from '../api/client'

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})
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
const nodes = [
  {
    id: 'p',
    parentId: null,
    title: 'Page',
    sortKey: 'a',
    version: 1,
    restricted: false,
    capabilities: page.capabilities,
  },
]
const spaces = ['s', 'destination'].map((id) => ({
  id,
  name: id,
  description: null,
  archivedAt: null,
  effectiveRole: 'editor' as const,
}))
const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: {
      'content-type':
        status >= 400 ? 'application/problem+json' : 'application/json',
    },
  })
const problem = (status: number) =>
  json(
    { status, type: 'about:blank', title: 'Private', code: 'unavailable' },
    status,
  )

function renderToolbar(cache: QueryClient) {
  cache.setQueryData(treeKey('s'), nodes)
  render(
    <QueryClientProvider client={cache}>
      <MemoryRouter>
        <PageToolbar
          spaceId="s"
          page={page}
          nodes={nodes}
          spaces={spaces}
          canCreate
        />
      </MemoryRouter>
    </QueryClientProvider>,
  )
  fireEvent.click(screen.getByRole('button', { name: 'Переместить' }))
  fireEvent.change(screen.getByLabelText('Пространство назначения'), {
    target: { value: 'destination' },
  })
}

it('blocks a move when the destination GET failed without a resolved tree', async () => {
  const cache = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })
  let posts = 0
  apiClient.invalidateCsrf()
  vi.stubGlobal('fetch', async (input: Request | string) => {
    const request =
      typeof input === 'string'
        ? new Request(new URL(input, location.origin))
        : input
    if (request.url.endsWith('/csrf')) return json({ token: 'csrf' })
    if (request.method === 'POST') {
      posts++
      return problem(409)
    }
    return problem(503)
  })
  renderToolbar(cache)
  await screen.findByRole('alert')
  expect(screen.getByRole('button', { name: 'Применить' })).toBeDisabled()
  // Guard the form too, not merely the clickable button.
  fireEvent.submit(
    screen.getByRole('button', { name: 'Применить' }).closest('form')!,
  )
  await waitFor(() => expect(cache.isMutating()).toBe(0))
  expect(posts).toBe(0)
  expect(cache.getQueryData(treeKey('destination'))).toBeUndefined()
  expect(cache.getQueryData(treeKey('s'))).toEqual(nodes)
})

it.each(['absent', 'failed', 'resolved'] as const)(
  'restores the exact %s destination cache after a rejected move',
  async (initial) => {
    const cache = new QueryClient({
      defaultOptions: { queries: { retry: false, staleTime: Infinity } },
    })
    let rejectMove!: (value: Response) => void
    let posts = 0
    let gets = 0
    apiClient.invalidateCsrf()
    vi.stubGlobal('fetch', async (input: Request | string) => {
      const request =
        typeof input === 'string'
          ? new Request(new URL(input, location.origin))
          : input
      if (request.url.endsWith('/csrf')) return json({ token: 'csrf' })
      if (request.method === 'POST') {
        posts++
        return new Promise<Response>((resolve) => {
          rejectMove = resolve
        })
      }
      gets++
      return initial === 'absent' && gets > 1 ? problem(503) : json([])
    })
    renderToolbar(cache)
    await waitFor(() =>
      expect(screen.getByRole('button', { name: 'Применить' })).toBeEnabled(),
    )
    // Model cache eviction/refetch failure between the resolved destination UI
    // and the optimistic transaction, without replacing QueryClient behavior.
    if (initial === 'absent')
      cache.removeQueries({ queryKey: treeKey('destination'), exact: true })
    if (initial === 'failed')
      cache
        .getQueryCache()
        .find({ queryKey: treeKey('destination') })!
        .setState({
          data: undefined,
          status: 'error',
          error: new Error('background GET failed'),
        })
    const before = cache.getQueryState(treeKey('destination'))
    fireEvent.submit(
      screen.getByRole('button', { name: 'Применить' }).closest('form')!,
    )
    await waitFor(() => expect(posts).toBe(1))
    expect(cache.getQueryData(treeKey('destination'))).toEqual(nodes)
    const removedStates: unknown[] = []
    const unsubscribe = cache.getQueryCache().subscribe((event) => {
      if (event.type === 'removed' && event.query.queryKey[2] === 'destination')
        removedStates.push(cache.getQueryState(treeKey('destination')))
    })
    rejectMove(problem(409))
    await waitFor(() => expect(cache.isMutating()).toBe(0))
    expect(cache.getQueryData(treeKey('s'))).toEqual(nodes)
    if (initial === 'absent') {
      expect(removedStates).toEqual([undefined])
      // The still-open target selector may start a new GET after removal. Its
      // 503 must leave no optimistic data; it must not resurrect the ghost.
      await waitFor(() =>
        expect(cache.getQueryState(treeKey('destination'))?.status).toBe(
          'error',
        ),
      )
      expect(cache.getQueryData(treeKey('destination'))).toBeUndefined()
      expect(screen.getByRole('button', { name: 'Применить' })).toBeDisabled()
    } else expect(cache.getQueryState(treeKey('destination'))).toEqual(before)
    unsubscribe()
  },
)
