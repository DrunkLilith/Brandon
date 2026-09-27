import { getWorkspaceMessages } from '../pages/messages'
import type { Messages } from './en'

const base = getWorkspaceMessages('ru')

export const ru = {
  ...base,
  app: {
    loading: 'Загрузка…',
    loadingLabel: 'Загрузка',
    sessionUnavailable: 'Не удалось проверить сеанс. Обновите страницу и попробуйте снова.',
    workspaceLabel: 'Рабочая область знаний',
    navigation: 'Основная навигация',
    breadcrumbs: 'Хлебные крошки',
    profile: 'Профиль',
    logout: 'Выйти',
  },
  login: {
    title: 'Вход',
    intro: 'Войдите в защищённое рабочее пространство.',
    email: 'Электронная почта',
    password: 'Пароль',
    invalidCredentials: 'Неверная почта или пароль',
    unavailable: 'Сервис временно недоступен. Попробуйте ещё раз.',
    submit: 'Войти',
    pending: 'Входим…',
  },
  language: {
    heading: 'Язык',
    russian: 'Русский',
    english: 'English',
  },
  failure: {
    offline: 'Нет подключения. Локальный черновик останется на этом устройстве.',
    forbidden: 'Недостаточно прав для этого действия.',
    notFound: 'Объект не найден или больше недоступен.',
    conflict: 'Изменение конфликтует с текущим состоянием. Обновите данные и повторите попытку.',
    rateLimited: 'Слишком много запросов. Повторите попытку позже.',
    serverUnavailable: 'Сервис временно недоступен. Попробуйте ещё раз.',
    generic: 'Не удалось выполнить запрос. Попробуйте ещё раз.',
  },
  notFound: {
    title: 'Страница не найдена',
    description: 'Страница недоступна или у вас нет к ней доступа.',
    action: 'Вернуться в рабочую область',
  },
} satisfies Messages
