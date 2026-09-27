import './styles.css'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { useEffect, useState, type ReactNode } from 'react'
import { BrowserRouter, Navigate, Route, Routes, useLocation } from 'react-router-dom'

import { AppShell } from './app-shell/app-shell'
import { AdminLayout } from './admin/admin-layout'
import { AuditPage } from './admin/audit'
import { DashboardPage } from './admin/dashboard'
import { GroupsPage } from './admin/groups'
import { SettingsPage } from './admin/settings'
import { SpacesPage } from './admin/spaces'
import { UsersPage } from './admin/users'
import { LoginPage } from './auth/login-page'
import { SessionProvider, useSession } from './auth/session'
import { NotFound } from './components/not-found'
import { OfflineBanner } from './components/offline-banner'
import { I18nProvider, LanguageSwitch, useI18n } from './i18n'

function Loading() {
  const { messages } = useI18n()
  return <main className="loading" aria-label={messages.app.loadingLabel}>{messages.app.loading}</main>
}
function SessionUnavailable() {
  const { messages } = useI18n()
  return <main className="loading" role="alert">{messages.app.sessionUnavailable}</main>
}

function WorkspaceBoundary({ children }: { children: ReactNode }) {
  const [client] = useState(() => new QueryClient({ defaultOptions: { queries: { retry: false } } }))
  useEffect(() => () => { void client.cancelQueries(); client.clear() }, [client])
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>
}

function LoginRoute() {
  const { session, loading, error } = useSession()
  const location = useLocation()
  if (loading) return <Loading />
  if (error) return <SessionUnavailable />
  const requested = (location.state as { from?: unknown } | null)?.from
  const destination = typeof requested === 'string' && requested.startsWith('/') && !requested.startsWith('//') ? requested : '/app'
  return session ? <Navigate to={destination} replace /> : <LoginPage />
}

function AppRoute() {
  const { session, loading, error } = useSession()
  const location = useLocation()
  if (loading) return <Loading />
  if (error) return <SessionUnavailable />
  return session ? <WorkspaceBoundary key={session.sessionId}><AppShell /></WorkspaceBoundary> : <Navigate to="/login" replace state={{ from: `${location.pathname}${location.search}${location.hash}` }} />
}

function AdminRoute() {
  const { session, loading, error } = useSession()
  const location = useLocation()
  if (loading) return <Loading />
  if (error) return <SessionUnavailable />
  if (!session) return <Navigate to="/login" replace state={{ from: `${location.pathname}${location.search}${location.hash}` }} />
  return session.workspaceAdmin ? <AdminLayout /> : <Navigate to="/app" replace />
}

export function App() {
  const [queryClient] = useState(() => new QueryClient({ defaultOptions: { queries: { retry: false } } }))
  return <I18nProvider>
    <LanguageSwitch />
    <OfflineBanner />
    <QueryClientProvider client={queryClient}><SessionProvider><BrowserRouter><Routes>
      <Route path="/login" element={<LoginRoute />} />
      <Route path="/app/*" element={<AppRoute />} />
      <Route path="/admin" element={<AdminRoute />}>
        <Route index element={<DashboardPage />} />
        <Route path="users" element={<UsersPage />} />
        <Route path="groups" element={<GroupsPage />} />
        <Route path="spaces" element={<SpacesPage />} />
        <Route path="audit" element={<AuditPage />} />
        <Route path="settings" element={<SettingsPage />} />
        <Route path="*" element={<NotFound />} />
      </Route>
      <Route path="*" element={<NotFound />} />
    </Routes></BrowserRouter></SessionProvider></QueryClientProvider>
  </I18nProvider>
}
