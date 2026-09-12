# Knowledge Platform v1 — проектная спецификация

Дата: 2026-09-12  
Статус: утверждённый дизайн, ожидает ревью перед планированием реализации

## 1. Резюме

Первая версия — самостоятельно разворачиваемая командная база знаний: Markdown-страницы, пространства, группы, детальные права, история версий, вложения, поиск и административная панель.

Система строится как модульный монолит:

- сервер и доменное ядро — Rust, Axum, Tokio, SQLx;
- веб-интерфейс и админка — React и TypeScript;
- основная БД — PostgreSQL;
- вложения — S3-совместимое хранилище MinIO;
- развёртывание — Docker Compose;
- Markdown — канонический формат содержимого;
- Git — экспорт и резервный переносимый снимок, но не рабочая БД.

Первый релиз рассчитан на одну self-hosted инсталляцию, но данные с самого начала принадлежат workspace. Это позволяет позже добавить несколько независимых организаций без переписывания доменной модели.

LLM, embeddings, CRDT, Telegram, desktop/mobile-клиенты, публичный маркетплейс и настоящее zero-knowledge E2EE в v1 не входят.

## 2. Цели

Система должна позволять владельцу чистого сервера:

1. Развернуть все компоненты одной командой.
2. Безопасно создать первого системного администратора.
3. Приглашать, блокировать и удалять участников.
4. Объединять пользователей в группы.
5. Создавать пространства и выдавать права пользователям и группам.
6. Вести иерархическую базу Markdown-страниц.
7. Загружать и использовать вложения.
8. Искать только среди доступного пользователю содержимого.
9. Просматривать и восстанавливать историю страниц.
10. Проверять административные и контентные действия в аудите.
11. Экспортировать знания в переносимое дерево Markdown и файлов.

Главный продуктовый принцип: права должны одинаково действовать в UI, API, поиске, истории, экспорте и вложениях.

## 3. Не-цели первой версии

В v1 не реализуются:

- локальные или облачные LLM;
- semantic search и vector database;
- совместное посимвольное редактирование и CRDT;
- offline-first синхронизация;
- нативные desktop/mobile приложения;
- Telegram-бот;
- SSO/SAML/OIDC;
- внешние плагины и публичный API для автоматизаций;
- полное P2P;
- E2EE, при котором сервер не способен расшифровать содержимое;
- биллинг, marketplace и продажа пакетов знаний;
- несколько workspace в пользовательском интерфейсе.

Границы модулей и идентификаторы сущностей не должны мешать добавить эти функции позже.

## 4. Модель доверия и шифрование

### 4.1. Модель угроз

Владелец инсталляции доверяет процессу backend и администратору хоста. Защищаемся от:

- чтения дампа PostgreSQL без ключей;
- чтения содержимого MinIO без ключей;
- утечки резервной копии данных отдельно от secret store;
- перехвата трафика;
- доступа обычного пользователя за пределами его прав;
- случайного или злонамеренного изменения административных данных без следа в аудите.

Не защищаемся от root-доступа к работающему серверу, дампа памяти backend или злонамеренного владельца master key. Интерфейс должен честно называть режим `server-side encryption`, а не `end-to-end encryption`.

### 4.2. Ключи

- При установке оператор предоставляет 256-битный master key через Docker secret или внешний secret store.
- Master key не хранится в PostgreSQL, MinIO, логах или экспортируемом архиве.
- На каждый workspace создаётся отдельный data-encryption key, зашифрованный master key.
- Для разных назначений через HKDF выводятся независимые ключи содержимого, поиска и вложений.
- Ротация master key повторно оборачивает workspace keys без перешифрования всего содержимого.
- Ротация workspace key выполняется фоновой задачей с поддержкой текущей и предыдущей версии ключа.

### 4.3. Шифрование данных

- Заголовок, Markdown-body и чувствительные метаданные страницы шифруются AEAD на backend до записи в PostgreSQL.
- Каждая запись содержит версию ключа, nonce и authentication tag.
- В качестве алгоритма используется AES-256-GCM или XChaCha20-Poly1305 из хорошо поддерживаемой Rust-библиотеки; окончательный выбор фиксируется implementation plan после проверки библиотек.
- Вложения шифруются потоково до отправки в MinIO. Имена объектов — непрозрачные UUID.
- TLS обязателен на внешней границе production-инсталляции.
- PostgreSQL и MinIO дополнительно размещаются на зашифрованных томах как второй слой защиты.

