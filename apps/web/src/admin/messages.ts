export type AdminLocale = 'ru' | 'en'

export function getAdminMessages(locale: AdminLocale) {
  const pick = (ru: string, en: string) => locale === 'ru' ? ru : en
  return {
    locale,
    brand: 'Knowledge',
    section: pick('Администрирование', 'Administration'),
    console: pick('Консоль администратора', 'Administration console'),
    loading: pick('Загрузка…', 'Loading…'),
    navigationLabel: pick('Навигация администрирования', 'Administration navigation'),
    navigation: {
      dashboard: pick('Обзор', 'Overview'), users: pick('Пользователи', 'Users'), groups: pick('Группы', 'Groups'),
      spaces: pick('Пространства', 'Spaces'), audit: pick('Аудит', 'Audit'), settings: pick('Настройки', 'Settings'),
    },
    pages: {
      dashboard: pick('Панель управления', 'Dashboard'), users: pick('Пользователи', 'Users'), groups: pick('Группы', 'Groups'),
      spaces: pick('Пространства', 'Spaces'), audit: pick('Аудит', 'Audit'), settings: pick('Настройки', 'Settings'),
    },
    common: {
      create: pick('Создать', 'Create'), save: pick('Сохранить', 'Save'), remove: pick('Удалить', 'Remove'), restore: pick('Восстановить', 'Restore'),
      archive: pick('Архивировать', 'Archive'), cancel: pick('Отмена', 'Cancel'), close: pick('Закрыть', 'Close'), description: pick('Описание', 'Description'),
      running: pick('Выполняется…', 'Working…'), confirmationNotice: pick('Действие применяется сразу и может завершить активные сеансы или доступ.', 'This action applies immediately and may end active sessions or access.'),
      emptyArchive: pick('Архив пуст.', 'The archive is empty.'), noDescription: pick('Без описания', 'No description'), hiddenEmail: pick('Адрес скрыт', 'Address hidden'),
    },
    errors: {
      forbidden: pick('Недостаточно прав для этого действия.', 'You do not have permission for this action.'),
      notFound: pick('Объект не найден или больше недоступен.', 'The item was not found or is no longer available.'),
      conflict: pick('Изменение конфликтует с текущим состоянием. Обновите данные и повторите попытку.', 'The change conflicts with the current state. Refresh and try again.'),
      rateLimited: pick('Слишком много запросов. Повторите попытку позже.', 'Too many requests. Try again later.'),
      generic: pick('Не удалось выполнить запрос. Попробуйте ещё раз.', 'The request failed. Try again.'),
    },
    labels: {
      role: { member: pick('Участник', 'Member'), admin: pick('Администратор', 'Administrator') },
      userStatus: { active: pick('Активен', 'Active'), suspended: pick('Приостановлен', 'Suspended') },
      grantRole: { viewer: pick('Читатель', 'Viewer'), editor: pick('Редактор', 'Editor'), owner: pick('Владелец', 'Owner') },
      subjectType: { user: pick('Пользователь', 'User'), group: pick('Группа', 'Group') },
      invitationStatus: (status: string) => ({ pending: pick('Ожидает', 'Pending'), accepted: pick('Принято', 'Accepted'), revoked: pick('Отозвано', 'Revoked'), expired: pick('Истекло', 'Expired') })[status] ?? status,
      exportStatus: (status: string) => ({ queued: pick('В очереди', 'Queued'), running: pick('Выполняется', 'Running'), ready: pick('Готов', 'Ready'), failed: pick('Ошибка', 'Failed'), expired: pick('Истёк', 'Expired') })[status] ?? status,
      health: (healthy: boolean) => healthy ? pick('доступно', 'available') : pick('недоступно', 'unavailable'),
    },
    users: {
      deleteMember: pick('Удалить участника', 'Delete member'), deleteFor: (name: string) => `${pick('Удалить участника', 'Delete member')} ${name}`,
      deleteTitle: pick('Удалить участника без возможности восстановления?', 'Permanently deactivate this member?'), confirmDelete: pick('Подтвердить удаление участника', 'Confirm member deletion'),
      invite: pick('Пригласить пользователя', 'Invite user'), members: pick('Участники', 'Members'), invitations: pick('Приглашения', 'Invitations'),
      user: pick('Пользователь', 'User'), status: pick('Статус', 'Status'), role: pick('Роль', 'Role'), actions: pick('Действия', 'Actions'),
      statusFor: (name: string) => `${pick('Статус', 'Status')} ${name}`, roleFor: (name: string) => `${pick('Роль', 'Role')} ${name}`,
      revokeSessions: pick('Отозвать сеансы', 'Revoke sessions'), revokeSessionsFor: (name: string) => `${pick('Отозвать сеансы участника', 'Revoke sessions for')} ${name}`,
      revokeSessionsTitle: pick('Отозвать все сеансы?', 'Revoke all sessions?'), confirmRevokeSessions: pick('Подтвердить отзыв сеансов', 'Confirm session revocation'),
      newInvitation: pick('Новое приглашение', 'New invitation'), invitationEmail: pick('Электронная почта приглашённого', 'Invitee email'), workspaceRole: pick('Роль в рабочем пространстве', 'Workspace role'),
      expiryHours: pick('Срок действия, часов', 'Expiry, hours'), createInvitation: pick('Создать приглашение', 'Create invitation'), creating: pick('Создание…', 'Creating…'),
      invalidEmail: pick('Введите корректный адрес электронной почты.', 'Enter a valid email address.'), invalidExpiry: pick('Срок действия должен быть от 1 до 720 часов.', 'Expiry must be between 1 and 720 hours.'),
      tokenNotice: pick('Сохраните токен сейчас. После закрытия он больше не показывается.', 'Save the token now. It will not be shown after closing.'), copyToken: pick('Скопировать токен', 'Copy token'),
      tokenCopied: pick('Токен скопирован', 'Token copied'), tokenCopyFailed: pick('Не удалось скопировать токен', 'Could not copy token'),
      revokeInvitation: pick('Отозвать', 'Revoke'), revokeInvitationFor: (email: string) => `${pick('Отозвать приглашение', 'Revoke invitation for')} ${email}`,
      revokeInvitationTitle: pick('Отозвать приглашение?', 'Revoke invitation?'), confirmRevokeInvitation: pick('Подтвердить отзыв приглашения', 'Confirm invitation revocation'),
      noInvitations: pick('Нет активных приглашений.', 'There are no active invitations.'),
    },
    groups: {
      groupFor: (name: string) => `${pick('Группа', 'Group')} ${name}`, edit: pick('Изменить группу', 'Edit group'), name: pick('Название', 'Name'),
      save: pick('Сохранить группу', 'Save group'), add: pick('Добавить', 'Add'), members: pick('Участники', 'Members'), activeUserSearch: pick('Поиск активного пользователя', 'Search active users'),
      removeMember: (name: string) => `${pick('Удалить', 'Remove')} ${name}`, addMember: (name: string) => `${pick('Добавить', 'Add')} ${name}`,
      create: pick('Создать группу', 'Create group'), nameLabel: pick('Название группы', 'Group name'), archive: pick('Архив групп', 'Group archive'), loadMore: pick('Показать ещё архивные группы', 'Load more archived groups'),
    },
    spaces: {
      edit: pick('Изменить пространство', 'Edit space'), selectedName: pick('Название выбранного пространства', 'Selected space name'), selectedDescription: pick('Описание выбранного пространства', 'Selected space description'), save: pick('Сохранить пространство', 'Save space'),
      accessFor: (name: string) => `${pick('Права доступа', 'Access grants')} ${name}`, accessTitle: (name: string) => `${pick('Права доступа:', 'Access grants:')} ${name}`,
      subjectType: pick('Тип субъекта', 'Subject type'), subject: pick('Пользователь или группа', 'User or group'), select: pick('Выберите', 'Select'), accessRole: pick('Роль доступа', 'Access role'),
      addGrant: pick('Добавить право доступа', 'Add access grant'), loadMoreSubjects: pick('Показать ещё субъекты', 'Load more subjects'), invalidSubject: pick('Выберите пользователя или группу из доступного списка.', 'Select a user or group from the available list.'),
      unavailableSubject: pick('Недоступный субъект', 'Unavailable subject'), roleFor: (name: string) => `${pick('Роль для', 'Role for')} ${name}`, retryRole: pick('Повторить изменение роли', 'Retry role change'),
      create: pick('Создать пространство', 'Create space'), name: pick('Название пространства', 'Space name'), active: pick('Активные пространства', 'Active spaces'), selector: pick('Пространство', 'Space'),
      archive: pick('Архив пространств', 'Space archive'), loadMoreArchived: pick('Показать ещё архивные пространства', 'Load more archived spaces'),
    },
    dashboard: {
      maintenanceFailures: pick('Ошибок обслуживания', 'Maintenance failures'),
      maintenanceHelp: pick('Проверьте состояние хранилища и журнал операций по идентификатору; задачи повторяются автоматически.', 'Check storage health and correlate the operations log with this ID; tasks retry automatically.'),
      system: pick('Состояние системы', 'System status'), database: pick('База данных', 'Database'), masterKey: pick('Главный ключ', 'Master key'), migrations: pick('Миграции', 'Migrations'), objectStorage: pick('Хранилище объектов', 'Object storage'),
      operations: pick('Операции', 'Operations'), operationsUnavailable: pick('Данные об операциях недоступны.', 'Operations data is unavailable.'), queuedJobs: pick('Задач в очереди', 'Queued jobs'), activeExports: pick('Активных экспортов', 'Active exports'),
      readiness: pick('Готовность', 'Readiness'), readinessUnavailable: pick('Готовность сервиса недоступна.', 'Service readiness is unavailable.'), ready: pick('Сервис готов.', 'Service is ready.'), notReady: pick('Сервис не готов.', 'Service is not ready.'),
      recentExports: pick('Последние экспорты', 'Recent exports'), noExports: pick('Экспортов пока нет.', 'There are no exports yet.'),
    },
    audit: {
      filters: pick('Фильтры', 'Filters'), actor: pick('Пользователь', 'Actor'), eventType: pick('Тип события', 'Event type'), target: pick('Целевой объект', 'Target'), targetType: pick('Тип объекта', 'Target type'),
      from: pick('С даты', 'From'), to: pick('По дату', 'To'), apply: pick('Применить фильтры', 'Apply filters'), events: pick('События', 'Events'), next: pick('Следующая страница', 'Next page'), idPlaceholder: 'UUID', fromPlaceholder: '2026-09-01T00:00:00Z', toPlaceholder: '2026-09-30T23:59:59Z',
    },
    settings: {
      rotation: pick('Ротация ключа пространства', 'Rotate workspace key'), confirmRotation: pick('Подтвердить ротацию', 'Confirm key rotation'),
      rotationNotice: pick('Создаётся новая версия ключа шифрования. Фоновая миграция возобновляется после сбоя; поиск недоступен до пересборки индекса. Сначала проверьте резервную копию и сохранность главного ключа.', 'Creates a new data-encryption key. Background migration resumes after interruption; search is unavailable until the index is rebuilt. Verify your backup and master-key recovery first.'),
      rotationState: (state: string) => ({ migrating: pick('Миграция шифротекста', 'Migrating ciphertext'), rebuilding: pick('Пересборка индекса', 'Rebuilding search index'), complete: pick('Ротация завершена', 'Rotation complete') })[state] ?? state,
      invalidRecovery: pick('Срок восстановления вложений должен быть от 1 до 3650 дней.', 'Attachment recovery must be between 1 and 3650 days.'), invalidRetention: pick('Срок хранения экспорта должен быть от 1 до 720 часов.', 'Export retention must be between 1 and 720 hours.'),
      storage: pick('Хранение и восстановление', 'Storage and recovery'), recoveryDays: pick('Срок восстановления вложений, дней', 'Attachment recovery, days'), retentionHours: pick('Срок хранения экспорта, часов', 'Export retention, hours'),
      saved: pick('Настройки сохранены.', 'Settings saved.'), saving: pick('Сохранение…', 'Saving…'), save: pick('Сохранить настройки', 'Save settings'), security: pick('Безопасность', 'Security'),
      sessionHint: pick('Отзыв сеансов отдельных пользователей доступен в разделе «Пользователи».', 'You can revoke individual user sessions in Users.'),
    },
    format: {
      dateTime: (value: string) => new Intl.DateTimeFormat(locale === 'ru' ? 'ru-RU' : 'en-US', { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(value)),
    },
  }
}
