# Knowledge Platform v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Построить полностью разворачиваемую self-hosted Markdown-базу знаний с сайтом, админкой, пользователями, группами, пространствами, детальными правами, версиями, поиском, вложениями, аудитом и server-side encryption — без LLM и CRDT.

**Architecture:** Модульный монолит на Rust с Axum API, SQLx/PostgreSQL и MinIO; чистые правила доступа вынесены в `knowledge-core`, криптография и blind index — в `knowledge-crypto`, persistence — в `knowledge-store`, orchestration/API — в `knowledge-server`. React SPA использует сгенерированный OpenAPI-клиент; все решения о доступе остаются на сервере.

**Tech Stack:** Rust 1.98 / edition 2024, Axum 0.8.9, Tokio 1.53.1, SQLx 0.9.0, AES-GCM 0.11.1, Argon2 0.6.0, PostgreSQL 18, MinIO, React 19.3.0, TypeScript 7.0.2, Vite 8.3.0, TanStack Query 5.102.8, Milkdown 7.22.1, Vitest 5.0.0, Playwright 1.63.0, pnpm 11.19.0.

**Spec:** `docs/superpowers/specs/2026-09-12-knowledge-platform-v1-design.md`

## Global Constraints

- Markdown is the canonical page representation; Git is export-only.
- First release is a single self-hosted installation with `workspace_id` on workspace-owned records.
- Backend is trusted; database/object-storage dumps must not expose page titles, Markdown, attachment names, or attachment bytes.
- Master key must never be stored in PostgreSQL, MinIO, logs, repository, or exports.
- Every page/revision/attachment/search/export endpoint must enforce the same permission engine.
- Mutations and their audit event commit in one PostgreSQL transaction.
- Concurrent page saves use integer versions and return RFC 9457 `409 Conflict` without losing the submitted draft.
- Errors use `application/problem+json`; resource enumeration returns `404` where appropriate.
- User-facing UI ships in Russian and English and remains keyboard accessible.
- LLM, vector search, CRDT, Telegram and native clients are excluded.
- No production behavior is written before a test has failed for the expected reason.

## Planned File Structure

```text
.
├── Cargo.toml                         # Rust workspace and shared dependency versions
├── Cargo.lock
├── rust-toolchain.toml                # Rust 1.98 pin
├── .cargo/config.toml                 # shared lints/aliases
├── crates/
│   ├── knowledge-core/
│   │   └── src/{lib,ids,permissions,pages,errors}.rs
│   ├── knowledge-crypto/
│   │   └── src/{lib,envelope,keys,blind_index,passwords}.rs
│   ├── knowledge-store/
│   │   ├── migrations/*.sql
│   │   └── src/{lib,users,groups,spaces,pages,links,search,attachments,audit,jobs}.rs
│   └── knowledge-server/
│       ├── src/{lib,main,config,state,problem,middleware}.rs
│       ├── src/routes/{mod,auth,users,groups,spaces,pages,search,attachments,audit,exports,health}.rs
│       └── tests/api_*.rs
├── apps/web/
│   ├── src/{main,app,router,i18n}.tsx
│   ├── src/api/{client,generated}.ts
│   ├── src/auth/*
│   ├── src/app-shell/*
│   ├── src/editor/*
│   ├── src/admin/*
│   ├── src/pages/*
│   └── src/**/*.test.tsx
├── tests/e2e/*.spec.ts
├── deploy/{compose.yaml,Caddyfile,.env.example}
├── scripts/{backup,restore,verify-backup}.{sh,ps1}
└── docs/{operations.md,security.md,user-guide.md}
```

---

### Task 1: Executable Monorepo Baseline

**Files:**
- Create: `Cargo.toml`, `rust-toolchain.toml`, `.cargo/config.toml`
- Create: `crates/knowledge-core/Cargo.toml`, `crates/knowledge-core/src/lib.rs`
- Create: `crates/knowledge-crypto/Cargo.toml`, `crates/knowledge-crypto/src/lib.rs`
- Create: `crates/knowledge-store/Cargo.toml`, `crates/knowledge-store/src/lib.rs`
- Create: `crates/knowledge-server/Cargo.toml`, `crates/knowledge-server/src/{lib,main,config,state}.rs`
- Create: `crates/knowledge-server/tests/health.rs`
- Create: `package.json`, `pnpm-workspace.yaml`, `apps/web/package.json`, `apps/web/src/{main,app}.tsx`
- Create: `apps/web/src/app.test.tsx`, `apps/web/{index.html,tsconfig.json,vite.config.ts}`
- Create: `.gitignore`, `.editorconfig`, `README.md`

**Interfaces:**
- Produces: `knowledge_server::build_router(AppState) -> axum::Router`
- Produces: `GET /api/v1/health/live -> 200 {"status":"ok"}`
- Produces: React `App` rendering product shell placeholder and health status.

- [ ] **Step 1: Write failing backend health test**

```rust
#[tokio::test]
async fn live_health_returns_ok() {
    let response = knowledge_server::build_test_router()
        .oneshot(Request::builder().uri("/api/v1/health/live").body(Body::empty()).unwrap())
        .await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(body_json(response).await, json!({"status": "ok"}));
}
```

