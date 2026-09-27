import { useEffect, useState } from 'react'
import { useI18n } from '../i18n'

export function OfflineBanner() {
  const { messages } = useI18n()
  const [online, setOnline] = useState(() => navigator.onLine)
  useEffect(() => {
    const connected = () => setOnline(true)
    const disconnected = () => setOnline(false)
    window.addEventListener('online', connected)
    window.addEventListener('offline', disconnected)
    return () => {
      window.removeEventListener('online', connected)
      window.removeEventListener('offline', disconnected)
    }
  }, [])
  return online ? null : <div className="offline-banner" role="status" aria-live="polite">{messages.failure.offline}</div>
}
