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

type AttributeSnapshot = {
  element: HTMLElement
  inert: string | null
  hidden: string | null
}

type ModalEntry = {
  dialog: HTMLElement
  backdrop: HTMLElement
  backdropAttributes: AttributeSnapshot
  opener: HTMLElement | null
  fallbackOpener: HTMLElement | null
  initialFocus(): HTMLElement | null
  cancel(): void
}

const modalStack: ModalEntry[] = []
let backgroundAttributes: AttributeSnapshot[] = []

const snapshotAttributes = (element: HTMLElement): AttributeSnapshot => ({
  element,
  inert: element.getAttribute('inert'),
  hidden: element.getAttribute('aria-hidden'),
})

const isolate = (element: HTMLElement) => {
  element.setAttribute('inert', '')
  element.setAttribute('aria-hidden', 'true')
}

const restore = ({ element, inert, hidden }: AttributeSnapshot) => {
  if (inert === null) element.removeAttribute('inert')
  else element.setAttribute('inert', inert)
  if (hidden === null) element.removeAttribute('aria-hidden')
  else element.setAttribute('aria-hidden', hidden)
}

const topModal = () => modalStack[modalStack.length - 1]

const focusModal = (entry: ModalEntry) => {
  const preferred = entry.initialFocus()
  const target = preferred && entry.dialog.contains(preferred)
    ? preferred
    : entry.dialog.querySelector<HTMLElement>(focusableSelector) ?? entry.dialog
  target.focus()
}

const onDocumentFocusIn = (event: FocusEvent) => {
  const active = topModal()
  if (active && (!(event.target instanceof Node) || !active.dialog.contains(event.target))) {
    focusModal(active)
  }
}

const onDocumentKeyDown = (event: KeyboardEvent) => {
  const active = topModal()
  if (!active) return
  if (event.key === 'Escape') {
    event.preventDefault()
    event.stopPropagation()
    active.cancel()
    return
  }
  if (event.key !== 'Tab') return
  const focusable = [...active.dialog.querySelectorAll<HTMLElement>(focusableSelector)]
  if (!focusable.length) {
    event.preventDefault()
    active.dialog.focus()
    return
  }
  const first = focusable[0]
  const last = focusable[focusable.length - 1]
  if (!active.dialog.contains(document.activeElement)) {
    event.preventDefault()
    const target = event.shiftKey ? last : first
    target.focus()
  } else if (event.shiftKey && document.activeElement === first) {
    event.preventDefault()
    last.focus()
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault()
    first.focus()
  }
}

const acquireBackground = () => {
  const backgrounds = new Set<HTMLElement>(
    [...document.body.children]
      .filter((element): element is HTMLElement =>
        element instanceof HTMLElement && !element.classList.contains('modal-backdrop'),
      ),
  )
  document.querySelectorAll<HTMLElement>('.language-switch, .admin-shell').forEach((element) => {
    if (!element.closest('.modal-backdrop')) backgrounds.add(element)
  })
  backgroundAttributes = [...backgrounds].map(snapshotAttributes)
  backgrounds.forEach(isolate)
  document.addEventListener('focusin', onDocumentFocusIn, true)
  document.addEventListener('keydown', onDocumentKeyDown, true)
}

const releaseBackground = () => {
  document.removeEventListener('focusin', onDocumentFocusIn, true)
  document.removeEventListener('keydown', onDocumentKeyDown, true)
  backgroundAttributes.forEach(restore)
  backgroundAttributes = []
}

const registerModal = (entry: ModalEntry) => {
  const previous = topModal()
  if (!previous) acquireBackground()
  else isolate(previous.backdrop)
  modalStack.push(entry)
  focusModal(entry)
  return () => {
    const index = modalStack.indexOf(entry)
    if (index < 0) return
    const wasTop = index === modalStack.length - 1
    modalStack.splice(index, 1)
    restore(entry.backdropAttributes)
    const active = topModal()
    if (!active) {
      releaseBackground()
      const target = entry.opener?.isConnected ? entry.opener
        : entry.fallbackOpener?.isConnected ? entry.fallbackOpener
          : null
      target?.focus()
    } else if (wasTop) {
      restore(active.backdropAttributes)
      if (entry.opener?.isConnected && active.dialog.contains(entry.opener)) entry.opener.focus()
      else focusModal(active)
    }
  }
}

export const hasOpenModal = () => modalStack.length > 0

export function ModalDialog({ title, children, onCancel, initialFocusRef }: {
  title: string
  children: ReactNode
  onCancel(): void
  initialFocusRef?: RefObject<HTMLElement | null>
}) {
  const titleId = useId()
  const dialogRef = useRef<HTMLElement>(null)
  const lifetimeOpener = useRef(document.activeElement instanceof HTMLElement ? document.activeElement : null)
  const cancel = useRef(onCancel)
  cancel.current = onCancel
  useEffect(() => {
    const opener = lifetimeOpener.current
    const dialog = dialogRef.current
    const backdrop = dialog?.closest<HTMLElement>('.modal-backdrop')
    if (!dialog || !backdrop) return
    const parent = topModal()
    return registerModal({
      dialog,
      backdrop,
      backdropAttributes: snapshotAttributes(backdrop),
      opener,
      fallbackOpener: parent?.fallbackOpener ?? parent?.opener ?? opener,
      initialFocus: () => initialFocusRef?.current ?? null,
      cancel: () => cancel.current(),
    })
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
