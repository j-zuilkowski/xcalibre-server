# Codex Desktop App — xcalibre-server Phase 9: Feature Parity

## What Phase 9 Builds

Closes the feature gap between xcalibre-server and the original calibre-web. After this phase,
xcalibre-server supports every major calibre-web feature plus its own AI/MCP additions.

- **Stage 1** — OPDS catalog, email/Kindle delivery, CBZ/CBR reader, bulk metadata edit,
  shelves UI completion
- **Stage 2** — OAuth/SSO (Google + GitHub), LDAP authentication, book metadata lookup
  (Open Library + Google Books — Goodreads API is deprecated)
- **Stage 3** — Kobo sync (device registration, library sync, reading progress, collections)
- **Stage 4** — Multi-library support (per-user library assignment, library switcher,
  admin library management)

## Key Design Decisions

- OPDS: XML feed, no auth required for browsing, token-gated for download — OPDS-PS 1.2
- Email delivery: SMTP config in `config.toml`; format sent as-is (no conversion in v1);
  Calibre binary conversion deferred to Phase 10
- CBZ reader: page-image viewer in React, images extracted server-side from zip;
  no external library required
- OAuth: `oauth2` crate; callback route `/auth/oauth/:provider/callback`;
  auto-creates local user on first login (email as username)
- LDAP: `ldap3` crate; bind DN + filter configurable in `config.toml`; falls back
  to local auth if LDAP is unreachable
- Goodreads API was deprecated in 2020 — use Open Library (`openlibrary.org/api`)
  and Google Books (`googleapis.com/books/v1`) instead
- Kobo sync: reverse-engineered protocol from calibre-web source; token-per-device
  in `kobo_devices` table; shelves ↔ Kobo collections bidirectional sync
- Multi-library: `library_id` added to books table; each library has its own
  Calibre DB path; users assigned a default library; admins can switch any user's library

## Key Schema Facts (new tables this phase)

```sql
-- Stage 1
shelves (id, user_id, name, is_public, created_at, updated_at)
shelf_books (shelf_id, book_id, added_at)
email_settings (id, smtp_host, smtp_port, smtp_user, smtp_password_enc,
                from_address, use_tls, updated_at)

-- Stage 2
oauth_accounts (id, user_id, provider, provider_user_id, email, created_at)
-- ldap config lives in config.toml, no new table

-- Stage 3
kobo_devices (id, user_id, device_id, device_name, sync_token, last_sync_at,
              created_at)
kobo_reading_state (id, device_id, book_id, kobo_position, percent_read,
                    last_modified)

-- Stage 4
libraries (id, name, calibre_db_path, created_at, updated_at)
-- books table: add library_id TEXT NOT NULL DEFAULT 'default'
-- users table: add default_library_id TEXT
```

## Reference Files

Read before starting each stage:
- `docs/ARCHITECTURE.md` — overall design constraints
- `docs/SCHEMA.md` — existing schema to understand what already exists
- `backend/src/api/mod.rs` — where to mount new route groups
- `backend/src/db/queries/books.rs` — existing query patterns to follow
- `backend/src/middleware/auth.rs` — auth patterns for new auth methods
- `backend/migrations/sqlite/` — existing migrations for numbering

---

## STAGE 1 — Quick Wins: OPDS, Email, CBZ Reader, Bulk Edit, Shelves

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, docs/SCHEMA.md, backend/src/api/mod.rs, and
backend/src/db/queries/books.rs. Now implement Stage 1 of Phase 9.

Five deliverables. Implement them in order.

─────────────────────────────────────────
DELIVERABLE 1 — OPDS Catalog
─────────────────────────────────────────

backend/src/api/opds.rs — new file:
  GET /opds                    → root catalog feed (navigation)
  GET /opds/catalog            → all books feed (acquisition, paginated)
  GET /opds/search             → OpenSearch description XML
  GET /opds/search?q=          → search results feed (acquisition)
  GET /opds/new                → recently added (last 30 days)

- All responses: Content-Type: application/atom+xml; charset=utf-8
- OPDS-PS 1.2 compliant: <feed>, <entry>, <link rel="acquisition">,
  <link rel="http://opds-spec.org/image"> for covers
- Download links point to existing GET /api/v1/books/:id/formats/:format/download
- Cover links point to existing GET /api/v1/books/:id/cover
- No auth required to browse; download links require Bearer token in header
  (OPDS clients send token via Authorization header)
