import { useQuery, useQueryClient } from '@tanstack/react-query'
import { createContext, useContext, useEffect, type ReactNode } from 'react'

import { ApiError, apiClient, unwrap } from '../api/client'
import type { components } from '../api/generated'

type Session = components['schemas']['Session']

export const sessionKey = ['session'] as const

interface SessionContextValue {
  session: Session | null
  loading: boolean
  error: boolean
  login(email: string, password: string): Promise<void>
  logout(): Promise<void>
}

const SessionContext = createContext<SessionContextValue | undefined>(undefined)

export function SessionProvider({ children }: { children: ReactNode }) {
  const queryClient = useQueryClient()
  const query = useQuery({
    queryKey: sessionKey,
    retry: false,
    queryFn: async () => {
      try { return unwrap(await apiClient.GET('/api/v1/auth/session')) }
      catch (error) {
        if (error instanceof ApiError && error.status === 401) return null
        throw error
      }
    },
  })

  useEffect(() => {
    const changed = (event: StorageEvent) => {
      if (event.key !== 'knowledge.auth.changed') return
      window.dispatchEvent(new Event('knowledge:flush-drafts'))
      queryClient.removeQueries({ predicate: (entry) => entry.queryKey[0] !== 'session' })
      void queryClient.invalidateQueries({ queryKey: sessionKey })
    }
    window.addEventListener('storage', changed)
    return () => window.removeEventListener('storage', changed)
  }, [queryClient])
  const resetIdentity = () => {
    window.dispatchEvent(new Event('knowledge:flush-drafts'))
    queryClient.removeQueries({ predicate: (entry) => entry.queryKey[0] !== 'session' })
    try { localStorage.setItem('knowledge.auth.changed', crypto.randomUUID()) } catch { /* Storage can be unavailable. */ }
  }

  const login = async (email: string, password: string) => {
    const session = unwrap(await apiClient.POST('/api/v1/auth/login', { body: { email, password } }))
    apiClient.invalidateCsrf()
    resetIdentity()
    queryClient.setQueryData(sessionKey, session)
  }
  const logout = async () => {
    window.dispatchEvent(new Event('knowledge:flush-drafts'))
    unwrap(await apiClient.POST('/api/v1/auth/logout'))
    apiClient.invalidateCsrf()
    resetIdentity()
    queryClient.setQueryData(sessionKey, null)
  }

  return <SessionContext.Provider value={{ session: query.data ?? null, loading: query.isPending, error: query.isError, login, logout }}>{children}</SessionContext.Provider>
}

export function useSession() {
  const value = useContext(SessionContext)
  if (!value) throw new Error('SessionProvider is missing')
  return value
}
