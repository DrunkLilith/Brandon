import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect, useRef, useState, type FormEvent } from 'react'

import { apiClient, unwrap, unwrapArray } from '../api/client'
import type { components } from '../api/generated'
import { sessionKey, useSession } from '../auth/session'
import { useI18n } from '../i18n'
import { ConfirmDialog, ErrorNotice, LoadingBlock, ModalDialog, PageHeader } from './shared'

type User = components['schemas']['User']
type Invitation = components['schemas']['Invitation']
type InvitationPayload = components['schemas']['InvitationCreateRequest']
type InvitationCreated = components['schemas']['InvitationCreated']

const usersKey = ['admin', 'users'] as const
const invitationsKey = ['admin', 'invitations'] as const

function UserRow({ user }: { user: User }) {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const { session } = useSession()
  const refreshOwnSession = async () => {
    if (user.id === session?.userId) await queryClient.invalidateQueries({ queryKey: sessionKey })
  }
  const [status, setStatus] = useState<'active' | 'suspended'>(user.status === 'suspended' ? 'suspended' : 'active')
  const [role, setRole] = useState(user.workspaceRole)
  const update = useMutation({
    mutationFn: async () => unwrap(await apiClient.PATCH('/api/v1/users/{id}', { params: { path: { id: user.id } }, body: { status, workspaceRole: role } })),
    onSuccess: async () => { await queryClient.invalidateQueries({ queryKey: usersKey }); await refreshOwnSession() },
  })
  const [confirmSessions, setConfirmSessions] = useState(false)
  const revokeSessions = useMutation({
    mutationFn: async () => unwrap(await apiClient.POST('/api/v1/users/{id}/revoke-sessions', { params: { path: { id: user.id } } })),
    onSuccess: async () => { setConfirmSessions(false); await refreshOwnSession() },
  })
  return <>
    <tr>
      <th scope="row"><strong>{user.displayName}</strong><span>{user.email ?? text.common.hiddenEmail}</span></th>
      <td><label><span className="sr-only">{text.users.statusFor(user.displayName)}</span><select value={status} onChange={(event) => setStatus(event.target.value as 'active' | 'suspended')}><option value="active">{text.labels.userStatus.active}</option><option value="suspended">{text.labels.userStatus.suspended}</option></select></label></td>
      <td><label><span className="sr-only">{text.users.roleFor(user.displayName)}</span><select value={role} onChange={(event) => setRole(event.target.value as 'admin' | 'member')}><option value="member">{text.labels.role.member}</option><option value="admin">{text.labels.role.admin}</option></select></label></td>
      <td className="row-actions"><button type="button" onClick={() => update.mutate()} disabled={update.isPending}>{text.common.save}</button><button type="button" onClick={() => setConfirmSessions(true)} aria-label={text.users.revokeSessionsFor(user.displayName)}>{text.users.revokeSessions}</button></td>
    </tr>
    {update.isError && <tr><td colSpan={4}><ErrorNotice error={update.error} /></td></tr>}
    {revokeSessions.isError && <tr><td colSpan={4}><ErrorNotice error={revokeSessions.error} /></td></tr>}
    {confirmSessions && <tr className="dialog-row"><td colSpan={4}><ConfirmDialog title={text.users.revokeSessionsTitle} confirmLabel={text.users.confirmRevokeSessions} pending={revokeSessions.isPending} onCancel={() => setConfirmSessions(false)} onConfirm={() => revokeSessions.mutate()} /></td></tr>}
  </>
}

