import { Link } from 'react-router-dom'
import { useI18n } from '../i18n'

export function NotFound() {
  const { messages } = useI18n()
  return <main className="content"><section className="empty-state"><h1>{messages.notFound.title}</h1><p>{messages.notFound.description}</p><Link to="/app">{messages.notFound.action}</Link></section></main>
}
