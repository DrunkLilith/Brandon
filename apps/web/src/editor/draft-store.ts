import type { components } from '../api/generated'
export type Draft = Pick<
  components['schemas']['Page'],
  'title' | 'markdown' | 'properties' | 'restricted'
>
export const draftKey = (
  workspaceId: string,
  pageId: string,
  version: number,
) => `${workspaceId}:${pageId}:${version}`
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