- [ ] **Step 2: Run RED backend test**

Run: `cargo test -p knowledge-server --test health live_health_returns_ok`  
Expected: FAIL because `knowledge_server` and `build_test_router` do not exist.

- [ ] **Step 3: Add workspace manifests and minimal health router**

```rust
pub fn build_test_router() -> Router {
    Router::new().route(
        "/api/v1/health/live",
        get(|| async { Json(serde_json::json!({"status": "ok"})) }),
    )
}
```

- [ ] **Step 4: Run GREEN backend test and workspace checks**

Run: `cargo test --workspace && cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`  
Expected: PASS, zero warnings.

- [ ] **Step 5: Write failing frontend shell test**

```tsx
it('renders the knowledge workspace landmark', () => {
  render(<App />)
  expect(screen.getByRole('main', { name: /knowledge workspace/i })).toBeVisible()
})
```

- [ ] **Step 6: Run RED frontend test**

Run: `pnpm --filter @knowledge/web test --run src/app.test.tsx`  
Expected: FAIL because the app shell is absent.

- [ ] **Step 7: Implement minimal React shell and tool configuration**

```tsx
export function App() {
  return <main aria-label="Knowledge workspace">Knowledge Platform</main>
}
```

- [ ] **Step 8: Run GREEN frontend test and commit**

Run: `pnpm --filter @knowledge/web test --run && pnpm --filter @knowledge/web build`  
Expected: PASS.  
Commit: `chore: bootstrap executable Rust and React workspace`

---

### Task 2: Typed Domain IDs, Errors and Permission Engine

**Files:**
- Create: `crates/knowledge-core/src/{ids,errors,permissions}.rs`
- Modify: `crates/knowledge-core/src/lib.rs`

**Interfaces:**
- Produces: newtypes `UserId`, `WorkspaceId`, `GroupId`, `SpaceId`, `PageId`, `RevisionId`, `AttachmentId`.
- Produces: `WorkspaceRole::{Admin,Member}`, `SpaceRole::{Viewer,Editor,Owner}`.
- Produces: `PermissionContext::can(Capability, ResourceContext) -> bool`.
- Produces: `DomainError::{NotFound,Forbidden,Conflict,Validation}`.

- [ ] **Step 1: Write failing table-driven permission tests**

```rust
#[test_case(SpaceRole::Viewer, Capability::ReadPage, true)]
#[test_case(SpaceRole::Viewer, Capability::EditPage, false)]
#[test_case(SpaceRole::Editor, Capability::EditPage, true)]
#[test_case(SpaceRole::Owner, Capability::ManageSpace, true)]
fn space_role_matrix(role: SpaceRole, capability: Capability, expected: bool) {
    let ctx = PermissionContext::active_member(role);
    assert_eq!(ctx.can(capability, ResourceContext::Unrestricted), expected);
}

#[test]
fn restricted_boundary_can_only_narrow_space_access() {
    let ctx = PermissionContext::active_member(SpaceRole::Owner);
    assert!(!ctx.can(Capability::ReadPage, ResourceContext::Restricted { allowlisted: false }));
}
```

- [ ] **Step 2: Run RED permission tests**

Run: `cargo test -p knowledge-core permissions`  
Expected: FAIL because role and permission types are missing.

- [ ] **Step 3: Implement IDs, roles, capabilities and pure permission evaluation**

```rust
pub fn can(&self, capability: Capability, resource: ResourceContext) -> bool {
    if !self.account_active || resource.denies_subject() { return false; }
    self.workspace_admin || self.space_role.is_some_and(|r| r.allows(capability))
}
```

- [ ] **Step 4: Add tests for suspended users, group/direct maximum and page non-escalation**

Run: `cargo test -p knowledge-core permissions`  
Expected: new tests fail before their matching branches are implemented, then PASS after the minimal branches are added.

- [ ] **Step 5: Run core checks and commit**

Run: `cargo test -p knowledge-core && cargo clippy -p knowledge-core --all-targets -- -D warnings`  
Expected: PASS.  
Commit: `feat(core): define identities and permission rules`

---

### Task 3: Cryptographic Envelopes, Workspace Keys and Passwords

**Files:**
- Create: `crates/knowledge-crypto/src/{envelope,keys,passwords}.rs`
- Modify: `crates/knowledge-crypto/src/lib.rs`
- Test: unit tests colocated with modules.

**Interfaces:**
- Produces: `MasterKey::from_base64(&SecretString) -> Result<MasterKey, CryptoError>`.
- Produces: `Keyring::{new_workspace,wrap,unwrap,rotate_master}`.
- Produces: `EnvelopeCipher::encrypt(&KeyVersion, aad: &[u8], plaintext: &[u8]) -> EncryptedEnvelope`.
- Produces: `EnvelopeCipher::decrypt(aad: &[u8], envelope: &EncryptedEnvelope) -> SecretVec<u8>`.
- Produces: `PasswordHasher::{hash,verify}` with Argon2id.

- [ ] **Step 1: Write failing envelope round-trip and AAD-binding tests**

