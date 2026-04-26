# Codex Desktop App — xcalibre-server Phase 14: Ecosystem + Accessibility

## What Phase 14 Builds

Six features completing the ecosystem and accessibility story:

- **Stage 1** — Goodreads / StoryGraph CSV import (reading history + shelf migration)
- **Stage 2** — Author management (bio, photo, global merge)
- **Stage 3** — Webhook delivery (push notifications to external systems on library events)
- **Stage 4** — Mobile download queue UI (queue view, storage management, batch download)
- **Stage 5** — Accessibility audit + remediation (WCAG 2.1 AA compliance)
- **Stage 6** — Author photo storage (extends Phase 14 Stage 2 with image upload + serving)

## Key Design Decisions

**Goodreads/StoryGraph Import:**
- Goodreads exports a CSV with columns: Title, Author, My Rating, Date Read, Bookshelves, Exclusive Shelf
- StoryGraph exports a similar CSV; fields differ slightly but can be normalized
- Import is best-effort: match by title + author; skip non-matching books; report unmatched
- Creates shelves from Goodreads "Bookshelves" column; marks books `is_read` from "read" shelf
- Handled as an async background job (same job queue as Calibre import) — can take minutes for large libraries

**Author Management:**
- New `author_profiles` table (1:1 with `authors`) for bio, photo path, birth/death dates, external IDs
- Author merge: books, shelves, and annotations pointing to source author are re-pointed to target; source deleted
- Author photo stored under `authors/{ab}/{id}.jpg` + `.webp` (same bucketing pattern as covers)
- Author detail page: new route `/authors/:id` — bio, photo, list of books

**Webhooks:**
- Event-driven: `book.added`, `book.deleted`, `import.completed`, `llm_job.completed`, `user.registered`
- Delivery: HTTPS POST to user-configured URL with JSON payload; signed with HMAC-SHA256 (secret in `webhooks` table)
- Retry: 3 attempts with exponential backoff (30s, 5m, 30m); give up after third failure; record last_error
- Delivery runs in the existing scheduler loop — no separate process

**Mobile Download Queue:**
- `local_downloads` SQLite table already exists (from Phase 6); the gap is the management UI
- Queue screen shows: in-progress (with progress bar), completed (with file size), failed (with retry button)
- Storage usage summary at top (total MB downloaded, space available via expo-file-system)
- Batch download: "Download all books in shelf" action on shelf detail screen

**Accessibility:**
- Target: WCAG 2.1 Level AA
- Audit scope: web only (mobile accessibility is platform-handled by Expo/React Native)
- Key areas: keyboard navigation through library grid, focus management in modals and slide-in panels, reader toolbar accessible without mouse, color contrast in both light and dark mode, screen reader announcements for async state changes (loading, results updated)

**Author Photo Storage:**
- Same `render_cover_variants` pattern as book covers: JPEG + WebP at two sizes
- Served via `GET /authors/:id/photo` — same auth gating as cover serving
- Falls back to a letter-based placeholder (same logic as CoverPlaceholder) if no photo

## Key Schema Facts (new tables this phase)

```sql
-- Stage 1 — Goodreads import log (migration 0016)
CREATE TABLE goodreads_import_log (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    filename    TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'pending'
                  CHECK(status IN ('pending', 'running', 'complete', 'failed')),
    total_rows  INTEGER,
    matched     INTEGER,
    unmatched   INTEGER,
    errors      TEXT,               -- JSON array of error strings
    created_at  TEXT NOT NULL,
    completed_at TEXT
);

-- Stage 2 — Author profiles (migration 0017)
CREATE TABLE author_profiles (
    author_id   TEXT PRIMARY KEY REFERENCES authors(id) ON DELETE CASCADE,
    bio         TEXT,
    photo_path  TEXT,               -- relative storage path, NULL if no photo
    born        TEXT,               -- ISO date or year string
    died        TEXT,
    website_url TEXT,
    openlibrary_id TEXT,
    updated_at  TEXT NOT NULL
);

-- Stage 3 — Webhooks (migration 0018)
CREATE TABLE webhooks (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    url         TEXT NOT NULL,
    secret      TEXT NOT NULL,      -- HMAC-SHA256 signing key (stored encrypted)
    events      TEXT NOT NULL,      -- JSON array: ["book.added", "import.completed", ...]
    enabled     INTEGER NOT NULL DEFAULT 1,
    last_delivery_at TEXT,
    last_error  TEXT,
    created_at  TEXT NOT NULL
);
CREATE INDEX idx_webhooks_user ON webhooks(user_id);

CREATE TABLE webhook_deliveries (
    id          TEXT PRIMARY KEY,
    webhook_id  TEXT NOT NULL REFERENCES webhooks(id) ON DELETE CASCADE,
    event       TEXT NOT NULL,
    payload     TEXT NOT NULL,      -- JSON
    status      TEXT NOT NULL DEFAULT 'pending'
                  CHECK(status IN ('pending', 'delivered', 'failed')),
    attempts    INTEGER NOT NULL DEFAULT 0,
    next_attempt_at TEXT,
    response_status INTEGER,
    created_at  TEXT NOT NULL,
    delivered_at TEXT
);
CREATE INDEX idx_webhook_deliveries_pending ON webhook_deliveries(status, next_attempt_at)
    WHERE status = 'pending';
```

