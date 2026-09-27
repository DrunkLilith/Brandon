import {
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from '@tanstack/react-query'
import { useState } from 'react'
import { apiClient, unwrap, unwrapArray } from '../api/client'
import { ErrorNotice, LoadingBlock } from '../admin/shared'
import { useI18n } from '../i18n'
export function PagePermissions({
  pageId,
  spaceId,
}: {
  pageId: string
  spaceId: string
}) {
  const { messages: text } = useI18n()
  const cache = useQueryClient(),
    [subject, setSubject] = useState(''),
    [role, setRole] = useState<'viewer' | 'editor'>('viewer')
  const key = ['workspace', 'permissions', pageId]
  const grants = useQuery({
    queryKey: key,
    queryFn: async ({ signal }) =>
      unwrapArray(
        await apiClient.GET('/api/v1/pages/{id}/permissions', {
          params: { path: { id: pageId } },
          signal,
        }),
      ),
  })
  const subjects = useInfiniteQuery({
    queryKey: ['workspace', 'subjects', spaceId],
    initialPageParam: undefined as string | undefined,
    queryFn: async ({ pageParam, signal }) =>
      unwrap(
        await apiClient.GET('/api/v1/spaces/{id}/subjects', {
          params: {
            path: { id: spaceId },
            query: { after: pageParam, limit: 100 },
          },
          signal,
        }),
      ),
    getNextPageParam: (page) => page.nextCursor ?? undefined,
  })
  const items = subjects.data?.pages.flatMap((page) => page.items) ?? []
  const mutation = useMutation({
    mutationFn: async (input: {
      subject: string
      role?: 'viewer' | 'editor'
    }) =>
      input.role
        ? unwrap(
            await apiClient.PUT('/api/v1/pages/{id}/permissions/{subject}', {
              params: { path: { id: pageId, subject: input.subject } },
              body: { role: input.role },
            }),
          )
        : unwrap(
            await apiClient.DELETE('/api/v1/pages/{id}/permissions/{subject}', {
              params: { path: { id: pageId, subject: input.subject } },
            }),
          ),
    onSuccess: async () => {
      setSubject('')
      await cache.invalidateQueries({ queryKey: key })
      await cache.invalidateQueries({ queryKey: ['workspace', 'tree'] })
    },
  })
  return (
    <section className="admin-panel">
      <h2>{text.workspace.permissions}</h2>
      {grants.isPending && <LoadingBlock />}
      {grants.isError && <ErrorNotice error={grants.error} />}
      {subjects.isError && <ErrorNotice error={subjects.error} />}
      {mutation.isError && <ErrorNotice error={mutation.error} />}
      {grants.data?.map((grant) => (
        <div
          className="row-actions"
          key={`${grant.subjectType}:${grant.subjectId}`}
        >
          <span>
            {items.find(
              (item) =>
                item.type === grant.subjectType && item.id === grant.subjectId,
            )?.label ?? grant.subjectId}
          </span>
          <label>
            {text.workspace.role(grant.subjectId)}
            <select
              disabled={mutation.isPending}
              value={grant.role}
              onChange={(e) =>
                mutation.mutate({
                  subject: `${grant.subjectType}:${grant.subjectId}`,
                  role: e.target.value === 'editor' ? 'editor' : 'viewer',
                })
              }
            >
              <option value="viewer">{text.labels.grantRole.viewer}</option>
              <option value="editor">{text.labels.grantRole.editor}</option>
            </select>
          </label>
          <button
            disabled={mutation.isPending}
            onClick={() =>
              mutation.mutate({
                subject: `${grant.subjectType}:${grant.subjectId}`,
              })
            }
          >
            {text.workspace.removeGrant(grant.subjectId)}
          </button>
        </div>
      ))}
      <form
        className="inline-form"
        onSubmit={(e) => {
          e.preventDefault()
          if (items.some((item) => `${item.type}:${item.id}` === subject))
            mutation.mutate({ subject, role })
        }}
      >
        <label>
          {text.workspace.subject}
          <select
            required
            value={subject}
            onChange={(e) => setSubject(e.target.value)}
          >
            <option value="">{text.spaces.select}</option>
            {items
              .filter(
                (item) =>
                  !grants.data?.some(
                    (g) =>
                      g.subjectType === item.type && g.subjectId === item.id,
                  ),
              )
              .map((item) => (
                <option
                  key={`${item.type}:${item.id}`}
                  value={`${item.type}:${item.id}`}
                >
                  {item.label}
                </option>
              ))}
          </select>
        </label>
        <select
          aria-label={text.spaces.accessRole}
          value={role}
          onChange={(e) =>
            setRole(e.target.value === 'editor' ? 'editor' : 'viewer')
          }
        >
          <option value="viewer">{text.labels.grantRole.viewer}</option>
          <option value="editor">{text.labels.grantRole.editor}</option>
        </select>
        <button disabled={mutation.isPending || !subject || grants.isPending}>
          {text.workspace.addGrant}
        </button>
      </form>
      {subjects.hasNextPage && (
        <button
          disabled={subjects.isFetchingNextPage}
          onClick={() => void subjects.fetchNextPage()}
        >
          {text.workspace.more}
        </button>
      )}
    </section>
  )
}
