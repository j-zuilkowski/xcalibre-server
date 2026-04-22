# calibre-web Rewrite — Database Schema

_Status: Draft_
_Last updated: 2026-04-20_

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
users ──────────── roles
  │
  ├── refresh_tokens
  ├── reading_progress ── books ── book_authors ── authors
  └── shelves                │
        └── shelf_books      ├── book_tags ─── tags
                             ├── book_user_state ─ users
                             ├── download_history ─ users
                             ├── formats
                             ├── identifiers
                             ├── series
                             └── custom_column_values ── custom_columns

llm_jobs ──────────────────── books (nullable)
llm_eval_results
migration_log
audit_log ─────────────────── users (nullable)
book_embeddings ────────────── books
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
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    email           TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,              -- argon2
    role_id         TEXT NOT NULL REFERENCES roles(id),
    is_active       INTEGER NOT NULL DEFAULT 1,
    force_pw_reset  INTEGER NOT NULL DEFAULT 0, -- set on migration import
    created_at      TEXT NOT NULL,
    last_modified   TEXT NOT NULL
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
                  CHECK(document_type IN ('novel', 'textbook', 'reference', 'magazine', 'datasheet', 'comic', 'unknown')),
    flags         TEXT,                        -- JSON: arbitrary feature flags
    indexed_at    TEXT,                        -- NULL or < last_modified = needs Meilisearch reindex
    created_at    TEXT NOT NULL,
    last_modified TEXT NOT NULL
);

CREATE INDEX idx_books_sort_title  ON books(sort_title);
CREATE INDEX idx_books_series      ON books(series_id);
CREATE INDEX idx_books_pubdate     ON books(pubdate);
CREATE INDEX idx_books_language    ON books(language);
CREATE INDEX idx_books_indexed_at  ON books(indexed_at);
```

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

One row per `autolibre-migrate` run.

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
| `llm_jobs` | `status` | Job queue polling |
| `refresh_tokens` | `token_hash` | Auth token lookup on every request |
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

## Open Schema Questions

All resolved. No open items.