### 4.4. Поиск без открытого индекса

Обычный PostgreSQL FTS противоречит прикладному шифрованию, потому что оставляет слова документа в индексе. Поэтому v1 использует keyed blind index:

1. Backend нормализует текст, выделяет слова и ограниченный набор префиксов.
2. Каждый поисковый токен преобразуется в HMAC с отдельным workspace search key.
3. PostgreSQL хранит только HMAC-токены, идентификатор страницы и частоту.
4. Запрос проходит ту же нормализацию и HMAC-преобразование.
5. SQL сначала применяет ACL и выбирает ограниченное число кандидатов.
6. Backend расшифровывает только разрешённых кандидатов, вычисляет точное совпадение и формирует snippets в памяти.

Ограничения v1: нет морфологического FTS, сложного fuzzy search и глобальной релевантности уровня Elasticsearch. Поиск поддерживает слова, фразы после проверки кандидатов, фильтры по пространству, автору и дате. Blind index считается производным секретным индексом и перестраивается при ротации search key.

## 5. Архитектура

### 5.1. Общая схема

```text
Browser
  │ HTTPS
  ▼
Reverse proxy
  ├── static React application
  └── /api/* → Rust application
                    ├── PostgreSQL
                    ├── MinIO
                    └── background job runner
```

Backend поставляется одним бинарным файлом и содержит HTTP API, доменные сервисы и лёгкий job runner. Модули общаются через типизированные интерфейсы внутри процесса. Отдельные микросервисы не создаются.

### 5.2. Серверные модули

- `auth`: вход, выход, сессии, пароли, CSRF, rate limiting;
- `users`: пользователи, статус аккаунта, профиль;
- `invitations`: одноразовые приглашения и их срок действия;
- `workspaces`: корневая граница данных и настроек;
- `groups`: группы и состав участников;
- `spaces`: пространства и их дерево страниц;
- `permissions`: вычисление эффективных прав и restricted overrides;
- `pages`: Markdown, иерархия, версия, архив и восстановление;
- `revisions`: неизменяемая история содержимого;
- `search`: blind index, ACL-filtered retrieval и snippets;
- `attachments`: потоковое шифрование, загрузка и скачивание;
- `audit`: append-only журнал значимых действий;
- `exports`: формирование переносимого архива;
- `jobs`: очистка, ротация ключей, экспорт и пересборка индекса;
- `health`: liveness, readiness и сведения о миграциях.

Модули не обращаются напрямую к таблицам другого модуля. Совместные операции координируются application service и одной транзакцией SQLx.

### 5.3. Клиент

React-приложение состоит из трёх зон:

- `/login` и принятие приглашения;
- `/app` — основная база знаний;
- `/admin` — администрирование инсталляции.

Клиент использует сгенерированные из OpenAPI TypeScript-типы. Сервер остаётся единственным источником решений о правах; скрытие кнопки в UI не считается проверкой доступа.

## 6. Доменная модель

Все идентификаторы — UUIDv7. Все временные метки хранятся в UTC. Большинство таблиц содержит `workspace_id`, даже если первая версия показывает один workspace.

### 6.1. Пользователи и сессии

`users`

- `id`, `workspace_id`;
- зашифрованные `display_name` и при необходимости профильные данные;
- нормализованный email и отдельный HMAC email для уникальности/поиска;
- password hash;
- статус: `invited | active | suspended | deleted`;
- `created_at`, `updated_at`, `last_login_at`.

`sessions`

- `id`, `user_id`;
- hash session token, а не сам token;
- `expires_at`, `last_seen_at`, IP/user-agent summary;
- время отзыва.

`invitations`

- `id`, `workspace_id`, email HMAC;
- роль workspace;
- hash одноразового token;
- пригласивший пользователь, срок действия, время принятия/отзыва.

### 6.2. Организация

`workspaces`

- `id`, зашифрованное имя;
- wrapped data keys и их версии;
- настройки безопасности и лимиты.

