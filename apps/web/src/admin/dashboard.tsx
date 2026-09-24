import { useQuery } from '@tanstack/react-query'

import { apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { adminMessages as text } from './messages'
import { apiObject, ErrorNotice, LoadingBlock, PageHeader } from './shared'

type AdminStatus = components['schemas']['AdminStatus']
type Readiness = components['schemas']['Readiness']
type ExportList = components['schemas']['ExportList']

const state = (healthy: boolean) => healthy ? 'доступно' : 'недоступно'

export function DashboardPage() {
  const status = useQuery({ queryKey: ['admin', 'status'], queryFn: async () => apiObject<AdminStatus>(unwrap(await apiClient.GET('/api/v1/admin/status'))) })
  const readiness = useQuery({ queryKey: ['admin', 'readiness'], queryFn: async () => apiObject<Readiness>(unwrap(await apiClient.GET('/api/v1/health/ready'))) })
  const exportsQuery = useQuery({ queryKey: ['admin', 'exports'], queryFn: async () => apiObject<ExportList>(unwrap(await apiClient.GET('/api/v1/exports', { params: { query: { limit: 10 } } }))) })
  return <section><PageHeader title={text.pages.dashboard} />
    <div className="dashboard-grid">
      <section className="admin-panel"><h2>Состояние системы</h2>{status.isPending ? <LoadingBlock /> : status.isError ? <ErrorNotice error={status.error} /> : <ul className="status-list"><li>База данных: {state(status.data.components.database)}</li><li>Главный ключ: {state(status.data.components.masterKey)}</li><li>Миграции: {state(status.data.components.migrations)}</li><li>Хранилище объектов: {state(status.data.components.objectStorage)}</li></ul>}</section>
      <section className="admin-panel"><h2>Операции</h2>{status.data ? <ul className="status-list"><li>Задач в очереди: {status.data.queuedJobs}</li><li>Активных экспортов: {status.data.activeExports}</li></ul> : <LoadingBlock />}</section>
      <section className="admin-panel"><h2>Готовность</h2>{readiness.isPending ? <LoadingBlock /> : readiness.isError ? <p role="alert" className="notice error-notice">Готовность сервиса недоступна.</p> : <p>{readiness.data.status === 'ready' ? 'Сервис готов.' : 'Сервис не готов.'}</p>}</section>
      <section className="admin-panel"><h2>Последние экспорты</h2>{exportsQuery.isPending ? <LoadingBlock /> : exportsQuery.isError ? <ErrorNotice error={exportsQuery.error} /> : exportsQuery.data.exports.length ? <ul className="admin-list">{exportsQuery.data.exports.map((item) => <li className="admin-list-item" key={item.id}><span>{item.status} · {item.spaceId}</span></li>)}</ul> : <p className="notice">Экспортов пока нет.</p>}</section>
    </div>
  </section>
}
