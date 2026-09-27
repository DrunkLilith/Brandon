import { describe, expect, it } from 'vitest'

import { en } from './en'
import { ru } from './ru'

type Leaf = string | ((...args: never[]) => string)

function flatten(value: object, prefix = ''): Map<string, Leaf> {
  const result = new Map<string, Leaf>()
  for (const [key, child] of Object.entries(value)) {
    const path = prefix ? `${prefix}.${key}` : key
    if (typeof child === 'string' || typeof child === 'function') {
      result.set(path, child as Leaf)
    } else if (child && typeof child === 'object') {
      for (const [nestedPath, leaf] of flatten(child, path)) {
        result.set(nestedPath, leaf)
      }
    } else {
      throw new TypeError(`Unsupported message leaf at ${path}`)
    }
  }
  return result
}

const formatterSamples: Record<string, readonly unknown[]> = {
  'labels.invitationStatus': ['pending'],
  'labels.exportStatus': ['ready'],
  'labels.health': [true],
  'users.statusFor': ['Ada'],
  'users.roleFor': ['Ada'],
  'users.revokeSessionsFor': ['Ada'],
  'users.revokeInvitationFor': ['ada@example.com'],
  'groups.groupFor': ['Docs'],
  'groups.removeMember': ['Ada'],
  'groups.addMember': ['Ada'],
  'spaces.accessFor': ['Docs'],
  'spaces.accessTitle': ['Docs'],
  'spaces.roleFor': ['Ada'],
  'workspace.version': [3],
  'workspace.compare': [3],
  'workspace.restore': [3],
  'workspace.base': [2],
  'workspace.current': [3],
  'workspace.role': ['subject-id'],
  'workspace.removeGrant': ['subject-id'],
  'workspace.download': ['guide.pdf'],
  'format.dateTime': ['2026-09-24T12:00:00Z'],
}

describe('locale resources', () => {
  it('keeps identical nonblank RU and EN leaves, including every formatter', () => {
    const english = flatten(en)
    const russian = flatten(ru)
    expect([...english.keys()].sort()).toEqual([...russian.keys()].sort())

    for (const [path, englishLeaf] of english) {
      const russianLeaf = russian.get(path)
      expect(typeof russianLeaf, path).toBe(typeof englishLeaf)
      if (typeof englishLeaf === 'string') {
        expect(englishLeaf.trim(), `en:${path}`).not.toBe('')
        expect((russianLeaf as string).trim(), `ru:${path}`).not.toBe('')
      } else {
        const sample = formatterSamples[path]
        expect(sample, `missing explicit formatter sample for ${path}`).toBeDefined()
        expect(englishLeaf(...(sample as never[])).trim(), `en:${path}`).not.toBe('')
        expect((russianLeaf as (...args: never[]) => string)(...(sample as never[])).trim(), `ru:${path}`).not.toBe('')
      }
    }

    const formatterPaths = [...english]
      .filter(([, leaf]) => typeof leaf === 'function')
      .map(([path]) => path)
      .sort()
    expect(Object.keys(formatterSamples).sort()).toEqual(formatterPaths)
  })
})
