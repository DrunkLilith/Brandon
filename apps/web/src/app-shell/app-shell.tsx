import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState, type FormEvent } from 'react'
import {
  Link,
  NavLink,
  useMatch,
  useNavigate,
  useSearchParams,
} from 'react-router-dom'
import { useSession } from '../auth/session'
import { apiClient, ApiError, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { ErrorNotice, hasOpenModal, LoadingBlock } from '../admin/shared'
import { SpaceAccess, SpaceEditor } from '../admin/spaces'
import { SpaceArchive } from '../pages/space-archive'
import { PageArchive } from '../pages/page-archive'
import { SpaceExports } from '../pages/space-exports'
import { useI18n } from '../i18n'
import { PageTree } from '../pages/page-tree'
import { PageView } from '../pages/page-view'
import { PageToolbar } from '../pages/page-toolbar'
import { pageQuery, spacesQuery, treeQuery } from '../pages/queries'
import { SearchDialog } from '../search/search-dialog'
import { Topbar } from './topbar'

type Space = components['schemas']['Space']

export function AppShell() {
  const { messages: text } = useI18n()
  const { session, logout } = useSession(),
    navigate = useNavigate(),
    [params] = useSearchParams(),
    queryClient = useQueryClient()
  const pageMatch = useMatch('/app/spaces/:spaceId/pages/:pageId'),
    spaceMatch = useMatch('/app/spaces/:spaceId')
  const overviewMatch = useMatch('/app')
  const spaceId = pageMatch?.params.spaceId ?? spaceMatch?.params.spaceId ?? '',
    pageId = pageMatch?.params.pageId
  const [searchOpen, setSearchOpen] = useState(!!params.get('q'))
  const [manageSpace, setManageSpace] = useState(false)
  const [newSpaceName, setNewSpaceName] = useState('')
  const [newSpaceDescription, setNewSpaceDescription] = useState('')
  useEffect(() => { setManageSpace(false) }, [spaceId])
  const spaces = useQuery(spacesQuery),
    tree = useQuery({ ...treeQuery(spaceId), enabled: !!spaceId }),
    page = useQuery({ ...pageQuery(pageId ?? ''), enabled: !!pageId })
  const createSpace = useMutation({
    mutationFn: async ({ name, description }: { name: string; description: string }) =>
      unwrap(await apiClient.POST('/api/v1/spaces', { body: { name, description: description || null } })),
    onSuccess: async (created) => {
      queryClient.setQueryData(
        spacesQuery.queryKey,
        (current: readonly Space[] | undefined) => current?.some((item) => item.id === created.id)
          ? current
          : [...(current ?? []), created],
      )
      await queryClient.invalidateQueries({ queryKey: ['admin', 'spaces'] })
      setNewSpaceName('')
      setNewSpaceDescription('')
      navigate(`/app/spaces/${created.id}`)
    },
  })
  const submitSpace = (event: FormEvent) => {
    event.preventDefault()
    const name = newSpaceName.trim()
    if (!name || createSpace.isPending) return
    createSpace.mutate({ name, description: newSpaceDescription.trim() })
  }
  const space = spaces.data?.find((s) => s.id === spaceId)
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        if (hasOpenModal()) return
        setSearchOpen(true)
      }
    }
    window.addEventListener('keydown', shortcut)
    return () => window.removeEventListener('keydown', shortcut)
  }, [])
  const missingSpace =
    (spaceId && spaces.isSuccess && !space) ||
    (!overviewMatch && !pageMatch && !spaceMatch)
  const canManageSpace = !!space && ['admin', 'owner'].includes(space.effectiveRole)
  const canCreatePage = !!space && ['admin', 'owner', 'editor'].includes(space.effectiveRole)
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
        <nav aria-label={text.app.navigation}>
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
        <main className="workspace-content" aria-label={text.app.workspaceLabel}>
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
              ) : session?.workspaceAdmin ? (
                <section className="first-space-card" aria-labelledby="first-space-title">
                  <div>
                    <p className="eyebrow">{text.spaces.create}</p>
                    <h2 id="first-space-title">{text.workspace.createFirstSpace}</h2>
                    <p>{text.workspace.noSpacesAdmin}</p>
                  </div>
                  <form className="first-space-form" onSubmit={submitSpace}>
                    <label>{text.spaces.name}<input autoComplete="off" value={newSpaceName} onChange={(event) => setNewSpaceName(event.target.value)} /></label>
                    <label>{text.common.description}<input autoComplete="off" value={newSpaceDescription} onChange={(event) => setNewSpaceDescription(event.target.value)} /></label>
                    <button className="primary-button" type="submit" disabled={!newSpaceName.trim() || createSpace.isPending}>
                      {createSpace.isPending ? text.common.running : text.spaces.create}
                    </button>
                  </form>
                  {createSpace.isError && <ErrorNotice error={createSpace.error} />}
                </section>
              ) : (
                <p>{text.workspace.noSpaces}</p>
              )}
              <SpaceArchive />
            </section>
          ) : (
            <>
              {pageId && session ? (
                <>
                  <div className="space-context-actions">
                    {canManageSpace && <>
                      <button className="space-context-button" type="button" aria-expanded={manageSpace} onClick={() => setManageSpace(!manageSpace)}>{text.workspace.manageSpace}</button>
                      <PageArchive key={spaceId} spaceId={spaceId} />
                    </>}
                    <PageToolbar spaceId={spaceId} page={page.data} nodes={tree.data ?? []} spaces={spaces.data} canCreate={canCreatePage} />
                  </div>
                  {manageSpace && space && <div className="space-management-panels"><SpaceEditor space={space} /><SpaceAccess space={space} /><SpaceExports key={spaceId} spaceId={spaceId} /></div>}
                  <PageView pageId={pageId} spaceId={spaceId} workspaceId={session.workspaceId} userId={session.userId} canRecover={canManageSpace} />
                </>
              ) : (
                <section className="space-welcome">
                  <div className="space-welcome__hero">
                    <div className="space-welcome__art" aria-hidden="true"><span /></div>
                    <p className="eyebrow">{text.workspace.emptySpaceEyebrow}</p>
                    <h1>{canCreatePage ? text.workspace.emptySpaceTitle : text.workspace.emptySpaceReadOnlyTitle}</h1>
                    <p className="space-welcome__description">{canCreatePage ? text.workspace.emptySpaceDescription : text.workspace.emptySpaceReadOnlyDescription}</p>
                    <PageToolbar variant="empty" spaceId={spaceId} nodes={tree.data ?? []} spaces={spaces.data} canCreate={canCreatePage} />
                  </div>
                  {canManageSpace && <aside className="space-welcome__tools" aria-labelledby="space-tools-title">
                    <div className="space-welcome__tools-heading"><h2 id="space-tools-title">{text.workspace.spaceTools}</h2><p>{text.workspace.spaceToolsHelp}</p></div>
                    <button className="space-tool-button" type="button" aria-label={text.workspace.manageSpace} aria-expanded={manageSpace} onClick={() => setManageSpace(!manageSpace)}>
                      <span className="space-tool-button__icon" aria-hidden="true">&#9881;</span><span className="space-tool-button__copy"><strong>{text.workspace.manageSpace}</strong><small>{text.workspace.manageSpaceHelp}</small></span><span className="space-tool-button__chevron" aria-hidden="true">&#8250;</span>
                    </button>
                    <PageArchive key={spaceId} spaceId={spaceId} appearance="tool" />
                  </aside>}
                  {manageSpace && space && <div className="space-management-panels"><SpaceEditor space={space} /><SpaceAccess space={space} /><SpaceExports key={spaceId} spaceId={spaceId} /></div>}
                </section>
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