`workspace_members`

- `workspace_id`, `user_id`;
- роль: `admin | member`;
- статус членства.

`groups`

- `id`, `workspace_id`;
- зашифрованное имя и описание;
- `created_by`, timestamps, soft-delete marker.

`group_members`

- `group_id`, `user_id`;
- `added_by`, `created_at`;
- уникальная пара group/user.

### 6.3. Пространства и права

`spaces`

- `id`, `workspace_id`;
- зашифрованные name/description;
- `created_by`, timestamps, soft-delete marker.

`space_grants`

- `space_id`;
- subject: ровно один `user_id` или `group_id`;
- роль: `owner | editor | viewer`;
- `granted_by`, timestamps.

`page_permissions`

- `page_id`;
- subject: user или group;
- роль: `editor | viewer`;
- `granted_by`, timestamps.

Restricted-страница не расширяет унаследованные права, а сужает их. Пользователь сначала обязан иметь доступ к пространству, затем — попасть в allow-list restricted-страницы. Дочерние страницы наследуют restricted-границу ближайшего ограниченного предка.

### 6.4. Страницы и история

`pages`

- `id`, `workspace_id`, `space_id`, `parent_id`;
- зашифрованные `title`, `body`, свойства и slug source;
- непрозрачный стабильный URL key;
- сортировочный ключ среди siblings;
- `version` для optimistic concurrency control;
- `created_by`, `updated_by`, timestamps;
- `archived_at`, `archived_by`;
- признак restricted access.

`page_revisions`

- `id`, `page_id`, последовательный номер версии;
- полная зашифрованная snapshot-копия title/body/properties;
- автор, причина изменения, timestamp;
- hash plaintext для контроля целостности внутри доверенного backend.

Ревизии неизменяемы. Восстановление старой версии создаёт новую текущую версию, а не переписывает историю.

Циклы в дереве запрещены. Перемещение страницы транзакционно проверяет новое родительство и права в обоих пространствах. Архивирование родителя скрывает всё поддерево; восстановление требует существующего и доступного родителя либо переносит страницу в корень пространства.

### 6.5. Вложения

`attachments`

- `id`, `workspace_id`, `page_id`;
- зашифрованное исходное имя и MIME metadata;
- непрозрачный object key;
- размер plaintext/ciphertext, checksum;
- версия ключа, nonce/header потокового шифрования;
- автор, timestamps, soft-delete marker.

Скачивание всегда идёт через авторизованный backend либо через очень короткоживущую подписанную ссылку к уже зашифрованному объекту. Расшифрованные публичные MinIO URL не выдаются.

### 6.6. Аудит и фоновые задачи

`audit_events`

- `id`, `workspace_id`, actor user/session;
- тип события, target type/id;
- timestamp, request/correlation id;
- безопасные структурированные metadata без plaintext содержимого;
- hash chain полей для обнаружения удаления или перестановки событий.

`jobs`

- тип, payload, status, attempts;
- время следующего запуска, lease owner/expiry;
- error summary без секретов.

## 7. Модель доступа

### 7.1. Роли

- `system administrator`: bootstrap, общесистемные настройки, управление workspace;
- `workspace admin`: пользователи, приглашения, группы, пространства и аудит;
- `space owner`: настройки пространства, гранты, контент;
- `editor`: чтение, создание и изменение страниц/вложений;
- `viewer`: чтение доступного содержимого и истории.

В первой single-workspace инсталляции system administrator одновременно является workspace admin. Модель разделена для будущего multi-tenant режима.

### 7.2. Правила вычисления

1. Suspended/deleted user всегда получает отказ.
2. System/workspace admin имеет административный доступ, но каждое использование фиксируется в аудите.
3. Space role — максимум из прямого и групповых grants.
4. Restricted boundary затем сужает множество субъектов.
5. Page permission внутри boundary может дать `viewer` или `editor`, но не может обойти отсутствие доступа к пространству.
6. Доступ к revision, attachment, search hit и export вычисляется через родительскую страницу.
7. Решение принимается сервером на каждом запросе; кэш ACL обязан инвалидироваться после изменения групп или grants.

Матрица прав реализуется как чистый доменный компонент и покрывается исчерпывающими parameterized tests.

