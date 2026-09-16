import type { Csrf, Problem } from './generated'

type JsonValue = Record<string, unknown> | unknown[] | string | number | boolean | null

export interface RequestOptions {
  method?: string
  json?: JsonValue
  body?: BodyInit
  signal?: AbortSignal
  csrf?: boolean
  response?: 'json' | 'binary' | 'empty'
  headers?: HeadersInit
}

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

export class ApiClient {
  #csrf: string | undefined

  invalidateCsrf() {
    this.#csrf = undefined
  }

  async #csrfToken(signal?: AbortSignal) {
    if (this.#csrf) return this.#csrf
    const response = await fetch('/api/v1/auth/csrf', { credentials: 'include', signal })
    if (!response.ok) throw await this.#error(response)
    this.#csrf = ((await response.json()) as Csrf).token
    return this.#csrf
  }

  async #error(response: Response) {
    let value: unknown
    if (response.headers.get('content-type')?.includes('application/problem+json')) {
      try { value = await response.json() } catch { value = undefined }
    }
    return new ApiError(safeProblem(value, response.status))
  }

  async request<T = unknown>(path: string, options: RequestOptions = {}): Promise<T> {
    const method = (options.method ?? 'GET').toUpperCase()
    const mutation = !['GET', 'HEAD', 'OPTIONS'].includes(method)
    const headers = new Headers(options.headers)
    let body = options.body
    if (options.json !== undefined) {
      headers.set('Content-Type', 'application/json')
      body = JSON.stringify(options.json)
    }
    if (mutation && options.csrf !== false) {
      headers.set('X-CSRF-Token', await this.#csrfToken(options.signal))
    }
    const response = await fetch(path, {
      method,
      headers,
      body,
      signal: options.signal,
      credentials: 'include',
    })
    if (!response.ok) {
      if (response.status === 401 || response.status === 403) this.invalidateCsrf()
      throw await this.#error(response)
    }
    if (options.response === 'empty' || response.status === 204) return undefined as T
    if (options.response === 'binary') return (await response.blob()) as T
    return await response.json() as T
  }
}

export const apiClient = new ApiClient()
