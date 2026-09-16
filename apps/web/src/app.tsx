import './styles.css'

import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { useState } from 'react'
import { BrowserRouter, Navigate, Route, Routes } from 'react-router-dom'

import { AppShell } from './app-shell/app-shell'
import { LoginPage } from './auth/login-page'
import { SessionProvider, useSession } from './auth/session'

function Loading() { return <main className="loading" aria-label="Загрузка">Загрузка…</main> }

function LoginRoute() {
  const { session, loading } = useSession()
  if (loading) return <Loading />
  return session ? <Navigate to="/app" replace /> : <LoginPage />
}

function AppRoute() {
  const { session, loading } = useSession()
  if (loading) return <Loading />
  return session ? <AppShell /> : <Navigate to="/login" replace />
}

function AdminRoute() {
  const { session, loading } = useSession()
  if (loading) return <Loading />
  if (!session) return <Navigate to="/login" replace />
  return session.workspaceAdmin ? <AppShell><section className="empty-state"><p className="eyebrow">Администрирование</p><h1>Панель управления</h1></section></AppShell> : <Navigate to="/app" replace />
}

export function App() {
  const [queryClient] = useState(() => new QueryClient({ defaultOptions: { queries: { retry: false } } }))
  return <QueryClientProvider client={queryClient}><SessionProvider><BrowserRouter><Routes>
    <Route path="/login" element={<LoginRoute />} />
    <Route path="/app/*" element={<AppRoute />} />
    <Route path="/admin/*" element={<AdminRoute />} />
    <Route path="*" element={<Navigate to="/app" replace />} />
  </Routes></BrowserRouter></SessionProvider></QueryClientProvider>
}
