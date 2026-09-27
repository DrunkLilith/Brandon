import { createPortal } from 'react-dom'
import { useEffect, useId, useRef, type ReactNode, type RefObject } from 'react'

import { ProblemAlert } from '../components/problem-alert'
import { useI18n } from '../i18n'

export function ErrorNotice({ error }: { error: unknown }) {
  return <ProblemAlert error={error} />
}

export function LoadingBlock() {
  const { messages: text } = useI18n()
  return <p className="notice" role="status">{text.loading}</p>
}

export function PageHeader({ title, children }: { title: string; children?: ReactNode }) {
  const { messages: text } = useI18n()
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
  const cancel = useRef(onCancel)
  cancel.current = onCancel
  useEffect(() => {
    const opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
    const dialog = dialogRef.current
    const backdrop = dialog?.closest<HTMLElement>('.modal-backdrop')
    if (!dialog || !backdrop) return

    const backgrounds = new Set<HTMLElement>(
      [...document.body.children]
        .filter((element): element is HTMLElement => element instanceof HTMLElement && element !== backdrop),
    )
    document.querySelectorAll<HTMLElement>('.language-switch, .admin-shell').forEach((element) => {
      if (!backdrop.contains(element)) backgrounds.add(element)
    })
    const previousAttributes = [...backgrounds].map((element) => ({
      element,
      inert: element.getAttribute('inert'),
      hidden: element.getAttribute('aria-hidden'),
    }))
    for (const background of backgrounds) {
      background.setAttribute('inert', '')
      background.setAttribute('aria-hidden', 'true')
    }

    const focusInitial = () => {
      const initial = initialFocusRef?.current ?? dialog.querySelector<HTMLElement>(focusableSelector) ?? dialog
      initial.focus()
    }
    const onFocusIn = (event: FocusEvent) => {
      if (!(event.target instanceof Node) || !dialog.contains(event.target)) focusInitial()
    }
    const onDocumentKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        cancel.current()
        return
      }
      if (event.key !== 'Tab') return
      const focusable = [...dialog.querySelectorAll<HTMLElement>(focusableSelector)]
      if (!focusable.length) {
        event.preventDefault()
        dialog.focus()
        return
      }
      const first = focusable[0]
      const last = focusable[focusable.length - 1]
      if (!dialog.contains(document.activeElement)) {
        event.preventDefault()
        ;(event.shiftKey ? last : first).focus()
      } else if (event.shiftKey && document.activeElement === first) {
        event.preventDefault()
        last.focus()
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault()
        first.focus()
      }
    }
    document.addEventListener('focusin', onFocusIn, true)
    document.addEventListener('keydown', onDocumentKeyDown, true)
    focusInitial()
    return () => {
      document.removeEventListener('focusin', onFocusIn, true)
      document.removeEventListener('keydown', onDocumentKeyDown, true)
      for (const { element, inert, hidden } of previousAttributes) {
        if (inert === null) element.removeAttribute('inert')
        else element.setAttribute('inert', inert)
        if (hidden === null) element.removeAttribute('aria-hidden')
        else element.setAttribute('aria-hidden', hidden)
      }
      opener?.focus()
    }
  }, [initialFocusRef])
  return createPortal(<div className="modal-backdrop"><section ref={dialogRef} className="admin-dialog" role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1}>
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
  const { messages: text } = useI18n()
  return <ModalDialog title={title} onCancel={onCancel}>
    <p>{text.common.confirmationNotice}</p>
    <div className="dialog-actions">
      <button type="button" className="secondary-button" onClick={onCancel} disabled={pending}>{text.common.cancel}</button>
      <button type="button" className="danger-button" onClick={onConfirm} disabled={pending}>{pending ? text.common.running : confirmLabel}</button>
    </div>
  </ModalDialog>
}
