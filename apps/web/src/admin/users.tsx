import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useRef, useState, type FormEvent } from 'react'

import { apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'
import { adminMessages as text } from './messages'
import { apiArray, apiObject, ConfirmDialog, ErrorNotice, LoadingBlock, PageHeader } from './shared'

type User = components['schemas']['User']
type Invitation = components['schemas']['Invitation']
type InvitationPayload = components['schemas']['InvitationCreateRequest']
type InvitationCreated = components['schemas']['InvitationCreated']

const usersKey = ['admin', 'users'] as const
const invitationsKey = ['admin', 'invitations'] as const

function UserRow({ user }: { user: User }) {
  const queryClient = useQueryClient()
  const [status, setStatus] = useState<'active' | 'suspended'>(user.status === 'suspended' ? 'suspended' : 'active')
  const [role, setRole] = useState(user.workspaceRole)
  const update = useMutation({
    mutationFn: async () => unwrap(await apiClient.PATCH('/api/v1/users/{id}', { params: { path: { id: user.id } }, body: { status, workspaceRole: role } })),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: usersKey }),
  })
  const [confirmSessions, setConfirmSessions] = useState(false)
  const revokeSessions = useMutation({
    mutationFn: async () => unwrap(await apiClient.POST('/api/v1/users/{id}/revoke-sessions', { params: { path: { id: user.id } } })),
    onSuccess: () => setConfirmSessions(false),
  })
  return <>
    <tr>
      <th scope="row"><strong>{user.displayName}</strong><span>{user.email ?? 'Адрес скрыт'}</span></th>
      <td><label><span className="sr-only">Статус {user.displayName}</span><select value={status} onChange={(event) => setStatus(event.target.value as 'active' | 'suspended')}><option value="active">Активен</option><option value="suspended">Приостановлен</option></select></label></td>
      <td><label><span className="sr-only">Роль {user.displayName}</span><select value={role} onChange={(event) => setRole(event.target.value as 'admin' | 'member')}><option value="member">Участник</option><option value="admin">Администратор</option></select></label></td>
      <td className="row-actions"><button type="button" onClick={() => update.mutate()} disabled={update.isPending}>Сохранить</button><button type="button" onClick={() => setConfirmSessions(true)} aria-label={`Отозвать сеансы участника ${user.displayName}`}>Отозвать сеансы</button></td>
    </tr>
    {update.isError && <tr><td colSpan={4}><ErrorNotice error={update.error} /></td></tr>}
    {revokeSessions.isError && <tr><td colSpan={4}><ErrorNotice error={revokeSessions.error} /></td></tr>}
    {confirmSessions && <tr className="dialog-row"><td colSpan={4}><ConfirmDialog title="Отозвать все сеансы?" confirmLabel="Подтвердить отзыв сеансов" pending={revokeSessions.isPending} onCancel={() => setConfirmSessions(false)} onConfirm={() => revokeSessions.mutate()} /></td></tr>}
  </>
}

