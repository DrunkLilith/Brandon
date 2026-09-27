import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState, type FormEvent } from 'react'

import { apiClient, unwrap } from '../api/client'
import { useI18n } from '../i18n'
import { ErrorNotice, LoadingBlock, PageHeader } from './shared'

const settingsKey = ['admin', 'settings'] as const

export function SettingsPage() {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const settings = useQuery({ queryKey: settingsKey, queryFn: async () => unwrap(await apiClient.GET('/api/v1/admin/settings')) })
  const [attachmentRecoveryDays, setAttachmentRecoveryDays] = useState(30)
  const [exportRetentionHours, setExportRetentionHours] = useState(24)
  const [validation, setValidation] = useState('')
  useEffect(() => {
    if (settings.data) {
      setAttachmentRecoveryDays(settings.data.attachmentRecoveryDays)
      setExportRetentionHours(settings.data.exportRetentionHours)
    }
  }, [settings.data])
  const update = useMutation({
    mutationFn: async () => unwrap(await apiClient.PATCH('/api/v1/admin/settings', { body: { attachmentRecoveryDays, exportRetentionHours } })),
    onSuccess: (data) => queryClient.setQueryData(settingsKey, data),
  })
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!Number.isInteger(attachmentRecoveryDays) || attachmentRecoveryDays < 1 || attachmentRecoveryDays > 3650) return setValidation(text.settings.invalidRecovery)
    if (!Number.isInteger(exportRetentionHours) || exportRetentionHours < 1 || exportRetentionHours > 720) return setValidation(text.settings.invalidRetention)
    setValidation('')
    update.mutate()
  }
  return <section><PageHeader title={text.pages.settings} />
    <section className="admin-panel"><h2>{text.settings.storage}</h2>{settings.isPending ? <LoadingBlock /> : settings.isError ? <ErrorNotice error={settings.error} /> : <form className="settings-form" onSubmit={submit} noValidate>
      <label>{text.settings.recoveryDays}<input type="number" min={1} max={3650} value={attachmentRecoveryDays} onChange={(event) => setAttachmentRecoveryDays(Number(event.target.value))} /></label>
      <label>{text.settings.retentionHours}<input type="number" min={1} max={720} value={exportRetentionHours} onChange={(event) => setExportRetentionHours(Number(event.target.value))} /></label>
      {validation && <p role="alert" className="form-error">{validation}</p>}
      {update.isError && <ErrorNotice error={update.error} />}
      {update.isSuccess && <p role="status">{text.settings.saved}</p>}
      <button type="submit" disabled={update.isPending}>{update.isPending ? text.settings.saving : text.settings.save}</button>
    </form>}</section>
    <section className="admin-panel"><h2>{text.settings.security}</h2><p className="notice">{text.settings.sessionHint}</p></section>
  </section>
}
