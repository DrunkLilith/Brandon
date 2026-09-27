import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { apiClient, unwrap } from '../api/client'
import { ErrorNotice, LoadingBlock } from '../admin/shared'
import { useI18n } from '../i18n'
import { treeKey } from './queries'

export function PageArchive({ spaceId }: { spaceId: string }) {
  const { messages: text } = useI18n()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const key = ['workspace', 'archived-pages', spaceId]
  const archived = useInfiniteQuery({
    queryKey: key, enabled: open, initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => unwrap(await apiClient.GET('/api/v1/spaces/{id}/archived-pages', { params: { path: { id: spaceId }, query: { after: pageParam, limit: 100 } } })),
    getNextPageParam: last => last.nextCursor ?? undefined,
  })
  const restore = useMutation({
    mutationFn: async (id: string) => unwrap(await apiClient.POST('/api/v1/pages/{id}/unarchive', { params: { path: { id } } })),
    onSuccess: async () => { await client.invalidateQueries({ queryKey: key }); await client.invalidateQueries({ queryKey: treeKey(spaceId) }) },
  })
  return <section>
    <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>{text.workspace.pageArchive}</button>
    {open && <section className="admin-panel" aria-label={text.workspace.pageArchive}>
      <p>{text.workspace.recoveryDeadline}</p>
      {archived.isPending && <LoadingBlock />}{archived.isError && <ErrorNotice error={archived.error} />}
      {restore.isError && <ErrorNotice error={restore.error} />}
      {archived.data?.pages.flatMap(p => p.items).map(item => <p key={item.id}><span>{item.name}</span> · {text.format.dateTime(item.archivedAt)} <button type="button" disabled={restore.isPending} onClick={() => restore.mutate(item.id)}>{text.workspace.restorePage}</button></p>)}
      {archived.isSuccess && !archived.data.pages.some(p => p.items.length) && <p>{text.common.emptyArchive}</p>}
      {archived.hasNextPage && <button type="button" disabled={archived.isFetchingNextPage} onClick={() => archived.fetchNextPage()}>{text.workspace.more}</button>}
    </section>}
  </section>
}
