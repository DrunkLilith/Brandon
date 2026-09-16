import createClient from 'openapi-fetch'

import type { components, paths } from './generated'

type Problem = components['schemas']['Problem']
type Csrf = components['schemas']['Csrf']

export class ApiError extends Error {
  readonly status: number
  readonly code: string
  readonly type: string

  constructor(problem: Pick<Problem, 'status' | 'code' | 'title' | 'type'>) {
    super(problem.title)
    this.name = 'ApiError'
    this.status = problem.status
    this.code = problem.code
    this.type = problem.type
  }
}

const safeProblem = (value: unknown, status: number): Problem => {
  if (value && typeof value === 'object') {
    const candidate = value as Partial<Problem>
    if (typeof candidate.code === 'string' && typeof candidate.title === 'string' && typeof candidate.type === 'string') {
      return { status, code: candidate.code, title: candidate.title, type: candidate.type, detail: '' }
    }
  }
  return { status, code: 'request_failed', title: 'Request failed', type: 'about:blank', detail: '' }
}

type ApiResult<Data, Error> =
  | { data: Data; error?: never; response: Response }
  | { data?: never; error: Error; response: Response }

export function unwrap<Data, Error>(result: ApiResult<Data, Error>): Data {
  if (result.error !== undefined) throw new ApiError(safeProblem(result.error, result.response.status))
  return result.data as Data
}

export function createApiClient() {
  let csrf: string | undefined
  const baseUrl = globalThis.location?.origin ?? 'http://localhost'
  const client = createClient<paths>({
    baseUrl,
    credentials: 'include',
    fetch: (request) => globalThis.fetch(request),
  })

  client.use({
    async onRequest({ request }) {
      const path = new URL(request.url).pathname
      if (!['GET', 'HEAD', 'OPTIONS'].includes(request.method) && path !== '/api/v1/auth/login') {
        if (!csrf) {
          const response = await fetch('/api/v1/auth/csrf', { credentials: 'include', signal: request.signal })
          const value: unknown = await response.json().catch(() => undefined)
          if (!response.ok) throw new ApiError(safeProblem(value, response.status))
          csrf = (value as Csrf).token
        }
        request.headers.set('X-CSRF-Token', csrf)
      }
      return request
    },
    onResponse({ response }) {
      if (response.status === 401 || response.status === 403) csrf = undefined
      return response
    },
  })

  return Object.assign(client, { invalidateCsrf: () => { csrf = undefined } })
}

export const apiClient = createApiClient()
