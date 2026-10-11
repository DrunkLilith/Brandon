import {
  createContext,
  createElement,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react'

import { en, type Messages } from './en'
import { ru } from './ru'

export type Locale = 'ru' | 'en'

const preferenceKey = 'knowledge.locale'
const resources: Record<Locale, Messages> = { ru, en }

function readPreference(): Locale {
  try {
    const stored = localStorage.getItem(preferenceKey)
    return stored === 'en' || stored === 'ru' ? stored : 'ru'
  } catch {
    return 'ru'
  }
}

type I18nValue = {
  locale: Locale
  messages: Messages
  setLocale(locale: Locale): void
}

const fallback: I18nValue = {
  locale: 'ru',
  messages: ru,
  setLocale: () => undefined,
}

const I18nContext = createContext<I18nValue>(fallback)

export function I18nProvider({ children }: { children: ReactNode }) {
  const [locale, updateLocale] = useState<Locale>(readPreference)
  const setLocale = (next: Locale) => {
    updateLocale(next)
    try {
      localStorage.setItem(preferenceKey, next)
    } catch {
      // The runtime choice still works when storage is unavailable.
    }
  }
  useEffect(() => {
    document.documentElement.lang = locale
  }, [locale])
  const value = useMemo(
    () => ({ locale, messages: resources[locale], setLocale }),
    [locale],
  )
  return createElement(I18nContext.Provider, { value }, children)
}

export function useI18n() {
  return useContext(I18nContext)
}

export function LanguageSwitch() {
  const { locale, messages, setLocale } = useI18n()
  return createElement(
    'section',
    { className: 'language-switch', 'aria-labelledby': 'language-switch-title' },
    createElement('h2', { id: 'language-switch-title', className: 'sr-only' }, messages.language.heading),
    createElement('span', { className: 'language-switch__globe', 'aria-hidden': 'true' }),
    createElement(
      'button',
      {
        type: 'button',
        'aria-label': messages.language.russian,
        'aria-pressed': locale === 'ru',
        title: messages.language.russian,
        onClick: () => setLocale('ru'),
      },
      createElement('span', { 'aria-hidden': 'true' }, 'RU'),
    ),
    createElement(
      'button',
      {
        type: 'button',
        'aria-label': messages.language.english,
        'aria-pressed': locale === 'en',
        title: messages.language.english,
        onClick: () => setLocale('en'),
      },
      createElement('span', { 'aria-hidden': 'true' }, 'EN'),
    ),
  )
}
