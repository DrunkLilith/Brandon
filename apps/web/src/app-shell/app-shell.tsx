import type { ReactNode } from 'react'

import { Sidebar } from './sidebar'
import { Topbar } from './topbar'
import { useSession } from '../auth/session'

export function AppShell({ children }: { children?: ReactNode }) {
  const { logout } = useSession()

  return <div className="app-shell">
    <Sidebar />
    <div className="app-column">
      <Topbar onLogout={logout} />
      <main className="content" aria-label="Рабочая область">
        {children ?? <section className="empty-state"><p className="eyebrow">Рабочее пространство</p><h1>Ваши знания — в одном месте</h1><p>Выберите страницу слева или создайте первую. Живые страницы и редактор появятся на следующем этапе.</p></section>}
      </main>
    </div>
  </div>
}
