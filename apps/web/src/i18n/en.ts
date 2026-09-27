import { getWorkspaceMessages } from '../pages/messages'

const base = getWorkspaceMessages('en')

export const en = {
  ...base,
  app: {
    loading: 'Loading…',
    loadingLabel: 'Loading',
    sessionUnavailable: 'Could not check your session. Refresh the page and try again.',
    workspaceLabel: 'Knowledge workspace',
    navigation: 'Main navigation',
    breadcrumbs: 'Breadcrumbs',
    profile: 'Profile',
    logout: 'Sign out',
  },
  login: {
    title: 'Sign in',
    intro: 'Sign in to the protected workspace.',
    email: 'Email address',
    password: 'Password',
    invalidCredentials: 'Invalid email or password.',
    unavailable: 'The service is temporarily unavailable. Try again.',
    submit: 'Sign in',
    pending: 'Signing in…',
  },
  language: {
    heading: 'Language',
    russian: 'Русский',
    english: 'English',
  },
  failure: {
    offline: 'No connection. Your local draft stays on this device.',
    forbidden: 'You do not have permission for this action.',
    notFound: 'The item was not found or is no longer available.',
    conflict: 'The change conflicts with the current state. Refresh the data and try again.',
    rateLimited: 'Too many requests. Try again later.',
    serverUnavailable: 'The service is temporarily unavailable. Try again.',
    generic: 'The request failed. Try again.',
  },
  notFound: {
    title: 'Page not found',
    description: 'The page may be unavailable or you may not have access.',
    action: 'Return to workspace',
  },
}

export type Messages = typeof en
