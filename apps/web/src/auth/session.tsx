import { useQuery, useQueryClient } from '@tanstack/react-query'
import { createContext, useContext, type ReactNode } from 'react'

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

  const login = async (email: string, password: string) => {
    const session = unwrap(await apiClient.POST('/api/v1/auth/login', { body: { email, password } }))
    apiClient.invalidateCsrf()
    queryClient.setQueryData(sessionKey, session)
  }
  const logout = async () => {
    unwrap(await apiClient.POST('/api/v1/auth/logout'))
    apiClient.invalidateCsrf()
    queryClient.setQueryData(sessionKey, null)
  }

  return <SessionContext.Provider value={{ session: query.data ?? null, loading: query.isPending, error: query.isError, login, logout }}>{children}</SessionContext.Provider>
}

export function useSession() {
  const value = useContext(SessionContext)
  if (!value) throw new Error('SessionProvider is missing')
  return value
}