```rust
#[test]
fn ciphertext_round_trips_only_with_matching_aad() {
    let cipher = test_cipher();
    let encrypted = cipher.encrypt(&KeyVersion(1), b"page:123", b"secret markdown").unwrap();
    assert_ne!(encrypted.ciphertext, b"secret markdown");
    assert_eq!(cipher.decrypt(b"page:123", &encrypted).unwrap().expose_secret(), b"secret markdown");
    assert!(cipher.decrypt(b"page:124", &encrypted).is_err());
}
```

- [ ] **Step 2: Run RED crypto test**

Run: `cargo test -p knowledge-crypto envelope`  
Expected: FAIL because crypto types are absent.

- [ ] **Step 3: Implement versioned AES-256-GCM envelope with random nonce and zeroizing keys**

Envelope serialized fields: `algorithm`, `key_version`, `nonce`, `ciphertext`; AAD includes workspace ID, entity type and entity ID.

- [ ] **Step 4: Add RED tests for malformed keys, nonce uniqueness, tampering and master-key rotation**

Run each named test before implementation; expected failures must identify the missing validation/rotation behavior.

- [ ] **Step 5: Implement HKDF-derived purpose keys and wrapped workspace DEK**

```rust
pub enum KeyPurpose { Content, Search, Attachment }
pub fn derive_key(dek: &WorkspaceKey, purpose: KeyPurpose) -> PurposeKey;
```

- [ ] **Step 6: Add RED password hashing tests and implement Argon2id**

Tests assert valid password verifies, wrong password fails, and stored string begins with `$argon2id$`.

- [ ] **Step 7: Run crypto suite and commit**

Run: `cargo test -p knowledge-crypto && cargo clippy -p knowledge-crypto --all-targets -- -D warnings`  
Expected: PASS.  
Commit: `feat(crypto): add versioned encryption and password hashing`

---

### Task 4: PostgreSQL Schema and Integration Test Harness

**Files:**
- Create: `crates/knowledge-store/migrations/0001_initial.sql`
- Create: `crates/knowledge-store/src/{pool,models}.rs`
- Modify: `crates/knowledge-store/src/lib.rs`
- Create: `crates/knowledge-store/tests/migrations.rs`
- Create: `deploy/compose.test.yaml`

**Interfaces:**
- Produces: `Store::connect(DatabaseSettings) -> Result<Store, StoreError>`.
- Produces: `Store::migrate() -> Result<(), StoreError>`.
- Produces tables and constraints defined in spec section 6.

- [ ] **Step 1: Write failing migration integration test**

```rust
#[sqlx::test(migrations = "./migrations")]
async fn initial_schema_enforces_one_grant_subject(pool: PgPool) {
    let result = sqlx::query("INSERT INTO space_grants (...) VALUES (... both user and group ...)")
        .execute(&pool).await;
    assert!(result.is_err());
}
```

- [ ] **Step 2: Start test PostgreSQL and run RED**

Run: `docker compose -f deploy/compose.test.yaml up -d postgres && cargo test -p knowledge-store --test migrations`  
Expected: FAIL because migration/table is absent.

- [ ] **Step 3: Add normalized schema, foreign keys, unique constraints and soft-delete fields**

The migration creates all spec entities, `version > 0` checks, XOR subject constraints and sibling ordering indexes. Encrypted columns use `bytea`; no plaintext title/body/name columns are created.

- [ ] **Step 4: Add RED tests for workspace isolation, revision immutability and page-parent consistency**

Expected: constraint tests fail before triggers/checks are added.

- [ ] **Step 5: Implement database constraints and migration checksum policy**

Revision `UPDATE/DELETE` is rejected by trigger except through documented retention maintenance role; cross-workspace foreign-key combinations are impossible.

- [ ] **Step 6: Run migrations twice and full store tests**

Run: `cargo test -p knowledge-store --test migrations`  
Expected: PASS on a clean database and after applying migrations once.

- [ ] **Step 7: Commit**

Commit: `feat(store): add encrypted multi-workspace schema`

---

### Task 5: Problem Details, Request IDs and Transactional Audit Primitive

**Files:**
- Create: `crates/knowledge-server/src/{problem,middleware}.rs`
- Create: `crates/knowledge-store/src/audit.rs`
- Create: `crates/knowledge-server/tests/api_errors.rs`
- Modify: `crates/knowledge-server/src/{lib,state}.rs`

**Interfaces:**
- Produces: `ApiProblem` implementing `IntoResponse` with RFC 9457 fields.
- Produces: `RequestIdLayer` and response header `x-request-id`.
- Produces: `AuditWriter::append(&mut Transaction, NewAuditEvent)`; no standalone commit method.

- [ ] **Step 1: Write RED API error test**

```rust
#[tokio::test]
async fn validation_error_is_problem_json_with_request_id() {
    let response = request_invalid_fixture().await;
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(response.headers()[CONTENT_TYPE], "application/problem+json");
    assert!(response.headers().contains_key("x-request-id"));
    assert_eq!(json(response).await["code"], "validation_failed");
}
```

- [ ] **Step 2: Run RED and implement `ApiProblem` plus request middleware**

Run: `cargo test -p knowledge-server --test api_errors` before and after implementation.  
Expected: first FAIL, then PASS.

