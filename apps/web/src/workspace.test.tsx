import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, it, vi } from 'vitest'
import { App } from './app'
import { apiClient } from './api/client'

// The adapter is the only non-network boundary replaced; canonical Markdown and all workspace behavior stay real.
vi.mock('./editor/milkdown-adapter', () => ({
  MilkdownAdapter: ({
    value,
    onChange,
  }: {
    value: string
    onChange(value: string): void
  }) => (
    <textarea
      aria-label="Визуальный Markdown"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    />
  ),
}))

const page = {
  id: 'p',
  spaceId: 's',
  parentId: null,
  stableKey: 'key',
  title: 'Начало',
  markdown: '# Текст',
  properties: {},
  restricted: true,
  version: 1,
  capabilities: { edit: true, managePermissions: true },
}
const tree = [
  {
    id: 'p',
    parentId: null,
    title: 'Начало',
    sortKey: 'a',
    version: 1,
    restricted: true,
    capabilities: page.capabilities,
  },
]
const json = (value: unknown, status = 200) =>
  new Response(JSON.stringify(value), {
    status,
    headers: {
      'content-type':
        status >= 400 ? 'application/problem+json' : 'application/json',
    },
  })
const problem = (status = 409) =>
  json(
    {
      type: 'about:blank',
      title: 'Conflict',
      code: 'version_conflict',
      status,
    },
    status,
  )
type Handler = (request: Request) => Promise<Response> | Response | undefined
function setup(
  handler: Handler = () => undefined,
  nodes = tree,
  route = '/app/spaces/s/pages/p',
) {
  window.history.replaceState({}, '', route)
  apiClient.invalidateCsrf()
  const fetcher = vi.fn(async (input: Request | string) => {
    const request =
      typeof input === 'string'
        ? new Request(new URL(input, location.origin))
        : input
    const custom = handler(request)
    if (custom) return custom
    const path = new URL(request.url).pathname
    if (path === '/api/v1/auth/session')
      return json({
        sessionId: 'session',
        userId: 'u',
        workspaceId: 'w',
        workspaceAdmin: true,
      })
    if (path === '/api/v1/auth/csrf') return json({ token: 'csrf' })
    if (path === '/api/v1/spaces')
      return json([
        {
          id: 's',
          name: 'Документы',
          description: null,
          effectiveRole: 'admin',
          archivedAt: null,
        },
      ])
    if (path === '/api/v1/pages') return json(nodes)
    if (path === '/api/v1/pages/p') return json(page)
    if (path.endsWith('/attachments')) return json([])
    if (path.endsWith('/backlinks')) return json([])
    if (path.endsWith('/revisions'))
      return json([
        { version: 1, authorId: 'u', createdAt: '2026-09-01T00:00:00Z' },
      ])
    if (path.endsWith('/permissions'))
      return json([{ subjectType: 'user', subjectId: 'u2', role: 'viewer' }])
    if (path.endsWith('/subjects'))
      return json({
        items: [
          { type: 'user', id: 'u2', label: 'Читатель', status: 'active' },
        ],
        nextCursor: null,
      })
    throw new Error(`Unhandled ${request.method} ${path}`)
  })
  vi.stubGlobal('fetch', fetcher)
  render(<App />)
  return { user: userEvent.setup(), fetcher }
}
afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
  localStorage.clear()
})

it('loads a real deep link with titles and bounded keyboard navigable tree rows', async () => {
  const nodes = Array.from({ length: 1000 }, (_, i) => ({
    ...tree[0],
    id: i === 0 ? 'p' : `p${i}`,
    title: `Страница ${i}`,
    sortKey: String(i).padStart(5, '0'),
  }))
  setup(() => undefined, nodes)
  expect(await screen.findByLabelText('Название страницы')).toHaveValue(
    'Начало',
  )
  const nav = screen.getByRole('tree')
  expect(within(nav).getAllByRole('treeitem').length).toBeLessThan(40)
  nav.focus()
  fireEvent.keyDown(nav, { key: 'End' })
  expect(
    await screen.findByRole('treeitem', { name: 'Страница 999' }),
  ).toBeVisible()
  expect(nav.getAttribute('aria-activedescendant')).toBe(
    screen.getByRole('treeitem', { name: 'Страница 999' }).id,
  )
})