## 8. Основные рабочие потоки

### 8.1. Bootstrap

1. Оператор запускает миграции.
2. Одноразовая CLI-команда создаёт workspace и первого администратора.
3. Начальный пароль передаётся интерактивно или через одноразовый secret, не через аргумент командной строки.
4. Повторный bootstrap завершается безопасной ошибкой и ничего не меняет.

### 8.2. Приглашение и группы

1. Admin создаёт приглашение с workspace role и сроком действия.
2. Пользователь открывает одноразовую ссылку, задаёт имя и пароль.
3. Token атомарно помечается использованным.
4. Admin добавляет пользователя в одну или несколько групп.
5. Каждое действие создаёт audit event.

### 8.3. Работа со страницей

1. Editor создаёт страницу в доступном пространстве или под доступным родителем.
2. Сервер шифрует содержимое, сохраняет page и первую revision в одной транзакции.
3. Клиент загружает `version=N`.
4. При сохранении отправляет ожидаемую версию.
5. Совпадение создаёт revision и версию `N+1`; несовпадение возвращает `409 Conflict` с текущей metadata.
6. Клиент сохраняет несданный текст в локальном черновике и предлагает сравнение/копирование, не затирая данные.

### 8.4. История и восстановление

Viewer видит список версий и diff только для доступной страницы. Editor выбирает revision и восстанавливает её как новую версию. Оригинальный автор и операция восстановления остаются в истории.

### 8.5. Архив и восстановление

Удаление страницы, пространства, группы или вложения сначала мягкое. Admin/owner может восстановить объект в течение настраиваемого периода. Окончательная очистка выполняется фоновой задачей после retention window и создаёт audit event.

### 8.6. Экспорт

Owner/admin запускает экспорт пространства. Job делает ACL-проверенный snapshot, расшифровывает его потоково и создаёт архив:

```text
space-name/
  pages/.../*.md
  attachments/...
  manifest.json
```

Markdown содержит стабильные ID и свойства в YAML frontmatter. Ссылка на архив короткоживущая; готовый экспорт автоматически удаляется. Экспорт — особо чувствительное аудируемое действие.

## 9. API

API version prefix: `/api/v1`. Контракт описывается OpenAPI.

Основные группы endpoints:

- `/auth/login`, `/auth/logout`, `/auth/session`, `/auth/csrf`;
- `/invitations/{token}`;
- `/users`, `/groups`, `/groups/{id}/members`;
- `/spaces`, `/spaces/{id}/grants`;
- `/pages`, `/pages/{id}`, `/pages/{id}/move`;
- `/pages/{id}/revisions`, `/pages/{id}/restore`;
- `/pages/{id}/permissions`;
- `/pages/{id}/attachments`;
- `/search`;
- `/audit-events`;
- `/exports`;
- `/health/live`, `/health/ready`.

HTTP semantics:

- JSON для metadata и команд;
- Markdown body передаётся строкой в JSON в v1;
- multipart или resumable upload для вложений;
- cursor pagination для списков;
- `ETag`/version для конкурентных изменений;
- `Idempotency-Key` для приглашений, загрузок и запуска jobs;
- ошибки — `application/problem+json` по RFC 9457;
- request ID возвращается клиенту и связывается с логами/аудитом.

Сервер не раскрывает существование недоступного объекта: где это важно для безопасности, возвращается `404`, а не `403`.

## 10. Интерфейс

### 10.1. Основное приложение

Desktop-first app shell:

- слева: переключатель пространства, дерево страниц, быстрые действия;
- сверху: глобальный поиск, breadcrumbs, профиль;
- по центру: Markdown editor с preview и raw mode;
- справа: свойства, backlinks, права и история в контекстной панели.

Минимальные экраны:

- login и принятие приглашения;
- пустое состояние workspace;
- список/дерево страниц;
- просмотр и редактирование Markdown;
- создание, переименование, перемещение, архивирование;
- история, diff и восстановление;
- загрузка и просмотр списка вложений;
- поиск и фильтры;
- управление доступом к странице для owner/admin;
- понятные состояния 403/404/409/offline/server unavailable.

