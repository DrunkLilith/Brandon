import { useState, type FormEvent } from 'react'
import { useNavigate, useParams } from 'react-router-dom'

import { apiClient, unwrap } from '../api/client'
import { ErrorNotice } from '../admin/shared'
import { useI18n } from '../i18n'

export function InvitationPage() {
  const { messages: text } = useI18n()
  const navigate = useNavigate()
  const { token = '' } = useParams()
  const [pending, setPending] = useState(false)
  const [error, setError] = useState<unknown>()
  const [validation, setValidation] = useState<'mismatch' | 'short' | null>(null)

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const data = new FormData(event.currentTarget)
    const displayName = String(data.get('displayName') ?? '').trim()
    const password = String(data.get('password') ?? '')
    const confirmation = String(data.get('confirmation') ?? '')
    if (password.length < 12) return setValidation('short')
    if (password !== confirmation) return setValidation('mismatch')
    setValidation(null)
    setError(undefined)
    setPending(true)
    try {
      unwrap(await apiClient.POST('/api/v1/invitations/{token}/accept', {
        params: { path: { token } },
        body: { displayName, password },
      }))
      navigate('/login', { replace: true, state: { invitationAccepted: true } })
    } catch (reason) {
      setError(reason)
    } finally {
      setPending(false)
    }
  }

  return <main className="login-page">
    <section className="login-card" aria-labelledby="invitation-title">
      <div className="brand-mark" aria-hidden="true">K</div>
      <p className="eyebrow">Knowledge Platform</p>
      <h1 id="invitation-title">{text.invitation.title}</h1>
      <p>{text.invitation.intro}</p>
      <form onSubmit={(event) => void submit(event)}>
        <label>{text.invitation.displayName}<input name="displayName" autoComplete="name" required /></label>
        <label>{text.login.password}<input name="password" type="password" autoComplete="new-password" required minLength={12} /></label>
        <label>{text.invitation.confirmPassword}<input name="confirmation" type="password" autoComplete="new-password" required minLength={12} /></label>
        {validation && <p role="alert" className="form-error">{validation === 'short' ? text.invitation.passwordTooShort : text.invitation.passwordMismatch}</p>}
        {error != null && <ErrorNotice error={error} />}
        <button className="primary-button" disabled={pending} type="submit">{pending ? text.invitation.pending : text.invitation.submit}</button>
      </form>
    </section>
  </main>
}