function InvitationDialog({ onClose }: { onClose(): void }) {
  const queryClient = useQueryClient()
  const [email, setEmail] = useState('')
  const [workspaceRole, setWorkspaceRole] = useState<'admin' | 'member'>('member')
  const [expiresInHours, setExpiresInHours] = useState(168)
  const [validation, setValidation] = useState('')
  const [created, setCreated] = useState<InvitationCreated | null>(null)
  const [copyStatus, setCopyStatus] = useState('')
  const idempotency = useRef<{ fingerprint: string; key: string } | null>(null)
  const mutation = useMutation({
    mutationFn: async (body: InvitationPayload): Promise<InvitationCreated> => {
      const fingerprint = JSON.stringify(body)
      if (idempotency.current?.fingerprint !== fingerprint) idempotency.current = { fingerprint, key: crypto.randomUUID() }
      return apiObject<InvitationCreated>(unwrap(await apiClient.POST('/api/v1/invitations', { params: { header: { 'Idempotency-Key': idempotency.current.key } }, body })))
    },
    onSuccess: (result) => {
      idempotency.current = null
      setCreated(result)
      queryClient.invalidateQueries({ queryKey: invitationsKey })
    },
  })
  const submit = (event: FormEvent) => {
    event.preventDefault()
    const normalized = email.trim().toLowerCase()
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(normalized)) return setValidation('Введите корректный адрес электронной почты.')
    if (!Number.isInteger(expiresInHours) || expiresInHours < 1 || expiresInHours > 720) return setValidation('Срок действия должен быть от 1 до 720 часов.')
    setValidation('')
    mutation.mutate({ email: normalized, workspaceRole, expiresInHours })
  }
  const copy = async () => {
    try {
      if (!navigator.clipboard) throw new Error('clipboard unavailable')
      await navigator.clipboard.writeText(created?.token ?? '')
      setCopyStatus('Токен скопирован')
    } catch {
      setCopyStatus('Не удалось скопировать токен')
    }
  }
  return <div className="modal-backdrop"><section className="admin-dialog" role="dialog" aria-modal="true" aria-labelledby="invite-title">
    <h2 id="invite-title">Новое приглашение</h2>
    {created ? <div className="token-result">
      <p>Сохраните токен сейчас. После закрытия он больше не показывается.</p>
      <code>{created.token}</code>
      <button type="button" onClick={copy}>Скопировать токен</button>
      <p role="status" aria-live="polite">{copyStatus}</p>
      <button type="button" className="secondary-button" onClick={onClose}>Закрыть</button>
    </div> : <form onSubmit={submit} noValidate>
      <label>Электронная почта приглашённого<input autoFocus type="email" value={email} onChange={(event) => setEmail(event.target.value)} /></label>
      <label>Роль в рабочем пространстве<select value={workspaceRole} onChange={(event) => setWorkspaceRole(event.target.value as 'admin' | 'member')}><option value="member">Участник</option><option value="admin">Администратор</option></select></label>
      <label>Срок действия, часов<input type="number" min={1} max={720} value={expiresInHours} onChange={(event) => setExpiresInHours(Number(event.target.value))} /></label>
      {validation && <p role="alert" className="form-error">{validation}</p>}
      {mutation.isError && <ErrorNotice error={mutation.error} />}
      <div className="dialog-actions"><button type="button" className="secondary-button" onClick={onClose}>Отмена</button><button type="submit" disabled={mutation.isPending}>{mutation.isPending ? 'Создание…' : 'Создать приглашение'}</button></div>
    </form>}
  </section></div>
}

function InvitationRow({ invitation }: { invitation: Invitation }) {
  const queryClient = useQueryClient()
  const [confirm, setConfirm] = useState(false)
  const revoke = useMutation({
    mutationFn: async () => unwrap(await apiClient.DELETE('/api/v1/invitations/{id}', { params: { path: { id: invitation.id } } })),
    onSuccess: () => { setConfirm(false); queryClient.invalidateQueries({ queryKey: invitationsKey }) },
  })
  const email = invitation.email ?? 'Адрес скрыт'
  return <li className="admin-list-item"><div><strong>{email}</strong><span>{invitation.workspaceRole === 'admin' ? 'Администратор' : 'Участник'} · {invitation.status}</span></div>{invitation.status === 'pending' && <button type="button" onClick={() => setConfirm(true)} aria-label={`Отозвать приглашение ${email}`}>Отозвать</button>}{revoke.isError && <ErrorNotice error={revoke.error} />}{confirm && <ConfirmDialog title="Отозвать приглашение?" confirmLabel="Подтвердить отзыв приглашения" pending={revoke.isPending} onCancel={() => setConfirm(false)} onConfirm={() => revoke.mutate()} />}</li>
}

export function UsersPage() {
  const [showInvitation, setShowInvitation] = useState(false)
  const users = useQuery({ queryKey: usersKey, queryFn: async () => apiArray<User>(unwrap(await apiClient.GET('/api/v1/users'))) })
  const invitations = useQuery({ queryKey: invitationsKey, queryFn: async () => apiArray<Invitation>(unwrap(await apiClient.GET('/api/v1/invitations'))) })
  return <section>
    <PageHeader title={text.pages.users}><button type="button" className="primary-button" onClick={() => setShowInvitation(true)}>Пригласить пользователя</button></PageHeader>
    <section className="admin-panel" aria-labelledby="users-title"><h2 id="users-title">Участники</h2>
      {users.isPending ? <LoadingBlock /> : users.isError ? <ErrorNotice error={users.error} /> : <div className="table-scroll"><table><thead><tr><th>Пользователь</th><th>Статус</th><th>Роль</th><th>Действия</th></tr></thead><tbody>{users.data.map((user) => <UserRow key={user.id} user={user} />)}</tbody></table></div>}
    </section>
    <section className="admin-panel" aria-labelledby="invitations-title"><h2 id="invitations-title">Приглашения</h2>
      {invitations.isPending ? <LoadingBlock /> : invitations.isError ? <ErrorNotice error={invitations.error} /> : invitations.data.length ? <ul className="admin-list">{invitations.data.map((invitation) => <InvitationRow key={invitation.id} invitation={invitation} />)}</ul> : <p className="notice">Нет активных приглашений.</p>}
    </section>
    {showInvitation && <InvitationDialog onClose={() => setShowInvitation(false)} />}
  </section>
}
