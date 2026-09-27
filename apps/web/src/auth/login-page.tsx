import { useState, type FormEvent } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { ApiError } from '../api/client'
import { useI18n } from '../i18n'
import { useSession } from './session'

export function LoginPage() {
  const { messages: text } = useI18n()
  const navigate = useNavigate()
  const location = useLocation()
  const { login } = useSession()
  const [error, setError] = useState<'credentials' | 'unavailable' | null>(null)
  const [pending, setPending] = useState(false)

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const data = new FormData(event.currentTarget)
    setPending(true)
    setError(null)
    try {
      await login(String(data.get('email') ?? ''), String(data.get('password') ?? ''))
      const requested = (location.state as { from?: unknown } | null)?.from
      navigate(typeof requested === 'string' && requested.startsWith('/') && !requested.startsWith('//') ? requested : '/app', { replace: true })
    } catch (failure) {
      setError(failure instanceof ApiError && failure.status === 401 ? 'credentials' : 'unavailable')
    } finally {
      setPending(false)
    }
  }

  return <main className="login-page">
    <section className="login-card" aria-labelledby="login-title">
      <div className="brand-mark" aria-hidden="true">K</div>
      <p className="eyebrow">Knowledge Platform</p>
      <h1 id="login-title">{text.login.title}</h1>
      <p className="login-intro">{text.login.intro}</p>
      {(location.state as { invitationAccepted?: unknown } | null)?.invitationAccepted === true && <p role="status">{text.invitation.accepted}</p>}
      <form onSubmit={submit}>
        <label>{text.login.email}<input name="email" type="email" autoComplete="username" required /></label>
        <label>{text.login.password}<input name="password" type="password" autoComplete="current-password" required /></label>
        {error && <p role="alert" className="form-error">
          {error === 'credentials' ? text.login.invalidCredentials : text.login.unavailable}
        </p>}
        <button className="primary-button" disabled={pending} type="submit">{pending ? text.login.pending : text.login.submit}</button>
      </form>
    </section>
  </main>
}
