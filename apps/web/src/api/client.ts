import createClient from 'openapi-fetch'

import type { components, paths } from './generated'

type Problem = components['schemas']['Problem']
type Csrf = components['schemas']['Csrf']
type Attachment = components['schemas']['Attachment']

interface UploadAttachmentInput {
  pageId: paths['/api/v1/pages/{id}/attachments']['post']['parameters']['path']['id']
  file: Blob | File | FormData
  filename?: string
  idempotencyKey?: string
  signal?: AbortSignal
}

interface DownloadAttachmentInput {
  attachmentId: paths['/api/v1/attachments/{id}']['get']['parameters']['path']['id']
  range?: string
}

interface DownloadExportInput {
  exportId: paths['/api/v1/exports/{id}/download']['get']['parameters']['path']['id']
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

type ApiResult = { data?: unknown; error?: unknown; response: Response }
type SuccessData<Result> = Result extends { data: infer Data } ? Data : never
type NormalizeGenerated<Data> = Data extends string | number | boolean | bigint | symbol | null | undefined ? Data
  : Data extends { readonly [index: number]: infer Item; readonly length: number }
  ? readonly NormalizeGenerated<Item>[]
  : Data extends object ? { readonly [Key in keyof Data]: NormalizeGenerated<Data[Key]> } : Data
type NormalizedData<Result> = NormalizeGenerated<SuccessData<Result>>

export function unwrap<Result extends ApiResult>(result: Result): NormalizedData<Result> {
  if (result.error !== undefined) throw new ApiError(safeProblem(result.error, result.response.status))
  return result.data as NormalizedData<Result>
}

export function unwrapArray<Result extends ApiResult>(result: Result): Extract<NormalizedData<Result>, readonly unknown[]> {
  const data: unknown = unwrap(result)
  if (!Array.isArray(data)) throw new TypeError('Expected an API array response')
  return data as Extract<NormalizedData<Result>, readonly unknown[]>
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

  const uploadAttachment = async ({ pageId, file, filename, idempotencyKey, signal }: UploadAttachmentInput): Promise<Attachment> => {
    const form = file instanceof FormData ? file : new FormData()
    const payload = file instanceof FormData ? file.get('file') : file
    if (!(payload instanceof Blob)) throw new TypeError('Attachment upload requires a binary file field')
    const suppliedName = typeof File !== 'undefined' && payload instanceof File ? payload.name : undefined
    if (!(file instanceof FormData)) form.append('file', payload, filename ?? suppliedName ?? 'attachment')
    const attachment = unwrap(await client.POST('/api/v1/pages/{id}/attachments', {
      params: {
        path: { id: pageId },
        query: { size: payload.size },
        header: idempotencyKey ? { 'Idempotency-Key': idempotencyKey } : undefined,
      },
      body: { file: filename ?? suppliedName ?? 'attachment' },
      bodySerializer: () => form,
      signal,
    }))
    if (!attachment) throw new TypeError('Attachment response was empty')
    return attachment
  }

  const requireBlob = (value: unknown): Blob => {
    if (value instanceof Blob) return value
    throw new TypeError('Binary response was not parsed as a Blob')
  }

  const downloadAttachment = async ({ attachmentId, range }: DownloadAttachmentInput): Promise<Blob> => {
    const result = await client.GET('/api/v1/attachments/{id}', {
      params: {
        path: { id: attachmentId },
        header: range ? { Range: range } : undefined,
      },
      parseAs: 'blob',
    })
    return requireBlob(unwrap(result))
  }

  const downloadExport = async ({ exportId }: DownloadExportInput): Promise<Blob> => {
    const result = await client.GET('/api/v1/exports/{id}/download', {
      params: { path: { id: exportId } },
      parseAs: 'blob',
    })
    return requireBlob(unwrap(result))
  }

  return Object.assign(client, {
    invalidateCsrf: () => { csrf = undefined },
    uploadAttachment,
    downloadAttachment,
    downloadExport,
  })
}

export const apiClient = createApiClient()