it.each([
  'Создать дочернюю',
  'Переименовать',
  'Переместить',
  'Архивировать',
  'Поднять',
])(
  'optimistically applies %s and restores exact tree and route on failure',
  async (action) => {
    let fail: ((r: Response) => void) | undefined
    const { user } = setup(
      (request) =>
        request.method !== 'GET'
          ? new URL(request.url).pathname === '/api/v1/auth/csrf'
            ? undefined
            : new Promise<Response>((resolve) => {
                fail = resolve
              })
          : undefined,
      action === 'Поднять'
        ? [{ ...tree[0], id: 'first', title: 'Первый', sortKey: '0' }, ...tree]
        : tree,
    )
    await screen.findByLabelText('Название страницы')
    await user.click(screen.getByRole('button', { name: action }))
    if (action === 'Создать дочернюю' || action === 'Переименовать') {
      await user.clear(screen.getByLabelText('Новое название'))
      await user.type(screen.getByLabelText('Новое название'), 'Изменено')
    }
    if (action === 'Переместить')
      await user.selectOptions(
        screen.getByLabelText('Родитель страницы'),
        'root',
      )
    await user.click(screen.getByRole('button', { name: 'Применить' }))
    await waitFor(() => expect(fail).toBeDefined())
    if (action === 'Переименовать' || action === 'Создать дочернюю')
      expect(
        within(screen.getByRole('tree')).getByText('Изменено'),
      ).toBeVisible()
    if (action === 'Архивировать')
      expect(within(screen.getByRole('tree')).queryByText('Начало')).toBeNull()
    fail!(problem())
    expect(await screen.findByRole('alert')).toBeVisible()
    expect(within(screen.getByRole('tree')).getByText('Начало')).toBeVisible()
    expect(within(screen.getByRole('tree')).queryByText('Изменено')).toBeNull()
    expect(window.location.pathname).toBe('/app/spaces/s/pages/p')
  },
)

it('recovers versioned drafts, shares raw/rich Markdown and clears only acknowledged draft', async () => {
  localStorage.setItem(
    'w:p:1',
    JSON.stringify({
      title: 'Черновик',
      markdown: 'Локальный текст',
      properties: {},
      restricted: true,
    }),
  )
  localStorage.setItem('w:p:99', 'other')
  let submitted: unknown
  const { user } = setup((request) =>
    request.method === 'PATCH'
      ? (async () => {
          submitted = await request.json()
          return json({ id: 'p', version: 2 })
        })()
      : undefined,
  )
  expect(await screen.findByLabelText('Название страницы')).toHaveValue(
    'Черновик',
  )
  await user.click(screen.getByRole('button', { name: 'Исходный Markdown' }))
  await user.type(screen.getByLabelText('Markdown'), ' новый')
  await user.click(screen.getByRole('button', { name: 'Визуальный редактор' }))
  expect(screen.getByLabelText('Визуальный Markdown')).toHaveValue(
    'Локальный текст новый',
  )
  expect(JSON.parse(localStorage.getItem('w:p:1')!).markdown).toBe(
    'Локальный текст новый',
  )
  await user.click(screen.getByRole('button', { name: 'Сохранить версию' }))
  await waitFor(() => expect(localStorage.getItem('w:p:1')).toBeNull())
  expect(localStorage.getItem('w:p:99')).toBe('other')
  expect(submitted).toMatchObject({
    expectedVersion: 1,
    title: 'Черновик',
    markdown: 'Локальный текст новый',
  })
})

