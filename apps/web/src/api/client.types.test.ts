import { expectTypeOf, it } from 'vitest'

import type { components } from './generated'
import { apiClient, unwrapArray } from './client'

type User = components['schemas']['User']
type Group = components['schemas']['Group']

it('infers generated endpoint array types without caller-selected response casts', () => {
  if (false) {
    void (async () => {
      const users = unwrapArray(await apiClient.GET('/api/v1/users'))
      expectTypeOf(users).toEqualTypeOf<readonly User[]>()

      // @ts-expect-error A users endpoint response cannot be selected as groups.
      const groups: readonly Group[] = users
      void groups
    })()
  }
})
