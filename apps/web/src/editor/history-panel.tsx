import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { apiClient, unwrap, unwrapArray } from '../api/client'
import { ConfirmDialog, ErrorNotice, LoadingBlock } from '../admin/shared'
import { workspaceMessages as text } from '../pages/messages'
import { pageQuery } from '../pages/queries'
import type { components } from '../api/generated'
import { DiffView } from './diff-view'
export function HistoryPanel({
  page,
  markdown,
  onRestored,
}: {
  page: components['schemas']['Page']
  markdown: string
  onRestored(page: components['schemas']['Page']): void
}) {
  const cache = useQueryClient(),
    [selected, setSelected] = useState<number | null>(null),
    [restore, setRestore] = useState<number | null>(null)
  const summaries = useQuery({
    queryKey: ['workspace', 'history', page.id],
    queryFn: async ({ signal }) =>
      unwrapArray(
        await apiClient.GET('/api/v1/pages/{id}/revisions', {
          params: { path: { id: page.id } },
          signal,
        }),
      ),
  })
  const revision = useQuery({
    queryKey: ['workspace', 'revision', page.id, selected],
    enabled: selected !== null,
    queryFn: async ({ signal }) =>
      unwrap(
        await apiClient.GET('/api/v1/pages/{id}/revisions/{version}', {
          params: { path: { id: page.id, version: selected! } },
          signal,
        }),
      ),
  })
  const mutation = useMutation({
    mutationFn: async (version: number) => {
      const acknowledged = unwrap(
        await apiClient.POST('/api/v1/pages/{id}/restore', {
          params: { path: { id: page.id } },
          body: { version, expectedVersion: page.version },
        }),
      )
      const current = await cache.fetchQuery({
        ...pageQuery(page.id),
        staleTime: 0,
      })
      return { ...current, version: acknowledged.version }
    },
    onSuccess: async (current) => {
      setRestore(null)
      onRestored(current)
      await cache.invalidateQueries({ queryKey: ['workspace'] })
    },
  })
  return (
    <section className="admin-panel">
      <h2>{text.workspace.history}</h2>
      {summaries.isPending && <LoadingBlock />}
      {summaries.isError && <ErrorNotice error={summaries.error} />}
      {mutation.isError && <ErrorNotice error={mutation.error} />}
      {summaries.data?.map((revision) => (
        <div key={revision.version}>
          <p>
            {text.workspace.version(revision.version)} · {revision.authorId} ·{' '}
            {text.format.dateTime(revision.createdAt)}
          </p>
          <button onClick={() => setSelected(revision.version)}>
            {text.workspace.compare(revision.version)}
          </button>
          {page.capabilities.edit && (
            <button onClick={() => setRestore(revision.version)}>
              {text.workspace.restore(revision.version)}
            </button>
          )}
        </div>
      ))}
      {selected !== null && revision.isPending && <LoadingBlock />}
      {revision.isError && <ErrorNotice error={revision.error} />}
      {revision.data && (
        <DiffView
          before={`${revision.data.title}\n\n${revision.data.markdown}`}
          after={`${page.title}\n\n${markdown}`}
        />
      )}
      {restore !== null && (
        <ConfirmDialog
          title={text.workspace.restoreTitle}
          confirmLabel={text.workspace.confirmRestore}
          pending={mutation.isPending}
          onCancel={() => {
            if (!mutation.isPending) setRestore(null)
          }}
          onConfirm={() => mutation.mutate(restore)}
        />
      )}
    </section>
  )
}
