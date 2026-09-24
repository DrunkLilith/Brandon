import { cleanup, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, expect, it, vi } from 'vitest'
import { ConflictDialog } from './conflict-dialog'
afterEach(() => {
  cleanup()
  vi.restoreAllMocks()
})
it('keeps submitted text and reports an unavailable clipboard without throwing', async () => {
  const user = userEvent.setup()
  vi.spyOn(navigator, 'clipboard', 'get').mockReturnValue(
    undefined as unknown as Clipboard,
  )
  render(
    <ConflictDialog
      base={1}
      submitted={{
        title: 'Моё',
        markdown: 'Несохранённый текст',
        properties: {},
        restricted: true,
      }}
      onClose={() => {}}
      onReload={() => {}}
      onResolve={() => {}}
    />,
  )
  await user.click(
    screen.getByRole('button', { name: 'Скопировать мой текст' }),
  )
  expect(await screen.findByRole('alert')).toBeVisible()
  expect(screen.getByText(/Несохранённый текст/)).toBeVisible()
})