it('retains exact submitted text on conflict and exposes base/current metadata', async () => {
  let conflicted = false
  const { user } = setup((request) => {
    if (request.method === 'PATCH') {
      conflicted = true
      return problem()
    }
    if (conflicted && new URL(request.url).pathname === '/api/v1/pages/p')
      return json({ ...page, version: 4, title: 'Сервер' })
  })
  await screen.findByLabelText('Название страницы')
  await user.click(screen.getByRole('button', { name: 'Исходный Markdown' }))
  await user.type(screen.getByLabelText('Markdown'), ' мой текст')
  await user.click(screen.getByRole('button', { name: 'Сохранить версию' }))
  const dialog = await screen.findByRole('dialog', { name: 'Конфликт версий' })
  expect(await within(dialog).findByText(/Текущая версия: 4/)).toBeVisible()
  expect(within(dialog).getByText(/Базовая версия: 1/)).toBeVisible()
  expect(screen.getByLabelText('Markdown')).toHaveValue('# Текст мой текст')
  expect(JSON.parse(localStorage.getItem('w:p:1')!).markdown).toBe(
    '# Текст мой текст',
  )
})

it('loads revision bodies only on diff and confirms restore with current version', async () => {
  let payloads = 0
  let restores = 0
  let sent: unknown
  const { user } = setup((request) => {
    if (new URL(request.url).pathname.endsWith('/revisions/1')) {
      payloads++
      return json({
        id: 'r',
        pageId: 'p',
        version: 1,
        title: 'Раньше',
        markdown: '<img src=x onerror=alert(1)>',
        properties: {},
        authorId: 'u',
        createdAt: '2026-09-01T00:00:00Z',
      })
    }
    if (
      request.method === 'POST' &&
      new URL(request.url).pathname.endsWith('/restore')
    )
      return (async () => {
        restores++
        sent = await request.json()
        return json({ id: 'p', version: 2 })
      })()
  })
  await screen.findByLabelText('Название страницы')
  await user.click(screen.getByRole('button', { name: 'История' }))
  expect(await screen.findByText('Версия 1')).toBeVisible()
  expect(payloads).toBe(0)
  await user.click(screen.getByRole('button', { name: 'Сравнить версию 1' }))
  expect(
    await screen.findByText(/<img src=x onerror=alert\(1\)>/),
  ).toBeVisible()
  expect(document.querySelector('img')).toBeNull()
  await user.click(
    screen.getByRole('button', { name: 'Восстановить версию 1' }),
  )
  expect(restores).toBe(0)
  await user.click(
    screen.getByRole('button', { name: 'Подтвердить восстановление' }),
  )
  await waitFor(() => expect(restores).toBe(1))
  expect(sent).toEqual({ version: 1, expectedVersion: 1 })
})

it('lists grants and updates/removes typed permission subjects', async () => {
  const mutations: string[] = []
  const { user } = setup((request) => {
    if (['PUT', 'DELETE'].includes(request.method)) {
      mutations.push(`${request.method} ${new URL(request.url).pathname}`)
      return request.method === 'DELETE'
        ? new Response(null, { status: 204 })
        : json({ subjectType: 'user', subjectId: 'u2', role: 'editor' })
    }
  })
  await screen.findByLabelText('Название страницы')
  await user.click(screen.getByRole('button', { name: 'Права' }))
  await user.selectOptions(await screen.findByLabelText('Роль u2'), 'editor')
  await waitFor(() =>
    expect(mutations).toContain('PUT /api/v1/pages/p/permissions/user%3Au2'),
  )
  await user.click(screen.getByRole('button', { name: 'Удалить право u2' }))
  await waitFor(() =>
    expect(mutations).toContain('DELETE /api/v1/pages/p/permissions/user%3Au2'),
  )
})

it.each(['Control', 'Meta'])(
  'opens focused search with %s+K, persists filters and renders snippets as text',
  async (modifier) => {
    const { user, fetcher } = setup((request) =>
      new URL(request.url).pathname === '/api/v1/search'
        ? json([
            {
              pageId: 'p',
              title: 'Найдено',
              snippet: '<img src=x onerror=alert(1)>',
              updatedAt: '2026-09-01T00:00:00Z',
            },
          ])
        : undefined,
    )
    await screen.findByLabelText('Название страницы')
    await user.keyboard(`{${modifier}>}k{/${modifier}}`)
    expect(
      await screen.findByRole('dialog', { name: 'Поиск по знаниям' }),
    ).toBeVisible()
    expect(screen.getByLabelText('Поисковый запрос')).toHaveFocus()
    await user.type(screen.getByLabelText('Поисковый запрос'), 'текст')
    await user.type(screen.getByLabelText('Автор'), 'u2')
    await user.click(screen.getByRole('button', { name: 'Найти' }))
    expect(
      await screen.findByText('<img src=x onerror=alert(1)>'),
    ).toBeVisible()
    expect(document.querySelector('img')).toBeNull()
    expect(new URLSearchParams(location.search).get('q')).toBe('текст')
    expect(new URLSearchParams(location.search).get('author')).toBe('u2')
    expect(
      fetcher.mock.calls.some(
        ([r]) =>
          r instanceof Request &&
          new URL(r.url).searchParams.get('author') === 'u2',
      ),
    ).toBe(true)
  },
)