## Reference Files

Read before starting each stage:
- `backend/src/api/admin.rs` — import job pattern to follow (Stage 1)
- `backend/src/db/queries/books.rs` — author query patterns (Stage 2)
- `backend/src/scheduler.rs` — scheduler loop to extend for webhook delivery (Stage 3)
- `apps/mobile/src/lib/downloads.ts` — existing download state (Stage 4)
- `apps/web/src/features/reader/EpubReader.tsx` — accessibility gaps (Stage 5)
- `backend/src/api/books.rs` — cover serving pattern to replicate for author photos (Stage 6)

---

## STAGE 1 — Goodreads / StoryGraph CSV Import

**Priority: High (common first-run migration action)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/api/admin.rs (the bulk import endpoint and job queue pattern),
backend/src/db/queries/books.rs, backend/src/db/queries/mod.rs,
and backend/migrations/sqlite/0015_annotations.sql (for migration format reference).
Now implement Goodreads and StoryGraph CSV import.

─────────────────────────────────────────
SCHEMA — migration 0016
─────────────────────────────────────────

backend/migrations/sqlite/0016_goodreads_import.sql:

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
      errors       TEXT,            -- JSON array of { row, title, author, reason }
      created_at   TEXT NOT NULL,
      completed_at TEXT
  );

backend/migrations/mariadb/0015_goodreads_import.sql — equivalent.

─────────────────────────────────────────
DELIVERABLE 1 — CSV parsing
─────────────────────────────────────────

backend/Cargo.toml — add:
  csv = "1"

backend/src/ingest/goodreads.rs — new file:

  pub struct GoodreadsRow {
    pub title: String,
    pub author: String,
    pub my_rating: u8,              -- 0 if not rated
    pub date_read: Option<String>,  -- YYYY/MM/DD or None
    pub bookshelves: Vec<String>,   -- comma-split "Bookshelves" column
    pub exclusive_shelf: String,    -- "read", "currently-reading", "to-read"
  }

  pub fn parse_goodreads_csv(bytes: &[u8]) -> Result<Vec<GoodreadsRow>, AppError>
    Use the `csv` crate. The Goodreads export format is:
      Book Id,Title,Author,Author l-f,Additional Authors,ISBN,ISBN13,My Rating,
      Average Rating,Publisher,Binding,Number of Pages,Year Published,
      Original Publication Year,Date Read,Date Added,Bookshelves,
      Bookshelves with positions,Exclusive Shelf,My Review,Spoiler,Private Notes,
      Read Count,Owned Copies

    Parse: Title, Author, My Rating, Date Read, Bookshelves, Exclusive Shelf.
    Skip header row. Treat empty Date Read as None.

  pub struct StorygraphRow {
    pub title: String,
    pub authors: String,
    pub read_status: String,        -- "read", "currently-reading", "to-read"
    pub star_rating: Option<f32>,
    pub date_finished: Option<String>,
    pub tags: Vec<String>,
  }

  pub fn parse_storygraph_csv(bytes: &[u8]) -> Result<Vec<StorygraphRow>, AppError>
    StoryGraph export columns:
      Title,Authors,Read Status,Star Rating (x/5),Review,Last Date Read,
      Dates Read,Tags,Owned

─────────────────────────────────────────
DELIVERABLE 2 — Import job
─────────────────────────────────────────

backend/src/api/users.rs — add route:
  POST /users/me/import/goodreads   — multipart upload of CSV file
  POST /users/me/import/storygraph  — multipart upload of CSV file
  GET  /users/me/import/:job_id     — poll import job status

  POST handler:
    1. Parse the uploaded CSV (parse_goodreads_csv or parse_storygraph_csv)
    2. Create a goodreads_import_log row (status = "pending")
    3. Spawn a background task (tokio::spawn):
       For each row:
         a. Search books by title + author (case-insensitive LIKE match)
         b. If found (one match):
            - If exclusive_shelf == "read": SET book_user_state.is_read = 1,
              read_at = date_read (or now())
            - If my_rating > 0: SET books.rating = my_rating * 2
              (Goodreads uses 1–5 stars; xcalibre-server uses 0–10)
            - For each shelf in bookshelves: find or create shelf by name,
              add book to shelf
            - Increment matched counter
         c. If not found: append { row, title, author, reason: "not_in_library" }
            to errors; increment unmatched counter
       4. UPDATE goodreads_import_log SET status = 'complete', completed_at = now()

  GET /users/me/import/:job_id:
    Return the goodreads_import_log row as JSON.
    Poll every 2 seconds from the frontend until status != 'pending' and != 'running'.

