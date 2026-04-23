# calibre-web Rewrite — API Contract

_Status: Current_
_Last updated: 2026-04-22_

---

## Design Principles

- All endpoints return `application/json`
- All list endpoints are paginated — no unbounded responses
- All timestamps in responses are ISO8601 UTC strings
- Auth via JWT in `Authorization: Bearer <token>` header (web) or httpOnly cookie (browser SPA)
- Errors follow a consistent shape (see Error Responses)
- `last_modified` included on all mutable resource responses — drives mobile sync
- LLM routes return `503` with graceful message when LLM is disabled or unreachable
- Role enforcement is server-side — frontend gates are UX only

---

## Base URL

```
http://{host}:{port}/api/v1
```

---

## Error Response Shape

All errors return this shape:

```typescript
interface ApiError {
  error: string        // machine-readable code e.g. "not_found", "unauthorized"
  message: string      // human-readable description
  details?: unknown    // optional field-level validation errors
}
```

| HTTP Status | `error` code | When |
|---|---|---|
| 400 | `bad_request` | Malformed request body or query params |
| 401 | `unauthorized` | Missing or expired token |
| 403 | `forbidden` | Valid token but insufficient role/permission |
| 404 | `not_found` | Resource does not exist |
| 409 | `conflict` | Duplicate (ISBN, username, etc.) |
| 413 | `payload_too_large` | Upload exceeds configured size limit |
| 422 | `unprocessable` | Valid JSON but fails validation |
| 422 | `no_extractable_format` | Book has no EPUB or PDF format available for text extraction |
| 503 | `llm_unavailable` | LLM feature requested but disabled or unreachable |
| 500 | `internal_error` | Unexpected server error |

---

## Shared Types

Defined in `packages/shared/types/`. Used by both web and mobile.

```typescript
interface PaginatedResponse<T> {
  items: T[]
  total: number
  page: number
  page_size: number
}

type DocumentType = 'novel' | 'textbook' | 'reference' | 'magazine' | 'datasheet' | 'comic' | 'unknown'

interface Book {
  id: string
  title: string
  sort_title: string
  description: string | null
  pubdate: string | null
  language: string | null
  rating: number | null           // 0–10
  document_type: DocumentType     // set at ingest; 'unknown' until classified
  series: SeriesRef | null
  series_index: number | null
  authors: AuthorRef[]
  tags: TagRef[]
  formats: FormatRef[]
  cover_url: string | null
  has_cover: boolean
  identifiers: Identifier[]
  created_at: string
  last_modified: string
  indexed_at: string | null
}

interface BookSummary {           // used in lists — subset of Book
  id: string
  title: string
  sort_title: string
  authors: AuthorRef[]
  series: SeriesRef | null
  series_index: number | null
  cover_url: string | null
  has_cover: boolean
  language: string | null
  rating: number | null
  document_type: DocumentType
  last_modified: string
}

interface AuthorRef { id: string; name: string; sort_name: string }
interface SeriesRef { id: string; name: string }
interface TagRef    { id: string; name: string; confirmed: boolean }
interface FormatRef { id: string; format: string; size_bytes: number }

interface Identifier {
  id: string
  id_type: string               // "isbn", "isbn13", "asin", "goodreads"
  value: string
}

interface Author {
  id: string
  name: string
  sort_name: string
  book_count: number
  last_modified: string
}

interface Series {
  id: string
  name: string
  sort_name: string
  book_count: number
  last_modified: string
}

interface Tag {
  id: string
  name: string
  source: 'manual' | 'llm' | 'calibre_import'
  book_count: number
  last_modified: string
}

interface ReadingProgress {
  id: string
  book_id: string
  format_id: string
  cfi: string | null
  page: number | null
  percentage: number            // 0.0–1.0
  updated_at: string
  last_modified: string
}

interface Shelf {
  id: string
  name: string
  is_public: boolean
  book_count: number
  created_at: string
  last_modified: string
}

interface User {
  id: string
  username: string
  email: string
  role: RoleRef
  is_active: boolean
  force_pw_reset: boolean
  created_at: string
  last_modified: string
}

interface RoleRef { id: string; name: string }

interface Role {
  id: string
  name: string
  can_upload: boolean
  can_bulk: boolean
  can_edit: boolean
  can_download: boolean
  last_modified: string
}
```