it('validates upload, shows indeterminate progress and cancels the actual request', async () => {
  let signal: AbortSignal | undefined
  const { user } = setup((request) =>
    request.method === 'POST' &&
    new URL(request.url).pathname.endsWith('/attachments')
      ? new Promise<Response>((_, reject) => {
          signal = request.signal
          signal.addEventListener('abort', () =>
            reject(new DOMException('Aborted', 'AbortError')),
          )
        })
      : undefined,
  )
  await screen.findByLabelText('Название страницы')
  await user.upload(screen.getByLabelText('Файл'), new File([], 'empty.txt'))
  expect(await screen.findByRole('alert')).toHaveTextContent('пуст')
  await user.upload(
    screen.getByLabelText('Файл'),
    new File(['abc'], 'file.txt'),
  )
  await user.click(screen.getByRole('button', { name: 'Загрузить файл' }))
  expect(await screen.findByRole('progressbar')).not.toHaveAttribute(
    'aria-valuenow',
  )
  await waitFor(() => expect(signal).toBeDefined())
  await user.click(screen.getByRole('button', { name: 'Отменить загрузку' }))
  await waitFor(() => expect(signal?.aborted).toBe(true))
  expect(await screen.findByText('Загрузка отменена')).toBeVisible()
  expect(screen.getByText(/Шифрование на сервере/)).toBeVisible()
})

it('keeps the same upload idempotency key across an uncertain retry', async () => {
  const keys: (string | null)[] = []
  const { user } = setup((request) => {
    if (
      request.method === 'POST' &&
      new URL(request.url).pathname.endsWith('/attachments')
    ) {
      keys.push(request.headers.get('Idempotency-Key'))
      return Promise.reject(new TypeError('network lost'))
    }
  })
  await screen.findByLabelText('Название страницы')
  await user.upload(screen.getByLabelText('Файл'), new File(['abc'], 'a.txt'))
  await user.click(screen.getByRole('button', { name: 'Загрузить файл' }))
  await screen.findByRole('alert')
  await user.click(screen.getByRole('button', { name: 'Загрузить файл' }))
  await waitFor(() => expect(keys).toHaveLength(2))
  expect(keys[1]).toBe(keys[0])
  expect(keys[0]).toBeTruthy()
})

it('lets an admin create a restricted page using the real create contract', async () => {
  let sent: unknown
  const { user } = setup((request) => {
    if (
      request.method === 'POST' &&
      new URL(request.url).pathname === '/api/v1/pages'
    )
      return (async () => {
        sent = await request.json()
        return json({ ...page, title: 'Закрыто' }, 201)
      })()
  })
  await screen.findByLabelText('Название страницы')
  await user.click(screen.getByRole('button', { name: 'Создать страницу' }))
  const dialog = screen.getByRole('dialog', { name: 'Создать страницу' })
  await user.type(within(dialog).getByLabelText('Новое название'), 'Закрыто')
  await user.click(within(dialog).getByLabelText('Ограниченный доступ'))
  await user.click(within(dialog).getByRole('button', { name: 'Применить' }))
  await waitFor(() =>
    expect(sent).toMatchObject({
      title: 'Закрыто',
      restricted: true,
      spaceId: 's',
      parentId: null,
    }),
  )
})

