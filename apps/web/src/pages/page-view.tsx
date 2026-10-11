import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState } from 'react'
import { ApiError, apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { ErrorNotice, LoadingBlock, ModalDialog } from '../admin/shared'
import { AttachmentList } from '../attachments/attachment-list'
import {
  createDraftDocumentId,
  currentDraftDocumentId,
  draftKey,
  discoverDrafts,
  readDraft,
  writeDraft,
  type Draft,
} from '../editor/draft-store'
import { MarkdownEditor } from '../editor/markdown-editor'
import type { MarkdownHandle } from '../editor/milkdown-adapter'
import { ConflictDialog } from '../editor/conflict-dialog'
import { HistoryPanel } from '../editor/history-panel'
import { PagePermissions } from './page-permissions'
import { Backlinks } from './backlinks'
import { useI18n } from '../i18n'
import { pageKey, pageQuery } from './queries'
type Page = components['schemas']['Page']

export function PageView({
  pageId,
  workspaceId,
  userId,
  spaceId,
  canRecover = false,
}: {
  pageId: string
  workspaceId: string
  userId: string
  spaceId: string
  canRecover?: boolean
}) {
  const query = useQuery(pageQuery(pageId))
  if (query.isPending) return <LoadingBlock />
  if (!query.data) return <ErrorNotice error={query.error} />
  if (query.data.spaceId !== spaceId)
    return (
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
    )
  return (
    <>
      {query.isError && <ErrorNotice error={query.error} />}
      <PageEditor key={`${userId}:${pageId}`} initial={query.data} workspaceId={workspaceId} userId={userId} canRecover={canRecover} />
    </>
  )
}
function PageEditor({
  initial,
  workspaceId,
  userId,
  canRecover,
}: {
  initial: Page
  workspaceId: string
  userId: string
  canRecover: boolean
}) {
  const { messages: text } = useI18n()
  const cache = useQueryClient(),
    [base, setBase] = useState(initial)
  const [draft, setDraft] = useState<Draft>(
    () =>
      readDraft(draftKey(workspaceId, initial.id, initial.version, userId)) ?? initial,
  )
  const [dirty, setDirty] = useState(
    () =>
      readDraft(draftKey(workspaceId, initial.id, initial.version, userId)) !== null,
  )
  const [status, setStatus] = useState<'local' | 'saved' | null>(null),
    [localError, setLocalError] = useState(false),
    [panel, setPanel] = useState<'history' | 'permissions' | 'backlinks' | null>(null),
    [properties, setProperties] = useState(() =>
      JSON.stringify(draft.properties, null, 2),
    )
  const [conflict, setConflict] = useState<{
    base: number
    submitted: Draft
    current?: Page
    error?: unknown
  } | null>(null)
  const [draftDocumentId, setDraftDocumentId] = useState(currentDraftDocumentId)
  const key = draftKey(workspaceId, base.id, base.version, userId, draftDocumentId)
  const [recoveryOpen, setRecoveryOpen] = useState(false)
  const [recoverable, setRecoverable] = useState<ReturnType<typeof discoverDrafts>>(() => {
    try { return discoverDrafts(workspaceId, initial.id, userId).filter((item) => item.key !== key) }
    catch { return [] }
  })
  useEffect(() => {
    const refresh = () => {
      try { setRecoverable(discoverDrafts(workspaceId, initial.id, userId).filter((item) => item.key !== key)) }
      catch { setLocalError(true) }
    }
    refresh()
    window.addEventListener('storage', refresh)
    window.addEventListener('focus', refresh)
    return () => { window.removeEventListener('storage', refresh); window.removeEventListener('focus', refresh) }
  }, [workspaceId, initial.id, userId, key])
  const editor = useRef<MarkdownHandle>(null)
  const openRecovery = () => {
    const current = { ...draft, markdown: editor.current?.flush() ?? draft.markdown }
    setDraft(current)
    try {
      writeDraft(key, current)
      setLocalError(false)
    } catch {
      setLocalError(true)
    }
    setRecoveryOpen(true)
  }
  const change = (next: Draft) => {
    setDirty(true)
    setDraft(next)
    try {
      writeDraft(key, next)
      setStatus('local')
      setLocalError(false)
    } catch {
      setLocalError(true)
    }
  }
  const replace = (page: Page) => {
    cache.setQueryData(pageKey(page.id), page)
    setDirty(false)
    setBase(page)
    setDraft(page)
    setProperties(JSON.stringify(page.properties, null, 2))
    setConflict(null)
    setStatus('saved')
  }
  useEffect(() => {
    if (initial.version > base.version && !dirty && !conflict) {
      setBase(initial)
      setDraft(initial)
      setProperties(JSON.stringify(initial.properties, null, 2))
    }
  }, [initial, base.version, dirty, conflict])
  const save = useMutation({
    mutationFn: async (input: { draft: Draft; version: number; key: string }) =>
      unwrap(
        await apiClient.PATCH('/api/v1/pages/{id}', {
          params: { path: { id: base.id } },
          body: { ...input.draft, expectedVersion: input.version },
        }),
      ),
    onSuccess: async (response, input) => {
      try {
        const stored = readDraft(input.key)
        if (
          stored?.title === input.draft.title &&
          stored.markdown === input.draft.markdown &&
          stored.restricted === input.draft.restricted &&
          JSON.stringify(stored.properties) ===
            JSON.stringify(input.draft.properties)
        )
          localStorage.removeItem(input.key)
      } catch {
        setLocalError(true)
      }
      const next = { ...base, ...input.draft, version: response.version }
      setDirty(false)
      setBase(next)
      cache.setQueryData(pageKey(base.id), next)
      setStatus('saved')
      await cache.invalidateQueries({ queryKey: ['workspace', 'tree'] })
      await cache.invalidateQueries({
        queryKey: ['workspace', 'history', base.id],
      })
    },
    onError: async (error, input) => {
      if (error instanceof ApiError && error.status === 409) {
        setConflict({ base: input.version, submitted: input.draft })
        try {
          const current = await cache.fetchQuery({
            queryKey: ['workspace', 'conflict-current', base.id],
            queryFn: async ({ signal }) =>
              unwrap(
                await apiClient.GET('/api/v1/pages/{id}', {
                  params: { path: { id: base.id } },
                  signal,
                }),
              ),
            staleTime: 0,
          })
          setConflict({ base: input.version, submitted: input.draft, current })
        } catch (reason) {
          setConflict({
            base: input.version,
            submitted: input.draft,
            error: reason,
          })
        }
      }
    },
  })
  return (
    <article className="page-workspace">
      {recoverable.length > 0 && <button onClick={openRecovery}>{text.workspace.recoverDrafts}</button>}
      {recoveryOpen && <ModalDialog title={text.workspace.recoverDrafts} onCancel={() => setRecoveryOpen(false)}>
        <p>{text.workspace.recoveryHelp}</p>
        <h3>{text.workspace.current(initial.version)}</h3><pre>{initial.markdown}</pre>
        {recoverable.map((item) => <section key={item.key}>
          <h3>{text.workspace.base(item.version)} · {item.draft.title}</h3>
          <pre>{item.draft.markdown}</pre><pre>{JSON.stringify(item.draft.properties, null, 2)}</pre>
          <button disabled={!base.capabilities.edit} onClick={() => {
            const recoveredDocumentId = createDraftDocumentId()
            const recoveredKey = draftKey(workspaceId, base.id, item.version, userId, recoveredDocumentId)
            try {
              writeDraft(recoveredKey, item.draft)
              setLocalError(false)
            } catch { setLocalError(true) }
            setDraftDocumentId(recoveredDocumentId)
            setBase({ ...initial, version: item.version })
            setDraft(item.draft)
            setProperties(JSON.stringify(item.draft.properties, null, 2))
            setDirty(true)
            setRecoveryOpen(false)
          }}>{text.workspace.continueEditing}</button>
        </section>)}
        <button onClick={() => setRecoveryOpen(false)}>{text.common.cancel}</button>
      </ModalDialog>}
      <header className="page-editor-header">
        <label className="page-title-field">
          {text.workspace.title}
          <input
            value={draft.title}
            readOnly={!base.capabilities.edit || save.isPending}
            onChange={(e) => change({ ...draft, title: e.target.value })}
          />
        </label>
        <div className="page-editor-meta"><span>{text.workspace.version(base.version)}</span><p role="status">{save.isPending ? text.workspace.saving : status ? text.workspace[status] : ''}</p></div>
      </header>
      <MarkdownEditor
        ref={editor}
        value={draft.markdown}
        readOnly={!base.capabilities.edit || save.isPending}
        onChange={(markdown) => {
          if (!save.isPending) change({ ...draft, markdown })
        }}
        onDirty={() => setDirty(true)}
        onUnmount={(markdown) => {
          try {
            writeDraft(key, { ...draft, markdown })
          } catch {
            // The page is leaving; do not update unmounted UI state.
          }
        }}
      />
      <details className="page-properties">
        <summary>{text.workspace.properties}</summary>
        <textarea
          aria-label={text.workspace.properties}
          value={properties}
          readOnly={!base.capabilities.edit || save.isPending}
          onChange={(e) => {
            setProperties(e.target.value)
            try {
              const value: unknown = JSON.parse(e.target.value)
              if (value && typeof value === 'object' && !Array.isArray(value))
                change({ ...draft, properties: value as Draft['properties'] })
            } catch {
              /* Keep invalid JSON visible; saving is disabled below. */
            }
          }}
        />
      </details>
      <label className="restricted-indicator">
        <input type="checkbox" checked={draft.restricted} disabled readOnly />
        {text.workspace.restricted}
      </label>
      <div className="page-editor-actions">
        {base.capabilities.edit && (
          <button
            className="primary-button"
            disabled={
              save.isPending ||
              !draft.title.trim() ||
              (() => {
                try {
                  return (
                    JSON.stringify(JSON.parse(properties)) !==
                    JSON.stringify(draft.properties)
                  )
                } catch {
                  return true
                }
              })()
            }
            onClick={() =>
              save.mutate({
                draft: {
                  title: draft.title,
                  markdown: editor.current?.flush() ?? draft.markdown,
                  properties: draft.properties,
                  restricted: draft.restricted,
                },
                version: base.version,
                key,
              })
            }
          >
            {text.workspace.save}
          </button>
        )}
        <button
          aria-pressed={panel === 'history'}
          onClick={() => setPanel(panel === 'history' ? null : 'history')}
        >
          {text.workspace.history}
        </button>
        <button aria-pressed={panel === 'backlinks'} onClick={()=>setPanel(panel==='backlinks'?null:'backlinks')}>{text.workspace.backlinks}</button>
        {base.restricted && base.capabilities.managePermissions && (
          <button
            aria-pressed={panel === 'permissions'}
            onClick={() =>
              setPanel(panel === 'permissions' ? null : 'permissions')
            }
          >
            {text.workspace.permissions}
          </button>
        )}
      </div>
      {localError && <p role="alert">{text.workspace.draftFailed}</p>}
      {save.isError && !conflict && <ErrorNotice error={save.error} />}
      {panel === 'history' && (
        <HistoryPanel
          page={base}
          markdown={draft.markdown}
          onRestored={replace}
        />
      )}
      {panel === 'permissions' && (
        <PagePermissions pageId={base.id} spaceId={base.spaceId} />
      )}
      {panel === 'backlinks' && <Backlinks pageId={base.id}/>}
      <AttachmentList pageId={base.id} editable={base.capabilities.edit} recoverable={canRecover && base.capabilities.edit} />
      {conflict && (
        <ConflictDialog
          {...conflict}
          onClose={() => setConflict(null)}
          onReload={replace}
          onResolve={(current) => {
            setBase(current)
            setConflict(null)
            try {
              writeDraft(draftKey(workspaceId, base.id, current.version, userId), draft)
            } catch {
              setLocalError(true)
            }
          }}
        />
      )}
    </article>
  )
}
