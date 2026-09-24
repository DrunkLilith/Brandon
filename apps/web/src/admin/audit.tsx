import { useQuery } from '@tanstack/react-query'
import { useEffect, useState, type FormEvent } from 'react'
import { useSearchParams } from 'react-router-dom'

import { apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { adminMessages as text } from './messages'
import { apiObject, ErrorNotice, LoadingBlock, PageHeader } from './shared'

type AuditEventList = components['schemas']['AuditEventList']

const filterNames = ['actor', 'type', 'target', 'targetType', 'from', 'to'] as const
type FilterName = typeof filterNames[number]
type Filters = Record<FilterName, string>

function readFilters(params: URLSearchParams): Filters {
  return Object.fromEntries(filterNames.map((name) => [name, params.get(name) ?? ''])) as unknown as Filters
}

export function AuditPage() {
  const [params, setParams] = useSearchParams()
  const serialized = params.toString()
  const [filters, setFilters] = useState(() => readFilters(params))
  useEffect(() => setFilters(readFilters(params)), [serialized])
  const after = params.get('after') ?? undefined
  const queryFilters = readFilters(params)
  const audit = useQuery({
    queryKey: ['admin', 'audit', serialized],
    queryFn: async () => apiObject<AuditEventList>(unwrap(await apiClient.GET('/api/v1/audit-events', { params: { query: {
      after,
      limit: 100,
      actor: queryFilters.actor || undefined,
      type: queryFilters.type || undefined,
      target: queryFilters.target || undefined,
      targetType: queryFilters.targetType || undefined,
      from: queryFilters.from || undefined,
      to: queryFilters.to || undefined,
    } } }))),
  })
  const apply = (event: FormEvent) => {
    event.preventDefault()
    const next = new URLSearchParams()
    for (const name of filterNames) if (filters[name].trim()) next.set(name, filters[name].trim())
    setParams(next)
  }
  const update = (name: FilterName, value: string) => setFilters((current) => ({ ...current, [name]: value }))
  return <section><PageHeader title={text.pages.audit} />
    <section className="admin-panel"><h2>Фильтры</h2><form className="filter-form" onSubmit={apply}>
      <label>Пользователь<input value={filters.actor} onChange={(event) => update('actor', event.target.value)} placeholder="UUID" /></label>
      <label>Тип события<input value={filters.type} onChange={(event) => update('type', event.target.value)} /></label>
      <label>Целевой объект<input value={filters.target} onChange={(event) => update('target', event.target.value)} placeholder="UUID" /></label>
      <label>Тип объекта<input value={filters.targetType} onChange={(event) => update('targetType', event.target.value)} /></label>
      <label>С даты<input type="text" inputMode="numeric" placeholder="2026-09-01T00:00:00Z" value={filters.from} onChange={(event) => update('from', event.target.value)} /></label>
      <label>По дату<input type="text" inputMode="numeric" placeholder="2026-09-30T23:59:59Z" value={filters.to} onChange={(event) => update('to', event.target.value)} /></label>
      <button type="submit">Применить фильтры</button>
    </form></section>
    <section className="admin-panel"><h2>События</h2>{audit.isPending ? <LoadingBlock /> : audit.isError ? <ErrorNotice error={audit.error} /> : <>
      <ol className="audit-list">{audit.data.events.map((event) => <li key={event.id}><div className="resource-heading"><strong>{event.type}</strong><time dateTime={event.createdAt}>{new Date(event.createdAt).toLocaleString('ru-RU')}</time></div><p>{event.targetType}{event.targetId ? ` · ${event.targetId}` : ''}</p><pre>{JSON.stringify(event.metadata, null, 2)}</pre></li>)}</ol>
      {audit.data.nextCursor && <button type="button" onClick={() => { const next = new URLSearchParams(params); next.set('after', audit.data.nextCursor!); setParams(next) }}>Следующая страница</button>}
    </>}</section>
  </section>
}