---

## Routes

### Auth

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| POST | `/auth/register` | No | — | Create first admin account (disabled after first user exists) |
| POST | `/auth/login` | No | — | Login, receive access + refresh tokens |
| POST | `/auth/logout` | Yes | Any | Revoke refresh token |
| POST | `/auth/refresh` | No | — | Exchange refresh token for new access token |
| GET | `/auth/me` | Yes | Any | Current user profile |
| PATCH | `/auth/me/password` | Yes | Any | Change own password |
| GET | `/auth/oauth/:provider` | No | — | Initiate OAuth flow (`google` or `github`) |
| GET | `/auth/oauth/:provider/callback` | No | — | OAuth callback; creates local user on first login |

#### `POST /auth/login`
```typescript
// Request
{ username: string; password: string }

// Response 200 (TOTP not enabled)
{
  access_token: string          // JWT, 15 min TTL
  refresh_token: string         // opaque, 30 day TTL
  user: User
}

// Response 200 (TOTP enabled)
{
  totp_required: true
  totp_token: string            // short-lived "totp_pending" JWT — use with /auth/totp/verify
}
```

#### `POST /auth/totp/verify`
```typescript
// Request (use totp_token from login response as Bearer token)
{ code: string }               // 6-digit TOTP code or 8-char backup code

// Response 200
{ access_token: string; refresh_token: string; user: User }
```

#### `POST /auth/totp/setup`
```typescript
// Response 200
{ otpauth_uri: string; backup_codes: string[] }  // render QR client-side
```

#### `POST /auth/totp/confirm`
```typescript
// Request — confirm setup by providing first valid code
{ code: string }

// Response 200
{ ok: true }
```

#### `DELETE /auth/totp`
Disables TOTP for the authenticated user. Requires valid TOTP code or backup code as `X-TOTP-Code` header.

#### `POST /auth/refresh`
```typescript
// Request
{ refresh_token: string }

// Response 200
{ access_token: string; refresh_token: string }
```

---

### Books

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/books` | Yes | Any | List books (paginated, filterable, sortable) |
| GET | `/books/:id` | Yes | Any | Single book detail |
| POST | `/books` | Yes | `can_upload` | Upload single book file |
| PATCH | `/books/:id` | Yes | `can_edit` | Update book metadata |
| DELETE | `/books/:id` | Yes | Admin | Delete book + all formats |
| GET | `/books/:id/cover` | Yes | Any | Serve cover image |
| POST | `/books/:id/cover` | Yes | `can_edit` | Upload or replace cover |
| GET | `/books/:id/formats/:format/download` | Yes | `can_download` | Download file |
| GET | `/books/:id/formats/:format/stream` | Yes | `can_download` | Stream file (range requests) |
| DELETE | `/books/:id/formats/:format` | Yes | Admin | Remove a specific format |
| GET | `/books/:id/progress` | Yes | Any | Reading progress for current user |
| PUT | `/books/:id/progress` | Yes | Any | Upsert reading progress |
| GET | `/books/:id/history` | Yes | Any | Audit log for this book |

#### `GET /books`
```typescript
// Query params
{
  q?: string                    // text search (title, author, tag)
  author_id?: string
  series_id?: string
  tag?: string[]                // multiple allowed
  language?: string
  format?: string
  rating_min?: number
  sort?: 'title' | 'author' | 'pubdate' | 'added' | 'rating'
  order?: 'asc' | 'desc'
  page?: number                 // default 1
  page_size?: number            // default 30, max 100
}

// Response 200
PaginatedResponse<BookSummary>
```

#### `POST /books` (single upload)
```typescript
// Request: multipart/form-data
{
  file: File                    // epub, pdf, mobi, cbz, etc.
  metadata?: string             // optional JSON override — title, author, etc.
}

// Response 201
Book
```

#### `PATCH /books/:id`
```typescript
// Request — all fields optional
{
  title?: string
  sort_title?: string
  description?: string
  pubdate?: string
  language?: string
  rating?: number
  series_id?: string | null
  series_index?: number | null
  authors?: string[]            // author IDs — replaces existing
  identifiers?: { id_type: string; value: string }[]
}

