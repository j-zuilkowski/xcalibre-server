# xcalibre-server — Feature Reference

A complete inventory of everything xcalibre-server provides. For API details see [`API.md`](API.md). For architecture see [`ARCHITECTURE.md`](ARCHITECTURE.md). For installation and usage see [`USER_GUIDE.md`](USER_GUIDE.md).

---

## Authentication & Security

- **Password login** — Argon2id hashing, JWT access tokens, rotating refresh tokens with `SameSite=Strict` HttpOnly cookies.
- **Two-factor authentication (TOTP)** — RFC 6238 time-based one-time passwords. Setup, confirm, disable, and 8 single-use backup codes (SHA-256-hashed at rest). Pending TOTP sessions use a short-lived signed token so the user's main session is never issued until the second factor passes.
- **Magic link login** — Passwordless email link, single-use, 15-minute expiry. Delivered via SMTP; login completes in any browser, not just the one that requested the link.
- **OAuth 2.0** — Google and GitHub providers. Account linking and unlinking from the profile page. New accounts can be created via OAuth without a local password.
- **LDAP authentication** — Bind-and-search against any LDAP/AD server. Role mapped from LDAP group membership.
- **API tokens** — Scoped (read/write/admin) and expiring. Issued from the profile page; used by OPDS clients, Kobo devices, and API scripts.
- **Account lockout** — Progressive brute-force protection: after `max_login_attempts` (default 10) failures, the account is locked for `lockout_duration_mins` (default 30). Returns 429 Too Many Requests when locked.
- **Per-IP TOTP rate limiting** — Locked TOTP accounts return 429 (not 401) to distinguish brute-force from invalid credentials.
- **Role-based access control** — `admin` and `user` roles with granular per-role capability flags (upload, download, public shelves, metadata edit, etc.).
- **Session management** — Active sessions listed in profile; individual session revocation; refresh token rotation (old token invalidated on each use).
- **Security headers** — `Content-Security-Policy`, `X-Frame-Options`, `X-Content-Type-Options`, `Strict-Transport-Security`, `Referrer-Policy` on every response.
- **Path traversal prevention** — All file-serving routes validate paths against the configured storage root before any I/O.
- **Domain allowlist/blocklist** — Admin-managed list controls which external domains are permitted for webhook delivery and OAuth callbacks.

---

## Library Management

### Browsing & Discovery
- **Grid and list views** — Cover-dominant grid or compact list with sortable columns.
- **Browse by facet** — Author, publisher, series, rating, language, tag, format, read/unread.
- **Letter-based browsing** — Authors and series indexed A–Z with jump-to-letter.
- **Hot/trending** — Most-downloaded books over a rolling window.
- **New releases** — Most recently added books.
- **Read/unread state** — Per-user, persisted across devices.
- **Reading statistics** — Books read, pages read, reading streaks, average session length.
- **Reading goals** — Annual target with progress indicator.
- **Download history** — Per-user history with timestamps.

### Search
- **Three-tier search** — FTS5 (always available) → Meilisearch (when running) → semantic vector search (when embedding model configured). Automatically falls back gracefully.
- **Full-text search** — FTS5 virtual table over title, author, series, publisher, description, tags. 11 sync triggers keep it current.
- **Meilisearch integration** — Millisecond-latency ranked search with typo tolerance. Fully optional — server runs without it.
- **Semantic / vector search** — Embedding-based similarity search via `sqlite-vec`. Optional; requires an embedding model endpoint.
- **Hybrid search** — BM25 + cosine similarity scores merged via Reciprocal Rank Fusion (RRF).
- **Advanced search** — Filter by multiple facets in a single query (author + tag + language + format).
- **OPDS OpenSearch descriptor** — Allows e-reader clients to discover and use the search endpoint.

### Book Management
- **Single upload** — Drag-and-drop or file picker. Auto-extracts title, author, cover, description from EPUB/PDF/CBZ/etc.
- **Bulk import** — Directory scan; skips duplicates by file hash.
- **Multiple formats per book** — EPUB, PDF, MOBI, AZW3, CBZ, CBR, DJVU, and more. Add formats to existing books without duplicating metadata.
- **Cover upload & auto-extract** — Upload a replacement cover or let the server extract from the book file. Cover stored in four variants: original, WebP, 240×240 thumbnail, 600×600 large.
- **Cover bucketing** — Covers stored in 256 two-character prefix buckets to stay within filesystem directory entry limits on any storage backend.
- **Metadata editing** — Edit title, author, series, publisher, description, tags, rating, language, identifiers (ISBN, Google Books, Open Library).
- **Bulk metadata editing** — Apply changes to multiple books in one operation.
- **Book merge** — Combine two book records: move formats, annotations, shelf links, and reading progress into a target book, then delete the source. Preview mode shows exactly what will change before committing.
- **Delete** — Delete a book (removes all formats and cover files) or delete a specific format only.
- **Inline serve** — Serve a book file directly to the browser without forcing a download header. Used for PDF.js and EPUB readers.
- **Highlights and annotations** — EPUB in-browser reader supports highlights; annotations stored per-user per-book.