- [ ] **Step 3: Write RED transaction rollback test**

Create a test mutation that writes an entity, forces audit insertion failure, and asserts neither row exists.

- [ ] **Step 4: Implement append-only audit writer requiring caller transaction**

Metadata serializer rejects fields named `password`, `token`, `key`, `body`, `content` and links events with a workspace hash chain.

- [ ] **Step 5: Run server/store tests and commit**

Commit: `feat(api): standardize errors request IDs and audit transactions`

---

### Task 6: Bootstrap, Authentication, Sessions and CSRF

**Files:**
- Create: `crates/knowledge-store/src/users.rs`
- Create: `crates/knowledge-server/src/routes/auth.rs`
- Create: `crates/knowledge-server/src/bin/bootstrap.rs`
- Create: `crates/knowledge-server/tests/api_auth.rs`
- Modify: `crates/knowledge-server/src/routes/mod.rs`

**Interfaces:**
- Produces: `POST /api/v1/auth/login`, `POST /auth/logout`, `GET /auth/session`, `GET /auth/csrf`.
- Produces: `CurrentUser` Axum extractor.
- Produces: `bootstrap --email <email>` reading password/master key from stdin/secrets.

- [ ] **Step 1: Write RED bootstrap idempotency test**

Test invokes bootstrap service twice; first creates workspace/admin, second returns `BootstrapAlreadyCompleted`, with exactly one active admin and an audit event.

- [ ] **Step 2: Implement minimal bootstrap transaction and CLI wrapper**

The CLI rejects password arguments and prints no password, token, DEK or master key.

- [ ] **Step 3: Write RED login/session tests**

```rust
assert!(set_cookie.contains("HttpOnly"));
assert!(set_cookie.contains("Secure"));
assert!(!database_session_row.token_hash.contains(&raw_cookie_token));
```

- [ ] **Step 4: Implement session creation, constant-time verification and cookie policy**

Session token is 256 random bits; database stores SHA-256 hash; login rotates any pre-auth session.

- [ ] **Step 5: Write RED tests for CSRF, expiry, logout, suspension and rate limiting**

Each state-changing request without CSRF returns 403 Problem Details; suspension and password change revoke all sessions.

- [ ] **Step 6: Implement CSRF and auth middleware; run tests**

Run: `cargo test -p knowledge-server --test api_auth`  
Expected: PASS.

- [ ] **Step 7: Commit**

Commit: `feat(auth): add secure bootstrap and session authentication`

---

### Task 7: Invitations and User Administration

**Files:**
- Create: `crates/knowledge-server/src/routes/users.rs`
- Extend: `crates/knowledge-store/src/users.rs`
- Create: `crates/knowledge-server/tests/api_users.rs`

**Interfaces:**
- Produces: `POST/GET /api/v1/invitations`, `POST /invitations/{token}/accept`, `DELETE /invitations/{id}`.
- Produces: `GET /users`, `PATCH /users/{id}`, `POST /users/{id}/revoke-sessions`.

- [ ] **Step 1: Write RED invitation lifecycle test**

Admin creates invite, token is returned once, accepting activates user, reusing token returns 404/expired without changing state.

- [ ] **Step 2: Implement invitation repository/service/routes with hashed tokens and email HMAC**

- [ ] **Step 3: Write RED tests for duplicate email, expiry, revocation and non-admin denial**

Expected denial is enumeration-safe and every accepted/revoked/suspended action has an audit event.

- [ ] **Step 4: Implement user list/suspend/reactivate/session-revoke operations**

- [ ] **Step 5: Run tests and commit**

Run: `cargo test -p knowledge-server --test api_users`  
Expected: PASS.  
Commit: `feat(users): add invitations and account administration`

---

### Task 8: Groups and Memberships

**Files:**
- Create: `crates/knowledge-store/src/groups.rs`
- Create: `crates/knowledge-server/src/routes/groups.rs`
- Create: `crates/knowledge-server/tests/api_groups.rs`

**Interfaces:**
- Produces: CRUD `/api/v1/groups`.
- Produces: `PUT/DELETE /groups/{group_id}/members/{user_id}`.
- Produces: `GroupRepository::subject_groups(UserId) -> Vec<GroupId>`.

- [ ] **Step 1: Write RED group CRUD and membership tests**

Tests cover create/list/rename/archive, duplicate membership idempotency and suspended-user rejection.

- [ ] **Step 2: Implement encrypted group names/descriptions and membership transactions**

- [ ] **Step 3: Write RED audit and authorization tests**

Member receives 404/403 as contract requires; admin mutation creates one redacted audit event.

- [ ] **Step 4: Implement authorization/audit and run tests**

Run: `cargo test -p knowledge-server --test api_groups`  
Expected: PASS.

- [ ] **Step 5: Commit**

Commit: `feat(groups): manage encrypted groups and memberships`

---

### Task 9: Spaces, Grants and Effective Access

**Files:**
- Create: `crates/knowledge-store/src/spaces.rs`
- Create: `crates/knowledge-server/src/routes/spaces.rs`
- Create: `crates/knowledge-server/tests/api_spaces.rs`
- Modify: `crates/knowledge-core/src/permissions.rs`