// Response 200
Book
```

#### `PUT /books/:id/progress`
```typescript
// Request
{
  format_id: string
  cfi?: string
  page?: number
  percentage: number
}

// Response 200
ReadingProgress
```

---

### Authors

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/authors` | Yes | Any | List authors (paginated) |
| GET | `/authors/:id` | Yes | Any | Author detail |
| GET | `/authors/:id/books` | Yes | Any | Books by author (paginated) |
| PATCH | `/authors/:id` | Yes | `can_edit` | Update author name/sort |
| DELETE | `/authors/:id` | Yes | Admin | Delete author (only if no books) |

#### `GET /authors`
```typescript
// Query params
{ q?: string; sort?: 'name' | 'book_count'; order?: 'asc' | 'desc'; page?: number; page_size?: number }

// Response 200
PaginatedResponse<Author>
```

---

### Series

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/series` | Yes | Any | List series (paginated) |
| GET | `/series/:id` | Yes | Any | Series detail |
| GET | `/series/:id/books` | Yes | Any | Books in series (ordered by index) |
| PATCH | `/series/:id` | Yes | `can_edit` | Update series name/sort |
| DELETE | `/series/:id` | Yes | Admin | Delete series (only if no books) |

---

### Tags

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/tags` | Yes | Any | List all tags |
| GET | `/tags/:id/books` | Yes | Any | Books with this tag |
| DELETE | `/tags/:id` | Yes | Admin | Delete tag (removes from all books) |

---

### Search

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/search` | Yes | Any | Full-text search (Meilisearch → FTS5 fallback) |
| GET | `/search/semantic` | Yes | Any | Semantic search (LLM embeddings) — 503 if unavailable |

#### `GET /search`
```typescript
// Query params
{ q: string; page?: number; page_size?: number }

// Response 200
{
  items: BookSummary[]
  total: number
  page: number
  page_size: number
  engine: 'meilisearch' | 'fts5'   // which engine served the query
}
```

#### `GET /search/semantic`
```typescript
// Query params
{ q: string; limit?: number }       // default limit 10

// Response 200
{
  items: Array<BookSummary & { score: number }>
  model_id: string                  // which embedding model was used
}

// Response 503
ApiError                            // llm_unavailable
```

---

### Shelves

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/shelves` | Yes | Any | List own shelves (+ public shelves) |
| POST | `/shelves` | Yes | Any | Create shelf |
| GET | `/shelves/:id` | Yes | Any | Shelf detail + books |
| PATCH | `/shelves/:id` | Yes | Owner/Admin | Rename, toggle public |
| DELETE | `/shelves/:id` | Yes | Owner/Admin | Delete shelf |
| POST | `/shelves/:id/books` | Yes | Owner | Add book to shelf |
| DELETE | `/shelves/:id/books/:book_id` | Yes | Owner | Remove book from shelf |
| PATCH | `/shelves/:id/books/reorder` | Yes | Owner | Reorder books on shelf |

#### `GET /shelves/:id`
```typescript
// Response 200
Shelf & { books: BookSummary[] }
```

---

### LLM

All LLM routes return `503` when `llm.enabled = false` or the endpoint is unreachable.

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/llm/health` | Yes | Any | LLM availability check |
| GET | `/books/:id/classify` | Yes | Any | Classify book — returns tag suggestions |
| POST | `/books/:id/tags/confirm` | Yes | `can_edit` | Confirm or reject LLM tag suggestions |
| POST | `/books/:id/tags/confirm-all` | Yes | `can_edit` | Confirm all pending tag suggestions |
| GET | `/books/:id/validate` | Yes | Any | Metadata validation report |
| GET | `/books/:id/quality` | Yes | Any | Content quality check |
| GET | `/books/:id/derive` | Yes | Any | Derived works — summary, related, questions |
| POST | `/organize` | Yes | Admin | Queue library organization job |

#### `GET /llm/health`
```typescript
// Response 200
{
  enabled: boolean
  librarian: { available: boolean; model_id: string | null; endpoint: string }
  architect: { available: boolean; model_id: string | null; endpoint: string }
}
```

#### `GET /books/:id/classify`
```typescript
// Response 200
{
  book_id: string
  suggestions: Array<{ name: string; confidence: number }>  // confidence 0.0–1.0
  model_id: string
  pending_count: number         // how many unconfirmed tags already exist
}

