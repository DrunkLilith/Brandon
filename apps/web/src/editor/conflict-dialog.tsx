import { useState } from 'react'
import type { components } from '../api/generated'
import { ErrorNotice, ModalDialog } from '../admin/shared'
import { useI18n } from '../i18n'
import type { Draft } from './draft-store'
export function ConflictDialog({
  base,
  submitted,
  current,
  error,
  onClose,
  onReload,
  onResolve,
}: {
  base: number
  submitted: Draft
  current?: components['schemas']['Page']
  error?: unknown
  onClose(): void
  onReload(page: components['schemas']['Page']): void
  onResolve(page: components['schemas']['Page']): void
}) {
  const { messages: text } = useI18n()
  const [copyState, setCopyState] = useState(false),
    [copyError, setCopyError] = useState<unknown>(),
    [confirm, setConfirm] = useState(false)
  return (
    <ModalDialog title={text.workspace.conflict} onCancel={onClose}>
      <p>{text.workspace.base(base)}</p>
      {current ? (
        <p>
          {text.workspace.current(current.version)} · {current.title}
        </p>
      ) : error ? (
        <ErrorNotice error={error} />
      ) : (
        <p role="status">{text.loading}</p>
      )}
      <pre>
        {submitted.title}
        {'\n\n'}
        {submitted.markdown}
      </pre>
      {copyState && <p role="status">{text.workspace.copied}</p>}
      {copyError != null && <ErrorNotice error={copyError} />}
      <button
        onClick={() => {
          void (async () => {
            try {
              if (!navigator.clipboard?.writeText)
                throw new Error('Clipboard unavailable')
              await navigator.clipboard.writeText(
                `${submitted.title}\n\n${submitted.markdown}`,
              )
              setCopyState(true)
            } catch (reason) {
              setCopyError(reason)
            }
          })()
        }}
      >
        {text.workspace.copy}
      </button>
      <button disabled={!current} onClick={() => setConfirm(true)}>
        {text.workspace.reload}
      </button>
      {confirm && current && (
        <div>
          <p>{text.workspace.reloadWarning}</p>
          <button onClick={() => onReload(current)}>
            {text.workspace.apply}
          </button>
        </div>
      )}
      <button
        disabled={!current}
        onClick={() => {
          if (current) onResolve(current)
        }}
      >
        {text.workspace.resolve}
      </button>
      <button onClick={onClose}>{text.common.close}</button>
    </ModalDialog>
  )
}
