import { useQuery } from '@tanstack/react-query'
import { useEffect, useState, type FormEvent } from 'react'
import { useSearchParams } from 'react-router-dom'

import { apiClient, unwrap } from '../api/client'
import { adminMessages as text } from './messages'
import { ErrorNotice, LoadingBlock, PageHeader } from './shared'

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
    queryFn: async () => unwrap(await apiClient.GET('/api/v1/audit-events', { params: { query: {
      after,
      limit: 100,
      actor: queryFilters.actor || undefined,
      type: queryFilters.type || undefined,
      target: queryFilters.target || undefined,
      targetType: queryFilters.targetType || undefined,
      from: queryFilters.from || undefined,
      to: queryFilters.to || undefined,
    } } })),
  })
  const apply = (event: FormEvent) => {
    event.preventDefault()
    const next = new URLSearchParams()
    for (const name of filterNames) if (filters[name].trim()) next.set(name, filters[name].trim())
    setParams(next)
  }
  const update = (name: FilterName, value: string) => setFilters((current) => ({ ...current, [name]: value }))
  return <section><PageHeader title={text.pages.audit} />
    <section className="admin-panel"><h2>{text.audit.filters}</h2><form className="filter-form" onSubmit={apply}>
      <label>{text.audit.actor}<input value={filters.actor} onChange={(event) => update('actor', event.target.value)} placeholder={text.audit.idPlaceholder} /></label>
      <label>{text.audit.eventType}<input value={filters.type} onChange={(event) => update('type', event.target.value)} /></label>
      <label>{text.audit.target}<input value={filters.target} onChange={(event) => update('target', event.target.value)} placeholder={text.audit.idPlaceholder} /></label>
      <label>{text.audit.targetType}<input value={filters.targetType} onChange={(event) => update('targetType', event.target.value)} /></label>
      <label>{text.audit.from}<input type="text" inputMode="numeric" placeholder={text.audit.fromPlaceholder} value={filters.from} onChange={(event) => update('from', event.target.value)} /></label>
      <label>{text.audit.to}<input type="text" inputMode="numeric" placeholder={text.audit.toPlaceholder} value={filters.to} onChange={(event) => update('to', event.target.value)} /></label>
      <button type="submit">{text.audit.apply}</button>
    </form></section>
    <section className="admin-panel"><h2>{text.audit.events}</h2>{audit.isPending ? <LoadingBlock /> : audit.isError ? <ErrorNotice error={audit.error} /> : <>
      <ol className="audit-list">{audit.data.events.map((event) => <li key={event.id}><div className="resource-heading"><strong>{event.type}</strong><time dateTime={event.createdAt}>{text.format.dateTime(event.createdAt)}</time></div><p>{event.targetType}{event.targetId ? ` · ${event.targetId}` : ''}</p><pre>{JSON.stringify(event.metadata, null, 2)}</pre></li>)}</ol>
      {audit.data.nextCursor && <button type="button" onClick={() => { const next = new URLSearchParams(params); next.set('after', audit.data.nextCursor!); setParams(next) }}>{text.audit.next}</button>}
    </>}</section>
  </section>
}
