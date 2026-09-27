import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { useState } from 'react'

import { ApiError } from './api/client'
import { ModalDialog } from './admin/shared'
import { OfflineBanner } from './components/offline-banner'
import { ProblemAlert } from './components/problem-alert'
import { MarkdownEditor } from './editor/markdown-editor'
import { I18nProvider, LanguageSwitch, useI18n } from './i18n'

afterEach(() => {
  cleanup()
  localStorage.clear()
  Object.defineProperty(navigator, 'onLine', { configurable: true, value: true })
  document.documentElement.lang = 'en'
  vi.restoreAllMocks()
})

function LocaleProbe() {
  const { messages, locale } = useI18n()
  const [draft, setDraft] = useState('')
  return <><LanguageSwitch /><p data-testid="locale">{locale}</p><p>{messages.login.title}</p><label>{messages.workspace.raw}<textarea value={draft} onChange={(event) => setDraft(event.target.value)} /></label></>
}

describe('runtime localization', () => {
  it('updates mounted UI immediately and survives a failed preference write', async () => {
    localStorage.setItem('knowledge.locale', 'corrupt')
    vi.spyOn(Storage.prototype, 'setItem').mockImplementation(() => {
      throw new DOMException('blocked', 'SecurityError')
    })
    const user = userEvent.setup()
    render(<I18nProvider><LocaleProbe /></I18nProvider>)

    expect(screen.getByRole('heading', { name: 'Язык' })).toBeVisible()
    expect(screen.getByText('Вход')).toBeVisible()
    await user.type(screen.getByRole('textbox', { name: 'Исходный Markdown' }), 'local draft')
    await user.click(screen.getByRole('button', { name: 'English' }))
    expect(screen.getByText('Sign in')).toBeVisible()
    expect(screen.getByTestId('locale')).toHaveTextContent('en')
    expect(screen.getByRole('textbox', { name: 'Raw Markdown' })).toHaveValue('local draft')
    expect(document.documentElement.lang).toBe('en')
  })

  it('restores a valid preference and safely falls back when reading storage fails', async () => {
    localStorage.setItem('knowledge.locale', 'en')
    const first = render(<I18nProvider><LocaleProbe /></I18nProvider>)
    expect(screen.getByText('Sign in')).toBeVisible()
    first.unmount()

    vi.spyOn(Storage.prototype, 'getItem').mockImplementation(() => {
      throw new DOMException('blocked', 'SecurityError')
    })
    render(<I18nProvider><LocaleProbe /></I18nProvider>)
    expect(screen.getByText('Вход')).toBeVisible()
  })
})

describe('resilient accessible states', () => {
  it.each([
    [403, 'Недостаточно прав'],
    [404, 'не найден'],
    [409, 'конфликтует'],
    [503, 'временно недоступен'],
  ])('renders a localized safe alert for HTTP %s', (status, expected) => {
    render(<I18nProvider><ProblemAlert error={new ApiError({ status, code: 'x', type: 'x', title: 'SECRET SERVER DETAIL' })} /></I18nProvider>)
    const alert = screen.getByRole('alert')
    expect(alert).toHaveTextContent(expected)
    expect(alert).not.toHaveTextContent('SECRET SERVER DETAIL')
  })

  it('announces connectivity loss without remounting or hiding a local draft', async () => {
    Object.defineProperty(navigator, 'onLine', { configurable: true, value: true })
    function DraftHarness() {
      const [draft, setDraft] = useState('')
      return <I18nProvider><OfflineBanner /><MarkdownEditor value={draft} onChange={setDraft} /></I18nProvider>
    }
    const user = userEvent.setup()
    render(<DraftHarness />)
    await user.click(screen.getByRole('button', { name: 'Исходный Markdown' }))
    await user.type(screen.getByRole('textbox', { name: 'Markdown' }), 'unsaved markdown')

    Object.defineProperty(navigator, 'onLine', { configurable: true, value: false })
    fireEvent(window, new Event('offline'))

    expect(screen.getByRole('status')).toHaveTextContent('Нет подключения')
    expect(screen.getByRole('textbox', { name: 'Markdown' })).toHaveValue('unsaved markdown')
  })

  it('focuses and traps both Tab directions, closes on Escape, and restores the opener', async () => {
    function DialogHarness() {
      const [open, setOpen] = useState(false)
      return <I18nProvider><button onClick={() => setOpen(true)}>Open</button>{open && <ModalDialog title="Dialog" onCancel={() => setOpen(false)}><button>First</button><button>Last</button></ModalDialog>}</I18nProvider>
    }
    const user = userEvent.setup()
    render(<DialogHarness />)
    const opener = screen.getByRole('button', { name: 'Open' })
    await user.click(opener)
    const first = screen.getByRole('button', { name: 'First' })
    const last = screen.getByRole('button', { name: 'Last' })
    expect(first).toHaveFocus()

    last.focus()
    await user.tab()
    expect(first).toHaveFocus()
    first.focus()
    await user.tab({ shift: true })
    expect(last).toHaveFocus()
    fireEvent.keyDown(last, { key: 'Escape' })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(opener).toHaveFocus()
  })
})
