import { useQuery } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import {
  Link,
  NavLink,
  useMatch,
  useNavigate,
  useSearchParams,
} from 'react-router-dom'
import { useSession } from '../auth/session'
import { ApiError } from '../api/client'
import { ErrorNotice, LoadingBlock } from '../admin/shared'
import { PageTree } from '../pages/page-tree'
import { PageView } from '../pages/page-view'
import { PageToolbar } from '../pages/page-toolbar'
import { pageQuery, spacesQuery, treeQuery } from '../pages/queries'
import { workspaceMessages as text } from '../pages/messages'
import { SearchDialog } from '../search/search-dialog'
import { Topbar } from './topbar'

export function AppShell() {
  const { session, logout } = useSession(),
    navigate = useNavigate(),
    [params] = useSearchParams()
  const pageMatch = useMatch('/app/spaces/:spaceId/pages/:pageId'),
    spaceMatch = useMatch('/app/spaces/:spaceId')
  const overviewMatch = useMatch('/app')
  const spaceId = pageMatch?.params.spaceId ?? spaceMatch?.params.spaceId ?? '',
    pageId = pageMatch?.params.pageId
  const [searchOpen, setSearchOpen] = useState(!!params.get('q'))
  const spaces = useQuery(spacesQuery),
    tree = useQuery({ ...treeQuery(spaceId), enabled: !!spaceId }),
    page = useQuery({ ...pageQuery(pageId ?? ''), enabled: !!pageId })
  const space = spaces.data?.find((s) => s.id === spaceId)
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setSearchOpen(true)
      }
    }
    window.addEventListener('keydown', shortcut)
    return () => window.removeEventListener('keydown', shortcut)
  }, [])
  const missingSpace =
    (spaceId && spaces.isSuccess && !space) ||
    (!overviewMatch && !pageMatch && !spaceMatch)
  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand-row">
          <span className="brand-mark small" aria-hidden="true">
            K
          </span>
          <strong>Knowledge</strong>
        </div>
        <label className="space-switcher">
          {text.workspace.spaces}
          <select
            value={spaceId}
            onChange={(e) =>
              navigate(
                e.target.value ? `/app/spaces/${e.target.value}` : '/app',
              )
            }
          >
            <option value="">{text.spaces.select}</option>
            {spaces.data?.map((s) => (
              <option key={s.id} value={s.id}>
                {s.name}
              </option>
            ))}
          </select>
        </label>
        <nav aria-label="Основная навигация">
          <NavLink to="/app" end>
            {text.workspace.overview}
          </NavLink>
          {session?.workspaceAdmin && <Link to="/admin">{text.section}</Link>}
        </nav>
        <section aria-label={text.workspace.tree} className="page-tree">
          {tree.isFetching && <LoadingBlock />}
          {tree.isError && <ErrorNotice error={tree.error} />}
          {tree.data && (
            <PageTree
              nodes={tree.data}
              selected={pageId}
              onSelect={(id) => navigate(`/app/spaces/${spaceId}/pages/${id}`)}
            />
          )}
        </section>
      </aside>
      <div className="app-column">
        <Topbar
          onLogout={logout}
          onSearch={() => setSearchOpen(true)}
          space={space}
          page={page.data}
        />
        <main className="workspace-content" aria-label="Рабочая область">
          {spaces.isPending ? (
            <LoadingBlock />
          ) : spaces.isError ? (
            <ErrorNotice error={spaces.error} />
          ) : missingSpace ? (
            <ErrorNotice
              error={
                new ApiError({
                  status: 404,
                  code: 'not_found',
                  title: 'Not found',
                  type: 'about:blank',
                })
              }
            />
          ) : !spaceId ? (
            <section className="empty-state">
              <h1>{text.workspace.overview}</h1>
              {spaces.data.length ? (
                <ul>
                  {spaces.data.map((s) => (
                    <li key={s.id}>
                      <Link to={`/app/spaces/${s.id}`}>{s.name}</Link>
                    </li>
                  ))}
                </ul>
              ) : (
                <p>{text.workspace.noSpaces}</p>
              )}
            </section>
          ) : (
            <>
              <PageToolbar
                spaceId={spaceId}
                page={page.data}
                nodes={tree.data ?? []}
                spaces={spaces.data}
                canCreate={
                  !!space &&
                  ['admin', 'owner', 'editor'].includes(space.effectiveRole)
                }
              />
              {pageId && session ? (
                <PageView
                  pageId={pageId}
                  spaceId={spaceId}
                  workspaceId={session.workspaceId}
                />
              ) : (
                <p>{text.workspace.choose}</p>
              )}
            </>
          )}
        </main>
      </div>
      {searchOpen && (
        <SearchDialog
          key={params.toString()}
          onClose={() => setSearchOpen(false)}
        />
      )}
    </div>
  )
}
