import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useRef } from 'react'
import { apiClient, unwrap } from '../api/client'
import { ErrorNotice, LoadingBlock } from '../admin/shared'
import { useI18n } from '../i18n'

export function SpaceExports({ spaceId }: { spaceId: string }) {
  const { messages: text } = useI18n()
  const client = useQueryClient()
  const requestKey = useRef<string | null>(null)
  const key = ['workspace', 'exports', spaceId]
  const exports = useInfiniteQuery({
    queryKey: key, initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => unwrap(await apiClient.GET('/api/v1/exports', { params: { query: { limit: 100, after: pageParam } } })),
    getNextPageParam: last => last.nextCursor ?? undefined,
    refetchInterval: query => query.state.data?.pages.some(p => p.exports.some(e => ['queued', 'running'].includes(e.status))) ? 2000 : false,
  })
  const create = useMutation({
    mutationFn: async () => {
      requestKey.current ??= crypto.randomUUID()
      return unwrap(await apiClient.POST('/api/v1/exports', { params: { header: { 'Idempotency-Key': requestKey.current } }, body: { spaceId } }))
    },
    onSuccess: async () => { requestKey.current = null; await client.invalidateQueries({ queryKey: key }) },
  })
  const download = useMutation({
    mutationFn: async (id: string) => {
      const blob = await apiClient.downloadExport({ exportId: id })
      const url = URL.createObjectURL(blob), link = document.createElement('a')
      try { link.href = url; link.download = `knowledge-export-${id}.tar`; document.body.append(link); link.click() }
      finally { link.remove(); setTimeout(() => URL.revokeObjectURL(url), 0) }
    },
  })
  const items = exports.data?.pages.flatMap(p => p.exports).filter(e => e.spaceId === spaceId) ?? []
  return <section className="admin-panel" aria-label={text.workspace.exports}>
    <h2>{text.workspace.exports}</h2><button type="button" disabled={create.isPending} onClick={() => create.mutate()}>{text.workspace.createExport}</button>
    <p>{text.workspace.exportNotice}</p>
    {exports.isPending && <LoadingBlock />}{exports.isError && <ErrorNotice error={exports.error} />}
    {create.isError && <ErrorNotice error={create.error} />}{download.isError && <ErrorNotice error={download.error} />}
    <div aria-live="polite">{items.map(item => <p key={item.id}>
      {text.format.dateTime(item.createdAt)} · {text.labels.exportStatus(item.status)}
      {item.expiresAt && <> · {text.workspace.expires}: {text.format.dateTime(item.expiresAt)}</>}
      {['queued', 'running'].includes(item.status) && <progress aria-label={text.workspace.exportProgress} />}
      {item.status === 'ready' && item.expiresAt && Date.parse(item.expiresAt) > Date.now() && <button type="button" disabled={download.isPending} onClick={() => download.mutate(item.id)}>{text.workspace.downloadExport}</button>}
    </p>)}</div>
    {exports.isSuccess && !items.length && <p>{text.dashboard.noExports}</p>}
    {exports.hasNextPage && <button type="button" disabled={exports.isFetchingNextPage} onClick={() => exports.fetchNextPage()}>{text.workspace.more}</button>}
  </section>
}
