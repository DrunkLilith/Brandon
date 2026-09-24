import { useMutation, useQuery } from '@tanstack/react-query'
import { apiClient, unwrapArray } from '../api/client'
import { ErrorNotice, LoadingBlock } from '../admin/shared'
import { workspaceMessages as text } from '../pages/messages'
import { Uploader } from './uploader'
export function AttachmentList({
  pageId,
  editable,
}: {
  pageId: string
  editable: boolean
}) {
  const query = useQuery({
    queryKey: ['workspace', 'attachments', pageId],
    queryFn: async ({ signal }) =>
      unwrapArray(
        await apiClient.GET('/api/v1/pages/{id}/attachments', {
          params: { path: { id: pageId } },
          signal,
        }),
      ),
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
      {query.isPending && <LoadingBlock />}
      {query.isError && <ErrorNotice error={query.error} />}
      {download.isError && <ErrorNotice error={download.error} />}
      {query.data?.length === 0 && <p>{text.workspace.noFiles}</p>}
      {query.data?.map((file) => (
        <div key={file.id}>
          <span>
            {file.name} · {file.size} B
          </span>
          <button
            disabled={download.isPending}
            onClick={() => download.mutate(file)}
          >
            {text.workspace.download(file.name)}
          </button>
        </div>
      ))}
      {editable && <Uploader pageId={pageId} />}
    </section>
  )
}