it('reorders siblings optimistically and restores exact order after network rejection', async () => {
  let reject!: (reason: unknown) => void
  let sent: unknown
  const { user } = setup(
    (request) =>
      request.method === 'POST' &&
      new URL(request.url).pathname.endsWith('/move')
        ? (async () => {
            sent = await request.json()
            return new Promise<Response>((_, no) => {
              reject = no
            })
          })()
        : undefined,
    [{ ...tree[0], id: 'first', title: 'Первый', sortKey: '0' }, ...tree],
  )
  await screen.findByLabelText('Название страницы')
  await user.click(screen.getByRole('button', { name: 'Поднять' }))
  await user.click(screen.getByRole('button', { name: 'Применить' }))
  await waitFor(() =>
    expect(sent).toEqual({ spaceId: 's', parentId: null, beforeId: 'first' }),
  )
  expect(
    within(screen.getByRole('tree'))
      .getAllByRole('treeitem')
      .map((n) => n.getAttribute('aria-label')),
  ).toEqual(['Начало', 'Первый'])
  reject(new TypeError('offline'))
  await screen.findByRole('alert')
  expect(
    within(screen.getByRole('tree'))
      .getAllByRole('treeitem')
      .map((n) => n.getAttribute('aria-label')),
  ).toEqual(['Первый', 'Начало'])
})

it('updates a clean editor when toolbar rename succeeds and advances its base', async () => {
  let title = 'Начало',
    version = 1
  const { user } = setup((request) => {
    if (new URL(request.url).pathname === '/api/v1/pages/p') {
      if (request.method === 'PATCH')
        return (async () => {
          const body = await request.json()
          title = body.title
          version++
          return json({ id: 'p', version })
        })()
      return json({ ...page, title, version })
    }
  })
  await screen.findByLabelText('Название страницы')
  await user.click(screen.getByRole('button', { name: 'Переименовать' }))
  await user.clear(screen.getByLabelText('Новое название'))
  await user.type(screen.getByLabelText('Новое название'), 'Новое имя')
  await user.click(screen.getByRole('button', { name: 'Применить' }))
  await waitFor(() =>
    expect(screen.getByLabelText('Название страницы')).toHaveValue('Новое имя'),
  )
  expect(screen.getByText('Версия 2')).toBeVisible()
})

it.each([401, 403, 404, 429, 503])(
  'shows a page %s failure instead of empty success or logout',
  async (status) => {
    setup((request) =>
      new URL(request.url).pathname === '/api/v1/pages/p'
        ? problem(status)
        : undefined,
    )
    expect(await screen.findByRole('alert')).toBeVisible()
    expect(location.pathname).toBe('/app/spaces/s/pages/p')
    expect(screen.queryByLabelText('Название страницы')).toBeNull()
  },
)

it('downloads a Blob with its name and revokes the object URL', async () => {
  const created = vi.fn((_blob: Blob) => 'blob:test'),
    revoked = vi.fn()
  vi.stubGlobal(
    'URL',
    class extends URL {
      static createObjectURL = created
      static revokeObjectURL = revoked
    },
  )
  const clicks: string[] = []
  const click = vi
    .spyOn(HTMLAnchorElement.prototype, 'click')
    .mockImplementation(function (this: HTMLAnchorElement) {
      clicks.push(this.download)
    })
  const { user } = setup((request) =>
    new URL(request.url).pathname === '/api/v1/pages/p/attachments'
      ? json([
          {
            id: 'a',
            pageId: 'p',
            name: 'note.txt',
            mime: 'text/plain',
            size: 3,
            archived: false,
          },
        ])
      : new URL(request.url).pathname === '/api/v1/attachments/a'
        ? new Response('abc')
        : undefined,
  )
  await user.click(
    await screen.findByRole('button', { name: 'Скачать note.txt' }),
  )
  await waitFor(() => expect(revoked).toHaveBeenCalledWith('blob:test'))
  expect(clicks).toEqual(['note.txt'])
  expect(created.mock.calls[0][0]).toBeInstanceOf(Blob)
  click.mockRestore()
})