**Interfaces:**
- Produces: CRUD `/api/v1/spaces`.
- Produces: CRUD `/spaces/{id}/grants` for user/group subjects.
- Produces: `AccessResolver::for_space(UserId, SpaceId, &mut Transaction) -> EffectiveAccess`.

- [ ] **Step 1: Write RED maximum-role resolution test**

User has direct Viewer and group Editor; result must be Editor. Removing group membership invalidates access immediately.

- [ ] **Step 2: Implement space repository, encrypted metadata and grant validation**

- [ ] **Step 3: Write RED API matrix for admin/owner/editor/viewer/no-access**

Matrix covers read, edit settings, manage grants and archive.

- [ ] **Step 4: Implement AccessResolver without long-lived ACL cache**

Initial implementation queries direct and group grants transactionally; caching is postponed until metrics justify it.

- [ ] **Step 5: Run tests and commit**

Run: `cargo test -p knowledge-server --test api_spaces && cargo test -p knowledge-core permissions`  
Expected: PASS.  
Commit: `feat(spaces): add spaces grants and effective access`

---

### Task 10: Encrypted Markdown Pages, Tree Operations and Revisions

**Files:**
- Create: `crates/knowledge-core/src/pages.rs`
- Create: `crates/knowledge-store/src/pages.rs`
- Create: `crates/knowledge-server/src/routes/pages.rs`
- Create: `crates/knowledge-server/tests/api_pages.rs`

**Interfaces:**
- Produces: CRUD `/api/v1/pages`, `POST /pages/{id}/move`.
- Produces: `GET /pages/{id}/revisions`, `POST /pages/{id}/restore`.
- Produces: `GET /pages/{id}/backlinks` and an ID-only derived link graph.
- Produces: `SavePage { expected_version, title, markdown, properties }`.

- [ ] **Step 1: Write RED page-create test asserting ciphertext at rest**

Create page through service, inspect raw `bytea`, assert neither title marker nor Markdown marker occurs, then fetch through authorized API and assert exact plaintext.

- [ ] **Step 2: Implement create/get with AEAD AAD bound to workspace/entity IDs**

Create page, revision 1 and audit event in one transaction.

- [ ] **Step 3: Write RED optimistic concurrency test**

Two requests use version 1; first produces version 2, second receives 409 containing `currentVersion: 2` and echoes no confidential current body.

- [ ] **Step 4: Implement save transaction and immutable revision snapshots**

- [ ] **Step 5: Write RED tests for move cycle, cross-space rights, ordering and archived subtree**

Cycle `A → B → A` is rejected; moving across spaces requires edit rights in source and destination.

- [ ] **Step 6: Implement tree validation, LexoRank-style sortable keys and soft archive/restore**

- [ ] **Step 7: Write RED revision restore test and implement restore-as-new-version**

Restore revision 1 from current version 3 creates version 4 and preserves revisions 1–3.

- [ ] **Step 8: Write RED backlink extraction and authorization tests**

Markdown links using stable page URL keys create `page_links(source_page_id, target_page_id)` rows in the save transaction. Renaming does not break links; removing a link deletes the derived row; backlinks reveal only source pages readable by the caller. Raw link labels and surrounding Markdown never enter the derived table.

- [ ] **Step 9: Implement link extraction, derived graph updates and backlinks route**

The parser consumes the Markdown AST rather than regex, ignores code blocks, resolves only same-workspace stable page keys and treats unresolved links as ordinary Markdown.

- [ ] **Step 10: Run tests and commit**

Run: `cargo test -p knowledge-server --test api_pages`  
Expected: PASS.  
Commit: `feat(pages): add encrypted Markdown trees and revisions`

---

### Task 11: Restricted Page Boundaries

**Files:**
- Modify: `crates/knowledge-core/src/permissions.rs`
- Extend: `crates/knowledge-store/src/pages.rs`
- Extend: `crates/knowledge-server/src/routes/pages.rs`
- Create: `crates/knowledge-server/tests/api_page_permissions.rs`

**Interfaces:**
- Produces: `PUT/DELETE /api/v1/pages/{id}/permissions/{subject}`.
- Produces: nearest restricted ancestor query and `PageAccessContext`.

- [ ] **Step 1: Write RED subtree permission matrix**

Fixtures include unrestricted parent, restricted child and grandchild; tests assert an allowlisted subject sees child/grandchild while an ordinary space owner does not unless workspace admin.

- [ ] **Step 2: Implement nearest-boundary resolution and page grants**

Page grants never create space access and cannot grant Owner.

- [ ] **Step 3: Write RED non-enumeration tests across page, revision and tree-list endpoints**

Unauthorized IDs return the same status/body class as absent IDs.

- [ ] **Step 4: Apply one shared page authorization guard to all existing routes**

- [ ] **Step 5: Run matrix and commit**

Run: `cargo test -p knowledge-server --test api_page_permissions`  
Expected: PASS.  
Commit: `feat(permissions): enforce restricted page subtrees`

---

### Task 12: Keyed Blind Search

**Files:**
- Create: `crates/knowledge-crypto/src/blind_index.rs`
- Create: `crates/knowledge-store/src/search.rs`
- Create: `crates/knowledge-server/src/routes/search.rs`
- Create: `crates/knowledge-server/tests/api_search.rs`

