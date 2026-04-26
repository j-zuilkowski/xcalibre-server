# calibre-web Rewrite — Database Schema

_Status: Current_
_Last updated: 2026-04-24_

---

## Design Principles

- All tables have `last_modified` — required for mobile offline sync (Decision D)
- UUIDs as primary keys — text representation for SQLite/MariaDB portability
- No ENUMs in DDL — TEXT + CHECK constraints work on both SQLite and MariaDB
- Separate migration files per database engine (`migrations/sqlite/`, `migrations/mariadb/`)
- All timestamps stored as ISO8601 UTC text in SQLite; DATETIME in MariaDB — sqlx handles mapping
- Soft deletes not used — hard delete with cascade; deletion is intentional in a library app

---

## Entity Relationship Overview

```
libraries
  └── books ──── book_authors ── authors ── author_profiles
  │      │
  │      ├── book_tags ─── tags
  │      ├── book_user_state ─ users
  │      ├── book_annotations ─ users
  │      ├── download_history ─ users
  │      ├── user_tag_restrictions ─ users
  │      ├── formats
  │      ├── identifiers
  │      ├── series
  │      ├── custom_column_values ── custom_columns
  │      └── book_chunks (+ book_chunks_fts virtual)
  │
users ──────────── roles
  │
  ├── refresh_tokens
  ├── sessions
  ├── api_tokens
  ├── oauth_accounts
  ├── kobo_devices ── kobo_reading_state ── books
  ├── reading_progress ── books
  ├── shelves
  │     └── shelf_books ── books
  ├── collections
  │     └── collection_books ── books
  ├── webhooks
  │     └── webhook_deliveries
  └── goodreads_import_log

llm_jobs ──────────────────── books (nullable)
llm_eval_results
migration_log
audit_log ─────────────────── users (nullable)
book_embeddings ────────────── books
email_settings                 (singleton row)
scheduled_tasks
```

---

## Tables

### `roles`

Permission sets assigned to users. Ships with two default rows: `admin` and `user`.

```sql
CREATE TABLE roles (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE,
    can_upload    INTEGER NOT NULL DEFAULT 0,   -- single book upload
    can_bulk      INTEGER NOT NULL DEFAULT 0,   -- bulk import (admin only by default)
    can_edit      INTEGER NOT NULL DEFAULT 1,   -- metadata editing
    can_download  INTEGER NOT NULL DEFAULT 1,
    created_at    TEXT NOT NULL,
    last_modified TEXT NOT NULL
);
```

**Default rows:**

| role | can_upload | can_bulk | can_edit | can_download |
|---|---|---|---|---|
| admin | 1 | 1 | 1 | 1 |
| user | 0 | 0 | 1 | 1 |

---

### `users`

```sql
CREATE TABLE users (
    id                  TEXT PRIMARY KEY,
    username            TEXT NOT NULL UNIQUE,
    email               TEXT NOT NULL UNIQUE,
    password_hash       TEXT NOT NULL,              -- argon2
    role_id             TEXT NOT NULL REFERENCES roles(id),
    is_active           INTEGER NOT NULL DEFAULT 1,
    force_pw_reset      INTEGER NOT NULL DEFAULT 0, -- set on migration import
    default_library_id  TEXT NOT NULL DEFAULT 'default'
                        REFERENCES libraries(id),
    created_at          TEXT NOT NULL,
    last_modified       TEXT NOT NULL
);

CREATE INDEX idx_users_username ON users(username);
CREATE INDEX idx_users_email ON users(email);
```

---

### `refresh_tokens`

```sql
CREATE TABLE refresh_tokens (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,           -- SHA256 of the raw token
    expires_at  TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    revoked_at  TEXT                            -- NULL = still valid
);

CREATE INDEX idx_refresh_tokens_user ON refresh_tokens(user_id);
CREATE INDEX idx_refresh_tokens_hash ON refresh_tokens(token_hash);
```

---

### `authors`

```sql
CREATE TABLE authors (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    sort_name     TEXT NOT NULL,               -- "Fitzgerald, F. Scott"
    last_modified TEXT NOT NULL
);

CREATE INDEX idx_authors_sort ON authors(sort_name);
```

---

### `author_profiles`

