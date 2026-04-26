# Codex Desktop App — xcalibre-server Phase 10: calibre-web Feature Parity (Remaining Gaps)

## What Phase 10 Builds

Closes the remaining feature gaps identified by comparing xcalibre-server against calibre-web
after Phase 9 completes. Stages are ordered lowest-priority first, highest-priority last.
Blocking dependencies are noted per stage.

- **Stage 1** — Per-user book state: read/unread toggle, book archival, download history
- **Stage 2** — OPDS breadth: browse feeds by author, series, publisher, language, ratings
- **Stage 3** — User content controls: per-user tag restrictions, proxy authentication
- **Stage 4** — Library hygiene: merge duplicate books, Calibre custom columns UI
- **Stage 5** — Additional readers: DJVU, audio books (MP3/M4B), MOBI/AZW3 in-browser
- **Stage 6** — Localization: i18n framework, English + 3 starter languages
- **Stage 7** — Admin infrastructure: scheduled task UI, in-app update checker
- **Stage 8** — Server-side format conversion + DJVU/CBZ RAG extraction (highest priority)
- **Stage 9** — Audio book transcription via Whisper (completes full RAG coverage)
- **Stage 10** — xCalibre read API: similarity, duplicate detection, metadata suggest, recommendations + service token auth

## Key Design Decisions

- Read/unread and archived states are per-user, not global — stored in a junction table,
  not a flag on books (a book can be read by user A and unread by user B)
- OPDS additional feeds reuse existing list_books query with different filter presets;
  no new DB queries required beyond what Stage 1 of Phase 9 built
- Proxy auth: trust a single configurable header (e.g. X-Remote-User); disabled by default;
  must be behind a reverse proxy — documented clearly in config
- Goodreads API was deprecated in 2020 — do not implement; Open Library (Phase 9) covers it
- Google Drive library hosting: deferred indefinitely (niche use case, GDrive API churn)
- DJVU: use djvu.js (WASM port) loaded client-side; no server-side processing needed
- Audio: HTML5 <audio> element with range-request streaming from existing stream_format endpoint;
  no new backend code required beyond adding audio MIME types
- MOBI/AZW3: use the `mobi` Rust crate to convert to EPUB in-flight on the backend;
  no Calibre binary dependency
- Format conversion (Stage 8): thin wrapper around Calibre's ebook-convert binary;
  binary path is configurable and optional — feature is disabled if binary not found;
  conversion runs in a temp dir, result streamed to client, temp files cleaned up
