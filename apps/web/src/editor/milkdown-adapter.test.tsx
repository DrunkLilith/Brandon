import { cleanup, render, screen, waitFor } from '@testing-library/react'
import { afterEach, expect, it } from 'vitest'
import { MilkdownAdapter } from './milkdown-adapter'
afterEach(cleanup)
it('renders real rich Markdown while HTML and unsafe URLs stay inert', async () => {
  render(
    <MilkdownAdapter
      value={
        '# Heading\n\n<script>window.bad=true</script>\n\n[bad](javascript:alert%281%29)\n\n![bad](data:image/svg+xml,test)'
      }
      onChange={() => {}}
    />,
  )
  expect(await screen.findByRole('heading', { name: 'Heading' })).toBeVisible()
  expect(document.querySelector('script')).toBeNull()
  expect(document.querySelector('a[href^="javascript:"]')).toBeNull()
  expect(document.querySelector('img[src^="data:"]')).toBeNull()
  await waitFor(() =>
    expect(document.querySelector('[contenteditable="true"]')).not.toBeNull(),
  )
})