```sql
CREATE TABLE author_profiles (
    author_id      TEXT PRIMARY KEY REFERENCES authors(id) ON DELETE CASCADE,
    bio            TEXT,
    photo_path     TEXT,
    born           TEXT,
    died           TEXT,
    website_url    TEXT,
    openlibrary_id TEXT,
    updated_at     TEXT NOT NULL
);
```

---

### `series`

```sql
CREATE TABLE series (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    sort_name     TEXT NOT NULL,
    last_modified TEXT NOT NULL
);
```

---

### `tags`

```sql
CREATE TABLE tags (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL UNIQUE,
    source        TEXT NOT NULL DEFAULT 'manual'
                  CHECK(source IN ('manual', 'llm', 'calibre_import')),
    last_modified TEXT NOT NULL
);
```

---

### `books`

Central entity. All other content hangs off this.

```sql
CREATE TABLE books (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL,
    sort_title    TEXT NOT NULL,               -- "Great Gatsby, The"
    description   TEXT,
    pubdate       TEXT,                        -- ISO8601 date
    language      TEXT,                        -- BCP 47 (e.g. "en", "fr")
    rating        INTEGER CHECK(rating BETWEEN 0 AND 10),
    series_id     TEXT REFERENCES series(id) ON DELETE SET NULL,
    series_index  REAL,                        -- position within series
    has_cover     INTEGER NOT NULL DEFAULT 0,
    cover_path    TEXT,                        -- bucketed: covers/{first2}/{uuid}.jpg
    document_type TEXT NOT NULL DEFAULT 'unknown'
                  CHECK(document_type IN ('novel', 'textbook', 'reference', 'magazine',
                                          'datasheet', 'comic', 'audiobook', 'unknown')),
    flags         TEXT,                        -- JSON object; known keys: {"publisher": "..."}
    library_id    TEXT NOT NULL DEFAULT 'default'
                  REFERENCES libraries(id),
    indexed_at    TEXT,                        -- NULL or < last_modified = needs Meilisearch reindex
    created_at    TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE INDEX idx_books_sort_title  ON books(sort_title);
CREATE INDEX idx_books_series      ON books(series_id);
CREATE INDEX idx_books_pubdate     ON books(pubdate);
CREATE INDEX idx_books_language    ON books(language);
CREATE INDEX idx_books_library_id  ON books(library_id);
CREATE INDEX idx_books_indexed_at  ON books(indexed_at);
```

**`flags` column:** A JSON object. Do not add keys that need indexed filtering — use a dedicated column instead. Currently defined keys:

| Key | Type | Usage |
|---|---|---|
| `publisher` | string | Book publisher; accessed via `json_extract(b.flags, '$.publisher')` in OPDS feeds and bulk filter queries |

---

### `book_user_state`

Per-user read/unread and archived state for books.

```sql
CREATE TABLE book_user_state (
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id     TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    is_read     INTEGER NOT NULL DEFAULT 0,
    is_archived INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (user_id, book_id)
);

CREATE INDEX idx_book_user_state_user ON book_user_state(user_id);
```

---

### `download_history`

Records successful file downloads for each user.

```sql
CREATE TABLE download_history (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id       TEXT NOT NULL,
    format        TEXT NOT NULL,
    downloaded_at TEXT NOT NULL
);

CREATE INDEX idx_download_history_user ON download_history(user_id);
CREATE INDEX idx_download_history_book ON download_history(book_id);
```

---

### `user_tag_restrictions`

Per-user allow/block tag controls applied at browse time.

```sql
CREATE TABLE user_tag_restrictions (
    user_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    mode     TEXT NOT NULL CHECK (mode IN ('allow', 'block')),
    PRIMARY KEY (user_id, tag_id)
);

CREATE INDEX idx_user_tag_restrictions_user ON user_tag_restrictions(user_id);
```

---

### `book_authors`

Many-to-many. `display_order` controls how authors are listed on the book detail page.

```sql
CREATE TABLE book_authors (
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    author_id     TEXT NOT NULL REFERENCES authors(id) ON DELETE CASCADE,
    display_order INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (book_id, author_id)
);

CREATE INDEX idx_book_authors_author ON book_authors(author_id);
```

---