- Pagination: use existing list_books query with page/page_size params;
  include <link rel="next"> and <link rel="previous"> in feed
- Mount under opds Blueprint: app.nest_service("/opds", opds_router())

backend/src/api/opds.rs tests (inline #[cfg(test)]):
  test_opds_root_returns_atom_xml
  test_opds_catalog_paginated
  test_opds_search_returns_results
  test_opds_download_requires_auth

─────────────────────────────────────────
DELIVERABLE 2 — Email / Send-to-Kindle
─────────────────────────────────────────

backend/migrations/sqlite/0006_email_settings.sql:
  CREATE TABLE email_settings (
    id              TEXT PRIMARY KEY DEFAULT 'singleton',
    smtp_host       TEXT NOT NULL DEFAULT '',
    smtp_port       INTEGER NOT NULL DEFAULT 587,
    smtp_user       TEXT NOT NULL DEFAULT '',
    smtp_password   TEXT NOT NULL DEFAULT '',
    from_address    TEXT NOT NULL DEFAULT '',
    use_tls         INTEGER NOT NULL DEFAULT 1,
    updated_at      TEXT NOT NULL
  );

backend/migrations/mariadb/0005_email_settings.sql — equivalent MariaDB DDL.

backend/src/db/queries/email_settings.rs:
  pub struct EmailSettings { all fields }
  pub async fn get_email_settings(db) -> Result<Option<EmailSettings>>
  pub async fn upsert_email_settings(db, settings) -> Result<EmailSettings>

backend/src/api/admin.rs — add two routes:
  GET  /api/v1/admin/email-settings  → returns current settings (password masked as "")
  PUT  /api/v1/admin/email-settings  → upserts; admin only

backend/src/api/books.rs — add one route:
  POST /api/v1/books/:id/send
  Body: { "to": "user@kindle.com", "format": "epub" }
  - Requires auth (any user)
  - Looks up book format file, reads from storage
  - Sends via lettre crate (add to Cargo.toml) using email_settings
  - Returns 204 on success, 503 if email not configured, 404 if format missing

Add lettre = { version = "0.11", features = ["tokio1-rustls-tls"] } to
backend/Cargo.toml.

Tests in backend/tests/test_email.rs:
  test_send_returns_503_when_not_configured
  test_admin_can_update_email_settings
  test_send_book_by_email (mock SMTP with wiremock or lettre's stub transport)

─────────────────────────────────────────
DELIVERABLE 3 — CBZ/CBR Comic Reader
─────────────────────────────────────────

backend/src/api/books.rs — add two routes:
  GET /api/v1/books/:id/comic/pages
    → returns { total_pages: N, pages: [{ index: 0, url: "/api/v1/books/:id/comic/page/0" }] }
  GET /api/v1/books/:id/comic/page/:index
    → extracts image at index from CBZ (zip) or CBR (rar via unrar crate),
      returns image bytes with correct Content-Type (image/jpeg or image/png)
    → CBZ: use zip crate; CBR: use unrar crate (add both to Cargo.toml)
    → Sort entries by filename before indexing
    → Return 404 if index out of range

Add zip = "0.6" and unrar = "0.5" to backend/Cargo.toml.

apps/web/src/features/reader/ComicReader.tsx — new component:
  - Fetches /comic/pages on mount
  - Displays current page image full-width
  - Previous/Next buttons + keyboard arrow key navigation
  - Page counter "3 / 42"
  - Preloads next page image

apps/web/src/features/reader/ReaderPage.tsx — update format detection:
  When format is CBZ or CBR, render <ComicReader bookId={id} /> instead of
  "Unsupported reader format"

Tests in backend/tests/test_comic.rs:
  test_comic_pages_returns_page_list
  test_comic_page_returns_image_bytes
  test_comic_page_out_of_range_returns_404

─────────────────────────────────────────
DELIVERABLE 4 — Bulk Metadata Edit
─────────────────────────────────────────

backend/src/api/books.rs — add one route:
  PATCH /api/v1/books
  Body: {
    "book_ids": ["id1", "id2"],
    "fields": {
      "tags":   { "mode": "append|overwrite|remove", "values": ["tag1"] },
      "series": { "mode": "overwrite", "value": "Dune" },
      "rating": { "mode": "overwrite", "value": 4 }
    }
  }
  - Requires admin role
  - Applies each field operation to all book_ids in a single transaction
  - Returns { updated: N, errors: [] }
  - Supported fields: tags, series, rating, language, publisher
  - title and author are excluded from bulk edit (too risky)

backend/src/db/queries/books.rs — add bulk_update_books() query helper.

Tests in backend/tests/test_bulk_edit.rs:
  test_bulk_append_tags
  test_bulk_overwrite_series
  test_bulk_edit_requires_admin
  test_bulk_edit_empty_ids_returns_400

─────────────────────────────────────────
DELIVERABLE 5 — Shelves UI Completion
─────────────────────────────────────────

The shelves DB schema already exists. Wire up the frontend.

backend/src/api/shelves.rs — new file (if routes don't exist):
  GET    /api/v1/shelves              → list user's shelves
  POST   /api/v1/shelves             → create shelf { name, is_public }
  DELETE /api/v1/shelves/:id         → delete (owner only)
  POST   /api/v1/shelves/:id/books   → add book { book_id }
  DELETE /api/v1/shelves/:id/books/:book_id → remove book
  GET    /api/v1/shelves/:id/books   → list books on shelf (paginated)

If routes already exist, read them first and only add what is missing.

apps/web/src/features/shelves/ShelvesPage.tsx — replace the "not wired up yet"
stub with a working UI:
  - List of user's shelves with book count
  - Create shelf button (name input, public toggle)
  - Click shelf → BookGrid showing that shelf's books
  - Remove book from shelf button on each card

apps/web/src/features/books/BookDetailPage.tsx — add "Add to shelf" dropdown
  showing user's shelf names; calls POST /api/v1/shelves/:id/books.

Tests in backend/tests/test_shelves.rs:
  test_create_shelf
  test_add_book_to_shelf
  test_remove_book_from_shelf
  test_list_shelf_books

apps/web/src/features/library/ShelvesPage.test.tsx — implement every test
  from localProject/TEST_SPEC.md section "ShelvesPage".
  Use renderWithProviders({ initialPath: "/shelves" }).
  Add MSW handlers:
    GET  /api/v1/shelves           → []
    POST /api/v1/shelves           → created shelf fixture
    GET  /api/v1/shelves/:id/books → []
    DELETE /api/v1/shelves/:id     → 204

─────────────────────────────────────────
VERIFICATION — TDD BUILD LOOP
─────────────────────────────────────────
cargo clippy --workspace -- -D warnings

TDD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --workspace 2>&1 | tail -40
    pnpm --filter @xs/web test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error output for that test.
      2. Read the relevant source file (component or handler).
      3. Fix the test if the assertion was wrong, fix the component/handler
         if the behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    pnpm --filter @xs/web dev &
    @Computer Use — open http://localhost:5173 in the in-app browser
    Verify:
      - /shelves — shelf list renders with book counts
      - /shelves — Create Shelf button opens name input and public toggle
      - /shelves — clicking a shelf shows its books in a grid
      - /books/1 — "Add to shelf" dropdown present and shows shelf names
      - /books/1/read/cbz — comic reader renders page image with prev/next buttons
        and page counter (e.g. "1 / 42")
    Kill the dev server: kill %1

pnpm --filter @xs/web build

Commit when all tests are green:
  git add apps/web/src/features/library/ShelvesPage.test.tsx \
          backend/tests/test_shelves.rs \
          backend/tests/test_email.rs \
          backend/tests/test_comic.rs \
          backend/tests/test_bulk_edit.rs
  git commit -m "test(phase9): shelves, email, comic, bulk-edit tests"
```

---

## STAGE 2 — Auth Integrations: OAuth, LDAP, Metadata Lookup

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, backend/src/middleware/auth.rs,
backend/src/api/auth.rs, and backend/src/db/queries/auth.rs.
Now implement Stage 2 of Phase 9.

Three deliverables.

─────────────────────────────────────────
DELIVERABLE 1 — OAuth / SSO
─────────────────────────────────────────

backend/migrations/sqlite/0007_oauth_accounts.sql:
  CREATE TABLE oauth_accounts (
    id                TEXT PRIMARY KEY,
    user_id           TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider          TEXT NOT NULL,           -- 'google' | 'github'
    provider_user_id  TEXT NOT NULL,
    email             TEXT NOT NULL,
    created_at        TEXT NOT NULL,
    UNIQUE(provider, provider_user_id)
  );
  CREATE INDEX idx_oauth_accounts_user_id ON oauth_accounts(user_id);

backend/migrations/mariadb/0006_oauth_accounts.sql — equivalent MariaDB DDL.

Add to backend/Cargo.toml:
  oauth2 = "4"
  reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }

config.toml — add optional [oauth] section:
  [oauth.google]
  client_id     = ""
  client_secret = ""
  [oauth.github]
  client_id     = ""
  client_secret = ""

backend/src/api/auth.rs — add four routes:
  GET /auth/oauth/:provider            → redirects to provider authorization URL
  GET /auth/oauth/:provider/callback   → exchanges code for token, fetches user
                                         profile, upserts oauth_accounts row,
                                         creates local user if first login
                                         (username = email, random password),
                                         issues JWT + refresh cookie, redirects to /
  Supported providers: google, github
  Return 400 for unknown provider.
  Return 501 if provider not configured in config.

backend/src/db/queries/oauth.rs:
  pub async fn find_by_provider(db, provider, provider_user_id) -> Result<Option<OauthAccount>>
  pub async fn create_oauth_account(db, user_id, provider, provider_user_id, email) -> Result<OauthAccount>

apps/web/src/features/auth/LoginPage.tsx — add "Sign in with Google" and
"Sign in with GitHub" buttons below the password form. Each links to
/auth/oauth/google and /auth/oauth/github respectively. Only render buttons
for providers that are configured (add GET /api/v1/auth/providers route that
returns { google: bool, github: bool }).

Tests in backend/tests/test_oauth.rs:
  test_oauth_unknown_provider_returns_400
  test_oauth_unconfigured_provider_returns_501
  test_oauth_callback_creates_user_on_first_login (mock provider token exchange)
  test_oauth_callback_reuses_existing_account

─────────────────────────────────────────
DELIVERABLE 2 — LDAP Authentication
─────────────────────────────────────────

Add to backend/Cargo.toml:
  ldap3 = { version = "0.11", default-features = false, features = ["tls-rustls"] }

config.toml — add optional [ldap] section:
  [ldap]
  enabled    = false
  url        = "ldap://localhost:389"
  bind_dn    = "cn=admin,dc=example,dc=com"
  bind_pw    = ""
  search_base = "ou=users,dc=example,dc=com"
  uid_attr   = "uid"          -- attribute that maps to username
  email_attr = "mail"

backend/src/auth/ldap.rs — new file:
  pub async fn authenticate_ldap(config, username, password)
    -> Result<Option<LdapUser>, LdapError>
  struct LdapUser { pub username: String, pub email: String }

  Logic:
  1. Bind with bind_dn/bind_pw
  2. Search for uid_attr=username under search_base
  3. If found, attempt bind with user's DN + supplied password
  4. Return LdapUser on success, None on wrong password, Err on connection failure

backend/src/api/auth.rs — update POST /api/v1/auth/login:
  After local password check fails (user not found OR wrong password),
  if ldap.enabled, try authenticate_ldap.
  On success: auto-create local user if not exists (username=ldap username,
  email=ldap email, random password, role=User), then issue JWT normally.
  LDAP connection failure should log a warning but not block local auth.

Tests in backend/tests/test_ldap.rs:
  test_ldap_disabled_skips_ldap (no LDAP connection attempt)
  test_ldap_auth_creates_user_on_first_login (mock LDAP with ldap3 test server or stub)
  test_ldap_wrong_password_returns_401
  test_ldap_connection_failure_falls_through_to_local

─────────────────────────────────────────
DELIVERABLE 3 — Book Metadata Lookup
─────────────────────────────────────────

(Note: Goodreads API was deprecated in 2020. Use Open Library and Google Books.)

backend/src/api/books.rs — add one route:
  GET /api/v1/books/:id/metadata-lookup
  Query params: ?source=openlibrary|googlebooks (default: openlibrary)
  - Fetches book's ISBN from identifiers table
  - If no ISBN, uses title + author as search query
  - Calls appropriate external API
  - Returns { source, title, authors, description, publisher, published_date,
              cover_url, isbn_13, categories } — never auto-applies to book
  - Returns 404 if no external match found
  - Returns 503 if external API unreachable (5s timeout)

Open Library: GET https://openlibrary.org/api/books?bibkeys=ISBN:{isbn}&format=json&jscmd=data
Google Books: GET https://www.googleapis.com/books/v1/volumes?q=isbn:{isbn}

backend/src/db/queries/books.rs — add get_book_identifiers(db, book_id) if not present.

apps/web/src/features/books/BookEditPage.tsx — add "Lookup Metadata" button:
  - Calls /metadata-lookup
  - Shows results in a side panel with "Apply" buttons per field
  - Apply button calls existing PATCH /api/v1/books/:id for that field only

Tests in backend/tests/test_metadata_lookup.rs:
  test_metadata_lookup_by_isbn_open_library (mock HTTP with wiremock)
  test_metadata_lookup_by_title_author_fallback
  test_metadata_lookup_external_timeout_returns_503
  test_metadata_lookup_no_match_returns_404

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

---

## STAGE 3 — Kobo Sync

**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, docs/SCHEMA.md, backend/src/api/mod.rs,
backend/src/db/queries/books.rs, and backend/src/api/auth.rs.
Now implement Stage 3 of Phase 9: Kobo sync.

The Kobo sync protocol is reverse-engineered and well-documented.
xcalibre-server acts as a Kobo content server. The Kobo device registers once,
then syncs its library and reading state on each connection.

─────────────────────────────────────────
SCHEMA
─────────────────────────────────────────

backend/migrations/sqlite/0008_kobo.sql:
  CREATE TABLE kobo_devices (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    device_id     TEXT NOT NULL UNIQUE,   -- sent by Kobo in X-Kobo-DeviceId header
    device_name   TEXT NOT NULL DEFAULT 'Kobo',
    sync_token    TEXT,                   -- opaque token returned to device
    last_sync_at  TEXT,
    created_at    TEXT NOT NULL
  );
  CREATE INDEX idx_kobo_devices_user_id ON kobo_devices(user_id);
  CREATE INDEX idx_kobo_devices_device_id ON kobo_devices(device_id);

  CREATE TABLE kobo_reading_state (
    id             TEXT PRIMARY KEY,
    device_id      TEXT NOT NULL REFERENCES kobo_devices(id) ON DELETE CASCADE,
    book_id        TEXT NOT NULL,
    kobo_position  TEXT,        -- Kobo CFI-like position string
    percent_read   REAL,
    last_modified  TEXT NOT NULL,
    UNIQUE(device_id, book_id)
  );

backend/migrations/mariadb/0007_kobo.sql — equivalent MariaDB DDL.

─────────────────────────────────────────
AUTH: KOBO TOKEN
─────────────────────────────────────────

Kobo devices authenticate via a token embedded in the URL path, not Bearer.
Add a kobo_auth middleware that:
  1. Reads :kobo_token from the path
  2. Looks up the API token (same api_tokens table from Phase 8) by SHA256 hash
  3. Resolves to a user; attaches KoboDevice to request extensions
  4. Rejects with 401 if token unknown

─────────────────────────────────────────
ROUTES (all under /kobo/:kobo_token/v1/)
─────────────────────────────────────────

backend/src/api/kobo.rs — new file:

  GET  /kobo/:kobo_token/v1/initialization
    → returns Kobo JSON config: store URLs pointing back to this server,
      feature flags; device_id from X-Kobo-DeviceId header auto-registers device

  GET  /kobo/:kobo_token/v1/library/sync
    → incremental sync: returns list of changed books since device's sync_token
    → each book entry: { BookMetadata: { title, authors, isbn, ... },
                          DownloadUrls: [{ Format: "EPUB", Url: "..." }] }
    → only returns books with EPUB or PDF formats (Kobo supports these)
    → updates device sync_token; next call uses new token for delta sync
    → shelves → Kobo collections: include CollectionChanges in response

  PUT  /kobo/:kobo_token/v1/library/:kobo_book_id/state
    Body: Kobo reading state JSON (position, percent_read, last_modified)
    → upsert into kobo_reading_state
    → also sync to the unified reading_progress table so web UI reflects progress
    → return 200

  GET  /kobo/:kobo_token/v1/library/:kobo_book_id/metadata
    → returns full Kobo-format book metadata for one book

  DELETE /kobo/:kobo_token/v1/library/:kobo_book_id
    → marks book as removed from Kobo device (does not delete from library)

  GET  /kobo/:kobo_token/v1/user/profile
    → returns Kobo user profile JSON (username, email)

backend/src/api/admin.rs — add:
  GET  /api/v1/admin/kobo-devices      → list all registered Kobo devices
  DELETE /api/v1/admin/kobo-devices/:id → revoke device

apps/web/src/features/admin/KoboDevicesPage.tsx — new page:
  Table of registered devices (name, last sync, user)
  Revoke button per device
  Sync URL display: {base_url}/kobo/{api_token}/v1/

─────────────────────────────────────────
DELIVERABLES SUMMARY
─────────────────────────────────────────
backend/migrations/sqlite/0008_kobo.sql
backend/migrations/mariadb/0007_kobo.sql
backend/src/api/kobo.rs
backend/src/db/queries/kobo.rs
apps/web/src/features/admin/KoboDevicesPage.tsx

Tests in backend/tests/test_kobo.rs:
  test_kobo_initialization_registers_device
  test_kobo_sync_returns_book_list
  test_kobo_sync_delta_returns_only_changed
  test_kobo_reading_state_syncs_to_progress_table
  test_kobo_unknown_token_returns_401

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

---

## STAGE 4 — Multi-Library Support

**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, docs/SCHEMA.md, backend/src/api/mod.rs,
backend/src/db/queries/books.rs, and backend/src/config.rs.
Now implement Stage 4 of Phase 9: multi-library support.

─────────────────────────────────────────
SCHEMA
─────────────────────────────────────────

backend/migrations/sqlite/0009_libraries.sql:
  CREATE TABLE libraries (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL UNIQUE,
    calibre_db_path TEXT NOT NULL,   -- absolute path to metadata.db
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
  );

  -- Seed the default library from config.app.calibre_db_path
  INSERT INTO libraries (id, name, calibre_db_path, created_at, updated_at)
  VALUES ('default', 'Default Library', '', datetime('now'), datetime('now'));

  -- Add library_id to books (nullable FK for backwards compat)
  ALTER TABLE books ADD COLUMN library_id TEXT NOT NULL DEFAULT 'default'
    REFERENCES libraries(id);
  CREATE INDEX idx_books_library_id ON books(library_id);

  -- Add default library preference to users
  ALTER TABLE users ADD COLUMN default_library_id TEXT NOT NULL DEFAULT 'default'
    REFERENCES libraries(id);

backend/migrations/mariadb/0008_libraries.sql — equivalent MariaDB DDL.

─────────────────────────────────────────
BACKEND
─────────────────────────────────────────

backend/src/db/queries/libraries.rs — new file:
  pub struct Library { pub id, pub name, pub calibre_db_path, pub created_at, pub updated_at }
  pub async fn list_libraries(db) -> Result<Vec<Library>>
  pub async fn get_library(db, id) -> Result<Option<Library>>
  pub async fn create_library(db, name, path) -> Result<Library>
  pub async fn delete_library(db, id) -> Result<bool>  -- reject if books still assigned

backend/src/api/admin.rs — add routes:
  GET    /api/v1/admin/libraries         → list all libraries
  POST   /api/v1/admin/libraries         → create { name, calibre_db_path }
  DELETE /api/v1/admin/libraries/:id     → delete (fails if books assigned)

backend/src/api/users.rs (or auth.rs) — add route:
  PATCH /api/v1/users/me/library         → set default_library_id for current user

backend/src/db/queries/books.rs — update list_books and get_book_by_id:
  Add library_id filter parameter to ListBooksParams.
  All book queries filter by the requesting user's default_library_id unless
  the user is admin (admins can see all libraries).

backend/src/middleware/auth.rs — attach user's default_library_id to
  AuthenticatedUser so handlers can access it without extra DB call.

─────────────────────────────────────────
FRONTEND
─────────────────────────────────────────

apps/web/src/features/admin/LibrariesPage.tsx — new page:
  Table of libraries (name, path, book count)
  Add library button (name + path fields)
  Delete button (disabled if books present, tooltip explains why)

apps/web/src/features/layout/Header.tsx (or equivalent nav) — add library
  switcher dropdown for users with access to multiple libraries.
  On switch: calls PATCH /api/v1/users/me/library, reloads book list.

─────────────────────────────────────────
MIGRATION SAFETY
─────────────────────────────────────────

The xs-migrate binary imports from a Calibre DB. Update it to accept
an optional --library-id flag (default: "default") so imported books are
tagged to the correct library.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_libraries.rs:
  test_admin_can_create_library
  test_admin_cannot_delete_library_with_books
  test_books_filtered_by_user_default_library
  test_user_can_switch_library
  test_admin_sees_all_libraries

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
```

---

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/review` — verify OPDS feed format, shelves wiring |
| Stage 2 | `/review` + `/security-review` — OAuth callback flow, LDAP bind credentials |
| Stage 3 | `/review` — Kobo sync delta correctness, reading state round-trip |
| Stage 4 | `/review` — library_id filter coverage, no cross-library data leaks |

Run `/engineering:deploy-checklist` after Stage 4 before merging to main.