// Response 503 — ApiError
```

#### `POST /books/:id/tags/confirm`
```typescript
// Request
{
  confirm: string[]             // tag names to confirm
  reject: string[]              // tag names to reject (removes from book_tags)
}

// Response 200
Book                            // updated book with confirmed tags
```

#### `GET /books/:id/validate`
```typescript
// Response 200
{
  book_id: string
  severity: 'ok' | 'warning' | 'error'
  issues: Array<{
    field: string
    severity: 'warning' | 'error'
    message: string
    suggestion: string | null
  }>
  model_id: string
}
```

#### `GET /books/:id/derive`
```typescript
// Response 200
{
  book_id: string
  summary: string
  related_titles: string[]
  discussion_questions: string[]
  model_id: string
}
```

---

### Admin — Users

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/admin/users` | Yes | Admin | List all users |
| POST | `/admin/users` | Yes | Admin | Create user |
| GET | `/admin/users/:id` | Yes | Admin | User detail |
| PATCH | `/admin/users/:id` | Yes | Admin | Update user (role, active, force_pw_reset) |
| DELETE | `/admin/users/:id` | Yes | Admin | Delete user |
| POST | `/admin/users/:id/reset-password` | Yes | Admin | Force password reset flag |
| DELETE | `/admin/users/:id/totp` | Yes | Admin | Disable TOTP for user (lockout recovery) |

---

### Admin — Roles

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/admin/roles` | Yes | Admin | List roles |
| PATCH | `/admin/roles/:id` | Yes | Admin | Update role permissions |

#### `PATCH /admin/roles/:id`
```typescript
// Request — all optional
{
  can_upload?: boolean
  can_bulk?: boolean
  can_edit?: boolean
  can_download?: boolean
}

// Response 200
Role
```

---

### Admin — Import

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| POST | `/admin/import/bulk` | Yes | Admin | Bulk import from zip or server path |
| GET | `/admin/import/:id` | Yes | Admin | Import job status |

#### `POST /admin/import/bulk`
```typescript
// Request: multipart/form-data OR JSON
{
  source: 'upload' | 'path'
  path?: string                 // server-side folder path (source = 'path')
  file?: File                   // zip archive (source = 'upload')
  dry_run?: boolean             // default false
}

// Response 202 Accepted
{ job_id: string }              // poll GET /admin/import/:id for status
```

#### `GET /admin/import/:id`
```typescript
// Response 200
{
  id: string
  status: 'pending' | 'running' | 'completed' | 'failed'
  dry_run: boolean
  records_total: number
  records_imported: number
  records_failed: number
  records_skipped: number
  failures: Array<{ file: string; reason: string }>
  started_at: string
  completed_at: string | null
}
```

---

### Admin — Migration

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| POST | `/admin/migrate` | Yes | Admin | Run `autolibre-migrate` |
| GET | `/admin/migrate/:id` | Yes | Admin | Migration run status |
| GET | `/admin/migrate` | Yes | Admin | Migration history |

#### `POST /admin/migrate`
```typescript
// Request
{
  source_path: string           // path to Calibre metadata.db
  dry_run?: boolean             // default false
}

// Response 202 Accepted
{ job_id: string }
```

---

### Content API (Agentic RAG Surface)

These routes expose book content as plain text. They have **no LLM dependency** — they work regardless of `llm.enabled` and never return 503. Designed to be consumed as agent tools by external frameworks (LangGraph, smolagents, MCP clients).

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/books/:id/chapters` | Yes | Any | List chapters with titles and word counts |
| GET | `/books/:id/text` | Yes | Any | Extract plain text — full book or single chapter |

#### `GET /books/:id/chapters`
```typescript
// Query params: none

// Response 200
{
  book_id: string
  format: string                  // "EPUB" or "PDF" — whichever was used
  chapters: Array<{
    index: number                 // 0-based spine position
    title: string                 // chapter title from OPF or "Pages N–M" for PDF
    word_count: number
  }>
}

// Response 404 — book not found
// Response 422 — { error: "no_extractable_format" } — no EPUB or PDF format on this book
```