function InvitationDialog({ onClose }: { onClose(): void }) {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const [email, setEmail] = useState('')
  const [workspaceRole, setWorkspaceRole] = useState<'admin' | 'member'>('member')
  const [expiresInHours, setExpiresInHours] = useState(168)
  const [validation, setValidation] = useState<'email' | 'expiry' | null>(null)
  const [created, setCreated] = useState<InvitationCreated | null>(null)
  const [copyStatus, setCopyStatus] = useState<'copied' | 'failed' | null>(null)
  const idempotency = useRef<{ fingerprint: string; key: string } | null>(null)
  const mounted = useRef(true)
  const emailInput = useRef<HTMLInputElement>(null)
  const copyButton = useRef<HTMLButtonElement>(null)
  useEffect(() => () => { mounted.current = false }, [])
  useEffect(() => { if (created) copyButton.current?.focus() }, [created])
  const mutation = useMutation({
    mutationFn: async (body: InvitationPayload): Promise<void> => {
      const fingerprint = JSON.stringify(body)
      if (idempotency.current?.fingerprint !== fingerprint) idempotency.current = { fingerprint, key: crypto.randomUUID() }
      const result = unwrap(await apiClient.POST('/api/v1/invitations', { params: { header: { 'Idempotency-Key': idempotency.current.key } }, body }))
      if (mounted.current) setCreated(result)
    },
    onSuccess: () => {
      idempotency.current = null
      queryClient.invalidateQueries({ queryKey: invitationsKey })
    },
  })
  const submit = (event: FormEvent) => {
    event.preventDefault()
    const normalized = email.trim().toLowerCase()
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(normalized)) return setValidation('email')
    if (!Number.isInteger(expiresInHours) || expiresInHours < 1 || expiresInHours > 720) return setValidation('expiry')
    setValidation(null)
    mutation.mutate({ email: normalized, workspaceRole, expiresInHours })
  }
  const copy = async () => {
    try {
      if (!navigator.clipboard) throw new Error('clipboard unavailable')
      await navigator.clipboard.writeText(created?.token ?? '')
      setCopyStatus('copied')
    } catch {
      setCopyStatus('failed')
    }
  }
  return <ModalDialog title={text.users.newInvitation} onCancel={onClose} initialFocusRef={emailInput}>
    {created ? <div className="token-result">
      <p>{text.users.tokenNotice}</p>
      <code>{created.token}</code>
      <button ref={copyButton} type="button" onClick={copy}>{text.users.copyToken}</button>
      <p role="status" aria-live="polite">{copyStatus === 'copied' ? text.users.tokenCopied : copyStatus === 'failed' ? text.users.tokenCopyFailed : ''}</p>
      <button type="button" className="secondary-button" onClick={onClose}>{text.common.close}</button>
    </div> : <form onSubmit={submit} noValidate>
      <label>{text.users.invitationEmail}<input ref={emailInput} type="email" value={email} onChange={(event) => setEmail(event.target.value)} /></label>
      <label>{text.users.workspaceRole}<select value={workspaceRole} onChange={(event) => setWorkspaceRole(event.target.value as 'admin' | 'member')}><option value="member">{text.labels.role.member}</option><option value="admin">{text.labels.role.admin}</option></select></label>
      <label>{text.users.expiryHours}<input type="number" min={1} max={720} value={expiresInHours} onChange={(event) => setExpiresInHours(Number(event.target.value))} /></label>
      {validation && <p role="alert" className="form-error">{validation === 'email' ? text.users.invalidEmail : text.users.invalidExpiry}</p>}
      {mutation.isError && <ErrorNotice error={mutation.error} />}
      <div className="dialog-actions"><button type="button" className="secondary-button" onClick={onClose}>{text.common.cancel}</button><button type="submit" disabled={mutation.isPending}>{mutation.isPending ? text.users.creating : text.users.createInvitation}</button></div>
    </form>}
  </ModalDialog>
}

function InvitationRow({ invitation }: { invitation: Invitation }) {
  const { messages: text } = useI18n()
  const queryClient = useQueryClient()
  const [confirm, setConfirm] = useState(false)
  const revoke = useMutation({
    mutationFn: async () => unwrap(await apiClient.DELETE('/api/v1/invitations/{id}', { params: { path: { id: invitation.id } } })),
    onSuccess: () => { setConfirm(false); queryClient.invalidateQueries({ queryKey: invitationsKey }) },
  })
  const email = invitation.email ?? text.common.hiddenEmail
  return <li className="admin-list-item"><div><strong>{email}</strong><span>{text.labels.role[invitation.workspaceRole]} · {text.labels.invitationStatus(invitation.status)}</span></div>{invitation.status === 'pending' && <button type="button" onClick={() => setConfirm(true)} aria-label={text.users.revokeInvitationFor(email)}>{text.users.revokeInvitation}</button>}{revoke.isError && <ErrorNotice error={revoke.error} />}{confirm && <ConfirmDialog title={text.users.revokeInvitationTitle} confirmLabel={text.users.confirmRevokeInvitation} pending={revoke.isPending} onCancel={() => setConfirm(false)} onConfirm={() => revoke.mutate()} />}</li>
}

export function UsersPage() {
  const { messages: text } = useI18n()
  const [showInvitation, setShowInvitation] = useState(false)
  const users = useQuery({ queryKey: usersKey, queryFn: async () => unwrapArray(await apiClient.GET('/api/v1/users')) })
  const invitations = useQuery({ queryKey: invitationsKey, queryFn: async () => unwrapArray(await apiClient.GET('/api/v1/invitations')) })
  return <section>
    <PageHeader title={text.pages.users}><button type="button" className="primary-button" onClick={() => setShowInvitation(true)}>{text.users.invite}</button></PageHeader>
    <section className="admin-panel" aria-labelledby="users-title"><h2 id="users-title">{text.users.members}</h2>
      {users.isPending ? <LoadingBlock /> : users.isError ? <ErrorNotice error={users.error} /> : <div className="table-scroll"><table><thead><tr><th>{text.users.user}</th><th>{text.users.status}</th><th>{text.users.role}</th><th>{text.users.actions}</th></tr></thead><tbody>{users.data.map((user) => <UserRow key={user.id} user={user} />)}</tbody></table></div>}
    </section>
    <section className="admin-panel" aria-labelledby="invitations-title"><h2 id="invitations-title">{text.users.invitations}</h2>
      {invitations.isPending ? <LoadingBlock /> : invitations.isError ? <ErrorNotice error={invitations.error} /> : invitations.data.length ? <ul className="admin-list">{invitations.data.map((invitation) => <InvitationRow key={invitation.id} invitation={invitation} />)}</ul> : <p className="notice">{text.users.noInvitations}</p>}
    </section>
    {showInvitation && <InvitationDialog onClose={() => setShowInvitation(false)} />}
  </section>
}
