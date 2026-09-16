import { useQuery, useQueryClient } from '@tanstack/react-query'
import { createContext, useContext, type ReactNode } from 'react'

import { ApiError, apiClient } from '../api/client'
import type { Session } from '../api/generated'

const sessionKey = ['session'] as const

interface SessionContextValue {
  session: Session | null
  loading: boolean
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
      try { return await apiClient.request<Session>('/api/v1/auth/session') }
      catch (error) {
        if (error instanceof ApiError && error.status === 401) return null
        throw error
      }
    },
  })

  const login = async (email: string, password: string) => {
    const session = await apiClient.request<Session>('/api/v1/auth/login', {
      method: 'POST', json: { email, password }, csrf: false,
    })
    apiClient.invalidateCsrf()
    queryClient.setQueryData(sessionKey, session)
  }
  const logout = async () => {
    await apiClient.request('/api/v1/auth/logout', { method: 'POST', response: 'empty' })
    apiClient.invalidateCsrf()
    queryClient.setQueryData(sessionKey, null)
  }

  return <SessionContext.Provider value={{ session: query.data ?? null, loading: query.isPending, login, logout }}>{children}</SessionContext.Provider>
}

export function useSession() {
  const value = useContext(SessionContext)
  if (!value) throw new Error('SessionProvider is missing')
  return value
}