#### `GET /books/:id/text`
```typescript
// Query params
{ chapter?: number }              // omit for full book; 0-based index matching /chapters

// Response 200
{
  book_id: string
  format: string
  chapter: number | null          // null when full book was requested
  text: string                    // plain text, whitespace-normalized
  word_count: number
}

// Response 404 — book not found
// Response 422 — { error: "no_extractable_format" }
```

---

### Admin — Jobs

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/admin/jobs` | Yes | Admin | List LLM jobs (filterable by status/type) |
| GET | `/admin/jobs/:id` | Yes | Admin | Job detail |
| DELETE | `/admin/jobs/:id` | Yes | Admin | Cancel pending job |

#### `GET /admin/jobs`
```typescript
// Query params
{ status?: string; job_type?: string; page?: number; page_size?: number }

// Response 200
PaginatedResponse<{
  id: string
  job_type: string
  status: string
  book_id: string | null
  book_title: string | null       // denormalized for display
  created_at: string
  started_at: string | null
  completed_at: string | null
  error_text: string | null
}>
```

---

### Admin — Prompt Evals

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/admin/evals/fixtures` | Yes | Admin | List available eval fixtures |
| POST | `/admin/evals/run` | Yes | Admin | Run eval suite |
| GET | `/admin/evals/results` | Yes | Admin | Eval result history |
| GET | `/admin/evals/matrix` | Yes | Admin | Model × fixture pass/fail matrix |
| POST | `/admin/evals/promote` | Yes | Admin | Promote prompt version to active |

#### `POST /admin/evals/run`
```typescript
// Request
{
  role?: 'librarian' | 'architect'   // omit to run all
  fixture?: string                    // omit to run all fixtures for role
  model_override?: string             // override configured model for this run
}

// Response 202 Accepted
{ job_id: string }
```

#### `GET /admin/evals/matrix`
```typescript
// Response 200
{
  fixtures: string[]
  models: string[]
  results: Array<{
    fixture: string
    model: string
    passed: boolean | null          // null = never run
    prompt_hash: string | null
    run_at: string | null
    latency_ms: number | null
  }>
}
```

---

### Admin — System

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/health` | No | — | Liveness check (no auth required) |
| GET | `/admin/system` | Yes | Admin | System stats |
| GET | `/admin/audit` | Yes | Admin | Audit log (paginated) |

#### `GET /health`
```typescript
// Response 200
{ status: 'ok'; version: string }
```

#### `GET /admin/system`
```typescript
// Response 200
{
  version: string
  db_engine: 'sqlite' | 'mariadb'
  db_size_bytes: number
  book_count: number
  format_count: number
  storage_used_bytes: number
  meilisearch: { available: boolean; indexed_count: number; pending_count: number }
  llm: { enabled: boolean; librarian_available: boolean; architect_available: boolean }
}
```

---

### Bulk Metadata Edit

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| POST | `/books/bulk-edit` | Yes | `can_edit` | Apply metadata changes to multiple books |

#### `POST /books/bulk-edit`
```typescript
// Request
{
  book_ids: string[]
  changes: {
    tags?: { mode: 'append' | 'overwrite' | 'skip_if_set'; values: string[] }
    language?: string
    series_id?: string | null
    rating?: number | null
  }
  llm_reclassify?: boolean      // queue classify job for all selected books
}

// Response 200
{
  updated: number
  jobs_queued: number           // if llm_reclassify = true
}
```

---

## Mobile Sync

Mobile clients use `last_modified` to determine what's stale.

### Sync Flow
1. Client sends `GET /books?since={last_sync_timestamp}` — server returns only books modified after that time
2. Client sends pending progress updates via `PUT /books/:id/progress`
3. Server returns `last_modified` on all responses — client stores highest seen value as next sync cursor

### `GET /books` sync param
```typescript
// Additional query param
{ since?: string }              // ISO8601 — returns only books where last_modified > since
```

---

## File Streaming

Book files support HTTP range requests for streaming readers (large epubs, PDFs, audiobooks).

```
GET /api/v1/books/:id/formats/:format/stream
Range: bytes=0-65535

206 Partial Content
Content-Range: bytes 0-65535/1048576
Content-Type: application/epub+zip
```

Axum's `tower-http ServeFile` handles this natively.

---

### Per-User Book State

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/books/:id/state` | Yes | Any | Read/unread + archived state for current user |
| PUT | `/books/:id/state` | Yes | Any | Update read/unread/archived state |

