import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useState, type FormEvent } from 'react'

import { apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { adminMessages as text } from './messages'
import { apiObject, ErrorNotice, LoadingBlock, PageHeader } from './shared'

type Settings = components['schemas']['Settings']

const settingsKey = ['admin', 'settings'] as const

export function SettingsPage() {
  const queryClient = useQueryClient()
  const settings = useQuery({ queryKey: settingsKey, queryFn: async () => apiObject<Settings>(unwrap(await apiClient.GET('/api/v1/admin/settings'))) })
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
    mutationFn: async () => apiObject<Settings>(unwrap(await apiClient.PATCH('/api/v1/admin/settings', { body: { attachmentRecoveryDays, exportRetentionHours } }))),
    onSuccess: (data) => queryClient.setQueryData(settingsKey, data),
  })
  const submit = (event: FormEvent) => {
    event.preventDefault()
    if (!Number.isInteger(attachmentRecoveryDays) || attachmentRecoveryDays < 1 || attachmentRecoveryDays > 3650) return setValidation('Срок восстановления вложений должен быть от 1 до 3650 дней.')
    if (!Number.isInteger(exportRetentionHours) || exportRetentionHours < 1 || exportRetentionHours > 720) return setValidation('Срок хранения экспорта должен быть от 1 до 720 часов.')
    setValidation('')
    update.mutate()
  }
  return <section><PageHeader title={text.pages.settings} />
    <section className="admin-panel"><h2>Хранение и восстановление</h2>{settings.isPending ? <LoadingBlock /> : settings.isError ? <ErrorNotice error={settings.error} /> : <form className="settings-form" onSubmit={submit} noValidate>
      <label>Срок восстановления вложений, дней<input type="number" min={1} max={3650} value={attachmentRecoveryDays} onChange={(event) => setAttachmentRecoveryDays(Number(event.target.value))} /></label>
      <label>Срок хранения экспорта, часов<input type="number" min={1} max={720} value={exportRetentionHours} onChange={(event) => setExportRetentionHours(Number(event.target.value))} /></label>
      {validation && <p role="alert" className="form-error">{validation}</p>}
      {update.isError && <ErrorNotice error={update.error} />}
      {update.isSuccess && <p role="status">Настройки сохранены.</p>}
      <button type="submit" disabled={update.isPending}>{update.isPending ? 'Сохранение…' : 'Сохранить настройки'}</button>
    </form>}</section>
    <section className="admin-panel"><h2>Безопасность</h2><p className="notice">Отзыв сеансов отдельных пользователей доступен в разделе «Пользователи».</p></section>
  </section>
}
