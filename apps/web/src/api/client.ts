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

  const uploadAttachment = async ({ pageId, file, filename, idempotencyKey }: UploadAttachmentInput): Promise<Attachment> => {
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