### `book_tags`

`confirmed` separates LLM suggestions from accepted tags. Unconfirmed tags are shown in the UI as pending — user accepts or rejects before they take effect.

```sql
CREATE TABLE book_tags (
    book_id    TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    tag_id     TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    confirmed  INTEGER NOT NULL DEFAULT 1,     -- 0 = LLM suggestion, pending user confirmation
    PRIMARY KEY (book_id, tag_id)
);

CREATE INDEX idx_book_tags_tag       ON book_tags(tag_id);
CREATE INDEX idx_book_tags_pending   ON book_tags(confirmed) WHERE confirmed = 0;
```

---

### `formats`

Each book can have multiple file formats (epub + pdf, etc.).

```sql
CREATE TABLE formats (
    id            TEXT PRIMARY KEY,
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    format        TEXT NOT NULL,               -- "EPUB", "PDF", "MOBI", "CBZ"
    path          TEXT NOT NULL,               -- relative to storage root
    size_bytes    INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    last_modified TEXT NOT NULL,
    UNIQUE (book_id, format)
);

CREATE INDEX idx_formats_book   ON formats(book_id);
CREATE INDEX idx_formats_format ON formats(format);
```

---

### `identifiers`

ISBN, Amazon ASIN, Goodreads ID, etc.

```sql
CREATE TABLE identifiers (
    id            TEXT PRIMARY KEY,
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    id_type       TEXT NOT NULL,               -- "isbn", "isbn13", "asin", "goodreads", "uuid"
    value         TEXT NOT NULL,
    last_modified TEXT NOT NULL,
    UNIQUE (book_id, id_type)
);

CREATE INDEX idx_identifiers_book  ON identifiers(book_id);
CREATE INDEX idx_identifiers_value ON identifiers(value);
```

---

### `shelves`

User-created reading lists / collections.

```sql
CREATE TABLE shelves (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    is_public     INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT NOT NULL,
    last_modified TEXT NOT NULL,
    UNIQUE (user_id, name)
);

CREATE INDEX idx_shelves_user ON shelves(user_id);
```

---

### `shelf_books`

```sql
CREATE TABLE shelf_books (
    shelf_id     TEXT NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    book_id      TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    display_order INTEGER NOT NULL DEFAULT 0,
    added_at     TEXT NOT NULL,
    PRIMARY KEY (shelf_id, book_id)
);

CREATE INDEX idx_shelf_books_book ON shelf_books(book_id);
```

---

### `reading_progress`

Stores position per user per book. CFI for epub, page number for PDF.
`last_modified` drives mobile sync — client sends its value, server returns newer record if it exists.

```sql
CREATE TABLE reading_progress (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    format_id     TEXT NOT NULL REFERENCES formats(id) ON DELETE CASCADE,
    cfi           TEXT,                        -- epub Canonical Fragment Identifier
    page          INTEGER,                     -- PDF page number
    percentage    REAL NOT NULL DEFAULT 0.0,   -- 0.0–1.0, derived from CFI/page
    updated_at    TEXT NOT NULL,
    last_modified TEXT NOT NULL,
    UNIQUE (user_id, book_id)
);

CREATE INDEX idx_progress_user ON reading_progress(user_id);
CREATE INDEX idx_progress_book ON reading_progress(book_id);
```

---

### `book_annotations`

Per-user reader annotations for epub highlights, notes, and bookmarks.

```sql
CREATE TABLE book_annotations (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id          TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    type             TEXT NOT NULL CHECK(type IN ('highlight', 'note', 'bookmark')),
    cfi_range        TEXT NOT NULL,
    highlighted_text TEXT,
    note             TEXT,
    color            TEXT NOT NULL DEFAULT 'yellow'
                     CHECK(color IN ('yellow', 'green', 'blue', 'pink')),
    created_at       TEXT NOT NULL,
    updated_at       TEXT NOT NULL
);

CREATE INDEX idx_annotations_user_book ON book_annotations(user_id, book_id);
```

---

### `custom_columns`

Imported from Calibre. Schema varies per library — flagged for manual review during migration.

