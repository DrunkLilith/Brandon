import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { apiClient, unwrap, unwrapArray } from '../api/client'
import { ErrorNotice, LoadingBlock } from '../admin/shared'
import { useI18n } from '../i18n'
import { Uploader } from './uploader'
export function AttachmentList({
  pageId,
  editable,
  recoverable = false,
}: {
  pageId: string
  editable: boolean
  recoverable?: boolean
}) {
  const { messages: text } = useI18n()
  const client = useQueryClient()
  const [archived, setArchived] = useState(false)
  const query = useInfiniteQuery({
    queryKey: ['workspace', 'attachments', pageId, archived],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ signal, pageParam }) =>
      unwrapArray(
        await apiClient.GET('/api/v1/pages/{id}/attachments', {
          params: { path: { id: pageId }, query: { after: pageParam, archived } },
          signal,
        }),
      ),
    getNextPageParam: last => last.length === 100 ? last.at(-1)?.id : undefined,
  })
  const changeArchive = useMutation({
    mutationFn: async (id: string) => archived
      ? unwrap(await apiClient.POST('/api/v1/attachments/{id}/unarchive', { params: { path: { id } } }))
      : unwrap(await apiClient.DELETE('/api/v1/attachments/{id}', { params: { path: { id } } })),
    onSuccess: async () => { await client.invalidateQueries({ queryKey: ['workspace', 'attachments', pageId] }); if (archived) setArchived(false) },
  })
  const download = useMutation({
    mutationFn: async ({ id, name }: { id: string; name: string }) => {
      const blob = await apiClient.downloadAttachment({ attachmentId: id })
      const url = URL.createObjectURL(blob)
      const link = document.createElement('a')
      try {
        link.href = url
        link.download = name
        document.body.append(link)
        link.click()
      } finally {
        link.remove()
        setTimeout(() => URL.revokeObjectURL(url), 0)
      }
    },
  })
  return (
    <section className="admin-panel">
      <h2>{text.workspace.attachments}</h2>
      {recoverable && <button type="button" aria-pressed={archived} onClick={() => setArchived(!archived)}>{text.workspace.attachmentArchive}</button>}
      {archived && <p>{text.workspace.recoveryDeadline}</p>}
      {query.isPending && <LoadingBlock />}
      {query.isError && <ErrorNotice error={query.error} />}
      {download.isError && <ErrorNotice error={download.error} />}
      {changeArchive.isError && <ErrorNotice error={changeArchive.error} />}
      {query.isSuccess && !query.data.pages.some(p => p.length) && <p>{archived ? text.common.emptyArchive : text.workspace.noFiles}</p>}
      {query.data?.pages.flat().map((file) => (
        <div key={file.id}>
          <span>
            {file.name} · {file.size} B
          </span>
          {!archived && <button
            disabled={download.isPending}
            onClick={() => download.mutate(file)}
          >
            {text.workspace.download(file.name)}
          </button>}
          {(archived ? recoverable : editable) && <button type="button" disabled={changeArchive.isPending} onClick={() => changeArchive.mutate(file.id)}>{archived ? text.workspace.restoreAttachment : text.workspace.archiveAttachment}</button>}
        </div>
      ))}
      {query.hasNextPage && <button type="button" disabled={query.isFetchingNextPage} onClick={() => query.fetchNextPage()}>{text.workspace.moreAttachments}</button>}
      {editable && !archived && <Uploader pageId={pageId} />}
    </section>
  )
}
