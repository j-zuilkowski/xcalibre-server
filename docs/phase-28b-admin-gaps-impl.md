# Phase 28b — Admin API Gaps Implementation

## Context
Rust 2021, Axum 0.7.
Phase 28a complete: failing tests in `backend/tests/test_admin_gaps.rs`.

Goal: implement admin parity endpoints for logs, backup, cover regeneration queueing, task cancellation, and domain allowlist/blocklist.

Important repository state note:
- `backend/src/api/admin.rs` may be absent.
- If missing, create a new admin module file and register it in router wiring.
- Do not assume in-place edits to that file are possible.

---

## 1. Log viewer endpoint

Add:
`GET /api/v1/admin/logs?lines=<n>&level=<info|warn|error>`

Behavior:
- Admin-only.
- Resolve file path from config `log.file`.
- If file logging not configured: return `404` with

```json
{ "error": "file logging not configured" }
```

- `lines` default `100`, max `500`.
- Validate `level`; invalid value returns `400`.
- Read tail efficiently by seeking near file end and scanning backward.
- Parse each line as JSON object.
- Filter by level when specified.
- Skip invalid JSON lines silently.

---

## 2. Metadata backup endpoint

Add:
`POST /api/v1/admin/backup`

Behavior:
- Admin-only.
- Config section `[backup] dir = "./backups"` (default when absent).
- Create backup directory if missing.
- Use SQL `VACUUM INTO ?` via sqlx.
- Filename format: `xcalibre-<timestamp>.db`.
- Guard with process-wide `AtomicBool` for single active backup.
- If another backup is active: `409`.
- Success: `200 { "path": "xcalibre-...db" }`.

---

## 3. Cover regeneration enqueue endpoint

Add:
`POST /api/v1/admin/covers/regenerate`

Request:

```json
{ "book_ids": [1, 2, 3] }
```

Behavior:
- Admin-only.
- Empty array means all books.
- Enqueue one task per target book.
- Add `TaskKind::CoverRegenerate(book_id)` variant to task model.
- Worker uses existing cover extraction/regeneration logic.
- Return `202 { "queued": <count> }` immediately.

---

## 4. Task cancellation endpoint

Add:
`DELETE /api/v1/admin/tasks/:task_id`

Behavior:
- Admin-only.
- `404` when task missing.
- `409` when task already completed/failed/cancelled.
- Otherwise set DB status to `cancelled` and return `200`.
- Background worker checks task row status before each work unit and exits early on cancelled tasks.

---

## 5. Domain allowlist/blocklist management

### Migration

Add:
- `backend/migrations/sqlite/0030_email_domains.sql`
- `backend/migrations/mariadb/0030_email_domains.sql`

Schema:

```sql
CREATE TABLE email_domains (
  id INTEGER PRIMARY KEY,
  domain TEXT NOT NULL UNIQUE,
  allow INTEGER NOT NULL,
  created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
```

### Endpoints

- `GET /api/v1/admin/domains?allow=true|false`
- `POST /api/v1/admin/domains` body `{ "domain": "...", "allow": true }`
- `DELETE /api/v1/admin/domains/:id`

All admin-only.

### Registration enforcement

In registration handler:
- run existing password validation first.
- before user insert, enforce domain list policy:
  - when list empty: allow all.
  - allowlist mode rows exist (`allow=true`): only those domains allowed.
  - blocklist mode rows exist (`allow=false`): those domains denied.

Return `400` with clear message on blocked domain.

---

## 6. Router and module wiring

- Register all new admin endpoints in API router.
- If `backend/src/api/admin.rs` is absent, create it and move/add handlers there.
- Keep existing route paths stable.

---

## 7. Docs update

Update `docs/API.md` Admin section with:
- logs endpoint and `log.file` behavior
- backup endpoint and `[backup].dir`
- cover regenerate endpoint
- task cancel endpoint behavior
- domain management endpoints and registration enforcement rules

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_admin_gaps 2>&1 | tail -100
cargo clippy -- -D warnings 2>&1 | tail -40
cargo audit 2>&1 | tail -40
```
Expected: **all `test_admin_gaps` tests pass**, zero clippy warnings, zero audit vulnerabilities.

## Commit
```bash
git add backend/migrations/sqlite/0030_email_domains.sql \
        backend/migrations/mariadb/0030_email_domains.sql \
        backend/src/api/admin.rs \
        backend/src/api/auth.rs \
        backend/src/tasks.rs \
        backend/src/api/mod.rs \
        docs/API.md
git commit -m "Phase 28b — admin API gaps implementation"
```

## Final Step
`Stop now. Do not run any more commands.`