```sql
CREATE TABLE custom_columns (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,                -- display name
    label        TEXT NOT NULL UNIQUE,         -- internal key e.g. "#read_date"
    column_type  TEXT NOT NULL,                -- "text", "int", "float", "bool", "datetime", "tags"
    is_multiple  INTEGER NOT NULL DEFAULT 0,   -- allows multiple values (like tags)
    created_at   TEXT NOT NULL
);
```

---

### `book_custom_values`

```sql
CREATE TABLE book_custom_values (
    id          TEXT PRIMARY KEY,
    book_id     TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    column_id   TEXT NOT NULL REFERENCES custom_columns(id) ON DELETE CASCADE,
    value_text  TEXT,
    value_int   INTEGER,
    value_float REAL,
    value_bool  INTEGER,
    UNIQUE (book_id, column_id)
);

CREATE INDEX idx_custom_values_book ON book_custom_values(book_id);
```

---

### `book_embeddings`

Vector embeddings for semantic search. SQLite uses `sqlite-vec` BLOB; MariaDB uses `VECTOR` type (11.7+).

```sql
-- SQLite
CREATE TABLE book_embeddings (
    book_id     TEXT PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
    model_id    TEXT NOT NULL,                 -- embedding model that generated this
    embedding   BLOB NOT NULL,                 -- float32 array
    created_at  TEXT NOT NULL
);
```

```sql
-- MariaDB
CREATE TABLE book_embeddings (
    book_id     CHAR(36) PRIMARY KEY,
    model_id    VARCHAR(255) NOT NULL,
    embedding   VECTOR(1536) NOT NULL,         -- dimension matches embedding model output
    created_at  DATETIME NOT NULL,
    FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);
```

---

### `audit_log`

Tracks all metadata changes in the multi-user library. `diff_json` stores before/after values for the changed fields only.

```sql
CREATE TABLE audit_log (
    id         TEXT PRIMARY KEY,
    user_id    TEXT REFERENCES users(id) ON DELETE SET NULL,
    action     TEXT NOT NULL
               CHECK(action IN ('create', 'update', 'delete', 'tag_confirm', 'tag_reject')),
    entity     TEXT NOT NULL
               CHECK(entity IN ('book', 'author', 'tag', 'series', 'shelf', 'user')),
    entity_id  TEXT NOT NULL,
    diff_json  TEXT,                           -- {"field": {"before": x, "after": y}}
    created_at TEXT NOT NULL
);

CREATE INDEX idx_audit_user      ON audit_log(user_id);
CREATE INDEX idx_audit_entity    ON audit_log(entity, entity_id);
CREATE INDEX idx_audit_created   ON audit_log(created_at);
```

---

### `llm_jobs`

Background job queue for long-running LLM tasks.

```sql
CREATE TABLE llm_jobs (
    id           TEXT PRIMARY KEY,
    job_type     TEXT NOT NULL
                 CHECK(job_type IN (
                     'classify', 'semantic_index', 'quality_check',
                     'validate_metadata', 'organize', 'derive'
                 )),
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK(status IN ('pending', 'running', 'completed', 'failed')),
    book_id      TEXT REFERENCES books(id) ON DELETE CASCADE,  -- NULL for library-wide jobs
    payload_json TEXT,                         -- job input parameters
    result_json  TEXT,                         -- job output
    error_text   TEXT,                         -- populated on failure
    created_at   TEXT NOT NULL,
    started_at   TEXT,
    completed_at TEXT
);

CREATE INDEX idx_llm_jobs_status  ON llm_jobs(status);
CREATE INDEX idx_llm_jobs_book    ON llm_jobs(book_id);
CREATE INDEX idx_llm_jobs_type    ON llm_jobs(job_type);
```

---

### `llm_eval_results`

Stores prompt eval results per model per prompt version.

```sql
CREATE TABLE llm_eval_results (
    id            TEXT PRIMARY KEY,
    fixture_name  TEXT NOT NULL,
    model_id      TEXT NOT NULL,               -- exact string from /v1/models
    prompt_hash   TEXT NOT NULL,               -- SHA256 of system prompt text
    role          TEXT NOT NULL,               -- "librarian" | "architect"
    passed        INTEGER NOT NULL,            -- overall pass/fail
    results_json  TEXT NOT NULL,               -- per-evaluator detail
    latency_ms    INTEGER NOT NULL,
    run_at        TEXT NOT NULL
);

CREATE INDEX idx_eval_fixture ON llm_eval_results(fixture_name);
CREATE INDEX idx_eval_model   ON llm_eval_results(model_id);
CREATE INDEX idx_eval_hash    ON llm_eval_results(prompt_hash);
```

