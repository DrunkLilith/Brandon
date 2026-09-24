import { getAdminMessages, type AdminLocale } from '../admin/messages'

export function getWorkspaceMessages(locale: AdminLocale) {
  const pick = (ru: string, en: string) => (locale === 'ru' ? ru : en)
  return {
    ...getAdminMessages(locale),
    workspace: {
      overview: pick('Обзор', 'Overview'),
      spaces: pick('Пространство', 'Space'),
      tree: pick('Дерево страниц', 'Page tree'),
      noSpaces: pick(
        'Нет доступных пространств. Обратитесь к администратору.',
        'No accessible spaces. Contact an administrator.',
      ),
      choose: pick(
        'Выберите страницу или создайте первую.',
        'Choose a page or create the first one.',
      ),
      create: pick('Создать страницу', 'Create page'),
      child: pick('Создать дочернюю', 'Create child'),
      rename: pick('Переименовать', 'Rename'),
      move: pick('Переместить', 'Move'),
      up: pick('Поднять', 'Move up'),
      archive: pick('Архивировать', 'Archive'),
      newTitle: pick('Новое название', 'New title'),
      destination: pick('Пространство назначения', 'Destination space'),
      parent: pick('Родитель страницы', 'Parent page'),
      root: pick('Корень', 'Root'),
      apply: pick('Применить', 'Apply'),
      title: pick('Название страницы', 'Page title'),
      raw: pick('Исходный Markdown', 'Raw Markdown'),
      rich: pick('Визуальный редактор', 'Rich editor'),
      preview: pick('Предпросмотр', 'Preview'),
      save: pick('Сохранить версию', 'Save revision'),
      saving: pick('Сохранение на сервере…', 'Saving to server…'),
      saved: pick('Версия сохранена на сервере', 'Revision saved to server'),
      local: pick('Черновик сохранён в браузере', 'Draft saved in browser'),
      draftFailed: pick(
        'Не удалось сохранить черновик в браузере. Скопируйте текст.',
        'Could not save browser draft. Copy your text.',
      ),
      history: pick('История', 'History'),
      permissions: pick('Права', 'Permissions'),
      properties: pick('Свойства JSON', 'JSON properties'),
      restricted: pick('Ограниченный доступ', 'Restricted access'),
      version: (n: number) => `${pick('Версия', 'Version')} ${n}`,
      compare: (n: number) =>
        `${pick('Сравнить версию', 'Compare revision')} ${n}`,
      restore: (n: number) =>
        `${pick('Восстановить версию', 'Restore revision')} ${n}`,
      confirmRestore: pick('Подтвердить восстановление', 'Confirm restore'),
      restoreTitle: pick(
        'Восстановить выбранную версию?',
        'Restore selected revision?',
      ),
      diff: pick('Сравнение с текущим текстом', 'Compare with current text'),
      diffBound: pick(
        'Показаны первые 100 000 символов каждой версии.',
        'Showing the first 100,000 characters of each revision.',
      ),
      conflict: pick('Конфликт версий', 'Version conflict'),
      base: (n: number) => `${pick('Базовая версия:', 'Base revision:')} ${n}`,
      current: (n: number) =>
        `${pick('Текущая версия:', 'Current revision:')} ${n}`,
      copy: pick('Скопировать мой текст', 'Copy my text'),
      copied: pick('Текст скопирован', 'Text copied'),
      reload: pick('Загрузить серверную версию', 'Load server revision'),
      resolve: pick('Продолжить слияние вручную', 'Continue manual merge'),
      reloadWarning: pick(
        'Заменить текст в редакторе серверной версией? Старый черновик останется в браузере.',
        'Replace editor text with server revision? The old draft stays in browser storage.',
      ),
      role: (id: string) => `${pick('Роль', 'Role')} ${id}`,
      removeGrant: (id: string) =>
        `${pick('Удалить право', 'Remove grant')} ${id}`,
      addGrant: pick('Добавить право', 'Add grant'),
      subject: pick('Субъект доступа', 'Access subject'),
      more: pick('Показать ещё', 'Show more'),
      search: pick('Поиск по знаниям', 'Search knowledge'),
      query: pick('Поисковый запрос', 'Search query'),
      author: pick('Автор', 'Author'),
      from: pick('С даты', 'From'),
      to: pick('По дату', 'To'),
      find: pick('Найти', 'Search'),
      noResults: pick('Ничего не найдено', 'No results'),
      attachments: pick('Вложения', 'Attachments'),
      file: pick('Файл', 'File'),
      upload: pick('Загрузить файл', 'Upload file'),
      cancelUpload: pick('Отменить загрузку', 'Cancel upload'),
      uploading: pick('Отправка файла…', 'Uploading file…'),
      cancelled: pick('Загрузка отменена', 'Upload cancelled'),
      uploaded: pick('Файл загружен', 'File uploaded'),
      invalidFile: pick(
        'Файл пуст или превышает 100 МиБ.',
        'File is empty or exceeds 100 MiB.',
      ),
      encrypted: pick(
        'Шифрование на сервере защищает файлы при хранении.',
        'Server-side encryption protects stored files.',
      ),
      download: (name: string) => `${pick('Скачать', 'Download')} ${name}`,
      noFiles: pick('Вложений пока нет.', 'No attachments yet.'),
    },
  }
}
export const workspaceMessages = getWorkspaceMessages('ru')