Редактор хранит локальный draft с page ID и server version. Автосохранение означает сохранение черновика в браузере; публикация серверной revision в v1 происходит явным действием или после подтверждённого debounce с заметным статусом. Ни один конфликт не приводит к молчаливой потере текста.

Mobile web поддерживает чтение, поиск и базовое редактирование. Идеальная мобильная UX и offline sync отложены.

### 10.2. Административная панель

`/admin` содержит:

- dashboard состояния системы;
- users и invitations;
- groups и memberships;
- spaces и grants;
- audit log с фильтрами;
- security settings, session revocation и retention;
- jobs/exports status;
- сведения о версии, миграциях, PostgreSQL и MinIO readiness.

Админка использует тот же API и permission engine, что и основное приложение.

### 10.3. Доступность и локализация

- клавиатурная навигация по главным операциям;
- видимый focus, семантическая разметка, достаточный contrast;
- UI-тексты вынесены в локализационные ресурсы;
- первая поставка содержит русский и английский интерфейсы;
- даты форматируются по locale, в API остаются UTC/ISO 8601.

## 11. Безопасность приложения

- Пароли хэшируются Argon2id с параметрами, измеренными на target hardware.
- Session token хранится только в `Secure`, `HttpOnly`, `SameSite=Lax/Strict` cookie.
- State-changing запросы требуют CSRF token и same-origin проверку.
- Login, invitations, export и upload защищаются rate limits.
- После смены пароля и suspension все сессии отзываются.
- Content Security Policy запрещает произвольные scripts.
- Markdown sanitization запрещает исполняемый HTML/JavaScript по умолчанию.
- Upload проверяет размер, заявленный/фактический MIME и безопасно обрабатывается как недоверенный файл.
- SQL только параметризованный через SQLx.
- Секреты и plaintext content не попадают в structured logs, traces и panic reports.
- Backup/restore документируется и тестируется; backup бесполезен без отдельной копии master key, о чём UI предупреждает явно.
- Audit metadata хранит IDs и типы изменений, но не содержимое страницы, пароли, tokens или ключи.

## 12. Надёжность и наблюдаемость

- Каждое изменение доменных данных и audit event выполняются в одной транзакции.
- MinIO upload использует staging object; запись attachment становится видимой только после успешного завершения и commit.
- Cleanup удаляет orphan staging objects.
- Jobs используют lease и at-least-once execution; обработчики идемпотентны.
- `/health/live` проверяет процесс, `/health/ready` — БД, миграции, наличие ключа и доступность object storage.
- Метрики: latency/error rate, DB pool, auth failures, job queue, upload/export failures.
- Логи структурированы и содержат request ID, но не секретные поля.
- Корректное завершение прекращает принимать запросы и заканчивает короткие транзакции.

Цель v1 — один экземпляр backend. Схема с leases, UUIDv7 и внешними session/job stores не должна блокировать горизонтальное масштабирование позже.

## 13. Развёртывание

Docker Compose включает:

- reverse proxy;
- Rust backend;
- собранный React frontend;
- PostgreSQL;
- MinIO.

Поставка содержит:

- `.env.example` без секретов;
- Docker secrets example;
- миграции с прямым и документированным rollback-путём там, где он безопасен;
- bootstrap CLI;
- команды backup/restore и проверку восстановления;
- healthchecks и resource limits;
- production checklist для TLS, volumes, keys, firewall и обновлений.

Миграции запускаются отдельной командой перед переключением версии. Backend не стартует в ready-state при несовместимой схеме.

## 14. Стратегия тестирования

### 14.1. Unit tests

- permission matrix и inheritance;
- restricted boundaries;
- password/session/token helpers;
- encryption envelopes и key versions;
- blind-index normalization;
- tree operations и запрет циклов;
- optimistic concurrency;
- audit redaction.

### 14.2. Integration tests с PostgreSQL и MinIO

- migrations на пустой и обновляемой схеме;
- транзакционность page + revision + audit;
- ACL на каждой ресурсной коллекции;
- зашифрованные поля не содержат plaintext marker;
- поиск не выдаёт inaccessible pages;
- upload/download/range/cleanup;
- rotation и восстановление ключей;
- backup/restore smoke test.

### 14.3. API contract tests