#### `PUT /books/:id/state`
```typescript
// Request — all fields optional
{ is_read?: boolean; is_archived?: boolean }

// Response 200
{ user_id: string; book_id: string; is_read: boolean; is_archived: boolean; updated_at: string }
```

---

### Download History

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/users/me/downloads` | Yes | Any | Paginated download history for current user |

#### `GET /users/me/downloads`
```typescript
// Query params
{ page?: number; page_size?: number }

// Response 200
PaginatedResponse<{
  id: string
  book_id: string
  book_title: string        // denormalized
  format: string
  downloaded_at: string
}>
```

---

### Per-User Tag Restrictions

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/users/me/tag-restrictions` | Yes | Any | List own tag allow/block rules |
| PUT | `/users/me/tag-restrictions` | Yes | Any | Replace own tag restriction list |
| GET | `/admin/users/:id/tag-restrictions` | Yes | Admin | List restrictions for any user |
| PUT | `/admin/users/:id/tag-restrictions` | Yes | Admin | Replace restrictions for any user |

#### `PUT /users/me/tag-restrictions`
```typescript
// Request
{
  restrictions: Array<{
    tag_id: string
    mode: 'allow' | 'block'
  }>
}

// Response 200
{ updated: number }
```

---

### Libraries

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/admin/libraries` | Yes | Admin | List all libraries |
| POST | `/admin/libraries` | Yes | Admin | Create library |
| PATCH | `/admin/libraries/:id` | Yes | Admin | Update library name |
| DELETE | `/admin/libraries/:id` | Yes | Admin | Delete library (only if empty) |
| PATCH | `/auth/me/library` | Yes | Any | Switch own default library |

#### `POST /admin/libraries`
```typescript
// Request
{ name: string; calibre_db_path?: string }

// Response 201
{ id: string; name: string; calibre_db_path: string; created_at: string }
```

---

### API Tokens

Long-lived tokens for MCP server and Kobo device authentication. Returned plain-text once on creation — not retrievable again.

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/admin/api-tokens` | Yes | Admin | List tokens (names + last used, never raw token) |
| POST | `/admin/api-tokens` | Yes | Admin | Create token |
| DELETE | `/admin/api-tokens/:id` | Yes | Admin | Revoke token |

#### `POST /admin/api-tokens`
```typescript
// Request
{ name: string }

// Response 201
{ id: string; name: string; token: string; created_at: string }
// `token` is the raw value — shown once, not stored
```

---

### Kobo Sync

Mounted at `/kobo/:token/` where `:token` is a long-lived API token. All paths match the reverse-engineered Kobo API.

| Method | Path | Auth | Notes |
|---|---|---|---|
| GET | `/kobo/:token/v1/initialization` | Token | Device registration — reads `X-Kobo-DeviceId` header |
| GET | `/kobo/:token/v1/library/sync` | Token | Incremental library sync with delta token |
| GET | `/kobo/:token/v1/library/:book_id/metadata` | Token | Single book metadata |
| POST | `/kobo/:token/v1/library/:book_id/state` | Token | Push reading state from device |
| GET | `/kobo/:token/v1/user/profile` | Token | User profile (required by Kobo handshake) |
| GET | `/kobo/:token/v1/library/tags` | Token | Shelves as Kobo collections |
| POST | `/kobo/:token/v1/library/tags` | Token | Create shelf/collection |
| DELETE | `/kobo/:token/v1/library/tags/:tag_id` | Token | Delete shelf/collection |

Kobo reading state is synced to `reading_progress.percentage`. `format_id` on the canonical progress record is never overwritten by a Kobo sync.

---

### OPDS Catalog

OPDS-PS 1.2 catalog. Browse endpoints return Atom/XML feeds. Download links require an API token embedded in the URL.

