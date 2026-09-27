import type { components } from '../api/generated'
export type Draft = Pick<
  components['schemas']['Page'],
  'title' | 'markdown' | 'properties' | 'restricted'
>
// Deliberately not sessionStorage: a newly opened/duplicated tab can inherit
// sessionStorage. Each document owns its writes; all prior documents remain
// discoverable for that authenticated identity after reload or a crash.
const documentId = crypto.randomUUID()
const prefix = (workspaceId: string, pageId: string, userId: string) =>
  `knowledge.draft.v2:${encodeURIComponent(userId)}:${encodeURIComponent(workspaceId)}:${encodeURIComponent(pageId)}:`
export const draftKey = (
  workspaceId: string,
  pageId: string,
  version: number,
  userId: string,
  tabId: string = documentId,
) => `${prefix(workspaceId, pageId, userId)}${version}:${tabId}`

export function discoverDrafts(workspaceId: string, pageId: string, userId: string) {
  const starts = prefix(workspaceId, pageId, userId)
  const result: { key: string; version: number; draft: Draft }[] = []
  // Unowned legacy workspace:page:version entries cannot be attributed safely.
  // Never adopt or display them as another user's draft.
  for (let index = 0; index < localStorage.length; index++) {
    const key = localStorage.key(index)
    if (!key?.startsWith(starts)) continue
    const version = Number(key.slice(starts.length).split(':')[0])
    const draft = readDraft(key)
    if (draft && Number.isSafeInteger(version) && version > 0) result.push({ key, version, draft })
  }
  return result.sort((a, b) => b.version - a.version || a.key.localeCompare(b.key))
}
export function readDraft(key: string): Draft | null {
  try {
    const value: unknown = JSON.parse(localStorage.getItem(key) ?? 'null')
    if (!value || typeof value !== 'object') return null
    const draft = value as Partial<Draft>
    return typeof draft.title === 'string' &&
      typeof draft.markdown === 'string' &&
      typeof draft.restricted === 'boolean' &&
      draft.properties !== null &&
      typeof draft.properties === 'object' &&
      !Array.isArray(draft.properties)
      ? {
          title: draft.title,
          markdown: draft.markdown,
          restricted: draft.restricted,
          properties: draft.properties,
        }
      : null
  } catch {
    return null
  }
}
export function writeDraft(key: string, draft: Draft) {
  localStorage.setItem(
    key,
    JSON.stringify({
      title: draft.title,
      markdown: draft.markdown,
      properties: draft.properties,
      restricted: draft.restricted,
    }),
  )
}