---

### `migration_log`

One row per `xs-migrate` run.

```sql
CREATE TABLE migration_log (
    id               TEXT PRIMARY KEY,
    source_path      TEXT NOT NULL,            -- path to Calibre DB
    status           TEXT NOT NULL DEFAULT 'pending'
                     CHECK(status IN ('pending', 'running', 'completed', 'failed')),
    records_total    INTEGER NOT NULL DEFAULT 0,
    records_imported INTEGER NOT NULL DEFAULT 0,
    records_failed   INTEGER NOT NULL DEFAULT 0,
    records_skipped  INTEGER NOT NULL DEFAULT 0,
    started_at       TEXT NOT NULL,
    completed_at     TEXT,
    log_json         TEXT                      -- per-record error detail
);
```

---

### `libraries`

Multi-library support. A `default` library row is seeded at migration time.

```sql
CREATE TABLE libraries (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    calibre_db_path TEXT NOT NULL,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

INSERT INTO libraries (id, name, calibre_db_path, created_at, updated_at)
VALUES ('default', 'Default Library', '', datetime('now'), datetime('now'));
```

---

### `api_tokens`

Long-lived tokens for MCP server and Kobo device authentication. SHA256-hashed at rest.

```sql
CREATE TABLE api_tokens (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,
    token_hash   TEXT NOT NULL UNIQUE,          -- SHA256 of the raw token
    created_by   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   TEXT NOT NULL,
    last_used_at TEXT
);

CREATE INDEX idx_api_tokens_created_by ON api_tokens(created_by);
CREATE INDEX idx_api_tokens_hash       ON api_tokens(token_hash);
```

---

### `email_settings`

Singleton row (always `id = 'singleton'`) for admin-configurable SMTP settings.

```sql
CREATE TABLE email_settings (
    id            TEXT PRIMARY KEY DEFAULT 'singleton',
    smtp_host     TEXT NOT NULL DEFAULT '',
    smtp_port     INTEGER NOT NULL DEFAULT 587,
    smtp_user     TEXT NOT NULL DEFAULT '',
    smtp_password TEXT NOT NULL DEFAULT '',
    from_address  TEXT NOT NULL DEFAULT '',
    use_tls       INTEGER NOT NULL DEFAULT 1,
    updated_at    TEXT NOT NULL
);
```

> **Note:** `smtp_host` is admin-configurable at runtime with no host-validation guard. Acceptable for single-admin self-hosted deployments; add validation if multi-admin roles are ever introduced.

---

### `oauth_accounts`

Links a local user to one or more OAuth provider identities. Never auto-links by email — always requires an explicit OAuth login to create the mapping.

```sql
CREATE TABLE oauth_accounts (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider         TEXT NOT NULL,              -- "google" | "github"
    provider_user_id TEXT NOT NULL,
    email            TEXT NOT NULL,
    created_at       TEXT NOT NULL,
    UNIQUE(provider, provider_user_id)
);

CREATE INDEX idx_oauth_accounts_user_id ON oauth_accounts(user_id);
```

---

### `kobo_devices`

Registered Kobo e-readers. Each device authenticates via a long-lived API token embedded in the URL path.

```sql
CREATE TABLE kobo_devices (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id    TEXT NOT NULL UNIQUE,
    device_name  TEXT NOT NULL DEFAULT 'Kobo',
    sync_token   TEXT,                           -- delta sync cursor; cleared on device reassignment
    last_sync_at TEXT,
    created_at   TEXT NOT NULL
);

CREATE INDEX idx_kobo_devices_user_id   ON kobo_devices(user_id);
CREATE INDEX idx_kobo_devices_device_id ON kobo_devices(device_id);
```

---

### `kobo_reading_state`

Reading position pushed from a Kobo device. `percent_read` is synced to `reading_progress.percentage` — `format_id` on the canonical progress record is never overwritten by a Kobo sync.

