import { useEffect, useRef, useState } from 'react'
import { useQueryClient } from '@tanstack/react-query'
import { apiClient } from '../api/client'
import { ErrorNotice } from '../admin/shared'
import { workspaceMessages as text } from '../pages/messages'
export function Uploader({ pageId }: { pageId: string }) {
  const cache = useQueryClient(),
    [file, setFile] = useState<File | null>(null),
    [invalid, setInvalid] = useState(false),
    [error, setError] = useState<unknown>(),
    [status, setStatus] = useState<
      'idle' | 'uploading' | 'cancelled' | 'uploaded'
    >('idle')
  const abort = useRef<AbortController | null>(null),
    input = useRef<HTMLInputElement>(null)
  const idempotencyKey = useRef<string | null>(null)
  useEffect(() => () => abort.current?.abort(), [])
  const upload = async () => {
    if (!file || abort.current) return
    const controller = new AbortController()
    abort.current = controller
    setError(undefined)
    setStatus('uploading')
    try {
      idempotencyKey.current ??= crypto.randomUUID()
      await apiClient.uploadAttachment({
        pageId,
        file,
        signal: controller.signal,
        idempotencyKey: idempotencyKey.current,
      })
      if (controller.signal.aborted) {
        setStatus('cancelled')
        return
      }
      idempotencyKey.current = null
      setStatus('uploaded')
      setFile(null)
      if (input.current) input.current.value = ''
      await cache.invalidateQueries({
        queryKey: ['workspace', 'attachments', pageId],
      })
    } catch (reason) {
      if (controller.signal.aborted) setStatus('cancelled')
      else {
        setError(reason)
        setStatus('idle')
      }
    } finally {
      abort.current = null
    }
  }
  return (
    <div>
      <p>{text.workspace.encrypted}</p>
      <label>
        {text.workspace.file}
        <input
          ref={input}
          type="file"
          disabled={status === 'uploading'}
          onChange={(e) => {
            idempotencyKey.current = null
            const chosen = e.target.files?.[0] ?? null
            const bad =
              chosen !== null &&
              (chosen.size === 0 || chosen.size > 100 * 1024 * 1024)
            setInvalid(bad)
            setFile(bad ? null : chosen)
            setError(undefined)
            setStatus('idle')
          }}
        />
      </label>
      {invalid && <p role="alert">{text.workspace.invalidFile}</p>}
      {error != null && <ErrorNotice error={error} />}
      <button
        disabled={!file || status === 'uploading'}
        onClick={() => void upload()}
      >
        {text.workspace.upload}
      </button>
      {status === 'uploading' && (
        <>
          <div role="progressbar" aria-label={text.workspace.uploading}>
            {text.workspace.uploading}
          </div>
          <button
            onClick={() => {
              // Abort cannot establish whether the server committed. Keep
              // this file's replay key until acknowledged success or reselection.
              abort.current?.abort()
            }}
          >
            {text.workspace.cancelUpload}
          </button>
        </>
      )}
      {status === 'cancelled' && (
        <p role="status">{text.workspace.cancelled}</p>
      )}
      {status === 'uploaded' && <p role="status">{text.workspace.uploaded}</p>}
    </div>
  )
}
