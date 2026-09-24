import { queryOptions } from '@tanstack/react-query'
import { apiClient, unwrap, unwrapArray } from '../api/client'
export const spacesQuery = queryOptions({
  queryKey: ['workspace', 'spaces'],
  queryFn: async ({ signal }) =>
    unwrapArray(await apiClient.GET('/api/v1/spaces', { signal })),
})
export const treeKey = (spaceId: string) =>
  ['workspace', 'tree', spaceId] as const
export const treeQuery = (spaceId: string) =>
  queryOptions({
    queryKey: treeKey(spaceId),
    queryFn: async ({ signal }) =>
      unwrapArray(
        await apiClient.GET('/api/v1/pages', {
          params: { query: { spaceId } },
          signal,
        }),
      ),
  })
export const pageKey = (id: string) => ['workspace', 'page', id] as const
export const pageQuery = (id: string) =>
  queryOptions({
    queryKey: pageKey(id),
    queryFn: async ({ signal }) =>
      unwrap(
        await apiClient.GET('/api/v1/pages/{id}', {
          params: { path: { id } },
          signal,
        }),
      ),
  })
