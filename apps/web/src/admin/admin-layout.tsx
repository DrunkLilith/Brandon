import { useEffect } from 'react'
import { NavLink, Outlet } from 'react-router-dom'

import { adminMessages as text } from './messages'

const navigation = [
  ['/admin', text.navigation.dashboard],
  ['/admin/users', text.navigation.users],
  ['/admin/groups', text.navigation.groups],
  ['/admin/spaces', text.navigation.spaces],
  ['/admin/audit', text.navigation.audit],
  ['/admin/settings', text.navigation.settings],
] as const

export function AdminLayout() {
  useEffect(() => { document.documentElement.lang = text.locale }, [])
  return <div className="admin-shell">
    <header className="admin-topbar">
      <NavLink to="/app" className="admin-brand"><span aria-hidden="true" className="brand-mark small">K</span><span>{text.brand}</span></NavLink>
      <span className="admin-context">{text.console}</span>
    </header>
    <div className="admin-body">
      <aside className="admin-sidebar">
        <nav aria-label={text.navigationLabel}>
          {navigation.map(([to, label]) => <NavLink key={to} to={to} end={to === '/admin'}>{label}</NavLink>)}
        </nav>
      </aside>
      <main className="admin-content" id="admin-content"><Outlet /></main>
    </div>
  </div>
}