─────────────────────────────────────────
DELIVERABLE 3 — Web UI
─────────────────────────────────────────

apps/web/src/features/profile/ImportPage.tsx — new page:
  Route: /profile/import

  Tabs: "Goodreads" | "StoryGraph"

  Each tab:
    - Brief explanation: "Upload your export file to import reading history and shelves"
    - Link to the export instructions (Goodreads: goodreads.com/review/import;
      StoryGraph: your profile → Export)
    - File drag-drop zone (.csv files only)
    - On drop: POST to the appropriate endpoint; poll GET for status
    - Progress: spinner with "Processing row N of M..."
    - Result summary:
        ✓ 127 books matched and updated
        ○ 14 books not found in your library (collapsible list of titles)
    - "Import again" button resets the form

  Add "Import History" link to Profile sidebar.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_goodreads_import.rs:
  test_parse_goodreads_csv_extracts_title_and_author
  test_parse_goodreads_csv_handles_empty_date_read
  test_parse_goodreads_csv_splits_bookshelves
  test_import_marks_matching_book_as_read
  test_import_sets_rating_from_goodreads_stars
  test_import_creates_shelf_if_not_exists
  test_import_adds_book_to_existing_shelf
  test_import_records_unmatched_books_in_errors
  test_import_status_endpoint_returns_progress
  test_parse_storygraph_csv_extracts_read_status

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/web/src/features/profile/ImportPage.tsx
git commit -m "Phase 14 Stage 1: Goodreads and StoryGraph CSV import (reading history + shelves)"
```

---

## STAGE 2 — Author Management (Bio, Photo, Merge)

**Priority: Medium**
**Blocks: Stage 6 (author photo upload uses the same storage path). Stage 6 can be done independently.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/db/queries/books.rs (author query patterns),
backend/src/api/admin.rs, backend/src/api/books.rs (cover upload pattern),
backend/migrations/sqlite/0016_goodreads_import.sql (migration format reference),
and docs/SCHEMA.md (authors table definition).
Now implement author management: profiles, global merge, and an author detail page.

─────────────────────────────────────────
SCHEMA — migration 0017
─────────────────────────────────────────

backend/migrations/sqlite/0017_author_profiles.sql:

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

backend/migrations/mariadb/0016_author_profiles.sql — equivalent.

─────────────────────────────────────────
DELIVERABLE 1 — Author profile API
─────────────────────────────────────────

backend/src/api/authors.rs — new file with routes:

  GET    /authors/:id             — author detail (books + profile)
  PATCH  /authors/:id             — update profile (bio, born, died, website, openlibrary_id) [can_edit]
  POST   /admin/authors/:id/merge — merge source author into target [Admin]

  GET /authors/:id response:
    {
      "id": "...",
      "name": "Terry Pratchett",
      "sort_name": "Pratchett, Terry",
      "profile": {
        "bio": "...",
        "photo_url": "/authors/ab/uuid.webp",  -- null if no photo
        "born": "1948",
        "died": "2015",
        "website_url": "https://www.terrypratchettbooks.com",
        "openlibrary_id": "OL25980A"
      },
      "book_count": 41,
      "books": BookSummary[]   -- paginated, sorted by pubdate DESC
    }

  PATCH /authors/:id:
    Fields: bio, born, died, website_url, openlibrary_id
    Upsert into author_profiles (INSERT OR REPLACE).
    Photo upload is a separate endpoint — see Stage 6.
    Return updated author detail.

  POST /admin/authors/:id/merge:
    Body: { "into_author_id": "uuid" }
    Merges source (id) into target (into_author_id):
    1. UPDATE book_authors SET author_id = into_author_id
       WHERE author_id = source_id
       AND NOT EXISTS (
         SELECT 1 FROM book_authors
         WHERE book_id = book_authors.book_id AND author_id = into_author_id
       )
       -- skip books already attributed to the target author
    2. DELETE FROM book_authors WHERE author_id = source_id
    3. DELETE FROM author_profiles WHERE author_id = source_id
    4. DELETE FROM authors WHERE id = source_id
    All in one transaction. Return: { "books_updated": N, "target_author": Author }

─────────────────────────────────────────
DELIVERABLE 2 — Author detail page (web)
─────────────────────────────────────────

apps/web/src/features/library/AuthorPage.tsx — new page:
  Route: /authors/:id

  Layout:
    Left column (1/4 width):
      - Author photo (or letter placeholder, same design as book cover placeholder)
      - Name (heading)
      - Bio (expandable — collapsed at 4 lines)
      - Born / Died
      - Website link (if set)
      - OpenLibrary link (if openlibrary_id set)

    Right column (3/4 width):
      - "N books" heading
      - Same cover grid as library, filtered to this author
      - Pagination

    Edit button (visible to can_edit users): opens a slide-in panel with PATCH form.
    Admin merge button (visible to admin): opens a combobox to pick the target author.

  Author name links in book cards and book detail already exist — update them to
  navigate to /authors/:id instead of filtering the library grid.

─────────────────────────────────────────
DELIVERABLE 3 — Admin author browser
─────────────────────────────────────────

apps/web/src/features/admin/AuthorsPage.tsx:
  Route: /admin/authors

  Table: Name | Books | Has Profile | Actions
  Actions: Edit profile (navigates to /authors/:id) | Merge

  Add "Authors" to the admin sidebar nav (below Tags).

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_author_management.rs:
  test_get_author_detail_includes_books
  test_get_author_detail_includes_profile_when_present
  test_get_author_detail_profile_null_when_absent
  test_patch_author_creates_profile
  test_patch_author_updates_existing_profile
  test_merge_author_moves_books_to_target
  test_merge_author_skips_duplicate_attributions
  test_merge_author_deletes_source
  test_merge_author_is_atomic

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/web/src/features/
git commit -m "Phase 14 Stage 2: author management — profiles, detail page, admin merge"
```

