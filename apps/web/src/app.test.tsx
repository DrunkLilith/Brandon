import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'

import { App } from './app'

describe('App', () => {
  it('renders the knowledge workspace landmark', () => {
    render(<App />)

    expect(
      screen.getByRole('main', { name: /knowledge workspace/i }),
    ).toBeVisible()
  })
})
