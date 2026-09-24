import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState } from 'react'
import { ApiError, apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { ErrorNotice, LoadingBlock } from '../admin/shared'
import { AttachmentList } from '../attachments/attachment-list'
import {
  draftKey,
  readDraft,
  writeDraft,
  type Draft,
} from '../editor/draft-store'
import { MarkdownEditor } from '../editor/markdown-editor'
import { ConflictDialog } from '../editor/conflict-dialog'
import { HistoryPanel } from '../editor/history-panel'
import { PagePermissions } from './page-permissions'
import { workspaceMessages as text } from './messages'
import { pageKey, pageQuery } from './queries'
type Page = components['schemas']['Page']

export function PageView({
  pageId,
  workspaceId,
  spaceId,
}: {
  pageId: string
  workspaceId: string
  spaceId: string
}) {
  const query = useQuery(pageQuery(pageId))
  if (query.isPending) return <LoadingBlock />
  if (query.isError) return <ErrorNotice error={query.error} />
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
    <PageEditor key={pageId} initial={query.data} workspaceId={workspaceId} />
  )
}
function PageEditor({
  initial,
  workspaceId,
}: {
  initial: Page
  workspaceId: string
}) {
  const cache = useQueryClient(),
    [base, setBase] = useState(initial)
  const [draft, setDraft] = useState<Draft>(
    () =>
      readDraft(draftKey(workspaceId, initial.id, initial.version)) ?? initial,
  )
  const [dirty, setDirty] = useState(
    () =>
      readDraft(draftKey(workspaceId, initial.id, initial.version)) !== null,
  )
  const [status, setStatus] = useState(''),
    [localError, setLocalError] = useState(false),
    [panel, setPanel] = useState<'history' | 'permissions' | null>(null),
    [properties, setProperties] = useState(() =>
      JSON.stringify(draft.properties, null, 2),
    )
  const [conflict, setConflict] = useState<{
    base: number
    submitted: Draft
    current?: Page
    error?: unknown
  } | null>(null)
  const key = draftKey(workspaceId, base.id, base.version)
  const change = (next: Draft) => {
    setDirty(true)
    setDraft(next)
    try {
      writeDraft(key, next)
      setStatus(text.workspace.local)
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
    setStatus(text.workspace.saved)
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
      setStatus(text.workspace.saved)
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
      <label>
        {text.workspace.title}
        <input
          value={draft.title}
          readOnly={!base.capabilities.edit || save.isPending}
          onChange={(e) => change({ ...draft, title: e.target.value })}
        />
      </label>
      <p>{text.workspace.version(base.version)}</p>
      <MarkdownEditor
        value={draft.markdown}
        readOnly={!base.capabilities.edit || save.isPending}
        onChange={(markdown) => {
          if (!save.isPending) change({ ...draft, markdown })
        }}
      />
      <details>
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
      <label>
        <input type="checkbox" checked={draft.restricted} disabled readOnly />
        {text.workspace.restricted}
      </label>
      <div className="row-actions">
        {base.capabilities.edit && (
          <button
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
                  markdown: draft.markdown,
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
          onClick={() => setPanel(panel === 'history' ? null : 'history')}
        >
          {text.workspace.history}
        </button>
        {base.restricted && base.capabilities.managePermissions && (
          <button
            onClick={() =>
              setPanel(panel === 'permissions' ? null : 'permissions')
            }
          >
            {text.workspace.permissions}
          </button>
        )}
      </div>
      <p role="status">{save.isPending ? text.workspace.saving : status}</p>
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
      <AttachmentList pageId={base.id} editable={base.capabilities.edit} />
      {conflict && (
        <ConflictDialog
          {...conflict}
          onClose={() => setConflict(null)}
          onReload={replace}
          onResolve={(current) => {
            setBase(current)
            setConflict(null)
            try {
              writeDraft(draftKey(workspaceId, base.id, current.version), draft)
            } catch {
              setLocalError(true)
            }
          }}
        />
      )}
    </article>
  )
}