**Interfaces:**
- Produces: `BlindIndexer::tokens(language, text) -> Vec<BlindToken>`.
- Produces: `GET /api/v1/search?q=&space=&author=&from=&to=`.
- Produces: ACL-filtered `SearchHit { page_id, title, snippet, updated_at }`.

- [ ] **Step 1: Write RED deterministic tokenization tests**

Cases cover Russian/English case folding, Unicode normalization, punctuation, repeated terms, phrase candidates and prefix lengths 3–12.

- [ ] **Step 2: Implement normalization and HMAC-SHA-256 token generation**

No raw word or reversible encoding is exposed by `BlindToken` debug/serialization.

- [ ] **Step 3: Write RED persistence test proving plaintext terms are absent**

After indexing marker `СЕКРЕТНЫЙ_МАРКЕР`, query raw DB dumps for UTF-8 marker and assert no match.

- [ ] **Step 4: Implement transactional reindex on page save/archive/restore**

- [ ] **Step 5: Write RED ACL search tests**

Search returns accessible candidates only, checks restricted boundary before decryption, generates snippets in memory and caps candidate count.

- [ ] **Step 6: Implement query route, ranking and filters**

Rank exact title phrase, title tokens, body phrase, body tokens, then recency; stable tie-breaker is page ID.

- [ ] **Step 7: Run tests and commit**

Run: `cargo test -p knowledge-crypto blind_index && cargo test -p knowledge-server --test api_search`  
Expected: PASS.  
Commit: `feat(search): add permission-aware blind search`

---

### Task 13: Encrypted Attachments

**Files:**
- Create: `crates/knowledge-store/src/attachments.rs`
- Create: `crates/knowledge-server/src/routes/attachments.rs`
- Create: `crates/knowledge-server/tests/api_attachments.rs`

**Interfaces:**
- Produces: `POST/GET /pages/{id}/attachments`.
- Produces: `GET/DELETE /attachments/{id}`.
- Produces: `ObjectStore` trait with MinIO implementation and in-memory test implementation.

- [ ] **Step 1: Write RED streaming round-trip test**

Upload multi-chunk bytes, assert object store contains no plaintext marker/name, download through API and assert byte equality.

- [ ] **Step 2: Implement staging-object upload and chunked AEAD stream format**

Each chunk authenticates workspace ID, attachment ID, chunk index and total declared size. Metadata becomes visible only after object finalize and SQL commit.

- [ ] **Step 3: Write RED tests for interrupted upload, MIME/size rejection and orphan cleanup**

- [ ] **Step 4: Implement validation, cleanup job and authorized download**

Download verifies page permission. HTTP range is initially supported only on chunk boundaries and returns documented 416 otherwise.

- [ ] **Step 5: Run tests and commit**

Run: `cargo test -p knowledge-server --test api_attachments`  
Expected: PASS.  
Commit: `feat(attachments): add encrypted staged file storage`

---

### Task 14: Audit Query, Jobs and Encrypted Export

**Files:**
- Create: `crates/knowledge-store/src/jobs.rs`
- Extend: `crates/knowledge-store/src/audit.rs`
- Create: `crates/knowledge-server/src/routes/{audit,exports}.rs`
- Create: `crates/knowledge-server/tests/{api_audit,api_exports}.rs`

**Interfaces:**
- Produces: `GET /api/v1/audit-events` with cursor/filter pagination.
- Produces: `POST/GET /api/v1/exports`, `GET /exports/{id}/download`.
- Produces: leased at-least-once `JobRunner` and idempotent handlers.

- [ ] **Step 1: Write RED audit filter and hash-chain verification tests**

Admin can filter actor/type/target/date; member denied; deleting/reordering fixture events makes verifier fail.

- [ ] **Step 2: Implement audit read model with encrypted safe metadata and stable cursors**

- [ ] **Step 3: Write RED job lease/retry/idempotency tests**

Two runners cannot own one active lease; expired lease is reclaimed; repeated export job produces one logical result.

- [ ] **Step 4: Implement PostgreSQL job leasing with `FOR UPDATE SKIP LOCKED`**

- [ ] **Step 5: Write RED export archive test**

Archive contains Markdown/YAML stable IDs and attachments only from requested accessible space; object store keeps encrypted archive; download is short-lived and audited.

- [ ] **Step 6: Implement streaming export, expiry and cleanup**

- [ ] **Step 7: Run tests and commit**

Run: `cargo test -p knowledge-server --test api_audit --test api_exports`  
Expected: PASS.  
Commit: `feat(operations): add audit queries jobs and exports`

---

### Task 15: OpenAPI Client, Login and Application Shell

**Files:**
- Create: `crates/knowledge-server/src/openapi.rs`
- Create: `apps/web/src/api/{client,generated}.ts`
- Create: `apps/web/src/auth/{session,login-page}.tsx`
- Create: `apps/web/src/app-shell/{app-shell,sidebar,topbar}.tsx`
- Create: corresponding `*.test.tsx` files.

