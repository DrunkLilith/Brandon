import { useQuery } from '@tanstack/react-query'

import { apiClient, unwrap } from '../api/client'
import { useI18n } from '../i18n'
import { ErrorNotice, LoadingBlock, PageHeader } from './shared'

export function DashboardPage() {
  const { messages: text } = useI18n()
  const status = useQuery({ queryKey: ['admin', 'status'], queryFn: async () => unwrap(await apiClient.GET('/api/v1/admin/status')) })
  const readiness = useQuery({ queryKey: ['admin', 'readiness'], queryFn: async () => unwrap(await apiClient.GET('/api/v1/health/ready')) })
  const exportsQuery = useQuery({ queryKey: ['admin', 'exports'], queryFn: async () => unwrap(await apiClient.GET('/api/v1/exports', { params: { query: { limit: 10 } } })) })
  return <section><PageHeader title={text.pages.dashboard} />
    <div className="dashboard-grid">
      <section className="admin-panel"><h2>{text.dashboard.system}</h2>{status.isPending ? <LoadingBlock /> : status.isError ? <ErrorNotice error={status.error} /> : <ul className="status-list"><li>{text.dashboard.database}: {text.labels.health(status.data.components.database)}</li><li>{text.dashboard.masterKey}: {text.labels.health(status.data.components.masterKey)}</li><li>{text.dashboard.migrations}: {text.labels.health(status.data.components.migrations)}</li><li>{text.dashboard.objectStorage}: {text.labels.health(status.data.components.objectStorage)}</li></ul>}</section>
      <section className="admin-panel" aria-label={text.dashboard.operations}><h2>{text.dashboard.operations}</h2>{status.isPending ? <LoadingBlock /> : status.isError ? <p role="alert" className="notice error-notice">{text.dashboard.operationsUnavailable}</p> : <ul className="status-list"><li>{text.dashboard.queuedJobs}: {status.data.queuedJobs}</li><li>{text.dashboard.activeExports}: {status.data.activeExports}</li></ul>}</section>
      <section className="admin-panel"><h2>{text.dashboard.readiness}</h2>{readiness.isPending ? <LoadingBlock /> : readiness.isError ? <p role="alert" className="notice error-notice">{text.dashboard.readinessUnavailable}</p> : <p>{readiness.data.status === 'ready' ? text.dashboard.ready : text.dashboard.notReady}</p>}</section>
      <section className="admin-panel"><h2>{text.dashboard.recentExports}</h2>{exportsQuery.isPending ? <LoadingBlock /> : exportsQuery.isError ? <ErrorNotice error={exportsQuery.error} /> : exportsQuery.data.exports.length ? <ul className="admin-list">{exportsQuery.data.exports.map((item) => <li className="admin-list-item" key={item.id}><span>{text.labels.exportStatus(item.status)} · {item.spaceId}</span></li>)}</ul> : <p className="notice">{text.dashboard.noExports}</p>}</section>
    </div>
  </section>
}