```sql
CREATE TABLE kobo_reading_state (
    id            TEXT PRIMARY KEY,
    device_id     TEXT NOT NULL REFERENCES kobo_devices(id) ON DELETE CASCADE,
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    kobo_position TEXT,                          -- opaque Kobo position string
    percent_read  REAL,
    last_modified TEXT NOT NULL,
    UNIQUE(device_id, book_id)
);
```

---

### `scheduled_tasks`

Cron-scheduled background jobs. The scheduler runs inside the Axum process and polls `next_run_at` on startup.

```sql
CREATE TABLE scheduled_tasks (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    task_type   TEXT NOT NULL
                CHECK(task_type IN ('classify_all', 'semantic_index_all', 'backup')),
    cron_expr   TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0, 1)),
    last_run_at TEXT,
    next_run_at TEXT,
    created_at  TEXT NOT NULL
);

CREATE INDEX idx_scheduled_tasks_due ON scheduled_tasks(enabled, next_run_at);
```

---

### `totp_backup_codes` (migration 0014)

```sql
-- users table gains two new columns:
ALTER TABLE users ADD COLUMN totp_secret TEXT;         -- NULL = TOTP disabled; AES-256-GCM encrypted at rest
ALTER TABLE users ADD COLUMN totp_enabled INTEGER NOT NULL DEFAULT 0;

CREATE TABLE totp_backup_codes (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    code_hash   TEXT NOT NULL,   -- SHA-256 of the 8-char code
    used_at     TEXT,            -- NULL = unused
    created_at  TEXT NOT NULL
);

CREATE INDEX idx_totp_backup_user ON totp_backup_codes(user_id);
```

- `totp_secret` is encrypted with AES-256-GCM; key derived from `jwt_secret` via HKDF — never stored in plaintext
- 8 single-use backup codes generated at setup; each stored as a SHA-256 hash
- `totp_pending` JWT issued after password check when TOTP is enabled — cannot access other routes until `POST /auth/totp/verify` succeeds

---

### `goodreads_import_log` (migration 0016)

Tracks Goodreads / StoryGraph CSV import runs per user.

```sql
CREATE TABLE goodreads_import_log (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    source       TEXT NOT NULL DEFAULT 'goodreads'
                 CHECK(source IN ('goodreads', 'storygraph')),
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK(status IN ('pending', 'running', 'complete', 'failed')),
    total_rows   INTEGER,
    matched      INTEGER NOT NULL DEFAULT 0,
    unmatched    INTEGER NOT NULL DEFAULT 0,
    errors       TEXT,
    created_at   TEXT NOT NULL,
    completed_at TEXT
);
```

---

### `webhooks` + `webhook_deliveries` (migration 0018)

Outbound webhooks with HMAC-signed payloads and retry tracking.

```sql
CREATE TABLE webhooks (
    id               TEXT PRIMARY KEY,
    user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url              TEXT NOT NULL,
    secret           TEXT NOT NULL,         -- HMAC-SHA256 key; derived via HKDF
    events           TEXT NOT NULL,         -- JSON array of event names
    enabled          INTEGER NOT NULL DEFAULT 1,
    last_delivery_at TEXT,
    last_error       TEXT,
    created_at       TEXT NOT NULL
);

CREATE INDEX idx_webhooks_user ON webhooks(user_id);

CREATE TABLE webhook_deliveries (
    id              TEXT PRIMARY KEY,
    webhook_id      TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event           TEXT NOT NULL,
    payload         TEXT NOT NULL,
    status          TEXT NOT NULL DEFAULT 'pending'
                    CHECK(status IN ('pending', 'delivered', 'failed')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    response_status INTEGER,
    created_at      TEXT NOT NULL,
    delivered_at    TEXT
);

CREATE INDEX idx_webhook_deliveries_pending ON webhook_deliveries(status, next_attempt_at)
    WHERE status = 'pending';
```

> **Note:** `url` is validated against SSRF blocklist at webhook creation time. Payload size capped at 1 MB at enqueue.

---

### `book_chunks` (migration 0019)

Sub-chapter chunks produced by the Phase 15.1 chunker. Each chunk carries a heading path for citation and an optional embedding for vector search.

