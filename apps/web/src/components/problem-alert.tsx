import { ApiError } from '../api/client'
import { useI18n } from '../i18n'

export function ProblemAlert({ error }: { error: unknown }) {
  const { messages } = useI18n()
  let message = messages.failure.generic
  if (!navigator.onLine) message = messages.failure.offline
  else if (error instanceof ApiError) {
    if (error.status === 403) message = messages.failure.forbidden
    else if (error.status === 404) message = messages.failure.notFound
    else if (error.status === 409) message = messages.failure.conflict
    else if (error.status === 429) message = messages.failure.rateLimited
    else if (error.status >= 500) message = messages.failure.serverUnavailable
  } else if (error instanceof TypeError) {
    message = messages.failure.serverUnavailable
  }
  return <p className="notice error-notice" role="alert">{message}</p>
}