it('never shows the previous session page cache after another user signs in', async () => {
  let loggedOut = false
  const { user } = setup((request) => {
    const path = new URL(request.url).pathname
    if (path === '/api/v1/auth/logout') {
      loggedOut = true
      return new Response(null, { status: 204 })
    }
    if (path === '/api/v1/auth/login')
      return json({
        sessionId: 'session2',
        userId: 'other',
        workspaceId: 'w',
        workspaceAdmin: false,
      })
    if (
      loggedOut &&
      (path === '/api/v1/pages/p' ||
        path === '/api/v1/spaces' ||
        path === '/api/v1/pages')
    )
      return new Promise<Response>(() => {})
  })
  await screen.findByLabelText('Название страницы')
  await user.click(screen.getByRole('button', { name: 'Профиль' }))
  await user.click(screen.getByRole('button', { name: 'Выйти' }))
  await user.type(
    await screen.findByLabelText('Электронная почта'),
    'other@example.com',
  )
  await user.type(screen.getByLabelText('Пароль'), 'password')
  await user.click(screen.getByRole('button', { name: 'Войти' }))
  await waitFor(() => expect(location.pathname).toBe('/app/spaces/s/pages/p'))
  expect(screen.queryByLabelText('Название страницы')).toBeNull()
  expect(screen.queryByText('Начало')).toBeNull()
})

it('adds a valid subject without duplicate grant options and retains pending/error states', async () => {
  let grants: unknown[] = []
  let sent: unknown
  let finish!: (r: Response) => void
  const { user } = setup((request) => {
    const path = new URL(request.url).pathname
    if (path.endsWith('/permissions')) return json(grants)
    if (request.method === 'PUT')
      return (async () => {
        sent = await request.json()
        return new Promise<Response>((resolve) => {
          finish = resolve
        })
      })()
  })
  await screen.findByLabelText('Название страницы')
  await user.click(screen.getByRole('button', { name: 'Права' }))
  await user.selectOptions(
    await screen.findByLabelText('Субъект доступа'),
    'user:u2',
  )
  await user.click(screen.getByRole('button', { name: 'Добавить право' }))
  await waitFor(() => expect(sent).toEqual({ role: 'viewer' }))
  expect(screen.getByRole('button', { name: 'Добавить право' })).toBeDisabled()
  grants = [{ subjectType: 'user', subjectId: 'u2', role: 'viewer' }]
  finish(json(grants[0]))
  await screen.findByLabelText('Роль u2')
  expect(
    within(screen.getByLabelText('Субъект доступа')).queryByRole('option', {
      name: 'Читатель',
    }),
  ).toBeNull()
})

it('does not delete a newer same-version draft written while a save is in flight', async () => {
  let finish!: (response: Response) => void
  const { user } = setup((request) =>
    request.method === 'PATCH'
      ? new Promise<Response>((resolve) => {
          finish = resolve
        })
      : undefined,
  )
  await screen.findByLabelText('Название страницы')
  await user.type(screen.getByLabelText('Название страницы'), ' мой')
  await user.click(screen.getByRole('button', { name: 'Сохранить версию' }))
  await waitFor(() => expect(finish).toBeDefined())
  const newer = {
    title: 'Другая вкладка',
    markdown: 'Более новый текст',
    properties: {},
    restricted: true,
  }
  localStorage.setItem('w:p:1', JSON.stringify(newer))
  finish(json({ id: 'p', version: 2 }))
  await screen.findByText('Версия 2')
  expect(JSON.parse(localStorage.getItem('w:p:1')!)).toEqual(newer)
})

it('keeps unsaved text in memory when storage fails and a toolbar mutation refreshes the page', async () => {
  let title = 'Начало',
    version = 1
  const { user } = setup((request) => {
    if (new URL(request.url).pathname === '/api/v1/pages/p') {
      if (request.method === 'PATCH')
        return (async () => {
          const body = await request.json()
          title = body.title
          version++
          return json({ id: 'p', version })
        })()
      return json({ ...page, title, version })
    }
  })
  await screen.findByLabelText('Название страницы')
  const failure = vi
    .spyOn(Storage.prototype, 'setItem')
    .mockImplementation(() => {
      throw new Error('Quota')
    })
  await user.type(
    screen.getByLabelText('Название страницы'),
    ' мой несохранённый',
  )
  await user.click(screen.getByRole('button', { name: 'Переименовать' }))
  await user.clear(screen.getByLabelText('Новое название'))
  await user.type(screen.getByLabelText('Новое название'), 'На сервере')
  await user.click(screen.getByRole('button', { name: 'Применить' }))
  await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull())
  expect(screen.getByLabelText('Название страницы')).toHaveValue(
    'Начало мой несохранённый',
  )
  failure.mockRestore()
})