```sql
CREATE TABLE book_chunks (
    id            TEXT PRIMARY KEY,
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chunk_index   INTEGER NOT NULL,
    chapter_index INTEGER NOT NULL,
    heading_path  TEXT,                    -- e.g. "Admin Guide > Part III > §12.3"
    chunk_type    TEXT NOT NULL DEFAULT 'text'
                    CHECK(chunk_type IN ('text', 'procedure', 'reference',
                                         'concept', 'example', 'image')),
    text          TEXT NOT NULL,
    word_count    INTEGER NOT NULL,
    has_image     INTEGER NOT NULL DEFAULT 0,
    embedding     BLOB,                    -- float32 vector (sqlite-vec)
    created_at    TEXT NOT NULL
);

CREATE INDEX idx_book_chunks_book ON book_chunks(book_id, chunk_index);
CREATE INDEX idx_book_chunks_type ON book_chunks(book_id, chunk_type);
CREATE INDEX idx_book_chunks_created_at ON book_chunks(created_at);
```

A companion FTS5 virtual table `book_chunks_fts` (migration 0021) indexes `text` and `heading_path` with sync triggers for BM25 search.

---

### `collections` + `collection_books` (migration 0020)

Groups related books for cross-document synthesis queries (Phase 15.3).

```sql
CREATE TABLE collections (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT,
    domain      TEXT NOT NULL DEFAULT 'technical'
                  CHECK(domain IN ('technical','electronics','culinary',
                                   'legal','academic','narrative')),
    owner_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    is_public   INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

CREATE TABLE collection_books (
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    added_at      TEXT NOT NULL,
    PRIMARY KEY (collection_id, book_id)
);

CREATE INDEX idx_collections_owner_id    ON collections(owner_id);
CREATE INDEX idx_collection_books_collection ON collection_books(collection_id);
CREATE INDEX idx_collection_books_book       ON collection_books(book_id);
```

`domain` is a hint to the chunker's boundary detection strategy — see ARCHITECTURE.md for per-domain chunking rules.

---

### `sessions` (migration 0024)

Explicit session records used by the Phase 17 `AuthKind` refactor for typed session tracking alongside refresh tokens.

```sql
CREATE TABLE sessions (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    session_type TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    expires_at   TEXT NOT NULL,
    created_at   TEXT NOT NULL
);

CREATE INDEX idx_sessions_user ON sessions(user_id);
CREATE INDEX idx_sessions_type ON sessions(session_type);
CREATE INDEX idx_sessions_hash ON sessions(token_hash);
```

---

### `api_tokens` — extended columns (migrations 0025–0026)

Two columns added to `api_tokens` post-Phase 17:

```sql
ALTER TABLE api_tokens ADD COLUMN expires_at INTEGER;   -- NULL = no expiry
ALTER TABLE api_tokens ADD COLUMN scope TEXT NOT NULL DEFAULT 'write';
-- scope values: 'read' | 'write' | 'admin'
```

Scope is enforced at the middleware layer — `read` tokens may not call mutating routes; `admin` tokens required for admin-only routes.

---

## SQLite vs MariaDB Notes

| Concern | SQLite | MariaDB |
|---|---|---|
| UUID storage | `TEXT` | `CHAR(36)` or `BINARY(16)` |
| Timestamps | `TEXT` (ISO8601) | `DATETIME` |
| Booleans | `INTEGER` (0/1) | `TINYINT(1)` |
| ENUMs | `TEXT + CHECK` | `ENUM(...)` or `TEXT + CHECK` |
| Partial indexes (`WHERE`) | ✅ Supported | ❌ Not supported — drop the `WHERE` clause |
| `RETURNING` clause | ✅ SQLite 3.35+ | ✅ MariaDB 10.5+ |
| Full-text search | FTS5 virtual tables | `FULLTEXT` index |
| Concurrent writes | WAL mode (adequate) | Native MVCC |
| Vector storage | `sqlite-vec` BLOB + HNSW index | `VECTOR(n)` type (MariaDB 11.7+) |

Migration files live in separate directories:
```
backend/migrations/
├── sqlite/
│   └── 0001_initial.sql
└── mariadb/
    └── 0001_initial.sql
```

The Rust `sqlx` query macros check against whichever `DATABASE_URL` is set at compile time.

