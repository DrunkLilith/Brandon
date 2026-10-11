import { describe, expect, it } from 'vitest'

import { getAdminMessages } from './messages'

describe('admin message boundary', () => {
  it('provides parallel locale-ready labels, parameterized announcements, and locale-aware dates', () => {
    const english = getAdminMessages('en')
    expect(english.backToWorkspace).toBe('Back to workspace')
    expect(english.users.invite).toBe('Invite user')
    expect(english.users.revokeSessionsFor('Ada')).toBe('Revoke sessions for Ada')
    expect(english.format.dateTime('2026-09-24T12:00:00Z')).toContain('2026')

    const russian = getAdminMessages('ru')
    expect(russian.backToWorkspace).toBe('В рабочую область')
    expect(russian.users.invite).toBe('Пригласить пользователя')
    expect(russian.users.revokeSessionsFor('Ада')).toBe('Отозвать сеансы участника Ада')
  })
})