| Method | Path | Auth | Description |
|---|---|---|---|
| GET | `/opds` | No | Root catalog feed |
| GET | `/opds/new` | No | Recently added books |
| GET | `/opds/popular` | No | Highly rated books |
| GET | `/opds/authors` | No | Author browse feed |
| GET | `/opds/authors/:id/books` | No | Books by author |
| GET | `/opds/series` | No | Series browse feed |
| GET | `/opds/series/:id/books` | No | Books in series |
| GET | `/opds/publishers` | No | Publisher browse feed |
| GET | `/opds/publishers/:publisher/books` | No | Books by publisher |
| GET | `/opds/languages` | No | Language browse feed |
| GET | `/opds/languages/:lang/books` | No | Books in language |
| GET | `/opds/ratings/:rating/books` | No | Books with this rating (0–10) |
| GET | `/opds/books/:id` | No | Single book entry |
| GET | `/opds/books/:id/formats/:format/download` | Token | Download (requires `?token=<api_token>`) |
| GET | `/opds/search` | No | OpenSearch description |

---

### Email (Send-to-Kindle)

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| POST | `/books/:id/send` | Yes | `can_download` | Send book format via SMTP |
| GET | `/admin/email-settings` | Yes | Admin | Get SMTP configuration |
| PATCH | `/admin/email-settings` | Yes | Admin | Update SMTP configuration |

#### `POST /books/:id/send`
```typescript
// Request
{ format: string; to: string }

// Response 202 Accepted
{ message: string }

// Response 503
ApiError    // email not configured
```

#### `PATCH /admin/email-settings`
```typescript
// Request — all fields optional
{
  smtp_host?: string
  smtp_port?: number
  smtp_user?: string
  smtp_password?: string
  from_address?: string
  use_tls?: boolean
}

// Response 200
{ smtp_host: string; smtp_port: number; from_address: string; use_tls: boolean; updated_at: string }
// smtp_password is never returned
```

---

### Scheduled Tasks

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/admin/scheduled-tasks` | Yes | Admin | List all scheduled tasks |
| POST | `/admin/scheduled-tasks` | Yes | Admin | Create scheduled task |
| PATCH | `/admin/scheduled-tasks/:id` | Yes | Admin | Update cron expression or toggle enabled |
| DELETE | `/admin/scheduled-tasks/:id` | Yes | Admin | Delete task |

#### `POST /admin/scheduled-tasks`
```typescript
// Request
{
  name: string
  task_type: 'classify_all' | 'semantic_index_all' | 'backup'
  cron_expr: string          // standard 5-field cron e.g. "0 3 * * *"
  enabled?: boolean          // default true
}

// Response 201
{
  id: string
  name: string
  task_type: string
  cron_expr: string
  enabled: boolean
  last_run_at: string | null
  next_run_at: string | null
  created_at: string
}
```

---

### Admin — System (Extended)

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/health` | No | — | Liveness check |
| GET | `/admin/system` | Yes | Admin | System stats |
| GET | `/admin/system/updates` | Yes | Admin | Compare running version against GitHub latest release |
| GET | `/admin/audit` | Yes | Admin | Audit log (paginated) |

#### `GET /admin/system/updates`
```typescript
// Response 200
{
  current_version: string
  latest_version: string
  update_available: boolean
  release_url: string | null
}
```

---

### i18n / Locale

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| GET | `/locale` | No | — | List available locales |
| PATCH | `/auth/me/locale` | Yes | Any | Set preferred locale |

#### `GET /locale`
```typescript
// Response 200
{
  available: Array<{ code: string; name: string }>  // e.g. [{code:"en",name:"English"},...]
  default: string
}
```

---

### Duplicate Book Merge (Admin)

| Method | Path | Auth | Role | Description |
|---|---|---|---|---|
| POST | `/admin/books/merge` | Yes | Admin | Merge source book into target |

#### `POST /admin/books/merge`
```typescript
// Request
{
  target_id: string          // book to keep
  source_id: string          // book to absorb and delete
  fields?: {
    title?: boolean          // overwrite target title from source?
    description?: boolean
    tags?: boolean           // merge tag sets
    identifiers?: boolean    // merge identifiers
  }
}

// Response 200
Book                         // updated target book
```

---

## Rate Limiting

LLM routes are rate-limited to prevent runaway classification jobs:

| Route group | Limit |
|---|---|
| Auth (`/auth/login`, `/auth/refresh`) | 10 req/min per IP |
| LLM classify/validate/quality/derive | 30 req/min per user |
| Bulk import / migration | 1 concurrent job per admin |
| All other routes | No limit (self-hosted, trusted network) |