---

## STAGE 3 — Webhook Delivery

**Priority: Medium (self-hosters frequently want external notifications)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/scheduler.rs, backend/src/lib.rs, backend/Cargo.toml,
backend/migrations/sqlite/0017_author_profiles.sql (for migration format reference),
and backend/src/api/admin.rs (for auth patterns).
Now implement webhook delivery.

─────────────────────────────────────────
SCHEMA — migration 0018
─────────────────────────────────────────

backend/migrations/sqlite/0018_webhooks.sql:

  CREATE TABLE webhooks (
      id          TEXT PRIMARY KEY,
      user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      url         TEXT NOT NULL,
      secret      TEXT NOT NULL,     -- AES-256-GCM encrypted HMAC key (same pattern as totp_secret)
      events      TEXT NOT NULL,     -- JSON array of event names
      enabled     INTEGER NOT NULL DEFAULT 1,
      last_delivery_at TEXT,
      last_error  TEXT,
      created_at  TEXT NOT NULL
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

backend/migrations/mariadb/0017_webhooks.sql — equivalent (drop WHERE clause from index).

─────────────────────────────────────────
EVENTS
─────────────────────────────────────────

Supported event names:
  book.added          — fired on successful book ingest
  book.deleted        — fired on DELETE /books/:id
  import.completed    — fired when a bulk import job finishes
  llm_job.completed   — fired when an LLM classify/validate job finishes
  user.registered     — fired on new user registration (admin webhooks only)

Payload shape (all events share this envelope):
  {
    "event": "book.added",
    "timestamp": "2026-04-22T20:00:00Z",
    "library_name": "My Library",
    "data": { /* event-specific */ }
  }

  book.added data:    { id, title, authors, formats, cover_url }
  book.deleted data:  { id, title }
  import.completed data: { job_id, total, succeeded, failed, duration_ms }
  llm_job.completed data: { job_id, type, book_id, title }
  user.registered data:   { id, username, role }

─────────────────────────────────────────
DELIVERABLE 1 — Webhook CRUD API
─────────────────────────────────────────

backend/src/api/webhooks.rs — new file:

  GET    /users/me/webhooks               — list own webhooks
  POST   /users/me/webhooks               — create webhook
  PATCH  /users/me/webhooks/:id           — update webhook (url, events, enabled)
  DELETE /users/me/webhooks/:id           — delete webhook
  POST   /users/me/webhooks/:id/test      — fire a test ping

  POST /users/me/webhooks body:
    { "url": "https://example.com/hook", "secret": "my-secret", "events": ["book.added"] }
    Validate url is HTTPS (reject HTTP — secrets would travel unencrypted).
    Validate events array contains only known event names.
    Encrypt the secret before storing (AES-256-GCM, same pattern as totp_secret).

  POST /users/me/webhooks/:id/test:
    Fire a delivery with event "ping" and payload { "message": "Webhook test from xcalibre-server" }.
    Return the delivery result synchronously (attempt the HTTP call immediately, max 5s timeout).

─────────────────────────────────────────
DELIVERABLE 2 — Delivery engine
─────────────────────────────────────────

backend/src/webhooks.rs — new file:

  pub async fn enqueue_event(db, event: &str, payload: serde_json::Value)
    1. Find all enabled webhooks WHERE events JSON contains event name
    2. For each webhook, INSERT INTO webhook_deliveries (status='pending', next_attempt_at=now())
    This is fire-and-forget from the caller — no waiting for delivery.

  pub async fn deliver_pending(db, http_client: &reqwest::Client)
    Called by the scheduler loop every 30 seconds.
    SELECT * FROM webhook_deliveries WHERE status='pending' AND next_attempt_at <= now() LIMIT 50
    For each delivery:
      1. Decrypt webhook.secret
      2. Build HMAC-SHA256 signature: HMAC-SHA256(secret, payload_json_string)
      3. POST to webhook.url with:
           Content-Type: application/json
           X-Xcalibre-server-Signature: sha256={hex_signature}
           X-Xcalibre-server-Event: {event}
         Body: payload JSON
         Timeout: 10 seconds
      4. On 2xx: UPDATE status='delivered', delivered_at=now(), response_status=N
                 UPDATE webhooks SET last_delivery_at=now(), last_error=NULL
      5. On failure (non-2xx or timeout):
           attempts += 1
           if attempts >= 3: UPDATE status='failed', last_error=error_message
           else: UPDATE next_attempt_at = now() + backoff (30s → 5m → 30m)
                 UPDATE webhooks SET last_error=error_message

  Add call sites for enqueue_event at:
    - book ingest completion (ingest pipeline)
    - DELETE /books/:id handler
    - import job completion
    - LLM job completion
    - POST /auth/register success

backend/src/scheduler.rs — add webhook delivery to the scheduler loop:
  Every 30 seconds: deliver_pending(&db, &http_client).await

─────────────────────────────────────────
DELIVERABLE 3 — SSRF prevention
─────────────────────────────────────────

Webhook URLs must not target internal/private IP ranges.
Apply the same SSRF check as the LLM endpoint validation:
  Resolve the URL's hostname; reject if it resolves to a private IP.
  Check at webhook creation time AND before each delivery.
  On private IP: reject with 422 { "error": "ssrf_blocked", "message": "Webhook URL must be a public endpoint" }

─────────────────────────────────────────
DELIVERABLE 4 — Web UI
─────────────────────────────────────────

apps/web/src/features/profile/WebhooksPage.tsx:
  Route: /profile/webhooks

  Table: URL | Events | Last delivery | Status | Actions (Test | Edit | Delete)
  Add webhook button opens a form (URL, secret, event checkboxes).
  "Test" button fires POST .../test and shows the result inline (delivered/failed + status code).

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_webhooks.rs:
  test_create_webhook_stores_encrypted_secret
  test_create_webhook_rejects_http_url
  test_create_webhook_rejects_unknown_events
  test_create_webhook_rejects_private_ip_ssrf
  test_enqueue_event_creates_delivery_for_subscribed_webhooks
  test_enqueue_event_skips_disabled_webhooks
  test_delivery_sends_correct_hmac_signature
  test_delivery_retries_on_failure
  test_delivery_marks_failed_after_3_attempts
  test_test_endpoint_fires_ping_synchronously

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/web/src/features/profile/WebhooksPage.tsx
git commit -m "Phase 14 Stage 3: webhook delivery — CRUD, HMAC signing, retry, SSRF guard"
```

---

## STAGE 4 — Mobile Download Queue UI

**Priority: Medium (polish — download infrastructure exists, UI does not)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read apps/mobile/src/lib/downloads.ts,
apps/mobile/src/app/book/[id].tsx,
apps/mobile/src/app/(tabs)/library.tsx,
and apps/mobile/src/app/(tabs)/profile.tsx.
Now add a download queue management UI.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

apps/mobile/src/lib/downloads.ts already handles individual file downloads
via expo-file-system and tracks state in the local SQLite DB (local_downloads table).
The gap is: there is no screen showing what's downloaded, what's downloading,
or how much storage is used. Users cannot manage or delete downloads.

─────────────────────────────────────────
DELIVERABLE 1 — Downloads screen
─────────────────────────────────────────

apps/mobile/src/app/downloads.tsx — new screen:

  Accessible from: Profile tab → "Downloads" row.

  Header: "Downloads"
  Storage summary bar at top:
    [Used: 1.2 GB] [████░░░░░░] [Available: 12.4 GB]
    Query expo-file-system.getFreeDiskStorageAsync() for available space.
    Sum file sizes from local_downloads for used space.

  Section 1 — In Progress (only shown if downloads are active):
    FlatList of downloading books:
      - Cover + title + format badge
      - ProgressBar (expo-file-system download progress callback)
      - Cancel button (calls FileSystem.downloadAsync cancellation)

  Section 2 — Downloaded (all completed downloads):
    FlatList grouped by format:
      - Cover + title + format + file size (human-readable: "3.4 MB")
      - Swipe-to-delete gesture → confirm alert → delete file + remove from DB
      - Tap: navigate to book reader

  Section 3 — Failed:
    FlatList:
      - Cover + title + error message
      - "Retry" button → re-attempt download

  Empty state: "No downloads yet. Tap ↓ on any book to download for offline reading."

─────────────────────────────────────────
DELIVERABLE 2 — Batch download (Shelf)
─────────────────────────────────────────

apps/mobile/src/app/shelf/[id].tsx — add "Download all" action:

  Header right button: "⬇ Download all" (only shown if shelf has books).
  On tap: confirmation alert — "Download N books? This may use up to X MB."
  On confirm: enqueue each book's preferred format (EPUB > PDF > first available)
  to downloads.ts; show toast "Download started for N books".

  Preferred format priority: user preference from settings if set; otherwise
  EPUB → MOBI → PDF → first available.

─────────────────────────────────────────
DELIVERABLE 3 — Storage warning
─────────────────────────────────────────

In apps/mobile/src/lib/downloads.ts — before starting any download:
  Check expo-file-system.getFreeDiskStorageAsync()
  If the download would leave less than 200MB free: show Alert.alert warning
  "Low storage: only Xmb remaining. Continue?" with Cancel/Download buttons.

─────────────────────────────────────────
DELIVERABLE 4 — Profile tab entry point
─────────────────────────────────────────

apps/mobile/src/app/(tabs)/profile.tsx — add "Downloads" row to the settings list:
  [Download icon]  Downloads  [N files · X MB]  [→]
  The subtitle "N files · X MB" queries the local_downloads table count and sums sizes.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
pnpm --filter @xs/mobile exec tsc --noEmit
# Manual: download a book; open Downloads screen; verify it appears with correct size
# Manual: swipe to delete; verify file is removed from device storage
git add apps/mobile/src/app/downloads.tsx apps/mobile/src/app/shelf/[id].tsx apps/mobile/src/app/(tabs)/profile.tsx apps/mobile/src/lib/
git commit -m "Phase 14 Stage 4: mobile download queue — queue view, batch shelf download, storage management"
```

---

## STAGE 5 — Accessibility Audit + Remediation (WCAG 2.1 AA)

**Priority: High (inclusivity; WCAG 2.1 AA is the standard target)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read apps/web/src/features/reader/EpubReader.tsx,
apps/web/src/features/library/LibraryPage.tsx,
apps/web/src/features/auth/LoginPage.tsx,
apps/web/src/features/admin/,
and apps/web/src/components/ui/.
Now audit and remediate WCAG 2.1 AA issues across the web app.

─────────────────────────────────────────
SCOPE
─────────────────────────────────────────

Audit and fix the following areas in priority order:

1. Keyboard navigation (WCAG 2.1.1, 2.1.2)
2. Focus management in modals and panels (WCAG 2.4.3)
3. Color contrast (WCAG 1.4.3)
4. Screen reader announcements for async updates (WCAG 4.1.3)
5. Form labels and error associations (WCAG 1.3.1, 3.3.1)
6. Reader toolbar keyboard access (WCAG 2.1.1)

─────────────────────────────────────────
DELIVERABLE 1 — Keyboard navigation
─────────────────────────────────────────

apps/web/src/features/library/LibraryPage.tsx:
  Book cards must be keyboard focusable and activatable:
  - Add tabIndex={0} to each book card div
  - Add onKeyDown handler: Enter/Space → navigate to book detail (same as click)
  - Arrow keys within the grid: left/right/up/down move focus between cards
    Implement a custom keyboard handler on the grid container.

apps/web/src/features/reader/EpubReader.tsx:
  Reader toolbar must be keyboard accessible:
  - Tab into the toolbar: settings gear, TOC button, back button are all focusable
  - When toolbar is hidden, it must NOT be reachable by Tab (aria-hidden + tabIndex=-1 when opacity=0)
  - Keyboard shortcut: Left/Right arrow keys turn pages (only when toolbar is not focused)
  - Escape: exit reader → navigate back to book detail
  Add a visually-hidden keyboard shortcuts help panel (press ? to toggle).

apps/web/src/components/ui/Sheet.tsx (slide-in panels):
  Focus trap when panel is open:
  - On open: focus the first focusable element inside the panel
  - Tab/Shift+Tab cycle only within the panel while it is open
  - Escape closes the panel and returns focus to the trigger element
  Use the @radix-ui/react-focus-scope or a manual trap implementation.

apps/web/src/components/ui/Dialog.tsx (destructive confirmation dialogs):
  - Focus the "Cancel" button (safer default) when dialog opens
  - Focus trap within dialog
  - Escape closes dialog (same as Cancel)
  - aria-modal="true" on dialog container
  - aria-labelledby pointing to the dialog heading

─────────────────────────────────────────
DELIVERABLE 2 — Color contrast
─────────────────────────────────────────

Check the following against WCAG 1.4.3 (4.5:1 for normal text, 3:1 for large text):

  Light mode:
    zinc-500 text on white background: #71717a on #ffffff = 4.6:1 ✓ (just passing)
    teal-600 links on white: #0d9488 on #ffffff = 4.5:1 ✓ (exactly at limit)
    zinc-400 placeholder text on zinc-50: #a1a1aa on #fafafa = 2.6:1 ✗ FAIL
    → Fix: change placeholder text to zinc-500 (#71717a) in light mode

  Dark mode:
    zinc-400 secondary text on zinc-900 surface: #a1a1aa on #18181b = 5.8:1 ✓
    teal-400 links on zinc-950: #2dd4bf on #09090b = 8.1:1 ✓
    zinc-500 placeholder on zinc-800 input: #71717a on #27272a = 2.9:1 ✗ FAIL
    → Fix: change input placeholder in dark mode to zinc-400

  Reader sepia theme:
    Verify body text contrast on sepia (#fdf6e3) background — must be ≥ 4.5:1.

Update apps/web/src/index.css or relevant Tailwind config classes to apply the fixes.

─────────────────────────────────────────
DELIVERABLE 3 — Screen reader announcements
─────────────────────────────────────────

Add aria-live regions for async state changes:

  apps/web/src/features/library/LibraryPage.tsx:
    <div aria-live="polite" aria-atomic="true" className="sr-only">
      {isLoading ? "Loading books..." : `${books.length} books loaded`}
    </div>

  apps/web/src/features/search/SearchPage.tsx:
    <div aria-live="polite" aria-atomic="true" className="sr-only">
      {isSearching ? "Searching..." : results.length === 0 ? "No results found" : `${results.length} results found`}
    </div>

  apps/web/src/features/admin/ (import progress):
    <div aria-live="polite" aria-atomic="true" className="sr-only">
      {importStatus}  // "Import started", "Processing...", "Import complete: 42 books added"
    </div>

  Toast notifications (apps/web/src/components/ui/Toast.tsx):
    Add role="status" and aria-live="polite" to the toast container.

─────────────────────────────────────────
DELIVERABLE 4 — Form labels and errors
─────────────────────────────────────────

Audit all forms for label association:

  apps/web/src/features/auth/LoginPage.tsx:
    - <label htmlFor="username"> must be associated with <input id="username">
    - Error messages: aria-describedby linking input to error paragraph
    - "Invalid username or password" announced via aria-live on the error container

  apps/web/src/features/admin/UsersPage.tsx (inline edit):
    - Inline edit inputs must have aria-label with the field name + user name context
    - Example: aria-label="Username for user John Doe"

  PATCH form in book detail (metadata edit):
    - All inputs must have <label> or aria-label
    - Required fields: aria-required="true"

─────────────────────────────────────────
DELIVERABLE 5 — Semantic HTML audit
─────────────────────────────────────────

Spot-check and fix:
  - Library grid: use <ul role="list"> with <li> per card — communicates count to screen readers
  - Admin tables: <table> with <th scope="col"> headers (not <div>-based tables)
  - Reader progress bar: <progress value={pct} max={100} aria-label="Reading progress: 42%">
  - Sidebar navigation: <nav aria-label="Main navigation"> wrapping the sidebar
  - Admin panel: <nav aria-label="Admin navigation"> for the admin sidebar

─────────────────────────────────────────
DELIVERABLE 6 — CI accessibility check
─────────────────────────────────────────

apps/web/package.json — add:
  "@axe-core/playwright": "^4"

apps/web/e2e/accessibility.spec.ts:
  import { checkA11y } from "@axe-core/playwright";

  test("library page has no critical a11y violations", async ({ page }) => {
    await loginAsAdmin(page);
    await page.goto("/library");
    await checkA11y(page, undefined, {
      runOnly: { type: "tag", values: ["wcag2a", "wcag2aa"] },
    });
  });

  test("reader has no critical a11y violations", async ({ page }) => { ... });
  test("login page has no critical a11y violations", async ({ page }) => { ... });
  test("admin panel has no critical a11y violations", async ({ page }) => { ... });

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
pnpm --filter @xs/web build
pnpm --filter @xs/web test:e2e -- --grep "accessibility"
# Manual: navigate entire app using only Tab, Shift+Tab, Enter, Escape, Arrow keys
# Manual: run with macOS VoiceOver or NVDA — verify library grid and reader are usable
git add apps/web/src/ apps/web/e2e/accessibility.spec.ts
git commit -m "Phase 14 Stage 5: WCAG 2.1 AA remediation — keyboard nav, contrast, screen reader, semantic HTML"
```

---

## STAGE 6 — Author Photo Storage

**Priority: Low (depends on Stage 2 author profiles being in place)**
**Blocks: nothing. Blocked by: Stage 2 must be complete first.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/api/books.rs (the cover upload and render_cover_variants sections),
backend/src/api/authors.rs (from Phase 14 Stage 2),
backend/src/storage.rs, and backend/src/db/queries/tags.rs (for DB update pattern).
Now add author photo upload and serving.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Phase 14 Stage 2 added author_profiles with a photo_path column (NULL if no photo).
This stage adds the actual upload, processing, and serving — mirroring the book
cover implementation exactly:
  - Same bucketed storage path: authors/{first2}/{author_id}.jpg and .webp
  - Same two sizes: full (400×400 square crop) and thumbnail (100×100)
  - Same content negotiation: WebP if Accept: image/webp, JPEG fallback
  - Same letter-placeholder when photo_path IS NULL

Note: author photos are square (1:1 ratio) vs book covers (2:3).

─────────────────────────────────────────
DELIVERABLE 1 — Upload endpoint
─────────────────────────────────────────

backend/src/api/authors.rs — add:

  POST /authors/:id/photo       — upload author photo [can_edit]
  GET  /authors/:id/photo       — serve author photo (or placeholder) [Any authenticated]

  POST /authors/:id/photo:
    Multipart body: { photo: File } (JPEG, PNG, WebP accepted)
    Processing (mirrors render_cover_variants):
      1. Decode image with the `image` crate
      2. Center-crop to 1:1 aspect ratio (square)
      3. Resize to 400×400 → save as JPEG (authors/{ab}/{id}.jpg)
      4. Resize to 100×100 → save as JPEG thumbnail (authors/{ab}/{id}.thumb.jpg)
      5. Encode as WebP at quality 85 → save (authors/{ab}/{id}.webp)
      6. Encode as WebP at quality 85 → save thumbnail (authors/{ab}/{id}.thumb.webp)
      7. UPDATE author_profiles SET photo_path = 'authors/{ab}/{id}.jpg', updated_at = now()
    Return 200: updated author profile object.

  GET /authors/:id/photo:
    ?size=thumb (optional) — serves thumbnail variant
    Content negotiation same as book cover: WebP if accepted, JPEG fallback.
    If photo_path IS NULL: return a generated placeholder SVG:
      <svg>: 400×400, deterministic background color from author name hash (teal/zinc palette),
      first letter of author name centered, large serif font.
      Content-Type: image/svg+xml
      This matches the web CoverPlaceholder component behavior.

─────────────────────────────────────────
DELIVERABLE 2 — Web UI integration
─────────────────────────────────────────

apps/web/src/features/library/AuthorPage.tsx (from Stage 2):
  Replace the static placeholder in the author photo slot with:
    <img src={`/api/v1/authors/${id}/photo`} ... />
  Add an upload button (visible to can_edit users) on hover over the photo:
    - File input (hidden), triggered by clicking the photo overlay
    - On file select: POST /authors/:id/photo
    - On success: refetch author detail (react-query invalidateQueries)

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_author_photos.rs:
  test_upload_photo_generates_jpeg_and_webp_variants
  test_upload_photo_updates_photo_path_in_profile
  test_serve_photo_returns_webp_when_accepted
  test_serve_photo_falls_back_to_jpeg_when_webp_not_accepted
  test_serve_photo_returns_svg_placeholder_when_no_photo
  test_serve_photo_placeholder_varies_by_author_name
  test_upload_photo_rejects_non_image_files

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/web/src/features/library/AuthorPage.tsx
git commit -m "Phase 14 Stage 6: author photo upload + serving (JPEG + WebP, placeholder SVG)"
```

---

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/review` — verify CSV parsing handles malformed rows gracefully, no overwrite of manually-set ratings without confirmation |
| Stage 2 | `/review` — verify author merge is atomic, duplicate attribution suppression is correct, admin-only enforcement |
| Stage 3 | `/review` + `/security-review` — verify HMAC signing is correct, SSRF guard fires at creation AND delivery, webhook secret encrypted at rest, no internal events leak user data |
| Stage 4 | `/review` — verify storage warning fires before download starts, swipe-delete removes file from device, batch download respects storage warning |
| Stage 5 | `/review` — verify focus trap in modals, keyboard navigation does not trap users, aria-live regions do not spam announcements |
| Stage 6 | `/review` + `/security-review` — verify uploaded photos are image-validated before processing, no path traversal in photo_path, placeholder SVG does not include unsanitized author name |

Run `/engineering:deploy-checklist` after Stage 6 before tagging v1.3.