### Metadata Enrichment
- **Open Library** — Search by title/author; retrieve cover, description, publisher, publication date, ISBN.
- **Google Books** — Same surface area; results cached to avoid re-fetching.
- **Enrichment cache** — Results stored per (title, author) pair; TTL configurable.
- **LLM-assisted classification** — When an LLM librarian role is configured, books can be auto-classified by genre, reading level, and subject. Suggestions only — user confirms before saving.
- **Metadata apply** — One-click apply from enrichment results: writes title/description/publisher/pubdate, upserts identifiers, optionally downloads and stores a new cover.

---

## E-Reader Support

### OPDS Catalog
- **Root catalog** — Standard Atom feed with navigation entries.
- **Facet feeds** — Author, series, publisher, language, rating, tag listing feeds, each with a books sub-feed.
- **New, hot, discover, stats feeds** — 30 most recent; 30 most downloaded; shelf navigation; library stats JSON.
- **Cover serving** — `GET /opds/cover/:id` (JPEG/WebP), `/thumb` (240×240), `/large` (600×600). 200-entry LRU cache.
- **OpenSearch** — `GET /opds/osd` descriptor for client search integration.
- **Token-authenticated downloads** — Download links include `?token=` so OPDS clients authenticate without HTTP Basic Auth.
- **OPDS shelf feed** — Public shelves browsable from any OPDS client.
- **OPDS read/unread feeds** — Filter to read or unread books from an OPDS client.
- **OPDS formats feed** — Browse by file format from an OPDS client.

### Kobo Sync
- **Library sync** — Delta sync via `sync_token` timestamp cursor. Device receives only changed books since last sync. Paginated at 100 books per call.
- **Reading state sync** — `PUT /library/:book_id/state` from device upserts `kobo_reading_state` and syncs progress to `reading_progress` so it appears in the web/mobile UI.
- **Tag/shelf sync** — Kobo shelves created and renamed from the device are reflected as xcalibre shelves.
- **Bookmark sync** — Kobo bookmarks stored per device and restored on re-sync.
- **Mock store endpoints** — 14 fake Kobo store endpoints required by older firmware for sync handshake. All return calibre-web-compatible JSON.
- **Cover serving by UUID** — `GET /kobo/:token/v1/images/:uuid/…` resolves the book UUID via the `identifiers` table, fetches the cover path, and serves the actual cover image through the configured storage backend. Falls back to a 1×1 white JPEG placeholder only when no cover exists.
- **Firmware compatibility** — Tested with Kobo firmware 4.20+; mock store endpoints ensure older firmware handshakes complete successfully.

### Send to Kindle
- Email delivery via SMTP. Configurable from, to, and subject template. Format selection (MOBI preferred).

---

## Admin

### User & Role Management
- Create, edit, disable, delete users.
- Per-role capability flags: upload, download, public shelves, metadata edit, admin.
- Password reset by admin (no email required).
- Force password reset on next login.

### Scheduled Tasks
- Cron-expression tasks: backup, re-index, cover regeneration, custom shell commands.
- Task status, cancellation, and history viewable via API.

### Database Backup
- `POST /api/v1/admin/backup` — creates a consistent, restorable SQLite snapshot using `VACUUM INTO`. Safe to run while the database is open and under write load. No checkpoint management required.
- Backup files named `xcalibre-<YYYYMMDD-HHMMSS>.db` and written to the configured `[backup] dir`.
- AtomicBool prevents concurrent backup runs (returns 409 if one is already in progress).
- Scheduled backups via the task scheduler (e.g., `0 3 * * *` for 3 AM daily).

### Cover Regeneration
- `POST /api/v1/admin/covers/regenerate` — enqueues a `CoverRegenerate` task per book (empty array = all books). Returns 202 immediately; workers process asynchronously.

### Log Viewer
- `GET /api/v1/admin/logs?lines=N&level=` — reads last N lines from the configured log file; parses as structured JSON; skips unparseable lines.

### Self-Update
- In-app update checker (`GET /api/v1/admin/update/check`) compares the running version against the latest GitHub release tag.
- `POST /api/v1/admin/update/apply` downloads and hot-swaps the binary (Linux only; requires write access to the binary path).

### Domain Rules
- Admin-managed allowlist/blocklist for external domains (webhook delivery targets, OAuth callback domains).

---

## Shelves (Reading Lists)

- Create, rename, delete shelves.
- Add/remove books from shelves.
- Public shelves — shareable without login.
- Reorder books within a shelf (drag-and-drop order persisted).
- Shelf browsable from OPDS clients.

---

## Collections & Chunked Search

