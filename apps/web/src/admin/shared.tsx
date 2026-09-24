import { createPortal } from 'react-dom'
import { useEffect, useId, useRef, type ReactNode, type RefObject } from 'react'

import { ApiError } from '../api/client'
import { adminMessages as text } from './messages'

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

const focusableSelector = 'button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), a[href], [tabindex]:not([tabindex="-1"])'

export function ModalDialog({ title, children, onCancel, initialFocusRef }: {
  title: string
  children: ReactNode
  onCancel(): void
  initialFocusRef?: RefObject<HTMLElement | null>
}) {
  const titleId = useId()
  const dialogRef = useRef<HTMLElement>(null)
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const background = document.querySelector<HTMLElement>('.admin-shell')
    const previousHidden = background?.getAttribute('aria-hidden')
    background?.setAttribute('inert', '')
    background?.setAttribute('aria-hidden', 'true')
    const dialog = dialogRef.current
    const initial = initialFocusRef?.current ?? dialog?.querySelector<HTMLElement>(focusableSelector)
    initial?.focus()
    return () => {
      background?.removeAttribute('inert')
      if (previousHidden === null) background?.removeAttribute('aria-hidden')
      else if (previousHidden !== undefined) background?.setAttribute('aria-hidden', previousHidden)
      opener?.focus()
    }
  }, [initialFocusRef])
  const onKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === 'Escape') {
      event.preventDefault()
      onCancel()
      return
    }
    if (event.key !== 'Tab') return
    const focusable = [...(dialogRef.current?.querySelectorAll<HTMLElement>(focusableSelector) ?? [])]
    if (!focusable.length) return
    const first = focusable[0]
    const last = focusable[focusable.length - 1]
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault()
      last.focus()
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault()
      first.focus()
    }
  }
  return createPortal(<div className="modal-backdrop"><section ref={dialogRef} className="admin-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} onKeyDown={onKeyDown}>
    <h2 id={titleId}>{title}</h2>
    {children}
  </section></div>, document.body)
}

export function ConfirmDialog({ title, confirmLabel, pending, onCancel, onConfirm }: {
  title: string
  confirmLabel: string
  pending: boolean
  onCancel(): void
  onConfirm(): void
}) {
  return <ModalDialog title={title} onCancel={onCancel}>
    <p>{text.common.confirmationNotice}</p>
    <div className="dialog-actions">
      <button type="button" className="secondary-button" onClick={onCancel} disabled={pending}>{text.common.cancel}</button>
      <button type="button" className="danger-button" onClick={onConfirm} disabled={pending}>{pending ? text.common.running : confirmLabel}</button>
    </div>
  </ModalDialog>
}
