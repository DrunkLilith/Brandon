import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { StrictMode, useState } from 'react'

import { ApiError } from './api/client'
import { ModalDialog } from './admin/shared'
import { OfflineBanner } from './components/offline-banner'
import { ProblemAlert } from './components/problem-alert'
import { MarkdownEditor } from './editor/markdown-editor'
import { I18nProvider, LanguageSwitch, useI18n } from './i18n'

afterEach(() => {
  cleanup()
  document.querySelectorAll('[data-modal-background-probe], [data-modal-opener-probe]').forEach((element) => element.remove())
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

function StackedDialogHarness({ onCancelA, onCancelB }: {
  onCancelA(): void
  onCancelB(): void
}) {
  const [aOpen, setAOpen] = useState(false)
  const [bOpen, setBOpen] = useState(false)
  return <I18nProvider>
    <button onClick={() => setAOpen(true)}>Open A</button>
    {aOpen && <ModalDialog title="Dialog A" onCancel={() => { onCancelA(); setAOpen(false) }}>
      <button onClick={() => setBOpen(true)}>Open B</button>
      <button>A last</button>
    </ModalDialog>}
    {bOpen && <ModalDialog title="Dialog B" onCancel={() => { onCancelB(); setBOpen(false) }}>
      <button>B first</button>
      <button onClick={() => setAOpen(false)}>Unmount A first</button>
    </ModalDialog>}
  </I18nProvider>
}

function SimultaneousDialogHarness() {
  const [aOpen, setAOpen] = useState(true)
  const [bOpen, setBOpen] = useState(true)
  return <I18nProvider>
    {aOpen && <ModalDialog title="Simultaneous A" onCancel={() => setAOpen(false)}>
      <button>A first</button>
    </ModalDialog>}
    {bOpen && <ModalDialog title="Simultaneous B" onCancel={() => setBOpen(false)}>
      <button>B first</button>
      <button onClick={() => setAOpen(false)}>Unmount lower A</button>
    </ModalDialog>}
  </I18nProvider>
}

function ReplacingDialogHarness() {
  const [dialog, setDialog] = useState<'a' | 'b' | null>('a')
  if (dialog === 'a') return <I18nProvider><ModalDialog key="a" title="Replacement A" onCancel={() => setDialog(null)}>
    <button onClick={() => setDialog('b')}>Replace with B</button>
  </ModalDialog></I18nProvider>
  if (dialog === 'b') return <I18nProvider><ModalDialog key="b" title="Replacement B" onCancel={() => setDialog(null)}>
    <button>B first</button>
  </ModalDialog></I18nProvider>
  return null
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
      return <I18nProvider><LanguageSwitch /><button onClick={() => setOpen(true)}>Open</button>{open && <ModalDialog title="Dialog" onCancel={() => setOpen(false)}><button>First</button><button>Last</button></ModalDialog>}</I18nProvider>
    }
    const user = userEvent.setup()
    render(<StrictMode><DialogHarness /></StrictMode>)
    const opener = screen.getByRole('button', { name: 'Open' })
    const englishSwitch = screen.getByRole('button', { name: 'English' })
    await user.click(opener)
    const first = screen.getByRole('button', { name: 'First' })
    const last = screen.getByRole('button', { name: 'Last' })
    expect(first).toHaveFocus()

    englishSwitch.focus()
    fireEvent.focusIn(englishSwitch)
    expect(first).toHaveFocus()
    expect(englishSwitch.closest('div')).toHaveAttribute('inert')

    last.focus()
    await user.tab()
    expect(first).toHaveFocus()
    first.focus()
    await user.tab({ shift: true })
    expect(last).toHaveFocus()
    fireEvent.keyDown(last, { key: 'Escape' })
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(opener).toHaveFocus()
    expect(englishSwitch.closest('div')).not.toHaveAttribute('inert')
  })

  it('gives stacked dialog ownership to the top entry and restores exact background state top-first', async () => {
    const probe = document.createElement('aside')
    probe.dataset.modalBackgroundProbe = 'true'
    probe.setAttribute('inert', 'kept')
    probe.setAttribute('aria-hidden', 'false')
    document.body.append(probe)
    const cancelA = vi.fn(), cancelB = vi.fn()
    const user = userEvent.setup()
    render(<StrictMode><StackedDialogHarness onCancelA={cancelA} onCancelB={cancelB} /></StrictMode>)
    const opener = screen.getByRole('button', { name: 'Open A' })
    await user.click(opener)
    const openB = screen.getByRole('button', { name: 'Open B' })
    await user.click(openB)

    const dialogA = screen.getByRole('dialog', { name: 'Dialog A', hidden: true })
    const dialogB = screen.getByRole('dialog', { name: 'Dialog B' })
    const firstB = screen.getByRole('button', { name: 'B first' })
    expect(firstB).toHaveFocus()
    expect(dialogA.closest('.modal-backdrop')).toHaveAttribute('inert')
    expect(dialogA.closest('.modal-backdrop')).toHaveAttribute('aria-hidden', 'true')
    expect(dialogB.closest('.modal-backdrop')).not.toHaveAttribute('inert')
    expect(probe).toHaveAttribute('aria-hidden', 'true')

    await user.tab({ shift: true })
    expect(screen.getByRole('button', { name: 'Unmount A first' })).toHaveFocus()
    await user.tab()
    expect(firstB).toHaveFocus()

    openB.focus()
    fireEvent.focusIn(openB)
    expect(firstB).toHaveFocus()
    fireEvent.keyDown(firstB, { key: 'Escape' })
    expect(cancelB).toHaveBeenCalledOnce()
    expect(cancelA).not.toHaveBeenCalled()
    expect(screen.queryByRole('dialog', { name: 'Dialog B' })).not.toBeInTheDocument()
    expect(screen.getByRole('dialog', { name: 'Dialog A' })).toBeVisible()
    expect(openB).toHaveFocus()
    expect(probe).toHaveAttribute('aria-hidden', 'true')

    fireEvent.keyDown(openB, { key: 'Escape' })
    expect(cancelA).toHaveBeenCalledOnce()
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(opener).toHaveFocus()
    expect(probe).toHaveAttribute('inert', 'kept')
    expect(probe).toHaveAttribute('aria-hidden', 'false')

    fireEvent.keyDown(document, { key: 'Escape' })
    expect(cancelA).toHaveBeenCalledOnce()
    expect(cancelB).toHaveBeenCalledOnce()
  })

  it('keeps the top dialog and background isolated when a lower dialog unmounts first', async () => {
    const probe = document.createElement('aside')
    probe.dataset.modalBackgroundProbe = 'true'
    document.body.append(probe)
    const cancelA = vi.fn(), cancelB = vi.fn()
    const user = userEvent.setup()
    render(<StrictMode><StackedDialogHarness onCancelA={cancelA} onCancelB={cancelB} /></StrictMode>)
    const opener = screen.getByRole('button', { name: 'Open A' })
    await user.click(opener)
    await user.click(screen.getByRole('button', { name: 'Open B' }))
    await user.click(screen.getByRole('button', { name: 'Unmount A first' }))

    expect(screen.queryByRole('dialog', { name: 'Dialog A', hidden: true })).not.toBeInTheDocument()
    const dialogB = screen.getByRole('dialog', { name: 'Dialog B' })
    expect(dialogB).toBeVisible()
    expect(probe).toHaveAttribute('inert')
    expect(probe).toHaveAttribute('aria-hidden', 'true')

    fireEvent.keyDown(dialogB, { key: 'Escape' })
    expect(cancelA).not.toHaveBeenCalled()
    expect(cancelB).toHaveBeenCalledOnce()
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
    expect(probe).not.toHaveAttribute('inert')
    expect(probe).not.toHaveAttribute('aria-hidden')
    expect(opener).toHaveFocus()
  })

  it.each(['B then A', 'A then B'])(
    'preserves the lifetime opener for simultaneous StrictMode dialogs closed %s',
    async (order) => {
      const probe = document.createElement('aside')
      probe.dataset.modalBackgroundProbe = 'true'
      probe.setAttribute('inert', 'kept')
      probe.setAttribute('aria-hidden', 'false')
      document.body.append(probe)
      const opener = document.createElement('button')
      opener.dataset.modalOpenerProbe = 'true'
      opener.textContent = 'Original opener'
      document.body.append(opener)
      opener.focus()
      expect(opener).toHaveFocus()
      const user = userEvent.setup()
      const addListener = vi.spyOn(document, 'addEventListener')
      const removeListener = vi.spyOn(document, 'removeEventListener')
      const listenerState = () => {
        const events = [
          ...addListener.mock.calls.map((args, index) => ({ type: args[0], delta: 1, order: addListener.mock.invocationCallOrder[index] })),
          ...removeListener.mock.calls.map((args, index) => ({ type: args[0], delta: -1, order: removeListener.mock.invocationCallOrder[index] })),
        ].filter((event) => event.type === 'focusin' || event.type === 'keydown')
          .sort((left, right) => left.order - right.order)
        return (['focusin', 'keydown'] as const).map((type) => {
          let active = 0, peak = 0
          for (const event of events) {
            if (event.type !== type) continue
            active += event.delta
            peak = Math.max(peak, active)
          }
          return { type, active, peak }
        })
      }

      render(<StrictMode><SimultaneousDialogHarness /></StrictMode>)
      expect(screen.getByRole('button', { name: 'B first' })).toHaveFocus()
      expect(listenerState()).toEqual([
        { type: 'focusin', active: 1, peak: 1 },
        { type: 'keydown', active: 1, peak: 1 },
      ])

      if (order === 'B then A') {
        fireEvent.keyDown(screen.getByRole('dialog', { name: 'Simultaneous B' }), { key: 'Escape' })
        const remaining = screen.getByRole('dialog', { name: 'Simultaneous A' })
        expect(remaining.contains(document.activeElement)).toBe(true)
        expect(probe).toHaveAttribute('aria-hidden', 'true')
        fireEvent.keyDown(remaining, { key: 'Escape' })
      } else {
        await user.click(screen.getByRole('button', { name: 'Unmount lower A' }))
        const remaining = screen.getByRole('dialog', { name: 'Simultaneous B' })
        expect(remaining.contains(document.activeElement)).toBe(true)
        expect(probe).toHaveAttribute('aria-hidden', 'true')
        fireEvent.keyDown(remaining, { key: 'Escape' })
      }

      expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
      expect(opener).toHaveFocus()
      expect(probe).toHaveAttribute('inert', 'kept')
      expect(probe).toHaveAttribute('aria-hidden', 'false')
      expect(listenerState()).toEqual([
        { type: 'focusin', active: 0, peak: 1 },
        { type: 'keydown', active: 0, peak: 1 },
      ])
    },
  )

  it.each([
    ['without StrictMode', false],
    ['with StrictMode', true],
  ] as const)(
    'restores the external opener after an atomic keyed A to B replacement %s',
    async (_label, strict) => {
      const probe = document.createElement('aside')
      probe.dataset.modalBackgroundProbe = 'true'
      probe.setAttribute('inert', 'kept')
      probe.setAttribute('aria-hidden', 'false')
      document.body.append(probe)
      const opener = document.createElement('button')
      opener.dataset.modalOpenerProbe = 'true'
      opener.textContent = 'Replacement opener'
      document.body.append(opener)
      opener.focus()
      expect(opener).toHaveFocus()

      const backgroundFocus: EventTarget[] = []
      const observeFocus = (event: FocusEvent) => {
        if (event.target === opener || event.target === probe || event.target === document.body) {
          backgroundFocus.push(event.target)
        }
      }
      document.addEventListener('focusin', observeFocus)
      const addListener = vi.spyOn(document, 'addEventListener')
      const removeListener = vi.spyOn(document, 'removeEventListener')
      const listenerState = () => {
        const events = [
          ...addListener.mock.calls.map((args, index) => ({ type: args[0], delta: 1, order: addListener.mock.invocationCallOrder[index] })),
          ...removeListener.mock.calls.map((args, index) => ({ type: args[0], delta: -1, order: removeListener.mock.invocationCallOrder[index] })),
        ].filter((event) => event.type === 'focusin' || event.type === 'keydown')
          .sort((left, right) => left.order - right.order)
        return (['focusin', 'keydown'] as const).map((type) => {
          let active = 0, peak = 0
          for (const event of events) {
            if (event.type !== type) continue
            active += event.delta
            peak = Math.max(peak, active)
          }
          return { type, active, peak }
        })
      }

      const view = strict
        ? <StrictMode><ReplacingDialogHarness /></StrictMode>
        : <ReplacingDialogHarness />
      const user = userEvent.setup()
      render(view)
      await user.click(screen.getByRole('button', { name: 'Replace with B' }))

      expect(screen.queryByRole('dialog', { name: 'Replacement A' })).not.toBeInTheDocument()
      const dialogB = screen.getByRole('dialog', { name: 'Replacement B' })
      expect(screen.getByRole('button', { name: 'B first' })).toHaveFocus()
      expect(backgroundFocus).toEqual([])
      expect(probe).toHaveAttribute('inert', '')
      expect(probe).toHaveAttribute('aria-hidden', 'true')
      expect(listenerState()).toEqual([
        { type: 'focusin', active: 1, peak: 1 },
        { type: 'keydown', active: 1, peak: 1 },
      ])

      fireEvent.keyDown(dialogB, { key: 'Escape' })
      const finalListeners = listenerState()
      addListener.mockRestore()
      removeListener.mockRestore()
      document.removeEventListener('focusin', observeFocus)

      expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
      expect(opener).toHaveFocus()
      expect(probe).toHaveAttribute('inert', 'kept')
      expect(probe).toHaveAttribute('aria-hidden', 'false')
      expect(finalListeners).toEqual([
        { type: 'focusin', active: 0, peak: 1 },
        { type: 'keydown', active: 0, peak: 1 },
      ])
    },
  )
})