- OpenAPI соответствует ответам;
- Problem Details имеет стабильные type/code;
- pagination, ETag/version и idempotency;
- CSRF, cookies, rate limits;
- отсутствие resource enumeration.

### 14.4. Browser tests Playwright

- bootstrap → login;
- invite → accept → group assignment;
- space grant для группы;
- editor создаёт/редактирует/восстанавливает страницу;
- viewer не может изменить содержимое;
- restricted page не видна постороннему участнику и поиску;
- конфликт двух вкладок сохраняет оба текста;
- attachment upload/download;
- admin audit filtering;
- mobile-width smoke flows.

### 14.5. Security verification

- dependency/license audit;
- secret scanning;
- fuzz/property tests для Markdown sanitization, crypto envelopes и tree operations;
- проверка OWASP ASVS-relevant controls;
- ручная попытка IDOR по page/revision/attachment/export IDs.

## 15. Критерии приёмки v1

Релиз принимается, когда:

1. Система разворачивается по документации на чистом Linux-хосте.
2. Bootstrap нельзя незаметно выполнить повторно.
3. Admin управляет invitations, users, groups, spaces и grants через UI.
4. Editor создаёт дерево Markdown-страниц, перемещает страницы и добавляет вложения.
5. Viewer читает, ищет и просматривает историю, но не изменяет данные.
6. Restricted subtree скрыт из API, поиска, revisions, attachments и export для неразрешённого пользователя.
7. Конкурентное изменение возвращает conflict UI без потери пользовательского текста.
8. В PostgreSQL и MinIO отсутствует открытый Markdown, заголовки и оригинальные имена файлов.
9. Backup восстанавливается на отдельной тестовой инсталляции при наличии master key.
10. Все обязательные unit, integration и Playwright tests проходят.
11. Production build не содержит критических dependency vulnerabilities.
12. Документация описывает установку, обновление, backup/restore, key handling и ограничения модели доверия.

## 16. Будущее расширение

Архитектура оставляет следующие точки расширения:

- `PageContentRepository` может получить CRDT update log без смены page ID и ACL;
- desktop-клиент сможет синхронизировать локальный SQLCipher store через отдельный sync API;
- `SearchProvider` сможет объединить blind search с permission-aware embeddings;
- AI assistant будет получать только ACL-filtered chunks и цитировать page/revision IDs;
- capture adapters добавят Telegram/email/mobile inbox;
- workspace boundary позволит multi-tenant SaaS;
- sealed workspace сможет использовать настоящий client-side E2EE, но потеряет серверный поиск/AI либо потребует отдельного доверенного compute node.

Эти возможности не должны преждевременно усложнять v1.

## 17. Принятые компромиссы

- Модульный монолит выбран вместо микросервисов ради транзакций, простого self-hosting и быстрой разработки.
- Централизованный server-first v1 выбран вместо CRDT/local-first, чтобы сначала доказать permission model и рабочий UX.
- Application-level encryption с доверенным backend выбран вместо zero-knowledge E2EE, потому что продукту нужны серверные права, поиск, экспорт и в будущем RAG.
- Blind index выбран вместо PostgreSQL FTS, чтобы не хранить открытые поисковые слова; качество fuzzy/morphological поиска сознательно ниже.
- Полные encrypted snapshots revisions выбраны вместо цепочки patches ради надёжного восстановления и простой проверки прав; storage overhead приемлем для v1.
- PostgreSQL и MinIO выбраны вместо Git filesystem как source of truth из-за транзакций, ACL и конкурентных изменений.
- Явная серверная revision и локальный draft выбраны вместо иллюзии real-time collaboration.

## 18. Открытые вопросы реализации, не меняющие дизайн

Перед кодированием implementation plan должен зафиксировать:

- конкретные версии Rust/Node/PostgreSQL/MinIO;
- выбранную Rust crypto library и формат envelope;
- Markdown editor после короткого технического spike;
- лимиты размеров body, upload и export;
- retention defaults;
- точную tokenization strategy для русского и английского blind search;
- порядок migrations и структуру crates/packages;
- поддерживаемые браузеры.

Это инженерные параметры внутри утверждённой архитектуры, а не продуктовые развилки.
