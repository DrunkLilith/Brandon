import { afterEach, expect, it } from 'vitest'
import { draftKey, readDraft, writeDraft } from './draft-store'

afterEach(() => localStorage.clear())

it('isolates authenticated identities and independently durable browser tabs', () => {
  const first = draftKey('workspace', 'page', 1, 'alice', 'first')
  const second = draftKey('workspace', 'page', 1, 'alice', 'second')
  const other = draftKey('workspace', 'page', 1, 'bob', 'first')
  writeDraft(first, { title: 'draft', markdown: 'private', properties: {}, restricted: false })
  expect(readDraft(other)).toBeNull()
  expect(readDraft(second)).toBeNull()
})
