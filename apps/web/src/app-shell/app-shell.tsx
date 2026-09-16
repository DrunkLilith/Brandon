import { useEffect, useRef, type ReactNode } from 'react'

import { Sidebar } from './sidebar'
import { Topbar } from './topbar'
import { useSession } from '../auth/session'

const isTyping = (target: EventTarget | null) => target instanceof HTMLElement && (
  target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName)
)

export function AppShell({ children }: { children?: ReactNode }) {
  const { logout } = useSession()
  const search = useRef<HTMLButtonElement>(null)
  useEffect(() => {
    const shortcut = (event: KeyboardEvent) => {
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === 'k' && !isTyping(event.target)) {
        event.preventDefault()
        search.current?.focus()
      }
    }
    window.addEventListener('keydown', shortcut)
    return () => window.removeEventListener('keydown', shortcut)
  }, [])

  return <div className="app-shell">
    <Sidebar />
    <div className="app-column">
      <Topbar ref={search} onLogout={logout} />
      <main className="content" aria-label="Рабочая область">
        {children ?? <section className="empty-state"><p className="eyebrow">Рабочее пространство</p><h1>Ваши знания — в одном месте</h1><p>Выберите страницу слева или создайте первую. Живые страницы и редактор появятся на следующем этапе.</p></section>}
      </main>
    </div>
  </div>
}