- Localization: react-i18next on the frontend; backend error messages stay in English
  (clients don't display raw backend errors to end users)
- Auto-updater: check GitHub releases API for new tags; notify admin in UI; never
  auto-install (too dangerous for self-hosted — admin decides when to update)

## Key Schema Facts (new tables this phase)

```sql
-- Stage 1
book_user_state (
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  book_id    TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
  is_read    INTEGER NOT NULL DEFAULT 0,
  is_archived INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (user_id, book_id)
)

download_history (
  id          TEXT PRIMARY KEY,
  user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  book_id     TEXT NOT NULL,
  format      TEXT NOT NULL,
  downloaded_at TEXT NOT NULL
)

-- Stage 3
user_tag_restrictions (
  user_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
  tag_id     TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  mode       TEXT NOT NULL CHECK (mode IN ('allow', 'block')),
  PRIMARY KEY (user_id, tag_id)
)

-- Stage 4
-- No new tables for merge (DELETE + UPDATE existing rows)
-- custom_columns and book_custom_values already exist in schema — Stage 4 adds UI only
```

## Reference Files

Read before starting each stage:
- `docs/ARCHITECTURE.md` — overall design constraints
- `docs/SCHEMA.md` — existing schema
- `backend/src/api/mod.rs` — where to mount new route groups
- `backend/src/db/queries/books.rs` — query patterns to follow
- `backend/migrations/sqlite/` — existing migrations for numbering
- `apps/web/src/features/reader/ReaderPage.tsx` — format dispatch for Stage 5
- `backend/src/api/books.rs` — download/stream handlers for Stage 8

---

## STAGE 1 — Per-User Book State: Read/Unread, Archive, Download History

**Priority: Low-Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, docs/SCHEMA.md, backend/src/api/mod.rs, and
backend/src/db/queries/books.rs. Now implement Stage 1 of Phase 10.

Three deliverables. Implement them in order.

─────────────────────────────────────────
DELIVERABLE 1 — Read/Unread Toggle + Archived Flag
─────────────────────────────────────────

backend/migrations/sqlite/0010_book_user_state.sql:
  CREATE TABLE book_user_state (
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id     TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    is_read     INTEGER NOT NULL DEFAULT 0,
    is_archived INTEGER NOT NULL DEFAULT 0,
    updated_at  TEXT NOT NULL,
    PRIMARY KEY (user_id, book_id)
  );
  CREATE INDEX idx_book_user_state_user ON book_user_state(user_id);

backend/migrations/mariadb/0009_book_user_state.sql — equivalent MariaDB DDL.

backend/src/db/queries/book_user_state.rs — new file:
  pub struct BookUserState { pub user_id, pub book_id, pub is_read, pub is_archived, pub updated_at }
  pub async fn get_state(db, user_id, book_id) -> Result<Option<BookUserState>>
  pub async fn set_read(db, user_id, book_id, is_read: bool) -> Result<()>
  pub async fn set_archived(db, user_id, book_id, is_archived: bool) -> Result<()>
  Both set_* use INSERT ... ON CONFLICT DO UPDATE.

backend/src/api/books.rs — add two routes:
  POST /api/v1/books/:id/read      Body: { "is_read": true|false }
  POST /api/v1/books/:id/archive   Body: { "is_archived": true|false }
  Both require auth. Return 204 on success.

backend/src/db/queries/books.rs — update ListBooksParams:
  Add optional filter: show_archived: Option<bool> (default false — hides archived books
  for the requesting user). Add optional filter: only_read: Option<bool>.
  The list_books query LEFT JOINs book_user_state on (user_id, book_id) and filters accordingly.

GET /api/v1/books response — include is_read and is_archived fields per book
  (NULL → false if no row in book_user_state for this user).

apps/web/src/features/library/BookCard.tsx — add a checkmark icon (read) and
  archive icon. Clicking toggles state via POST /books/:id/read or /books/:id/archive.
  Archived books are hidden from the main library grid by default.
  Add a "Show archived" toggle to LibraryPage filters.

apps/web/src/features/library/BookDetailPage.tsx — add "Mark as read" and
  "Archive" buttons in the action bar.

Tests in backend/tests/test_book_user_state.rs:
  test_mark_book_as_read
  test_toggle_read_false
  test_archive_book_hidden_from_list
  test_show_archived_filter
  test_state_is_per_user_not_global

─────────────────────────────────────────
DELIVERABLE 2 — Download History
─────────────────────────────────────────

backend/migrations/sqlite/0011_download_history.sql:
  CREATE TABLE download_history (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id       TEXT NOT NULL,
    format        TEXT NOT NULL,
    downloaded_at TEXT NOT NULL
  );
  CREATE INDEX idx_download_history_user ON download_history(user_id);
  CREATE INDEX idx_download_history_book ON download_history(book_id);

backend/migrations/mariadb/0010_download_history.sql — equivalent MariaDB DDL.

backend/src/api/books.rs — in the existing download_format handler, after the file
  is served successfully, INSERT a download_history row for the authenticated user.
  Fire-and-forget (do not block the response on the insert — use tokio::spawn).

backend/src/api/books.rs — add one route:
  GET /api/v1/books/downloads
    Query params: ?page=1&page_size=50
    Returns paginated list: [{ book_id, title, format, downloaded_at }]
    Joined with books table for title. Auth required.

apps/web/src/features/library/DownloadHistoryPage.tsx — new page:
  Paginated table of user's download history (title, format, date).
  Link each row to the book detail page.
  Add link to this page in the user profile dropdown / nav.

Tests in backend/tests/test_download_history.rs:
  test_download_records_history_entry
  test_download_history_is_per_user
  test_download_history_pagination

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/ docs/
git commit -m "Phase 10 Stage 1: per-user read/unread, archive, download history"
```

---

## STAGE 2 — OPDS Breadth: Browse Feeds by Author, Series, Publisher, Language, Ratings

**Priority: Low-Medium**
**Blocks: nothing. Blocked by: Stage 1 of Phase 9 (OPDS root must exist).**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, backend/src/api/opds.rs, and
backend/src/db/queries/books.rs. Now implement Stage 2 of Phase 10.

Extend the existing OPDS catalog with browse feeds that mirror the calibre-web
OPDS navigation structure. All feeds are OPDS-PS 1.2 Atom XML.

─────────────────────────────────────────
NEW ROUTES (add to backend/src/api/opds.rs)
─────────────────────────────────────────

Navigation feeds (return rel="subsection" entries):
  GET /opds/authors              → list all authors (name, book count), paginated
  GET /opds/authors/:id          → books by that author (acquisition feed)
  GET /opds/series               → list all series, paginated
  GET /opds/series/:id           → books in that series (acquisition feed)
  GET /opds/publishers           → list all publishers (from identifiers or custom field)
  GET /opds/publishers/:id       → books by that publisher
  GET /opds/languages            → list all languages present in library
  GET /opds/languages/:lang_code → books in that language (acquisition feed)
  GET /opds/ratings              → list rating buckets (1★ through 5★)
  GET /opds/ratings/:rating      → books at that rating (1-10 mapped to 1-5 stars)

Each navigation entry includes:
  <link rel="subsection" type="application/atom+xml;profile=opds-catalog;kind=navigation">

Each acquisition entry includes:
  <link rel="http://opds-spec.org/acquisition" href="/api/v1/books/:id/formats/:fmt/download">
  <link rel="http://opds-spec.org/image" href="/api/v1/books/:id/cover">

─────────────────────────────────────────
QUERY HELPERS (add to backend/src/db/queries)
─────────────────────────────────────────

In backend/src/db/queries/books.rs or a new opds_queries.rs:
  pub async fn list_opds_authors(db, page, page_size) -> Result<Vec<(String, String, i64)>>
    -- (author_id, author_name, book_count)
  pub async fn list_opds_series(db, page, page_size) -> Result<Vec<(String, String, i64)>>
    -- (series_id, series_name, book_count)
  pub async fn list_opds_languages(db) -> Result<Vec<(String, i64)>>
    -- (language_code, book_count)
  pub async fn list_opds_ratings(db) -> Result<Vec<(i64, i64)>>
    -- (rating_value, book_count)
  Reuse existing list_books with filter params for the acquisition feeds.

─────────────────────────────────────────
PAGINATION
─────────────────────────────────────────

All navigation feeds:
  Default page_size = 50
  Include <link rel="next"> and <link rel="previous"> where applicable
  Include <opensearch:totalResults> and <opensearch:itemsPerPage>

─────────────────────────────────────────
TESTS (add to backend/tests/test_opds.rs)
─────────────────────────────────────────
  test_opds_authors_feed_returns_atom
  test_opds_author_books_acquisition_feed
  test_opds_series_feed
  test_opds_language_feed
  test_opds_ratings_feed

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
git add backend/ apps/
git commit -m "Phase 10 Stage 2: OPDS browse feeds for author, series, publisher, language, ratings"
```

---

## STAGE 3 — User Content Controls: Tag Restrictions + Proxy Authentication

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, docs/SCHEMA.md, backend/src/middleware/auth.rs,
backend/src/api/books.rs, and backend/src/config.rs.
Now implement Stage 3 of Phase 10. Two deliverables.

─────────────────────────────────────────
DELIVERABLE 1 — Per-User Tag Restrictions
─────────────────────────────────────────

backend/migrations/sqlite/0012_user_tag_restrictions.sql:
  CREATE TABLE user_tag_restrictions (
    user_id  TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    mode     TEXT NOT NULL CHECK (mode IN ('allow', 'block')),
    PRIMARY KEY (user_id, tag_id)
  );
  CREATE INDEX idx_user_tag_restrictions_user ON user_tag_restrictions(user_id);

backend/migrations/mariadb/0011_user_tag_restrictions.sql — equivalent MariaDB DDL.

backend/src/db/queries/user_tag_restrictions.rs — new file:
  pub async fn get_restrictions(db, user_id) -> Result<Vec<UserTagRestriction>>
  pub async fn set_restriction(db, user_id, tag_id, mode) -> Result<()>
  pub async fn remove_restriction(db, user_id, tag_id) -> Result<()>

backend/src/api/admin.rs — add routes (admin only):
  GET    /api/v1/admin/users/:id/tag-restrictions
  POST   /api/v1/admin/users/:id/tag-restrictions
    Body: { "tag_id": "...", "mode": "allow"|"block" }
  DELETE /api/v1/admin/users/:id/tag-restrictions/:tag_id

backend/src/db/queries/books.rs — update list_books:
  After fetching results, filter out books whose tags include any 'block' tag
  for the requesting user. If the user has any 'allow' restrictions, filter to
  books that have at least one 'allow' tag. Apply in the SQL query, not in Rust,
  using a subquery or CTE:
    -- Block filter: exclude books that have a blocked tag for this user
    AND NOT EXISTS (
      SELECT 1 FROM book_tags bt2
      JOIN user_tag_restrictions r ON r.tag_id = bt2.tag_id
      WHERE bt2.book_id = b.id AND r.user_id = ? AND r.mode = 'block'
    )

apps/web/src/features/admin/UsersPage.tsx — add "Tag Restrictions" button per
  user that opens a modal showing current restrictions with add/remove controls.
  Tag picker reuses the existing tag autocomplete component.

Tests in backend/tests/test_tag_restrictions.rs:
  test_blocked_tag_hides_book_from_list
  test_allow_restriction_limits_visible_books
  test_admin_can_set_restriction
  test_restriction_does_not_affect_other_users

─────────────────────────────────────────
DELIVERABLE 2 — Proxy Authentication
─────────────────────────────────────────

config.toml — add optional [auth.proxy] section:
  [auth.proxy]
  enabled       = false
  header        = "X-Remote-User"      -- header name to read username from
  email_header  = "X-Remote-Email"     -- optional email header

backend/src/config.rs — add ProxyAuthSection struct with enabled, header,
  email_header fields. Include in AuthSection.

backend/src/middleware/auth.rs — extend the auth middleware:
  If proxy auth is enabled AND the configured header is present in the request:
    1. Read username from the header value (trim whitespace)
    2. Look up or create a local user with that username
       (auto-create with role=User, random password, email from email_header or "")
    3. Issue session the same way as a normal login (attach AuthenticatedUser extension)
    4. Skip all other auth checks for this request
  If header is absent, fall through to normal JWT/token auth.

  IMPORTANT: proxy auth must only be enabled behind a trusted reverse proxy.
  Add a startup warning log if proxy auth is enabled:
    "WARN: proxy auth is enabled — ensure this server is behind a trusted reverse proxy"

Tests in backend/tests/test_proxy_auth.rs:
  test_proxy_auth_disabled_ignores_header
  test_proxy_auth_creates_user_on_first_request
  test_proxy_auth_reuses_existing_user
  test_proxy_auth_requires_header_presence

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
git add backend/ apps/
git commit -m "Phase 10 Stage 3: per-user tag restrictions, proxy authentication"
```

---

## STAGE 4 — Library Hygiene: Merge Duplicate Books + Custom Columns UI

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, docs/SCHEMA.md, backend/src/api/books.rs,
backend/src/db/queries/books.rs, and backend/src/api/admin.rs.
Now implement Stage 4 of Phase 10. Two deliverables.

─────────────────────────────────────────
DELIVERABLE 1 — Merge Duplicate Books
─────────────────────────────────────────

The goal: merge book B into book A. After merge, A has all formats from both
books, all identifiers from both books (deduplicated), all authors from both
books (deduplicated), all tags from both books. Book B is then deleted.
All reading_progress, shelf_books, book_user_state rows for book B are
reassigned to book A. The operation is atomic (single transaction).

backend/src/db/queries/books.rs — add:
  pub async fn merge_books(db, primary_id: &str, duplicate_id: &str) -> anyhow::Result<()>
    Steps inside a transaction:
    1. Reassign formats:      UPDATE formats SET book_id = primary WHERE book_id = duplicate
    2. Reassign identifiers:  INSERT OR IGNORE INTO identifiers ... SELECT from duplicate
                              then DELETE FROM identifiers WHERE book_id = duplicate
    3. Merge authors:         INSERT OR IGNORE INTO book_authors ...
                              then DELETE FROM book_authors WHERE book_id = duplicate
    4. Merge tags:            INSERT OR IGNORE INTO book_tags ...
                              then DELETE FROM book_tags WHERE book_id = duplicate
    5. Reassign progress:     UPDATE reading_progress SET book_id = primary WHERE book_id = duplicate
                              ON CONFLICT DO UPDATE SET percentage = MAX(...)
    6. Reassign shelf_books:  INSERT OR IGNORE ... then DELETE duplicates
    7. Reassign book_user_state similarly
    8. Delete duplicate book: DELETE FROM books WHERE id = duplicate
    9. Update FTS:            triggers handle this automatically

backend/src/api/books.rs — add one route:
  POST /api/v1/books/:id/merge
    Body: { "duplicate_id": "..." }
    Requires admin role.
    Returns 204 on success.
    Returns 400 if primary_id == duplicate_id.
    Returns 404 if either book not found.

apps/web/src/features/admin/ — add a "Merge Books" tool (can be a modal on
  BookDetailPage.tsx or a dedicated AdminMergePage.tsx):
    Step 1: search for the duplicate book by title
    Step 2: show side-by-side preview (both books' metadata, formats, authors)
    Step 3: confirm merge button
    Step 4: on success, redirect to the surviving book's detail page

Tests in backend/tests/test_merge.rs:
  test_merge_transfers_formats
  test_merge_deduplicates_identifiers
  test_merge_transfers_reading_progress
  test_merge_deletes_duplicate_book
  test_merge_requires_admin
  test_merge_same_book_returns_400

─────────────────────────────────────────
DELIVERABLE 2 — Calibre Custom Columns UI
─────────────────────────────────────────

The schema already exists (custom_columns and book_custom_values tables from
the initial migration). The backend queries and API may or may not exist —
read backend/src/api/books.rs and backend/src/db/queries/books.rs first.

If the API routes do not exist, add them to backend/src/api/books.rs:
  GET  /api/v1/books/custom-columns
    → list all defined custom columns (id, name, label, column_type, is_multiple)
  GET  /api/v1/books/:id/custom-values
    → return book's custom column values: [{ column_id, label, column_type, value }]
  PATCH /api/v1/books/:id/custom-values
    Body: [{ "column_id": "...", "value": "..." }]
    → upsert custom values for the book. Requires can_edit permission.

If the query helpers do not exist, add to backend/src/db/queries/books.rs:
  pub async fn list_custom_columns(db) -> Result<Vec<CustomColumn>>
  pub async fn get_book_custom_values(db, book_id) -> Result<Vec<BookCustomValue>>
  pub async fn upsert_book_custom_values(db, book_id, values) -> Result<()>

apps/web/src/features/library/BookDetailPage.tsx — add a "Custom Fields" section
  below the main metadata. Fetch /books/:id/custom-values. Display each column's
  label and value. If user has can_edit permission, make values inline-editable
  (text input, number input, or checkbox depending on column_type). Save on blur
  via PATCH /books/:id/custom-values.

apps/web/src/features/admin/ — add a CustomColumnsPage.tsx:
  Table of existing custom columns (name, label, type, is_multiple)
  Add column form: name, label, type (text|integer|float|bool|datetime), is_multiple toggle
  POST /api/v1/books/custom-columns to create
  DELETE /api/v1/books/custom-columns/:id to remove (warn: deletes all values)

Tests in backend/tests/test_custom_columns.rs:
  test_list_custom_columns
  test_set_custom_value_for_book
  test_custom_value_type_validation
  test_admin_can_create_column

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/
git commit -m "Phase 10 Stage 4: merge duplicate books, Calibre custom columns UI"
```

---

## STAGE 5 — Additional Readers: DJVU, Audio Books, MOBI/AZW3 + RAG Text Extraction

**Priority: Low-Medium**
**Blocks: nothing. MOBI/AZW3 does NOT require Stage 8 (format conversion) — uses mobi
Rust crate for in-flight EPUB conversion instead.**
**Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, apps/web/src/features/reader/ReaderPage.tsx,
backend/src/api/books.rs, backend/src/ingest/text.rs, and backend/Cargo.toml.
Now implement Stage 5 of Phase 10. Four deliverables.

─────────────────────────────────────────
DELIVERABLE 1 — DJVU Reader
─────────────────────────────────────────

DJVU files are served via the existing stream_format endpoint. Client-side
rendering uses djvu.js (a WASM/asm.js port of DjVuLibre).

apps/web/src/features/reader/DjvuReader.tsx — new component:
  - On mount, fetch the DJVU file from /api/v1/books/:id/formats/djvu/stream
    as an ArrayBuffer
  - Load it into DjVu.App (djvu.js API) rendering into a <canvas> element
  - Previous/Next page buttons + keyboard arrow navigation
  - Page counter "3 / 42"
  - Loading spinner while WASM initializes

Add djvu.js to apps/web package.json:
  "djvu.js": "^0.3.2"
  (or import from CDN with a dynamic import if package is not available on npm)

apps/web/src/features/reader/ReaderPage.tsx — update format dispatch:
  When normalizedFormat === "djvu", render <DjvuReader bookId={params.bookId} format="djvu" />

backend/src/api/books.rs — update validated_download_format_extension:
  Add "djvu" to the allowlist.

No backend DJVU processing is needed — the file is streamed as-is.

─────────────────────────────────────────
DELIVERABLE 2 — Audio Book Streaming (MP3, M4B, OGG, OPUS, FLAC)
─────────────────────────────────────────

Audio files are already stored as formats with their extension. The existing
stream_format endpoint already supports range requests, which browsers need
for audio seeking. The only changes needed are:

backend/src/api/books.rs — update validated_download_format_extension:
  Add audio extensions to the allowlist: "mp3", "m4b", "m4a", "ogg", "opus",
  "flac", "wav", "aac"

backend/src/api/books.rs — update the MIME type mapping in stream_format:
  Use mime_guess for all formats (it already handles audio types correctly).
  Ensure Content-Type: audio/mpeg for mp3, audio/mp4 for m4b/m4a,
  audio/ogg for ogg/opus, audio/flac for flac.

apps/web/src/features/reader/AudioReader.tsx — new component:
  - Renders a native HTML5 <audio> element with controls
  - src="/api/v1/books/:id/formats/:format/stream"
  - Displays book title, author, cover art
  - Reports playback percentage to onProgressChange (via timeupdate event)
  - Restores position from initialProgress.page (treating page as seconds offset)
    via audio.currentTime = initialProgress.page on load

apps/web/src/features/reader/ReaderPage.tsx — update format dispatch:
  const AUDIO_FORMATS = ["mp3", "m4b", "m4a", "ogg", "opus", "flac", "wav", "aac"]
  When normalizedFormat is in AUDIO_FORMATS, render <AudioReader />.

apps/web/src/features/library/BookDetailPage.tsx — show a "Play" button
  (instead of "Read") for audio format books.

─────────────────────────────────────────
DELIVERABLE 3 — MOBI / AZW3 In-Browser Reader
─────────────────────────────────────────

Strategy: the backend converts MOBI/AZW3 to EPUB in-flight using the `mobi`
Rust crate, then the existing EpubReader renders it. No Calibre binary needed.

backend/Cargo.toml — add:
  mobi = "0.7"

backend/tests/fixtures/minimal.mobi — commit a valid minimal MOBI test fixture
  (single chapter, public domain content). Create it by encoding a short text
  MOBI using the mobi crate's test utilities or use a real tiny public-domain MOBI.

backend/tests/test_mobi_reader.rs — write tests FIRST:
  test_mobi_to_epub_returns_epub_zip
    - Upload minimal.mobi fixture, assert GET /formats/mobi/to-epub returns
      Content-Type application/epub+zip and body starts with PK\x03\x04
  test_azw3_to_epub_returns_epub_zip
    - Same for an AZW3 fixture
  test_to_epub_returns_400_for_epub_format
    - GET /formats/epub/to-epub returns 400 (wrong endpoint)
  test_to_epub_returns_404_for_missing_format
    - Book has no MOBI format, returns 404
  test_to_epub_requires_download_permission
    - User without can_download gets 403

backend/src/api/books.rs — add route:
  GET /api/v1/books/:id/formats/:format/to-epub

  Handler:
  1. Validate format is "mobi" or "azw3" (case-insensitive) → 400 otherwise
  2. Check can_download permission → 403 if not
  3. find_format_file(db, book_id, format) → 404 if missing
  4. safe_storage_path(storage_root, format_file.path) — same guard as stream_format
  5. tokio::fs::read(path).await → bytes
  6. mobi::Mobi::new(&bytes) → parse, map error to AppError::Internal
  7. Build EPUB ZIP in memory (zip crate, already in Cargo.toml):
       - mimetype entry (stored, not deflated — EPUB spec requires this)
       - META-INF/container.xml
       - OEBPS/content.opf  (title + author from mobi.metadata)
       - OEBPS/toc.ncx
       - OEBPS/chapter{N}.xhtml per chapter from mobi.content_as_chapters()
  8. Return zip bytes:
       Content-Type: application/epub+zip
       Content-Disposition: inline; filename="{title}.epub"

  No unwrap() anywhere. All errors map to AppError.

Add route to router() alongside existing stream and download routes.

apps/web/src/features/reader/ReaderPage.tsx — update format dispatch:
  const isMobiFamily = normalizedFormat === "mobi" || normalizedFormat === "azw3"
  const epubStreamUrl = isMobiFamily
    ? `/api/v1/books/${params.bookId}/formats/${params.format}/to-epub`
    : undefined

  Render EpubReader for (isEpub || isMobiFamily), passing streamUrl={epubStreamUrl}.

apps/web/src/features/reader/EpubReader.tsx — if it does not already accept a
  streamUrl prop that overrides where it fetches book bytes, add one:
    If streamUrl is provided, pass it to epub.js Book.open() instead of the default
    /formats/epub/stream URL.

apps/web/src/features/library/BookDetailPage.tsx — ensure MOBI and AZW3 format
  entries show a "Read" button (in addition to "Download"), linking to
  /books/:id/read/mobi or /books/:id/read/azw3.

apps/mobile/src/features/reader/EpubReaderScreen.tsx — same pattern:
  When format is mobi or azw3, pass the /to-epub URL to foliojs-port's Book.open().

─────────────────────────────────────────
DELIVERABLE 4 — MOBI/AZW3 RAG Text Extraction
─────────────────────────────────────────

Context: backend/src/ingest/text.rs is the sole text extraction pipeline that
feeds semantic search (embeddings), the /books/:id/text route, the
/books/:id/chapters route, and LLM classify/validate/quality jobs. Currently
list_chapters() and extract_text() both fall through to empty Vec / empty
String for any format other than EPUB and PDF. MOBI/AZW3 books are silently
excluded from all RAG features. Since the `mobi` crate is already added in
Deliverable 3, plugging it in here is straightforward.

TXT files are also handled here (trivial — read the whole file as a single chapter).

DJVU and CBZ/CBR require OCR via external binaries and are handled in Stage 8.
Audio books require speech-to-text transcription and are handled in Stage 9.

backend/src/ingest/text.rs — extend list_chapters():
  Add a TXT arm:
    "TXT" => {
        let content = fs::read_to_string(path).unwrap_or_default();
        let word_count = content.split_whitespace().count();
        vec![Chapter { index: 0, title: "Full Text".to_string(), word_count }]
    },

  Also extend extract_text() for TXT:
    "TXT" => fs::read_to_string(path).unwrap_or_default(),

backend/src/ingest/text.rs — extend list_chapters():
  Add a MOBI/AZW3 arm:
    "MOBI" | "AZW3" => list_mobi_chapters(path).unwrap_or_default(),

  fn list_mobi_chapters(path: &Path) -> anyhow::Result<Vec<Chapter>> {
      let bytes = fs::read(path)?;
      let book = mobi::Mobi::new(&bytes)?;
      let chapters = book.content_as_chapters()
          .unwrap_or_default();
      Ok(chapters
          .into_iter()
          .enumerate()
          .map(|(index, ch)| {
              let text = strip_html_to_text(&ch.content);
              let word_count = text.split_whitespace().count();
              let title = ch.title
                  .filter(|t| !t.trim().is_empty())
                  .unwrap_or_else(|| format!("Chapter {}", index + 1));
              Chapter { index: index as u32, title, word_count }
          })
          .collect())
  }

backend/src/ingest/text.rs — extend extract_text():
  Add a MOBI/AZW3 arm:
    "MOBI" | "AZW3" => extract_mobi_text(path, chapter).unwrap_or_default(),

  fn extract_mobi_text(path: &Path, chapter: Option<u32>) -> anyhow::Result<String> {
      let bytes = fs::read(path)?;
      let book = mobi::Mobi::new(&bytes)?;
      let chapters = book.content_as_chapters().unwrap_or_default();

      if let Some(chapter_index) = chapter {
          return Ok(chapters
              .get(chapter_index as usize)
              .map(|ch| strip_html_to_text(&ch.content))
              .unwrap_or_default());
      }

      let parts: Vec<String> = chapters
          .iter()
          .map(|ch| strip_html_to_text(&ch.content))
          .filter(|t| !t.is_empty())
          .collect();
      Ok(parts.join("\n\n---\n\n"))
  }

Add `use mobi;` to the imports at the top of backend/src/ingest/text.rs.

Note: normalize_format() already uppercases the format string, so the match
arms "MOBI" and "AZW3" will be reached correctly from both "mobi"/"azw3"
inputs and "MOBI"/"AZW3" inputs.

Tests in backend/tests/test_ingest_text.rs (or extend existing test file):
  test_mobi_list_chapters_returns_chapters
    - Call list_chapters on backend/tests/fixtures/minimal.mobi
    - Assert result is non-empty and each Chapter has a non-empty title
      and word_count > 0
  test_mobi_extract_text_full_returns_content
    - Call extract_text on minimal.mobi with chapter = None
    - Assert result is a non-empty string containing expected words from
      the fixture
  test_mobi_extract_text_by_chapter
    - Call extract_text on minimal.mobi with chapter = Some(0)
    - Assert result is non-empty and shorter than the full extraction
  test_txt_extract_text_returns_content
    - Write a temp file with known text content
    - Call list_chapters with format = "txt" — assert one chapter returned,
      word_count matches the file's word count
    - Call extract_text with format = "txt", chapter = None — assert content matches
  test_unknown_format_returns_empty
    - Call list_chapters and extract_text with format = "cbz"
    - Assert both return empty (no panic, graceful degradation confirmed)

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/ backend/tests/fixtures/
git commit -m "Phase 10 Stage 5: DJVU reader, audio streaming, MOBI/AZW3 reader + RAG text extraction"
```

---

## STAGE 6 — Localization

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read apps/web/src/features/auth/LoginPage.tsx, apps/web/src/features/library/LibraryPage.tsx,
and apps/web/src/app/(tabs)/library.tsx. Now implement Stage 6 of Phase 10: i18n.

─────────────────────────────────────────
FRAMEWORK
─────────────────────────────────────────

apps/web — add to package.json:
  "i18next": "^23",
  "react-i18next": "^14"

Create apps/web/src/i18n.ts:
  Configure i18next with:
    - Language detection: browser language via navigator.language, fallback to 'en'
    - Namespace: 'translation'
    - Resources loaded from /locales/{lang}/translation.json
    - Missing key fallback: return key as-is (never crash on missing translation)

Create apps/web/public/locales/en/translation.json:
  Cover all user-visible strings in the web app. Key format: snake_case strings.
  Example structure:
    {
      "nav": { "library": "Library", "search": "Search", "admin": "Admin" },
      "auth": { "login": "Sign in", "logout": "Sign out", "register": "Create account" },
      "library": { "empty": "No books found", "upload": "Upload book", ... },
      "reader": { "loading": "Loading reader...", "unsupported": "Unsupported format" },
      "admin": { "users": "Users", "jobs": "Jobs", ... },
      "errors": { "not_found": "Not found", "server_error": "Server error" }
    }

Create starter translations (machine-translate en, human review later):
  apps/web/public/locales/fr/translation.json — French
  apps/web/public/locales/de/translation.json — German
  apps/web/public/locales/es/translation.json — Spanish

─────────────────────────────────────────
INTEGRATION
─────────────────────────────────────────

apps/web/src/main.tsx — import and initialize i18n before rendering the app.

Replace all hardcoded user-visible strings throughout apps/web/src/ with
  const { t } = useTranslation() calls, e.g.:
    "Sign in" → t('auth.login')
    "No books found" → t('library.empty')
  Do this for every string a user sees — labels, buttons, error messages,
  placeholders, empty states.

Do NOT translate: log messages, API route strings, config keys, code comments.

apps/web/src/features/layout/ (or equivalent) — add a language selector dropdown
  in the header or user profile menu. Calls i18next.changeLanguage(lang) and
  persists choice to localStorage.

─────────────────────────────────────────
MOBILE
─────────────────────────────────────────

apps/mobile — add:
  "i18next": "^23",
  "react-i18next": "^14",
  "react-native-localize": "^3"

Mirror the same translation key structure. Create:
  apps/mobile/src/locales/en/translation.json — copy relevant keys from web
  apps/mobile/src/locales/fr/translation.json
  apps/mobile/src/locales/de/translation.json
  apps/mobile/src/locales/es/translation.json

Replace hardcoded strings in mobile screens with t() calls.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
pnpm --filter @xs/web build
pnpm --filter @xs/mobile tsc --noEmit
Manually verify: switch language to French, confirm UI re-renders in French.
git add apps/ 
git commit -m "Phase 10 Stage 6: i18n framework, EN/FR/DE/ES starter translations"
```

---

## STAGE 7 — Admin Infrastructure: Scheduled Tasks UI + Update Checker

**Priority: Low**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, backend/src/api/admin.rs, backend/src/db/queries/,
and apps/web/src/features/admin/. Now implement Stage 7 of Phase 10. Two deliverables.

─────────────────────────────────────────
DELIVERABLE 1 — Scheduled Task Admin UI
─────────────────────────────────────────

xcalibre-server has an LLM job queue (llm_jobs table). Add a scheduled task layer
on top: admins can schedule recurring jobs (e.g. "re-classify all unclassified
books every Sunday at 02:00").

backend/migrations/sqlite/0013_scheduled_tasks.sql:
  CREATE TABLE scheduled_tasks (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    task_type    TEXT NOT NULL,   -- 'classify_all' | 'semantic_index_all' | 'backup'
    cron_expr    TEXT NOT NULL,   -- standard 5-field cron: "0 2 * * 0"
    enabled      INTEGER NOT NULL DEFAULT 1,
    last_run_at  TEXT,
    next_run_at  TEXT,
    created_at   TEXT NOT NULL
  );

backend/migrations/mariadb/0012_scheduled_tasks.sql — equivalent MariaDB DDL.

backend/src/scheduler.rs — new file:
  On startup, spawn a tokio task that:
    1. Every 60 seconds, query scheduled_tasks WHERE enabled = 1 AND next_run_at <= now()
    2. For each due task, dispatch the job (INSERT into llm_jobs with appropriate type)
    3. Update last_run_at = now(), compute next_run_at from cron_expr using the
       `cron` crate (add to Cargo.toml: cron = "0.12")
    4. Log a warning if a task fails to dispatch, but do not crash the scheduler

backend/src/api/admin.rs — add routes (admin only):
  GET    /api/v1/admin/scheduled-tasks
  POST   /api/v1/admin/scheduled-tasks
    Body: { "name": "...", "task_type": "...", "cron_expr": "0 2 * * 0", "enabled": true }
    Validate cron_expr is parseable before inserting — return 400 if invalid.
  PATCH  /api/v1/admin/scheduled-tasks/:id
    Body: { "enabled": false } or { "cron_expr": "..." }
  DELETE /api/v1/admin/scheduled-tasks/:id

apps/web/src/features/admin/ScheduledTasksPage.tsx — new page:
  Table of scheduled tasks (name, type, cron expression, enabled toggle, last run, next run)
  Add task button (name, type dropdown, cron expression input)
  Enable/disable toggle per task
  Delete button per task
  Cron expression hint: show human-readable description below input
    (e.g. "0 2 * * 0" → "Every Sunday at 02:00")

Tests in backend/tests/test_scheduled_tasks.rs:
  test_create_scheduled_task
  test_invalid_cron_returns_400
  test_disable_task_skips_execution
  test_due_task_creates_llm_job

─────────────────────────────────────────
DELIVERABLE 2 — In-App Update Checker
─────────────────────────────────────────

Check GitHub releases API for new xcalibre-server versions. Notify admin in UI.
Never auto-install. Admin decides when to update.

backend/src/api/admin.rs — add route:
  GET /api/v1/admin/update-check
    - Fetches https://api.github.com/repos/xcalibre/xcalibre-server/releases/latest
      with 10-second timeout
    - Compares tag_name to the current build version (embed via env!("CARGO_PKG_VERSION"))
    - Returns: { current_version: "1.2.3", latest_version: "1.3.0", update_available: true,
                 release_url: "https://github.com/..." }
    - Returns 503 if GitHub API is unreachable (do not surface error to user — return
      { update_available: false, error: "unreachable" })
    - Admin only.
    - Cache the result for 1 hour in memory (tokio::sync::RwLock<Option<CachedResult>>)
      to avoid hammering the GitHub API.

apps/web/src/features/admin/DashboardPage.tsx — add an "Update Available" banner
  at the top when update_available is true. Show current and latest versions.
  Include a link to the release page (release_url). Include dismiss button
  (persists to localStorage for 24 hours).

Tests in backend/tests/test_update_check.rs:
  test_update_check_returns_current_version
  test_update_check_github_unreachable_returns_503_gracefully
  test_update_check_requires_admin

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/
git commit -m "Phase 10 Stage 7: scheduled tasks UI, in-app update checker"
```

---

## STAGE 8 — Server-Side Format Conversion (Highest Priority)

**Priority: High — most impactful remaining gap**
**Blocks: MOBI/AZW3 reader already solved independently in Stage 5 via mobi crate.**
**This stage enables: EPUB→MOBI/AZW3 for Kindle delivery, EPUB→PDF, AZW3→EPUB fallback.**
**Blocked by: nothing (can run independently of other stages).**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, backend/src/api/books.rs, backend/src/config.rs,
and backend/src/db/queries/books.rs. Now implement Stage 8 of Phase 10:
server-side format conversion via Calibre's ebook-convert binary.

─────────────────────────────────────────
DESIGN
─────────────────────────────────────────

Calibre's ebook-convert binary converts between 20+ ebook formats. xcalibre-server
acts as a thin, safe wrapper:
  1. Copy the source file to a secure temp directory
  2. Run ebook-convert source.epub output.mobi with a 60-second timeout
  3. Stream the output file to the client
  4. Clean up the temp directory unconditionally (even on error)

The binary path is configurable and the feature is silently disabled if the
binary is not found (no crash on startup, API returns 501 Not Implemented).

─────────────────────────────────────────
CONFIGURATION
─────────────────────────────────────────

backend/src/config.rs — add to AppSection:
  pub calibre_convert_path: String   -- default: ""
  (empty string means feature disabled)

backend/src/config.rs — add to validate_config():
  If calibre_convert_path is non-empty, verify the file exists and is executable.
  If not, log a warning: "calibre_convert_path is set but binary not found — conversion disabled"
  Do NOT bail — treat as disabled.

─────────────────────────────────────────
CONVERSION MODULE
─────────────────────────────────────────

backend/src/convert.rs — new file:

  pub enum ConversionError {
    Disabled,          -- binary not configured
    Timeout,           -- conversion exceeded 60s
    Failed(String),    -- ebook-convert non-zero exit, include stderr
    Io(std::io::Error),
  }

  pub async fn convert_book(
    binary_path: &str,
    source_bytes: &[u8],
    source_format: &str,
    target_format: &str,
  ) -> Result<Vec<u8>, ConversionError>
    1. If binary_path is empty → return Err(ConversionError::Disabled)
    2. Create tempdir via tempfile::tempdir()
    3. Write source_bytes to tempdir/input.{source_format}
    4. Run: tokio::process::Command::new(binary_path)
              .arg("tempdir/input.{source_format}")
              .arg("tempdir/output.{target_format}")
              .kill_on_drop(true)
              .output()
       with tokio::time::timeout(Duration::from_secs(60), ...)
    5. On timeout: kill process, clean temp, return Err(ConversionError::Timeout)
    6. On non-zero exit: read stderr, clean temp, return Err(ConversionError::Failed(stderr))
    7. Read tempdir/output.{target_format} → bytes
    8. Clean temp (tempdir auto-drops, or explicit removal)
    9. Return Ok(bytes)

  Supported conversion pairs (validate before running):
    epub  → mobi, azw3, pdf, txt, rtf, docx, fb2
    mobi  → epub, pdf, txt
    azw3  → epub, mobi, pdf, txt
    pdf   → epub, txt
  Return 400 for unsupported source→target pair.

─────────────────────────────────────────
CONVERSION ROUTE
─────────────────────────────────────────

backend/src/api/books.rs — add route:
  GET /api/v1/books/:id/formats/:format/convert/:target_format

  Handler:
  1. Check can_download permission → 403 if not
  2. find_format_file(db, book_id, format) → 404 if not found
  3. Validate source_format → target_format is a supported pair → 400 if not
  4. safe_storage_path(storage_root, format_file.path) → path traversal guard
  5. tokio::fs::read(path) → source_bytes
  6. convert_book(&state.config.app.calibre_convert_path, &source_bytes, format, target) await
     Map errors:
       Disabled → 501 Not Implemented (body: "format conversion not configured")
       Timeout  → 504 Gateway Timeout
       Failed   → 500 (log stderr, do not expose to client)
       Io       → 500
  7. Return converted bytes:
       Content-Type: from mime_guess for target_format
       Content-Disposition: attachment; filename="{title}.{target_format}"

─────────────────────────────────────────
SEND-TO-KINDLE INTEGRATION
─────────────────────────────────────────

backend/src/api/books.rs — update the existing POST /api/v1/books/:id/send handler:

Current behavior: sends the file as-is in the requested format.
New behavior:
  - If the stored format matches the requested format → send as-is (existing behavior)
  - If the stored format differs from the requested format AND conversion is configured:
      convert using convert_book(), then send the converted bytes
  - If the stored format differs AND conversion is not configured:
      return 422 Unprocessable Entity with body:
        "Format conversion not available. Install Calibre and configure calibre_convert_path."

Add target_format field to the send request body:
  { "to": "user@kindle.com", "format": "epub", "target_format": "mobi" }
  target_format is optional; if omitted, send as-is in "format".

─────────────────────────────────────────
FRONTEND
─────────────────────────────────────────

apps/web/src/features/library/BookDetailPage.tsx — update the "Send to Kindle"
  dialog:
    - If the book has an EPUB format and conversion is available (check via a
      GET /api/v1/system/capabilities endpoint — add this endpoint to return
      { conversion_available: bool }):
        Show a "Send as MOBI" option in addition to "Send as EPUB"
    - If conversion is not available, show only the stored formats as send options

apps/web/src/features/library/BookDetailPage.tsx — per-format actions:
  Add a "Convert and download" button next to each format's "Download" button.
  Opens a dropdown of supported target formats.
  Links to /api/v1/books/:id/formats/:format/convert/:target_format with
  Content-Disposition attachment triggering a browser download.

─────────────────────────────────────────
SYSTEM CAPABILITIES ENDPOINT
─────────────────────────────────────────

backend/src/api/ — add to the existing system routes (or mod.rs):
  GET /api/v1/system/capabilities
    Returns: {
      "conversion_available": bool,   -- calibre_convert_path is configured and binary exists
      "llm_available": bool,          -- existing llm.enabled check
      "meilisearch_available": bool   -- existing meilisearch check
    }
  No auth required (frontend needs it before login for feature toggling).

─────────────────────────────────────────
DELIVERABLE 3 — RAG Text Extraction: DJVU + CBZ/CBR
─────────────────────────────────────────

Context: backend/src/ingest/text.rs currently falls through to empty for DJVU
and CBZ/CBR formats. Stage 5 added MOBI/AZW3 and TXT. This deliverable adds
OCR-based text extraction for image-format books.

DJVU uses the djvutxt binary (part of DjVuLibre). CBZ/CBR uses tesseract.
Both binary paths are optional and configurable — if absent, the format falls
through to empty (graceful degradation, no crash).

─── CONFIGURATION ───

backend/src/config.rs — add to AppSection:
  pub djvutxt_path: String    -- default: "" (disabled if empty)
  pub tesseract_path: String  -- default: "" (disabled if empty)

Same validation pattern as calibre_convert_path: if non-empty and binary not
found, log a warning at startup and treat as disabled. Do NOT bail.

─── DJVU TEXT EXTRACTION ───

backend/src/ingest/text.rs — extend list_chapters() and extract_text():

  "DJVU" => list_djvu_chapters(path, &config.app.djvutxt_path).unwrap_or_default(),

  fn list_djvu_chapters(path: &Path, binary: &str) -> anyhow::Result<Vec<Chapter>> {
      if binary.is_empty() { return Ok(vec![]); }
      // djvutxt outputs the entire text of all pages; treat as one chapter
      let out = std::process::Command::new(binary)
          .arg(path)
          .output()?;
      anyhow::ensure!(out.status.success(), "djvutxt failed");
      let text = String::from_utf8_lossy(&out.stdout).into_owned();
      let word_count = text.split_whitespace().count();
      Ok(vec![Chapter { index: 0, title: "Full Text".to_string(), word_count }])
  }

  "DJVU" => extract_djvu_text(path, &config.app.djvutxt_path).unwrap_or_default(),

  fn extract_djvu_text(path: &Path, binary: &str) -> anyhow::Result<String> {
      if binary.is_empty() { return Ok(String::new()); }
      let out = std::process::Command::new(binary)
          .arg(path)
          .output()?;
      anyhow::ensure!(out.status.success(), "djvutxt failed");
      Ok(String::from_utf8_lossy(&out.stdout).into_owned())
  }

  Use tokio::task::spawn_blocking to avoid blocking the async runtime when
  calling the synchronous std::process::Command.

─── CBZ/CBR TEXT EXTRACTION (OCR) ───

CBZ and CBR are ZIP/RAR archives of page images. For each page image, run
tesseract to extract text. Process pages in order, join with newlines.

backend/src/ingest/text.rs — extend list_chapters() and extract_text():

  "CBZ" | "CBR" => list_cbz_chapters(path, &config.app.tesseract_path).unwrap_or_default(),

  fn list_cbz_chapters(path: &Path, tesseract: &str) -> anyhow::Result<Vec<Chapter>> {
      if tesseract.is_empty() { return Ok(vec![]); }
      let text = extract_cbz_text_all(path, tesseract)?;
      let word_count = text.split_whitespace().count();
      Ok(vec![Chapter { index: 0, title: "Full Text".to_string(), word_count }])
  }

  fn extract_cbz_text_all(path: &Path, tesseract: &str) -> anyhow::Result<String> {
      // 1. Open ZIP (use zip crate, already in Cargo.toml)
      // 2. Iterate entries sorted by name (natural page order)
      // 3. For each entry that is an image (png/jpg/webp/gif by extension):
      //    a. Write image bytes to a temp file
      //    b. Run: tesseract <img_path> stdout --psm 3 (stdout mode outputs to stdout)
      //    c. Collect stdout as UTF-8 text
      // 4. Join all page texts with "\n\n"
      // CBR (RAR): if format is CBR, return empty and log a warning:
      //   "CBR extraction requires unrar; use CBZ for OCR support"
      //   (RAR extraction is legally complex; CBZ/ZIP is universally supported)
      use zip::ZipArchive;
      let file = std::fs::File::open(path)?;
      let mut archive = ZipArchive::new(file)?;
      let mut pages: Vec<(String, Vec<u8>)> = Vec::new();
      for i in 0..archive.len() {
          let mut entry = archive.by_index(i)?;
          let name = entry.name().to_string();
          let ext = std::path::Path::new(&name)
              .extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
          if ["png", "jpg", "jpeg", "webp", "gif"].contains(&ext.as_str()) {
              let mut buf = Vec::new();
              std::io::Read::read_to_end(&mut entry, &mut buf)?;
              pages.push((name, buf));
          }
      }
      pages.sort_by(|(a, _), (b, _)| a.cmp(b));

      let mut parts = Vec::new();
      let tmp = tempfile::tempdir()?;
      for (name, bytes) in pages {
          let ext = std::path::Path::new(&name)
              .extension().and_then(|e| e.to_str()).unwrap_or("png");
          let img_path = tmp.path().join(format!("page.{ext}"));
          std::fs::write(&img_path, &bytes)?;
          let out = std::process::Command::new(tesseract)
              .arg(&img_path).arg("stdout").arg("--psm").arg("3")
              .output()?;
          if out.status.success() {
              let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
              if !text.is_empty() { parts.push(text); }
          }
      }
      Ok(parts.join("\n\n"))
  }

  Use tokio::task::spawn_blocking for all CBZ processing (heavy I/O + subprocess).

─── TESTS ───

Tests in backend/tests/test_ingest_text.rs (extend existing file):

  test_djvu_returns_empty_when_binary_not_configured
    - Call list_chapters with format="djvu" and djvutxt_path="" in config
    - Assert returns empty vec (no error)

  test_cbz_returns_empty_when_tesseract_not_configured
    - Call list_chapters with format="cbz" and tesseract_path="" in config
    - Assert returns empty vec (no error)

  test_cbz_extract_text_integration (use #[ignore]):
    - Requires tesseract in PATH
    - Create a minimal CBZ fixture: one PNG page with known text rendered via
      the `image` crate (solid-color image with text overlay if possible, or
      just assert the function runs without panic and returns a String)
    - Run with: cargo test -- --ignored

─────────────────────────────────────────
SECURITY NOTES
─────────────────────────────────────────

- Never pass user-supplied strings as shell arguments. All paths come from the
  database (safe_storage_path validated) and the target_format is allowlisted.
- Use tokio::process::Command (not std::process::Command) to avoid blocking the runtime.
- kill_on_drop(true) ensures the child process is killed if the future is dropped.
- The temp directory is always cleaned up (tempfile::TempDir drops on scope exit).
- The ebook-convert binary is never run with elevated privileges.
- Log the conversion command at DEBUG level but never log file contents.
- Same constraints apply to djvutxt and tesseract: allowlisted binary paths from
  config only, all file paths from DB only, temp dirs always cleaned up.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_convert.rs:
  test_convert_disabled_returns_501
    (no binary configured — expect 501)
  test_convert_unsupported_pair_returns_400
    (epub → cbz → expect 400)
  test_convert_requires_download_permission
    (user without can_download → 403)
  test_convert_epub_to_mobi (integration — skip if ebook-convert not in PATH)
    Use #[ignore] attribute; run manually with: cargo test -- --ignored
    Assert output bytes start with PalmDOC magic bytes (BOOKMOBI)
  test_send_with_conversion_target_format
    (mock convert_book, assert email attachment uses converted bytes)

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/
git commit -m "Phase 10 Stage 8: format conversion via ebook-convert, DJVU/CBZ RAG extraction, Kindle delivery"
```

---

## STAGE 9 — Audio Book Transcription (RAG via Remote Whisper API)

**Priority: Medium — completes full RAG coverage across all supported formats**
**Blocks: nothing. Blocked by: nothing (runs independently).**
**Model: GPT-5.3-Codex**

**Architecture note:** Whisper runs on a separate server or VM — NOT inside the
xcalibre-server Docker container. xcalibre-server sends the audio file over HTTP to a
self-hosted OpenAI-compatible ASR server (faster-whisper-server or LocalAI)
and receives the transcript as JSON. This follows the same pattern as the
existing LLM integration: network call with timeout, silent fallback, never
surface errors to users. No binary path, no subprocess, no extra runtime
dependencies in the xcalibre-server container.

**Recommended Whisper server:** `faster-whisper-server`
  Docker: `docker run -p 8000:8000 fedirz/faster-whisper-server:latest-cpu`
  GPU:    `docker run -p 8000:8000 --gpus all fedirz/faster-whisper-server:latest-cuda`
  Exposes: POST /v1/audio/transcriptions (OpenAI-compatible)

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, backend/src/ingest/text.rs, backend/src/api/admin.rs,
backend/src/config.rs, and the existing LLM client code (backend/src/llm/ or
equivalent — find where reqwest is used to call the LLM API).
Now implement Stage 9 of Phase 10: audio book transcription via a remote
OpenAI-compatible Whisper API server.

─────────────────────────────────────────
DESIGN
─────────────────────────────────────────

Audio files (MP3, M4B, OGG, OPUS, FLAC, WAV, AAC) cannot be text-extracted
synchronously. Transcription runs as a background job:
  1. When a book with an audio format is uploaded or manually queued, a
     `transcribe` job is inserted into llm_jobs.
  2. The job worker POSTs the audio file as multipart/form-data to
     {whisper_api_url}/v1/audio/transcriptions — identical to the OpenAI
     Whisper API — and receives a JSON response { "text": "..." }.
  3. The transcript is stored as a synthetic TXT format entry in the formats
     table, making it visible to list_chapters/extract_text and therefore
     the existing embedding pipeline.

The xcalibre-server container never runs Whisper itself. All heavy compute is
offloaded to the dedicated Whisper server.

─────────────────────────────────────────
CONFIGURATION
─────────────────────────────────────────

backend/src/config.rs — add a new [whisper] section:

  [whisper]
  api_url  = ""        -- e.g. "http://whisper-host:8000" (disabled if empty)
  api_key  = ""        -- optional bearer token; leave empty if server is unauthenticated
  model    = "base"    -- whisper model name to request (tiny/base/small/medium/large)
  language = ""        -- ISO 639-1 language hint; leave empty for auto-detect
  timeout_secs = 600   -- per-job timeout (audiobooks can be long); default 10 minutes

Add WhisperSection struct. Include in AppConfig.

validate_config(): if whisper.api_url is non-empty, parse it as a URL — bail
  with a clear error if it is not a valid HTTP/HTTPS URL. Do NOT make a
  network request at startup. Log: "Whisper transcription enabled: {api_url}"

─────────────────────────────────────────
SCHEMA
─────────────────────────────────────────

The existing llm_jobs table already has a job_type column. Add 'transcribe' as
a valid value. No migration needed if llm_jobs has no CHECK constraint on
job_type; if it does, add a migration to widen it:

backend/migrations/sqlite/0014_transcribe_job_type.sql:
  -- Only needed if llm_jobs.job_type has a CHECK constraint.
  -- Read backend/migrations/sqlite/ to determine the correct approach before
  -- writing this migration. Match the existing pattern exactly.

─────────────────────────────────────────
TRANSCRIPTION CLIENT
─────────────────────────────────────────

backend/src/transcribe.rs — new file:

  #[derive(Debug)]
  pub enum TranscriptionError {
      Disabled,                -- whisper.api_url is empty
      Timeout,                 -- request exceeded timeout_secs
      ApiError(u16, String),   -- HTTP error status + body
      Network(reqwest::Error),
      Io(std::io::Error),
  }

  pub async fn transcribe_audio(
      cfg: &WhisperSection,
      audio_bytes: Vec<u8>,
      filename: &str,          -- e.g. "audiobook.mp3" (for MIME type detection)
  ) -> Result<String, TranscriptionError>

    1. If cfg.api_url is empty → return Err(TranscriptionError::Disabled)

    2. Build a reqwest multipart form:
         Part "file": audio_bytes, filename=filename, MIME from extension
         Part "model": cfg.model
         Part "response_format": "json"
         If cfg.language is non-empty: Part "language": cfg.language

    3. Build request:
         POST {cfg.api_url}/v1/audio/transcriptions
         Header Authorization: Bearer {cfg.api_key}  (only if api_key non-empty)
         Header User-Agent: xcalibre-server/{CARGO_PKG_VERSION}
         Body: multipart form

    4. Wrap with tokio::time::timeout(Duration::from_secs(cfg.timeout_secs), ...)

    5. On timeout → Err(TranscriptionError::Timeout)
    6. On non-2xx response → Err(TranscriptionError::ApiError(status, body))
    7. Parse response JSON: { "text": "..." } → return Ok(text)

  Use the existing reqwest::Client from AppState (do not create a new client
  per request — reuse the shared client). Follow the same pattern as the
  existing LLM HTTP calls.

─────────────────────────────────────────
JOB WORKER INTEGRATION
─────────────────────────────────────────

backend/src/jobs/ (or wherever the LLM job worker loop lives):

  Add a handler for job_type = "transcribe":

  async fn handle_transcribe_job(job, db, config, storage_root, http_client):
    1. Parse job.book_id and job.format (audio format to transcribe)
    2. find_format_file(db, book_id, format) — skip if not found (log warning)
    3. safe_storage_path(storage_root, format_file.path)
    4. tokio::fs::read(&audio_path).await → audio_bytes
    5. transcribe_audio(&config.whisper, audio_bytes, filename).await
       On Disabled:  mark job as skipped, log info "Whisper not configured"
       On Timeout:   mark job as failed, log warning "Transcription timed out"
       On ApiError:  mark job as failed, log error (do NOT include audio bytes in log)
       On Network:   mark job as failed, log error
       On Ok(text):
         a. Derive transcript path: books/<book_id>/<book_id>.transcript.txt
         b. tokio::fs::write(storage_root / transcript_path, &text).await?
         c. INSERT OR REPLACE INTO formats (id, book_id, format, path, size_bytes,
            created_at, last_modified) with format='TXT', path=transcript_path
            (makes it visible to list_chapters / extract_text as TXT)
         d. Trigger re-embedding for the book (same call as post-upload indexing)
         e. Mark job as completed

─────────────────────────────────────────
TRIGGER: AUTO-QUEUE ON UPLOAD
─────────────────────────────────────────

backend/src/api/books.rs — in the upload_book handler, after inserting the format:
  const AUDIO_FORMATS: &[&str] = &["mp3","m4b","m4a","ogg","opus","flac","wav","aac"];
  If uploaded format (lowercased) is in AUDIO_FORMATS
  AND config.whisper.api_url is non-empty:
    INSERT INTO llm_jobs (id, book_id, job_type, format, status, created_at)
    VALUES (uuid, book_id, 'transcribe', format, 'pending', now())

─────────────────────────────────────────
MANUAL QUEUE ROUTE
─────────────────────────────────────────

backend/src/api/admin.rs — add route (admin only):
  POST /api/v1/admin/books/:id/transcribe
    Body: { "format": "mp3" }
    Returns 202 Accepted: { "job_id": "..." }
    Returns 400 if format is not an audio type
    Returns 422 if whisper.api_url is empty ("Whisper API not configured")
    Returns 404 if book or format not found

─────────────────────────────────────────
SYSTEM CAPABILITIES
─────────────────────────────────────────

Update GET /api/v1/system/capabilities to include:
  "whisper_available": bool   -- whisper.api_url is non-empty and valid URL

─────────────────────────────────────────
FRONTEND
─────────────────────────────────────────

apps/web/src/features/library/BookDetailPage.tsx — for audio format books:
  If capabilities.whisper_available:
    Show "Transcribe for search" button next to the audio format.
    On click, POST /api/v1/admin/books/:id/transcribe with { format }.
    Poll job status every 10s (GET /api/v1/admin/jobs/:job_id or equivalent).
    Show spinner while pending/running, checkmark when complete, error on failure.
  If the book already has a TXT format row (transcript exists):
    Show "Transcript available — fully searchable" badge instead of the button.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_transcribe.rs:

  test_transcribe_disabled_returns_422
    - whisper.api_url = "" in config
    - POST /api/v1/admin/books/:id/transcribe → 422

  test_transcribe_enqueues_job_for_audio_format
    - whisper.api_url = "http://localhost:9999" (no real server needed)
    - POST /api/v1/admin/books/:id/transcribe with format="mp3"
    - Assert llm_jobs has one row with job_type='transcribe', status='pending'

  test_transcribe_returns_400_for_non_audio_format
    - POST /admin/books/:id/transcribe with format="epub" → 400

  test_upload_audio_auto_queues_job
    - Upload an MP3 file with whisper.api_url configured
    - Assert llm_jobs has one 'transcribe' job for the uploaded book

  test_transcribe_client_sends_correct_multipart (mock HTTP):
    - Use wiremock or httpmock to intercept the POST to /v1/audio/transcriptions
    - Assert multipart contains parts: file, model, response_format
    - Mock returns { "text": "hello world" }
    - Assert transcribe_audio returns Ok("hello world")

  test_transcribe_client_timeout
    - Mock server delays response beyond timeout_secs
    - Assert transcribe_audio returns Err(TranscriptionError::Timeout)

  test_transcribe_worker_writes_txt_format (integration, #[ignore]):
    - Requires a running faster-whisper-server at WHISPER_TEST_URL env var
    - Use a 1-second silent MP3 fixture
    - Run handle_transcribe_job
    - Assert formats table has a TXT row for the book
    - Assert transcript file exists on disk
    - cargo test -- --ignored

─────────────────────────────────────────
DEPLOYMENT NOTE (for docs/ARCHITECTURE.md or README)
─────────────────────────────────────────

Add a note explaining how to run the Whisper server separately:

  Whisper Server (optional — required for audio book transcription):
    docker run -d --name whisper \
      -p 8000:8000 \
      fedirz/faster-whisper-server:latest-cpu

  GPU (NVIDIA):
    docker run -d --name whisper \
      -p 8000:8000 --gpus all \
      fedirz/faster-whisper-server:latest-cuda

  Then in xcalibre-server config.toml:
    [whisper]
    api_url = "http://whisper-host:8000"
    model   = "base"

  The Whisper server is NOT part of the xcalibre-server docker-compose. Run it
  separately on any host or VM with sufficient RAM (base model: ~1 GB).

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/ docs/
git commit -m "Phase 10 Stage 9: audio transcription via remote Whisper API, full RAG coverage"
```

---

---

## STAGE 10 — xCalibre Read API: Similarity, Deduplication, Metadata Suggest, Recommendations + Service Token Auth

**Priority: Medium — required for xCalibre desktop client intelligence features**
**Blocks: nothing. Blocked by: Phase 4 Stage 4 (sqlite-vec must exist for similarity/recommendations).**
**Model: GPT-5.3-Codex**

**Context:** xCalibre is a standalone desktop ebook reader and processing engine (separate
project, Tauri/Rust). It pushes processed assets to xcalibre-server for storage and serving.
It also queries xcalibre-server's RAG layer for four intelligence features: similar books,
duplicate detection before import, metadata suggestions, and reading recommendations.
xcalibre-server must expose read endpoints for all four, plus issue long-lived service tokens
that xCalibre stores in the OS keychain.

All four read endpoints must degrade silently — if the vector index is not ready
(sqlite-vec not populated), return empty results with 200, never 503 to xCalibre.

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, backend/src/api/books.rs, backend/src/api/auth.rs,
backend/src/db/queries/books.rs, and backend/src/api/mod.rs.
Now implement Stage 10 of Phase 10: the xCalibre read API and service token auth.
Five deliverables. Implement in order.

─────────────────────────────────────────
DELIVERABLE 1 — Service Token Auth
─────────────────────────────────────────

xCalibre is a trusted first-party desktop client that needs a long-lived token
to authenticate with xcalibre-server. Short-lived JWTs are wrong here — xCalibre may
be offline for days and must not require re-auth on reconnect.

backend/migrations/sqlite/0015_service_tokens.sql:
  CREATE TABLE service_tokens (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,  -- SHA-256 of the raw token
    role        TEXT NOT NULL DEFAULT 'service',
    created_by  TEXT NOT NULL REFERENCES users(id),
    created_at  TEXT NOT NULL,
    last_used_at TEXT,
    expires_at  TEXT   -- NULL = never expires
  );
  CREATE INDEX idx_service_tokens_hash ON service_tokens(token_hash);

backend/migrations/mariadb/0013_service_tokens.sql — equivalent MariaDB DDL.

backend/src/db/queries/service_tokens.rs — new file:
  pub async fn create_service_token(db, name, created_by, expires_at) -> Result<(ServiceToken, String)>
    -- generates a random 32-byte token, stores SHA-256 hash, returns (record, raw_token)
    -- raw_token is shown ONCE to the admin and never stored in plaintext
  pub async fn lookup_by_token(db, raw_token: &str) -> Result<Option<ServiceToken>>
    -- SHA-256 hash the input, query by token_hash
    -- on hit, update last_used_at = now()
  pub async fn list_service_tokens(db, created_by) -> Result<Vec<ServiceToken>>
  pub async fn revoke_service_token(db, id) -> Result<()>

backend/src/middleware/auth.rs — extend auth middleware:
  Before checking JWT, check for Authorization: Bearer <token> where the token
  does NOT parse as a JWT (no dots). Look it up via lookup_by_token. If found
  and not expired, attach a synthetic AuthenticatedUser with:
    role = ServiceRole (new variant)
    user_id = service_tokens.id  (use token id as the "user" for audit logs)
  ServiceRole grants: can_read, can_download. Does NOT grant admin.

backend/src/api/admin.rs — add routes (admin only):
  POST   /api/v1/admin/service-tokens
    Body: { "name": "xCalibre Desktop", "expires_at": null }
    Returns: { "id": "...", "name": "...", "token": "<raw — shown once>", "created_at": "..." }
  GET    /api/v1/admin/service-tokens
    Returns list (never includes raw token or hash — id/name/created_at/last_used_at only)
  DELETE /api/v1/admin/service-tokens/:id
    Revokes token immediately.

apps/web/src/features/admin/ServiceTokensPage.tsx — new page:
  Table of service tokens (name, created, last used, expires).
  "Create token" button → modal with name field → on submit shows raw token
  in a copy-to-clipboard box with warning "This token will not be shown again."
  Revoke button per token (with confirmation).
  Link in admin nav.

Tests in backend/tests/test_service_tokens.rs:
  test_service_token_grants_read_access
  test_service_token_does_not_grant_admin
  test_revoked_token_returns_401
  test_expired_token_returns_401
  test_token_hash_not_exposed_in_list_endpoint

─────────────────────────────────────────
DELIVERABLE 2 — Similar Books Endpoint
─────────────────────────────────────────

Uses the existing sqlite-vec embedding index from Phase 4 Stage 4.

backend/src/api/books.rs — add route:
  GET /api/v1/books/:id/similar
    Query params: ?limit=5 (max 20, default 5)
    Auth: required (JWT or service token)

  Handler:
  1. Fetch the book's embedding vector from the vectors table (or wherever
     Phase 4 stores per-book embeddings) — 404 if book not found
  2. If no embedding exists for this book (not yet indexed):
     return 200 with { "books": [], "indexed": false }
  3. Query sqlite-vec for the N nearest neighbors by cosine similarity,
     excluding the book itself
  4. Join results with books table to get title, authors, cover_path
  5. Return:
     {
       "books": [
         {
           "id": "...",
           "title": "...",
           "authors": ["..."],
           "cover_path": "...",
           "similarity_score": 0.92
         }
       ],
       "indexed": true
     }

backend/src/db/queries/books.rs — add:
  pub async fn find_similar_books(
      db, vector: &[f32], exclude_id: &str, limit: u8
  ) -> Result<Vec<SimilarBook>>

Tests in backend/tests/test_similar.rs:
  test_similar_returns_empty_when_not_indexed
  test_similar_excludes_self
  test_similar_respects_limit
  test_similar_requires_auth

─────────────────────────────────────────
DELIVERABLE 3 — Duplicate Lookup Endpoint
─────────────────────────────────────────

Used by xCalibre before pushing a new asset to check if it already exists.

backend/src/api/books.rs — add route:
  GET /api/v1/books/lookup
    Query params:
      ?isbn=9780743273565         (exact match on identifiers table)
      ?title=Great+Gatsby&author=Fitzgerald  (fuzzy FTS5 match)
    At least one param required. ISBN takes priority if both provided.
    Auth: required (JWT or service token)

  Handler — ISBN path:
  1. Query identifiers WHERE id_type = 'isbn' AND value = ?
  2. If found: return { "exists": true, "book_id": "...", "title": "...", "authors": [...] }
  3. If not found: return { "exists": false }

  Handler — title+author path:
  1. Run FTS5 query on books table: MATCH '{title} {author}'
  2. Take the top result only (highest rank)
  3. If rank score above threshold: return { "exists": true, ... }
  4. If no match or below threshold: return { "exists": false }
  5. Include "confidence": "exact"|"fuzzy" in response to let xCalibre
     decide whether to warn or silently skip

Tests in backend/tests/test_lookup.rs:
  test_lookup_by_isbn_finds_exact_match
  test_lookup_by_isbn_returns_false_for_unknown
  test_lookup_by_title_author_finds_close_match
  test_lookup_returns_false_not_404_for_no_match
  test_lookup_requires_at_least_one_param
  test_lookup_requires_auth

─────────────────────────────────────────
DELIVERABLE 4 — Metadata Suggest Endpoint
─────────────────────────────────────────

Used by xCalibre's metadata editor to pre-fill fields from existing library records.

backend/src/api/books.rs — add route:
  GET /api/v1/books/metadata-suggest
    Query params: ?q=<search string>&limit=5 (max 10, default 5)
    Auth: required (JWT or service token)

  Handler:
  1. Run FTS5 search on books with the query string (same as existing search)
  2. For each result, JOIN: authors, tags, identifiers
  3. Return a richer payload than standard search — include all fields
     xCalibre may want to copy:
     {
       "suggestions": [
         {
           "id": "...",
           "title": "...",
           "sort_title": "...",
           "authors": [{ "name": "...", "sort_name": "..." }],
           "publisher": "...",
           "language": "...",
           "description": "...",
           "tags": ["fiction", "classic"],
           "identifiers": [{ "id_type": "isbn", "value": "..." }],
           "rating": 8,
           "pubdate": "1925-04-10"
         }
       ]
     }

  Reuse existing list_books query infrastructure — this is a thin wrapper
  with a different response shape. Do not duplicate the query logic.

Tests in backend/tests/test_metadata_suggest.rs:
  test_suggest_returns_matching_books
  test_suggest_includes_authors_and_tags
  test_suggest_empty_query_returns_400
  test_suggest_requires_auth

─────────────────────────────────────────
DELIVERABLE 5 — Recommendations Endpoint
─────────────────────────────────────────

Returns unread books the user is likely to enjoy, based on the current book's
vector and the user's reading history vectors.

backend/src/api/books.rs — add route:
  GET /api/v1/books/:id/recommendations
    Query params:
      ?limit=5           (max 20, default 5)
      ?exclude_read=true (default true — hide already-read books)
    Auth: required (JWT or service token)

  Handler:
  1. Fetch current book's embedding vector — if none, return { "books": [], "indexed": false }
  2. If the authenticated user has reading history (book_user_state.is_read = 1):
     a. Fetch vectors for up to 20 most recently read books
     b. Compute a "taste vector": element-wise average of all read book vectors,
        L2-normalized. This represents the user's reading profile.
     c. Blend: recommendation_vector = 0.6 * current_book_vector
                                      + 0.4 * taste_vector
        (current book has higher weight — it's the primary signal)
        L2-normalize the blended vector.
  3. If no reading history: recommendation_vector = current_book_vector
  4. Query sqlite-vec for nearest neighbors using recommendation_vector,
     excluding the current book and (if exclude_read=true) all books in
     book_user_state WHERE user_id = ? AND is_read = 1
  5. Return same shape as /similar endpoint, plus:
     "has_reading_history": bool

  All vector math (average, normalize, blend) done in Rust, not SQL.
  If sqlite-vec is not initialized: return { "books": [], "indexed": false }

backend/src/db/queries/books.rs — add:
  pub async fn get_read_book_vectors(db, user_id, limit: u8) -> Result<Vec<(String, Vec<f32>)>>
  pub async fn find_recommendations(
      db, vector: &[f32], exclude_ids: &[String], limit: u8
  ) -> Result<Vec<SimilarBook>>

Tests in backend/tests/test_recommendations.rs:
  test_recommendations_without_history_falls_back_to_similar
  test_recommendations_exclude_read_books
  test_recommendations_blends_taste_vector
  test_recommendations_returns_empty_when_not_indexed
  test_recommendations_requires_auth

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/
git commit -m "Phase 10 Stage 10: xCalibre read API — similarity, dedup, metadata suggest, recommendations, service tokens"
```

---

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/review` — verify per-user state isolation, no cross-user leaks |
| Stage 2 | `/review` — verify OPDS feed XML is valid OPDS-PS 1.2 |
| Stage 3 | `/review` + `/security-review` — proxy auth header trust, restriction bypass |
| Stage 4 | `/review` — merge transaction atomicity, custom column type validation |
| Stage 5 | `/review` — MOBI→EPUB ZIP structure, safe_storage_path reuse |
| Stage 6 | `/review` — no hardcoded strings remain, fallback key behavior |
| Stage 7 | `/review` — scheduler cron parsing, GitHub API rate limiting |
| Stage 8 | `/review` + `/security-review` — shell injection prevention, temp dir cleanup, timeout handling, OCR binary safety |
| Stage 9 | `/review` + `/security-review` — Whisper binary safety, temp dir cleanup, transcript storage path traversal |
| Stage 10 | `/review` + `/security-review` — service token hash storage, token exposure in logs, vector math correctness |

Run `/engineering:deploy-checklist` after Stage 10 before merging to main.