---

## Indexes Summary

All foreign keys are indexed. Additional indexes on high-query-frequency columns:

| Table | Column | Reason |
|---|---|---|
| `books` | `sort_title` | Library grid default sort |
| `books` | `series_id` | Series browsing |
| `books` | `pubdate` | Date-based filtering |
| `books` | `indexed_at` | Meilisearch reindex queue (`WHERE indexed_at IS NULL OR indexed_at < last_modified`) |
| `authors` | `sort_name` | Author browser sort |
| `book_tags` | `confirmed = 0` | Pending LLM suggestion badge (SQLite partial index) |
| `identifiers` | `value` | ISBN duplicate detection on ingest |
| `reading_progress` | `user_id` | Per-user progress queries |
| `book_annotations` | `user_id, book_id` | Reader annotation queries in epub view |
| `llm_jobs` | `status` | Job queue polling |
| `refresh_tokens` | `token_hash` | Auth token lookup on every request |
| `api_tokens` | `token_hash` | MCP / Kobo token lookup |
| `oauth_accounts` | `user_id` | OAuth provider lookup per user |
| `kobo_devices` | `device_id` | Device lookup on every Kobo sync request |
| `books` | `library_id` | Per-library browse and filtering |
| `scheduled_tasks` | `enabled, next_run_at` | Scheduler poll — find due tasks |
| `totp_backup_codes` | `user_id` | Backup code lookup per user |
| `audit_log` | `entity, entity_id` | History view per book/author/etc. |
| `audit_log` | `created_at` | Chronological admin audit feed |

---

## Cover Storage Convention

Covers stored under a bucketed path relative to the storage root:

```
covers/
├── ab/
│   └── abcd1234-ef56-....jpg
├── cd/
│   └── cdef5678-ab12-....jpg
```

- First 2 characters of the book UUID form the bucket directory
- Prevents large flat directories on FAT32 / older NAS filesystems
- `books.cover_path` stores the relative path: `covers/ab/abcd1234-....jpg`
- Thumbnails stored alongside: `covers/ab/abcd1234-....thumb.jpg`

---

## Table Count

The schema has **41 tables** across 26 migration files:

| # | Table | Migration |
|---|---|---|
| 1 | `roles` | 0001 |
| 2 | `users` | 0001 |
| 3 | `refresh_tokens` | 0001 |
| 4 | `authors` | 0001 |
| 5 | `series` | 0001 |
| 6 | `tags` | 0001 |
| 7 | `books` | 0001 |
| 8 | `book_authors` | 0001 |
| 9 | `book_tags` | 0001 |
| 10 | `formats` | 0001 |
| 11 | `identifiers` | 0001 |
| 12 | `shelves` | 0001 |
| 13 | `shelf_books` | 0001 |
| 14 | `reading_progress` | 0001 |
| 15 | `custom_columns` | 0001 |
| 16 | `book_custom_values` | 0001 |
| 17 | `llm_jobs` | 0001 |
| 18 | `llm_eval_results` | 0001 |
| 19 | `migration_log` | 0001 |
| 20 | `audit_log` | 0001 |
| 21 | `book_embeddings` | 0001 |
| 22 | `api_tokens` | 0005 |
| 23 | `email_settings` | 0006 |
| 24 | `oauth_accounts` | 0007 |
| 25 | `kobo_devices` | 0008 |
| 26 | `kobo_reading_state` | 0008 |
| 27 | `libraries` | 0009 |
| 28 | `book_user_state` | 0010 |
| 29 | `download_history` | 0011 |
| 30 | `user_tag_restrictions` | 0012 |
| 31 | `scheduled_tasks` | 0013 |
| 32 | `totp_backup_codes` | 0014 |
| 33 | `book_annotations` | 0015 |
| 34 | `goodreads_import_log` | 0016 |
| 35 | `author_profiles` | 0017 |
| 36 | `webhooks` | 0018 |
| 37 | `webhook_deliveries` | 0018 |
| 38 | `book_chunks` | 0019 |
| 39 | `collections` | 0020 |
| 40 | `collection_books` | 0020 |
| 41 | `sessions` | 0024 |

## Open Schema Questions

All resolved. No open items.