it('keeps the editor mounted if current-version metadata fails after a conflict', async () => {
  let conflict = false
  const { user } = setup((request) => {
    if (request.method === 'PATCH') {
      conflict = true
      return problem()
    }
    if (conflict && new URL(request.url).pathname === '/api/v1/pages/p')
      return problem(503)
  })
  await screen.findByLabelText('Название страницы')
  await user.type(screen.getByLabelText('Название страницы'), ' моё')
  await user.click(screen.getByRole('button', { name: 'Сохранить версию' }))
  await screen.findByRole('alert')
  expect(screen.getByLabelText('Название страницы')).toHaveValue('Начало моё')
  expect(screen.getByRole('dialog', { name: 'Конфликт версий' })).toBeVisible()
})

it('reports unknown workspace routes as not found', async () => {
  setup(() => undefined, tree, '/app/unknown-route')
  expect(await screen.findByRole('alert')).toHaveTextContent('Объект не найден')
  expect(screen.queryByRole('heading', { name: 'Обзор' })).toBeNull()
})

it('does not offer move-up for the first sibling', async () => {
  setup()
  await screen.findByLabelText('Название страницы')
  expect(screen.getByRole('button', { name: 'Поднять' })).toBeDisabled()
})

it('loads the current server version only after explicit conflict reload confirmation', async () => {
  let conflicted = false
  const { user } = setup((request) => {
    if (request.method === 'PATCH') {
      conflicted = true
      return problem()
    }
    if (conflicted && new URL(request.url).pathname === '/api/v1/pages/p')
      return json({ ...page, title: 'Серверная версия', version: 4 })
  })
  await screen.findByLabelText('Название страницы')
  await user.type(screen.getByLabelText('Название страницы'), ' моё')
  await user.click(screen.getByRole('button', { name: 'Сохранить версию' }))
  await screen.findByText(/Текущая версия: 4/)
  await user.click(
    screen.getByRole('button', { name: 'Загрузить серверную версию' }),
  )
  expect(screen.getByLabelText('Название страницы')).toHaveValue('Начало моё')
  await user.click(screen.getByRole('button', { name: 'Применить' }))
  expect(screen.getByLabelText('Название страницы')).toHaveValue(
    'Серверная версия',
  )
  expect(screen.getByText('Версия 4')).toBeVisible()
  expect(JSON.parse(localStorage.getItem('w:p:1')!).title).toBe('Начало моё')
})

it('restores all search filters from browser history and keeps snippets as text', async () => {
  const { user } = setup(
    (request) =>
      new URL(request.url).pathname === '/api/v1/search' ? json([]) : undefined,
    tree,
    '/app/spaces/s/pages/p?q=old&space=s&author=u&from=2026-01-01T00%3A00%3A00Z&to=2026-02-01T00%3A00%3A00Z',
  )
  expect(await screen.findByLabelText('Поисковый запрос')).toHaveValue('old')
  const dialog = screen.getByRole('dialog')
  expect(within(dialog).getByLabelText('Пространство')).toHaveValue('s')
  expect(screen.getByLabelText('Автор')).toHaveValue('u')
  expect(screen.getByLabelText('С даты')).toHaveValue('2026-01-01T00:00:00Z')
  expect(screen.getByLabelText('По дату')).toHaveValue('2026-02-01T00:00:00Z')
  await user.clear(screen.getByLabelText('Поисковый запрос'))
  await user.type(screen.getByLabelText('Поисковый запрос'), 'new')
  await user.click(screen.getByRole('button', { name: 'Найти' }))
  window.history.back()
  await waitFor(() =>
    expect(screen.getByLabelText('Поисковый запрос')).toHaveValue('old'),
  )
})