- **Collections** — Group books into a named, domain-tagged collection for chunked semantic search.
- **Sub-chapter chunking** — Text split at section, procedure, or paragraph boundaries (not fixed token size). Domain-aware: `technical`, `electronics`, `culinary`, `legal`, `academic`, `narrative`.
- **Procedural list detection** — Numbered step sequences kept as atomic chunks.
- **Chunked semantic search** — Search within a collection at the chunk level; returns the most relevant passages, not just book titles.
- **Cross-document synthesis** — `/synthesize` endpoint (MCP-accessible) queries multiple books in a collection and synthesizes a single answer with source attribution.
- **BM25 + cosine + RRF** — Hybrid ranking at the chunk level.
- **Optional cross-encoder re-ranking** — Second-pass LLM re-ranking of top-K chunks; configurable; off by default.

---

## LLM / AI Features (Optional)

All AI features are **disabled by default** (`enable_llm_features = false`). When disabled, all LLM endpoints return 503 silently — no errors surface to users. All LLM calls have a 10-second timeout with silent fallback.

- **Classification and tagging** — Auto-classify books by genre, reading level, and subject using the Librarian LLM role.
- **Semantic search** — Embedding model generates vectors for chunks; queries embedded on the fly.
- **Cross-document synthesis** — Architect LLM role synthesizes answers across multiple books.
- **Metadata validation** — Validate book metadata against internet sources (Open Library, Google Books). Flag mismatches for review; never auto-overwrite.
- **Per-role LLM config** — Each LLM feature (`librarian`, `architect`, `embedding`) has its own endpoint, model, system prompt, and timeout.
- **Model auto-discovery** — When `model = ""` in config, the client probes `GET /v1/models` and uses the first result. Works with LM Studio and Ollama.

---

## Storage Backends

- **Local filesystem** — Default. Files stored at `storage.path`.
- **S3-compatible** — AWS S3, MinIO, Backblaze B2. Configured via `[storage.s3]` block. All file reads and writes go through the `StorageBackend` trait; handlers are backend-agnostic.
- **HTTP range requests** — `GET /api/v1/books/:id/formats/:ext` supports `Range` headers for large file streaming and resumable downloads.

---

## Internationalisation

- UI translations for English, French, German, and Spanish.
- `i18n-check` CI workflow validates translation key coverage across all locales on every push.

---

## Webhooks

- Register external HTTP endpoints to receive events: `book.created`, `book.updated`, `book.deleted`, `user.created`, `backup.completed`, and more.
- Delivery retried with exponential backoff. Payload capped at 1 MB. Domain allowlist enforced at registration.

---

## API & Integrations

- **REST API** — Full OpenAPI 3.0 spec (`/api/v1/openapi.json`). Every endpoint documented with request/response schemas.
- **MCP server** — `calibre-mcp` binary exposes `search_books`, `get_book_metadata`, `list_chapters`, `get_book_text`, `semantic_search` as MCP tools. Used by Merlin and Claude Code for direct library access.
- **KAG integration** — `POST /api/v1/graph/triples` and `GET /api/v1/graph/traverse` used by Merlin's `XcalibreKAGPlugin` to fuse session knowledge triples with book knowledge triples. Book-level triple extraction runs at ingestion time.
- **Prometheus metrics** — `/metrics` endpoint (disable with `XCS_DISABLE_METRICS=1`).
- **CORS** — Configurable allowed origins for browser clients.

---

## Mobile App

- Expo (iOS + Android) with the same feature surface as the web app: browse, search, read, download, shelves, profile, TOTP setup.
- Offline-capable cover caching.

---

## Deployment

- **Docker Compose** (recommended) — single `docker compose up -d`. Includes app, optional Meilisearch, optional SMTP relay.
- **Synology NAS** — Docker Compose via Container Manager.
- **Bare metal** — `cargo run -p backend`; no OS dependencies beyond SQLite.
- **Raspberry Pi 4** — Supported; ARM64 Docker image published.
- **Multi-arch Docker** — `linux/amd64` and `linux/arm64` published to `ghcr.io`.
- **Automatic migrations** — sqlx migrations run at startup. No manual steps between versions.
- **Zero-downtime updates** — `POST /api/v1/admin/update/apply` on Linux; Docker pull + restart on other platforms.

---

## Developer & Quality

- **TDD** — Every feature has a failing integration test committed before implementation. No production code ships without a corresponding test.
- **Dual-database** — All queries and migrations maintain parity between SQLite (default) and MariaDB (production scale).
- **Zero clippy warnings** — `cargo clippy -- -D warnings` enforced in CI on every push.
- **`cargo audit`** — CVE scan in CI.
- **Playwright E2E** — Full browser tests against a live server + Vite frontend.
- **CI** — GitHub Actions: test, clippy, audit, E2E, Docker build, i18n coverage check.

---

_Last updated: May 2026. Version 2.7.0_
