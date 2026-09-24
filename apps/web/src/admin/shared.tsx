import type { ReactNode } from 'react'

import { ApiError } from '../api/client'
import { adminMessages as text } from './messages'

export function apiObject<T extends object>(value: unknown): T {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new TypeError('Expected an API object response')
  return value as T
}

export function apiArray<T>(value: unknown): readonly T[] {
  if (!Array.isArray(value)) throw new TypeError('Expected an API array response')
  return value as readonly T[]
}

export function safeErrorMessage(error: unknown) {
  if (error instanceof ApiError) {
    if (error.status === 403) return text.errors.forbidden
    if (error.status === 404) return text.errors.notFound
    if (error.status === 409) return text.errors.conflict
    if (error.status === 429) return text.errors.rateLimited
  }
  return text.errors.generic
}

export function ErrorNotice({ error }: { error: unknown }) {
  return <p className="notice error-notice" role="alert">{safeErrorMessage(error)}</p>
}

export function LoadingBlock() {
  return <p className="notice" role="status">{text.loading}</p>
}

export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  return <header className="admin-page-header"><div><p className="eyebrow">{text.section}</p><h1>{title}</h1></div>{children}</header>
}

export function ConfirmDialog({ title, confirmLabel, pending, onCancel, onConfirm }: {
  title: string
  confirmLabel: string
  pending: boolean
  onCancel(): void
  onConfirm(): void
}) {
  return <div className="modal-backdrop"><section className="admin-dialog" role="dialog" aria-modal="true" aria-labelledby="confirm-title">
    <h2 id="confirm-title">{title}</h2>
    <p>Действие применяется сразу и может завершить активные сеансы или доступ.</p>
    <div className="dialog-actions">
      <button type="button" className="secondary-button" onClick={onCancel} disabled={pending}>Отмена</button>
      <button type="button" className="danger-button" onClick={onConfirm} disabled={pending}>{pending ? 'Выполняется…' : confirmLabel}</button>
    </div>
  </section></div>
}
