import { useInfiniteQuery, useMutation, useQueryClient } from '@tanstack/react-query'
import { useState } from 'react'
import { apiClient, unwrap } from '../api/client'
import { ErrorNotice, LoadingBlock } from '../admin/shared'
import { useI18n } from '../i18n'

// The endpoint supplies only recoverable spaces the caller can manage; this is
// available outside the administrator console for ordinary space owners.
export function SpaceArchive() {
  const { messages: text } = useI18n()
  const client = useQueryClient()
  const [open, setOpen] = useState(false)
  const key = ['workspace', 'archived-spaces']
  const archived = useInfiniteQuery({
    queryKey: key, enabled: open, initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam }) => unwrap(await apiClient.GET('/api/v1/spaces/archived', { params: { query: { after: pageParam, limit: 100 } } })),
    getNextPageParam: last => last.nextCursor ?? undefined,
  })
  const restore = useMutation({
    mutationFn: async (id: string) => unwrap(await apiClient.POST('/api/v1/spaces/{id}/restore', { params: { path: { id } } })),
    onSuccess: async () => { await client.invalidateQueries({ queryKey: key }); await client.invalidateQueries({ queryKey: ['workspace', 'spaces'] }) },
  })
  return <section>
    <button type="button" aria-expanded={open} onClick={() => setOpen(!open)}>{text.spaces.archive}</button>
    {open && <section aria-label={text.spaces.archive}>
      {archived.isPending && <LoadingBlock />}{archived.isError && <ErrorNotice error={archived.error} />}
      {restore.isError && <ErrorNotice error={restore.error} />}
      {archived.data?.pages.flatMap(p => p.items).map(item => <p key={item.id}>{item.name} <button type="button" disabled={restore.isPending} onClick={() => restore.mutate(item.id)}>{text.common.restore}</button></p>)}
      {archived.isSuccess && !archived.data.pages.some(p => p.items.length) && <p>{text.common.emptyArchive}</p>}
      {archived.hasNextPage && <button type="button" disabled={archived.isFetchingNextPage} onClick={() => archived.fetchNextPage()}>{text.spaces.loadMoreArchived}</button>}
    </section>}
  </section>
}
