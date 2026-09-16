import { useState, type FormEvent } from 'react'
import { useLocation, useNavigate } from 'react-router-dom'

import { useSession } from './session'

export function LoginPage() {
  const navigate = useNavigate()
  const location = useLocation()
  const { login } = useSession()
  const [error, setError] = useState(false)
  const [pending, setPending] = useState(false)

  const submit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    const data = new FormData(event.currentTarget)
    setPending(true)
    setError(false)
    try {
      await login(String(data.get('email') ?? ''), String(data.get('password') ?? ''))
      const requested = (location.state as { from?: unknown } | null)?.from
      navigate(typeof requested === 'string' && requested.startsWith('/') && !requested.startsWith('//') ? requested : '/app', { replace: true })
    } catch {
      setError(true)
    } finally {
      setPending(false)
    }
  }

  return <main className="login-page">
    <section className="login-card" aria-labelledby="login-title">
      <div className="brand-mark" aria-hidden="true">K</div>
      <p className="eyebrow">Knowledge Platform</p>
      <h1 id="login-title">Вход</h1>
      <p className="login-intro">Войдите в защищённое рабочее пространство.</p>
      <form onSubmit={submit}>
        <label>Электронная почта<input name="email" type="email" autoComplete="username" required /></label>
        <label>Пароль<input name="password" type="password" autoComplete="current-password" required /></label>
        {error && <p role="alert" className="form-error">Неверная почта или пароль</p>}
        <button className="primary-button" disabled={pending} type="submit">{pending ? 'Входим…' : 'Войти'}</button>
      </form>
    </section>
  </main>
}
