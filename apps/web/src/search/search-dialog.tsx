import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useRef, useState } from 'react'
import { useNavigate, useSearchParams } from 'react-router-dom'
import { apiClient, unwrapArray } from '../api/client'
import { ErrorNotice, LoadingBlock, ModalDialog } from '../admin/shared'
import { useI18n } from '../i18n'
import { pageQuery } from '../pages/queries'
import { SearchResults } from './search-results'
export function SearchDialog({ onClose }: { onClose(): void }) {
  const { messages: text } = useI18n()
  const [params, setParams] = useSearchParams(),
    cache = useQueryClient(),
    navigate = useNavigate()
  const input = useRef<HTMLInputElement>(null),
    [error, setError] = useState<unknown>()
  const filters = {
    q: params.get('q') ?? '',
    space: params.get('space') ?? '',
    author: params.get('author') ?? '',
    from: params.get('from') ?? '',
    to: params.get('to') ?? '',
  }
  const [draft, setDraft] = useState(filters)
  const results = useQuery({
    queryKey: ['workspace', 'search', filters],
    enabled: !!filters.q.trim(),
    queryFn: async ({ signal }) =>
      unwrapArray(
        await apiClient.GET('/api/v1/search', {
          params: {
            query: {
              q: filters.q,
              space: filters.space || undefined,
              author: filters.author || undefined,
              from: filters.from || undefined,
              to: filters.to || undefined,
            },
          },
          signal,
        }),
      ),
  })
  const select = async (id: string) => {
    try {
      const page = await cache.fetchQuery(pageQuery(id))
      navigate(`/app/spaces/${page.spaceId}/pages/${page.id}${location.search}`)
      onClose()
    } catch (reason) {
      setError(reason)
    }
  }
  return (
    <ModalDialog
      title={text.workspace.search}
      onCancel={onClose}
      initialFocusRef={input}
    >
      <form
        onSubmit={(e) => {
          e.preventDefault()
          const next = new URLSearchParams(params)
          for (const [key, value] of Object.entries(draft)) {
            if (value.trim()) next.set(key, value.trim())
            else next.delete(key)
          }
          setParams(next)
        }}
      >
        <label>
          {text.workspace.query}
          <input
            ref={input}
            value={draft.q}
            onChange={(e) => setDraft({ ...draft, q: e.target.value })}
          />
        </label>
        {(['space', 'author', 'from', 'to'] as const).map((key) => (
          <label key={key}>
            {key === 'space' ? text.workspace.spaces : text.workspace[key]}
            <input
              value={draft[key]}
              onChange={(e) => setDraft({ ...draft, [key]: e.target.value })}
              placeholder={
                key === 'from' || key === 'to' ? '2026-09-01T00:00:00Z' : 'UUID'
              }
            />
          </label>
        ))}
        <button type="submit">{text.workspace.find}</button>
      </form>
      {results.isFetching && <LoadingBlock />}
      {results.isError && <ErrorNotice error={results.error} />}{' '}
      {error != null && <ErrorNotice error={error} />}
      {results.data && (
        <SearchResults hits={results.data} onSelect={(id) => void select(id)} />
      )}
      <button onClick={onClose}>{text.common.close}</button>
    </ModalDialog>
  )
}