**Interfaces:**
- Produces: `/api/v1/openapi.json` in development/build generation.
- Produces: typed frontend API client with Problem Details decoding.
- Produces: guarded `/login`, `/app`, `/admin` routes.

- [ ] **Step 1: Write RED OpenAPI contract snapshot test**

Test validates paths, Problem Details schema and security cookie/CSRF scheme.

- [ ] **Step 2: Generate TypeScript client and enforce clean regeneration diff**

Run: `pnpm api:generate && git diff --exit-code apps/web/src/api/generated.ts` in CI.

- [ ] **Step 3: Write RED login behavior test**

User submits credentials, client fetches CSRF/session, redirects to `/app`; invalid credentials render localized generic error.

- [ ] **Step 4: Implement QueryClient, session provider, login and route guards**

- [ ] **Step 5: Write RED app-shell keyboard/navigation test**

Shell exposes navigation landmarks, space switcher, page tree region, search button and profile menu.

- [ ] **Step 6: Implement responsive shell and run tests**

Run: `pnpm --filter @knowledge/web test --run && pnpm --filter @knowledge/web build`  
Expected: PASS.

- [ ] **Step 7: Commit**

Commit: `feat(web): add typed login and workspace shell`

---

### Task 16: Administrative UI for Users, Groups and Spaces

**Files:**
- Create: `apps/web/src/admin/{admin-layout,dashboard,users,groups,spaces,audit,settings}.tsx`
- Create: `apps/web/src/admin/**/*.test.tsx`
- Modify: `apps/web/src/router.tsx`

**Interfaces:**
- Produces `/admin`, `/admin/users`, `/admin/groups`, `/admin/spaces`, `/admin/audit`, `/admin/settings`.

- [ ] **Step 1: Write RED invitation management component test**

Test opens dialog, validates email, submits role/expiry, displays token once and supports secure copy confirmation.

- [ ] **Step 2: Implement users/invitations screens through generated API hooks**

- [ ] **Step 3: Write RED group membership and space grant tests**

Tests cover searchable user selection, idempotent add/remove, user/group grant subject and role validation.

- [ ] **Step 4: Implement groups, spaces and grants screens**

- [ ] **Step 5: Write RED audit/settings/dashboard tests**

Audit filters keep cursor state; security controls revoke sessions; readiness failures are visible without exposing secrets.

- [ ] **Step 6: Implement remaining admin routes; run accessibility checks**

Run: `pnpm --filter @knowledge/web test --run`  
Expected: PASS with no React act or accessibility warnings.

- [ ] **Step 7: Commit**

Commit: `feat(web): add complete administration console`

---

### Task 17: Page Tree, Markdown Editor, History, Search and Attachments UI

**Files:**
- Create: `apps/web/src/pages/{page-tree,page-view,page-toolbar,page-permissions}.tsx`
- Create: `apps/web/src/editor/{markdown-editor,draft-store,conflict-dialog,history-panel,diff-view}.tsx`
- Create: `apps/web/src/search/{search-dialog,search-results}.tsx`
- Create: `apps/web/src/attachments/{attachment-list,uploader}.tsx`
- Create: corresponding `*.test.tsx` files.

**Interfaces:**
- Produces full `/app/spaces/:spaceId/pages/:pageId` workflow.
- Produces local draft key `{workspaceId}:{pageId}:{serverVersion}`.

- [ ] **Step 1: Write RED page-tree operations tests**

Create child, rename, reorder, move and archive use optimistic UI but roll back on Problem Details response.

- [ ] **Step 2: Implement virtualized accessible tree and page route**

- [ ] **Step 3: Write RED draft/conflict tests**

Typing persists local draft; successful save clears matching draft; 409 shows submitted and current-version metadata and never discards text.

- [ ] **Step 4: Implement Milkdown editor, raw Markdown mode, draft store and save status**

- [ ] **Step 5: Write RED history/restore/permissions tests and implement panels**

Diff is calculated client-side from authorized revision payloads; restore requires confirmation and updates current version.

- [ ] **Step 6: Write RED search tests and implement command palette**

Keyboard shortcut focuses search; filters serialize to URL; result snippets are treated as text, not HTML.

- [ ] **Step 7: Write RED attachment tests and implement upload/download UI**

Progress, cancellation, validation Problem Details and encrypted-storage messaging are visible.

- [ ] **Step 8: Run frontend suite and commit**

Run: `pnpm --filter @knowledge/web test --run && pnpm --filter @knowledge/web build`  
Expected: PASS.  
Commit: `feat(web): deliver Markdown knowledge workspace`

---

### Task 18: Localization, Accessibility and Failure States

**Files:**
- Create: `apps/web/src/i18n/{index,en,ru}.ts`
- Create: `apps/web/src/components/{problem-alert,offline-banner,not-found}.tsx`
- Create: `apps/web/src/i18n/i18n.test.ts`
- Modify: all user-facing components to use message keys.

**Interfaces:**
- Produces: runtime RU/EN language switch with persisted preference.
- Produces: common rendering for offline, 403/404, 409 and 5xx states.

- [ ] **Step 1: Write RED locale completeness test**

Test flattens `en` and `ru` dictionaries and asserts identical key sets and no blank values.

- [ ] **Step 2: Implement i18n provider and replace literal product UI strings**

- [ ] **Step 3: Write RED keyboard/focus/failure-state tests**

Dialogs trap/restore focus; errors use alerts; tree/editor/search are reachable without pointer; offline draft remains available.

- [ ] **Step 4: Implement shared states and accessibility fixes**

- [ ] **Step 5: Run browser unit checks and commit**

Run: `pnpm --filter @knowledge/web test --run && pnpm --filter @knowledge/web build`  
Expected: PASS.  
Commit: `feat(web): add localization accessibility and resilient states`

---

### Task 19: Production Compose, Backup/Restore and Operations Documentation

**Files:**
- Create: `deploy/{compose.yaml,Caddyfile,.env.example}`
- Create: `crates/knowledge-server/Dockerfile`, `apps/web/Dockerfile`
- Create: `scripts/{backup,restore,verify-backup}.{sh,ps1}`
- Create: `docs/{operations,security,user-guide}.md`
- Create: `tests/operations/{compose-smoke.sh,compose-smoke.ps1}`

**Interfaces:**
- Produces: production deployment with Caddy, backend, frontend, PostgreSQL and MinIO.
- Produces: encrypted backup bundle plus manifest; master key remains separate.

- [ ] **Step 1: Write failing Compose smoke script**

Script starts clean project, waits for readiness, runs bootstrap, asserts login page/API health, then shuts down without deleting named volumes unless explicit test cleanup flag is set.

- [ ] **Step 2: Add multi-stage images, non-root users, healthchecks and secret mounts**

Compose has no default production passwords. Startup fails with a precise error if master key or required credentials are missing.

- [ ] **Step 3: Run RED backup/restore verification**

Seed marker page/attachment, back up DB/MinIO metadata and ciphertext, restore into separate Compose project, assert marker round-trip through API with copied key; assert restore without key fails safely.

- [ ] **Step 4: Implement equivalent Bash and PowerShell backup/restore scripts with explicit target validation**

Both variants require an absolute backup directory, reject workspace/repository roots, write identical manifest checksums, and never copy the master key. CI compares the produced manifests from both entry points.

- [ ] **Step 5: Write operations, security and user documentation**

Documents include install, bootstrap, upgrade, key custody/rotation, backup/restore drill, threat model, permission semantics, search limitations and recovery paths with exact commands.

- [ ] **Step 6: Run smoke and verification; commit**

Run on Windows: `pwsh -File tests/operations/compose-smoke.ps1` and `pwsh -File scripts/verify-backup.ps1 -TestMode`.  
Run on Linux: `bash tests/operations/compose-smoke.sh` and `bash scripts/verify-backup.sh --test-mode`.  
Expected: PASS.  
Commit: `ops: add secure deployment backup and documentation`

---

### Task 20: End-to-End, Security and Release Gate

**Files:**
- Create: `playwright.config.ts`, `tests/e2e/{auth,admin,pages,permissions,conflicts,attachments}.spec.ts`
- Create: `.github/workflows/ci.yml`
- Create: `scripts/release-check.ps1`
- Modify: `README.md`

**Interfaces:**
- Produces: one release command that verifies backend, frontend, migrations, E2E, dependencies and clean generated artifacts.

- [ ] **Step 1: Write RED Playwright bootstrap-to-page scenario**

Scenario: bootstrap admin → login → invite editor/viewer → create group/space/grant → editor writes Markdown → viewer reads and cannot edit.

- [ ] **Step 2: Add RED security scenarios**

Tests attempt IDOR for page/revision/attachment/export, restricted-search leakage, CSRF mutation, stale version overwrite, executable Markdown and raw object-store access.

- [ ] **Step 3: Implement only fixes required by failing E2E scenarios**

Each discovered defect first receives the smallest reproducing test at the lowest appropriate layer, then the fix.

- [ ] **Step 4: Add CI and deterministic release gate**

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --filter @knowledge/web lint
pnpm --filter @knowledge/web test --run
pnpm --filter @knowledge/web build
pnpm exec playwright test
cargo audit
pnpm audit --prod
git diff --exit-code
```

- [ ] **Step 5: Run backup restore and fresh-host acceptance**

Use a new Compose project name and empty volumes; follow only `docs/operations.md`. Record command output in release notes, not secrets.

- [ ] **Step 6: Verify every spec acceptance criterion and commit**

Run: `pwsh -File scripts/release-check.ps1`  
Expected: all checks PASS and worktree is clean.  
Commit: `test: enforce end-to-end v1 release gate`

## Execution Order and Checkpoints

- Checkpoint A after Tasks 1–6: runnable encrypted/authenticated backend foundation.
- Checkpoint B after Tasks 7–11: complete users/groups/spaces/pages/ACL API.
- Checkpoint C after Tasks 12–14: search, attachments, audit, jobs and export.
- Checkpoint D after Tasks 15–18: complete site and admin interface.
- Checkpoint E after Tasks 19–20: deployable, documented and release-verified v1.

At every checkpoint:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm --filter @knowledge/web test --run
pnpm --filter @knowledge/web build
git status --short
```

Integration checkpoints additionally run PostgreSQL/MinIO/Playwright through Docker Compose.
