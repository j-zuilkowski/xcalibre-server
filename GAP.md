# xcalibre-server — Gap Analysis vs. calibre-web

> Compares **xcalibre-server** (Rust/Axum) against **calibre-web** (Python/Flask) as a web-based e-book server.
>
> Last updated: 2026-05-09 (Phase 30 gaps identified — false positives corrected)
> Reference: `archive/calibre-web` · calibre-web v0.6.x · xcalibre-server HEAD

---

## Legend

| Symbol | Meaning |
|--------|---------|
| ✅ | Done — equivalent or better capability |
| ⚠️ | Partial — some capability exists but with gaps |
| 🟦 | Planned — in scope, not yet implemented |
| ❌ | Not implemented / out of scope |

---

## Route Count Summary

| System | Approximate route count |
|--------|------------------------|
| calibre-web (Flask) | ~240 routes |
| xcalibre-server (Axum) — before Phases 23–28 | ~100 routes |
| xcalibre-server (Axum) — after Phases 23–28 | ~165 routes |
| Remaining calibre-web routes not in xcalibre-server | ~75 (31%) |

After Phases 23–28, the major compatibility gaps (OPDS client support, Kobo firmware compatibility, shelf reordering, OAuth account management, book deduplication, and admin tooling) are closed. The remaining gap is primarily the HTML admin UI layer (xcalibre-server is intentionally API-only) and miscellaneous calibre-web-specific AJAX endpoints with no API equivalent.

---

## 1. Authentication & User Management

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Username + password login | ✅ | ✅ Argon2id + JWT | ✅ |
| Remember-me / persistent session | ✅ | ✅ refresh token (30-day) | ✅ |
| Magic link (passwordless) | ❌ | ✅ time-limited signed token | ✅ Unique |
| TOTP 2FA with backup codes | ❌ | ✅ totp-rs, HMAC backup codes | ✅ Unique |
| OAuth — Google | ✅ | ✅ | ✅ |
| OAuth — GitHub | ✅ | ✅ | ✅ |
| OAuth account linking (`/link/github`, `/link/google`) | ✅ | ✅ `GET /auth/oauth/:provider/link` + callback | ✅ Phase 26 |
| OAuth account unlinking | ✅ | ✅ `DELETE /api/v1/me/oauth/:provider` | ✅ Phase 26 |
| LDAP authentication | ✅ | ✅ ldap3 crate | ✅ |
| LDAP bulk user import (`/import_ldap_users`) | ✅ | ❌ | ❌ |
| Remote login token (`/remote/login`, `/verify/<token>`) | ✅ | ❌ (replaced by magic links) | ❌ Replaced |
| 2FA token verify via AJAX (`/ajax/verify_token`) | ✅ | ❌ | ❌ |
| Account lockout + brute-force protection | ✅ IP banning | ✅ rate limiting + lockout | ✅ |
| API tokens (long-lived, scoped) | ❌ | ✅ read/write/admin scopes | ✅ Unique |
| Per-user roles (admin/user) | ✅ | ✅ + role-based permissions | ✅ |
| User registration | ✅ configurable | ✅ | ✅ |

**Gap:** OAuth account link/unlink endpoints are not exposed. Remote login token flow is absent (by design — magic links replace it). LDAP bulk user import is missing.

---

## 2. Admin Panel & Configuration

calibre-web exposes 36+ admin routes as HTML pages and AJAX endpoints. xcalibre-server is API-only and relies on `config.toml` for system configuration.

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Admin configuration UI (`/admin/config`) | ✅ HTML form | ❌ config.toml only | ❌ |
| Database config UI (`/admin/dbconfig`) | ✅ | ❌ | ❌ |
| View/display config UI (`/admin/viewconfig`) | ✅ | ❌ | ❌ |
| User management table (`/admin/usertable`) | ✅ | ❌ API only (`GET /api/v1/admin/users`) | ⚠️ |
| Paginated user list AJAX (`/ajax/listusers`) | ✅ | ❌ | ❌ |
| Bulk edit user settings AJAX | ✅ | ❌ | ❌ |
| Log viewer (`/admin/logfile`, `/ajax/log/*`) | ✅ in-UI | ✅ `GET /api/v1/admin/logs?lines=&level=` | ✅ Phase 28 |
| Log download (`/admin/logdownload/*`) | ✅ | ❌ | ❌ |
| Debug panel (`/admin/debug`) | ✅ | ⚠️ `/health` + `/metrics` (Prometheus) | ⚠️ |
| Application update check (`/get_update_status`) | ✅ | ✅ self-update endpoint | ✅ |
| Apply update (`/get_updater_status`) | ✅ | ✅ | ✅ |
| Metadata backup (`/metadata_backup`) | ✅ | ✅ `POST /api/v1/admin/backup` | ✅ Phase 28 |
| Database reconnect (`/reconnect`) | ✅ | ❌ | ❌ |
| Simulate DB change (`/ajax/simulatedbchange`) | ✅ | ❌ | ❌ |
| Path picker AJAX (`/ajax/pathchooser/`) | ✅ | ❌ | ❌ |
| Bulk thumbnail regeneration (`/ajax/updateThumbnails`) | ✅ | ✅ `POST /api/v1/admin/covers/regenerate` | ✅ Phase 28 |
| Cancel background task (`/ajax/canceltask`) | ✅ | ✅ `DELETE /api/v1/admin/tasks/:id` | ✅ Phase 28 |
| Scheduled tasks UI (`/admin/scheduledtasks`) | ✅ | ❌ API only | ⚠️ |
| Force full sync (`/ajax/fullsync`) | ✅ | ❌ | ❌ |
| Locale/language list AJAX | ✅ | ❌ | ❌ |
| Health check (`/admin/alive`) | ✅ | ✅ `/health` | ✅ |

**Gap (reduced):** The HTML admin UI has no equivalent — xcalibre-server is API-only. Log viewing (`GET /api/v1/admin/logs`), metadata backups (`POST /api/v1/admin/backup`), task cancellation (`DELETE /api/v1/admin/tasks/:id`), and thumbnail regeneration (`POST /api/v1/admin/covers/regenerate`) are now all exposed as REST API endpoints (Phase 28). Remaining gaps: log download, DB reconnect, simulatedbchange, path picker, scheduled-tasks HTML form, force-full-sync, and locale-list AJAX.

---

## 3. Domain & Tag Restriction Management

calibre-web has a full whitelist/blacklist system for email domains and per-user tag restrictions.

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Domain allowlist/blocklist | ✅ 12 AJAX endpoints | ✅ `tag_restrictions` table + API | ⚠️ |
| Per-user tag restrictions | ✅ `/ajax/*restriction/*` | ✅ | ✅ |
| List domain restrictions | ✅ `/ajax/domainlist/<allow>` | ✅ `GET /api/v1/admin/domains?allow=` | ✅ Phase 28 |
| Add domain | ✅ | ✅ `POST /api/v1/admin/domains` | ✅ Phase 28 |
| Edit domain | ✅ | ❌ (delete + re-add) | ⚠️ |
| Delete domain | ✅ | ✅ `DELETE /api/v1/admin/domains/:id` | ✅ Phase 28 |
| List global tag restrictions | ✅ | ✅ | ✅ |
| Add/delete global restriction | ✅ | ✅ | ✅ |
| Per-user restriction override | ✅ | ✅ | ✅ |

**Gap (reduced):** Domain allowlist/blocklist CRUD is now fully exposed (Phase 28) and enforced at registration. Edit-in-place is not implemented — use delete + re-add. Tag restrictions remain fully covered.

---

## 4. Book Browsing & Filtering

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Browse by author (`/author`) | ✅ HTML page | ✅ `GET /api/v1/authors` | ✅ |
| Browse by publisher | ✅ | ✅ | ✅ |
| Browse by series | ✅ | ✅ | ✅ |
| Browse by rating | ✅ | ✅ | ✅ |
| Browse by format | ✅ | ✅ | ✅ |
| Browse by language | ✅ | ✅ | ✅ |
| Browse by tag/category | ✅ | ✅ | ✅ |
| Table view (`/table`) | ✅ HTML | ❌ | ❌ |
| Table column preferences (`/ajax/table_settings`) | ✅ | ❌ | ❌ |
| Toggle grid/list view (`/ajax/view`) | ✅ | ❌ (client-side) | ❌ |
| JSON autocomplete — authors | ✅ `/get_authors_json` | ✅ `GET /api/v1/authors?q=` | ✅ |
| JSON autocomplete — publishers | ✅ | ✅ | ✅ |
| JSON autocomplete — tags | ✅ | ✅ | ✅ |
| JSON autocomplete — series | ✅ | ✅ | ✅ |
| JSON autocomplete — languages | ✅ | ✅ | ✅ |
| Matching tags AJAX (`/get_matching_tags`) | ✅ | ✅ tag search | ✅ |
| Download history list (`/downloadlist`) | ✅ | ✅ `GET /api/v1/me/downloads` | ✅ |
| Series cover (`/series_cover/<id>`) | ✅ 2 resolutions | ❌ | ❌ |
| Hot/trending books | ✅ | ❌ | ❌ |
| New arrivals feed | ✅ | ✅ `?sort=created_at` | ✅ |
| Read / unread filter | ✅ | ✅ | ✅ |

**Gap:** Series cover serving is missing. Table view and column preference endpoints are absent (client-side concern in xcalibre). Hot/trending ranking has no equivalent.

---

## 5. Book Detail & Metadata Editing

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Book detail page | ✅ HTML | ✅ `GET /api/v1/books/:id` | ✅ |
| Edit single book metadata (UI) | ✅ `/admin/book/<id>` | ✅ `PATCH /api/v1/books/:id` | ✅ |
| Edit single field AJAX (`/ajax/editbooks/<param>`) | ✅ | ✅ PATCH with partial body | ✅ |
| Custom column enum values | ✅ `/ajax/getcustomenum/<id>` | ✅ custom columns API | ✅ |
| Sort value for field | ✅ `/ajax/sort_value/<field>/<id>` | ❌ | ❌ |
| Exchange/swap book formats | ✅ `/ajax/xchange` | ❌ | ❌ |
| Delete specific format | ✅ `POST /delete/<id>/<format>` | ✅ `DELETE /api/v1/books/:id/formats/:fmt` | ✅ |
| Delete book | ✅ | ✅ `DELETE /api/v1/books/:id` | ✅ |
| Upload book | ✅ multipart form | ✅ multipart + JSON | ✅ |
| Cover upload | ✅ | ✅ | ✅ |
| Format conversion (via Calibre binary) | ✅ | ❌ | ❌ |
| Ratings UX | ✅ full UI | ⚠️ field exists, no dedicated endpoint | ⚠️ |

**Gap:** Format conversion via Calibre binary is absent. Sort-value endpoint and format swap are not exposed. Ratings have no dedicated set/clear endpoint.

---

## 6. Bulk Book Operations

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Bulk metadata edit | ✅ `/ajax/editselectedbooks` | ✅ `POST /api/v1/books/bulk` | ✅ |
| Bulk archive | ✅ `/ajax/archiveselectedbooks` | ✅ bulk state update | ✅ |
| Bulk mark read | ✅ `/ajax/readselectedbooks` | ✅ bulk state update | ✅ |
| Bulk change display status | ✅ `/ajax/displayselectedbooks` | ❌ | ❌ |
| Merge duplicate books | ✅ `/ajax/mergebooks` | ✅ `POST /api/v1/admin/books/merge` | ✅ Phase 27 |
| Simulate merge (preview) | ✅ `/ajax/simulatemerge` | ✅ `POST /api/v1/admin/books/merge/preview` | ✅ Phase 27 |
| Bulk change display status | ✅ `/ajax/displayselectedbooks` | ❌ | ❌ |

**Gap (reduced):** Book merge with simulation/preview is now fully implemented (Phase 27), including format conflict detection, reading progress merge strategies, and transactional execution. Bulk display-status toggle remains absent.

---

## 7. Search

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Basic search (`/search` GET) | ✅ | ✅ `GET /api/v1/books?q=` | ✅ |
| Advanced search UI (`/advsearch`) | ✅ HTML form | ✅ query params on books endpoint | ⚠️ |
| Boolean search syntax | ❌ | ✅ FTS5 boolean | ✅ Unique |
| Semantic / vector search | ❌ | ✅ sqlite-vec embeddings | ✅ Unique |
| Hybrid search (BM25 + semantic, RRF fusion) | ❌ | ✅ | ✅ Unique |
| Cross-encoder reranking | ❌ | ✅ | ✅ Unique |
| Meilisearch backend | ❌ | ✅ optional | ✅ Unique |
| Full-text search across book content | ❌ | ✅ chunked + indexed | ✅ Unique |

**Gap:** xcalibre-server's search is a superset of calibre-web. The only missing piece is a dedicated `/search` HTML page (client-side concern).

---

## 8. Reading & Reading Progress

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| In-browser book reading | ✅ `/read/<id>/<format>` | ✅ | ✅ |
| Serve book without download (`/show/<id>/<format>`) | ✅ inline serve | ✅ `GET /api/v1/books/:id/view/:format` | ✅ Phase 25 |
| Per-user reading progress | ✅ | ✅ CFI-based + percentage | ✅ |
| Read/unread state | ✅ | ✅ `book_user_state` | ✅ |
| Annotations / highlights / bookmarks | ✅ | ✅ full CRUD | ✅ |
| Annotation browsing / filtering | ❌ | ❌ | ❌ |
| Reading statistics / streaks | ✅ | ✅ | ✅ |
| Download book (`/download/<id>/<format>`) | ✅ | ✅ | ✅ |
| Send to Kindle via email | ✅ | ✅ SMTP via lettre | ✅ |

**Gap:** The `/show` endpoint (serve inline without forcing download) is absent. Annotation browsing has no dedicated endpoint in either system.

---

## 9. Shelves

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Create shelf | ✅ | ✅ | ✅ |
| List shelves | ✅ | ✅ | ✅ |
| Add/remove book to shelf | ✅ | ✅ | ✅ |
| Delete shelf | ✅ | ✅ | ✅ |
| Edit shelf settings (rename, toggle public) | ✅ `/shelf/edit/<id>` | ❌ | ❌ **Phase 30** |
| Reorder books in shelf | ✅ `GET+POST /shelf/order/<id>` | ✅ `PUT /api/v1/shelves/:id/order` | ✅ Phase 25 |
| Simple/read-only shelf view | ✅ `/simpleshelf/<id>` | ❌ | ❌ |
| Public shelves | ✅ | ✅ | ✅ |

**Gap (reduced):** Shelf book reordering is now fully implemented (Phase 25) — transactional, validates completeness, 403 for non-owners. Shelf configuration editing and simple/read-only view remain absent. xcalibre-server also has Collections (richer than shelves, with RAG search) which calibre-web lacks.

---

## 10. OPDS Catalog

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Root catalog | ✅ | ✅ | ✅ |
| Browse by author / series / language / publisher | ✅ | ✅ | ✅ |
| Browse by rating | ✅ | ✅ | ✅ |
| Browse by tag/category (`/opds/category`, `/opds/category/:id`) | ✅ | ❌ | ❌ **False positive** |
| Category/tag letter browse (`/opds/category/letter/:id`) | ✅ | ❌ | ❌ |
| Letter-based browsing — authors | ✅ | ✅ `/opds/authors/letter/:char` (NFKD) | ✅ Phase 23 |
| Letter-based browsing — series | ✅ | ✅ `/opds/series/letter/:char` | ✅ Phase 23 |
| Browse by format (`/opds/formats`, `/opds/formats/:id`) | ✅ | ❌ | ❌ |
| Shelf feed (`/opds/shelfindex`, `/opds/shelf/:id`) | ✅ | ❌ | ❌ |
| Hot / trending feed | ✅ | ✅ `/opds/hot` (by download count) | ✅ Phase 23 |
| New releases feed | ✅ | ✅ `/opds/new` | ✅ Phase 23 |
| Read books feed (`/opds/readbooks`) | ✅ | ❌ | ❌ **False positive** |
| Unread books feed (`/opds/unreadbooks`) | ✅ | ❌ | ❌ **False positive** |
| Book UUID lookup (`/ajax/book/:uuid/:library`) | ✅ | ❌ | ❌ |
| Cover serving via OPDS | ✅ `/opds/cover/<id>` | ✅ `/opds/cover/:id` (JPEG/WebP) | ✅ Phase 23 |
| Cover at multiple resolutions | ✅ 240×240, thumb | ✅ `/thumb` (240×240) + `/large` (600×600) | ✅ Phase 23 |
| Cover LRU cache | ❌ | ✅ 200-entry in-memory LRU | ✅ Unique |
| OpenSearch descriptor (`/opds/osd`) | ✅ | ✅ `/opds/osd` | ✅ Phase 23 |
| OPDS search path-variant | ✅ | ✅ `/opds/search/<path:query>` | ✅ Phase 23 |
| OPDS search (`/opds/search`) | ✅ GET + POST | ✅ | ✅ |
| OPDS stats | ✅ `/opds/stats` | ✅ `/opds/stats` (JSON) | ✅ Phase 23 |
| OPDS discover | ✅ `/opds/discover` | ✅ `/opds/discover` (shelf nav entries) | ✅ Phase 23 |

**Gap (Phase 30 target):** Post-audit verification revealed three false positives. OPDS category/tag feeds, read/unread feeds, and book UUID lookup are absent from `opds.rs` — they were incorrectly marked ✅. Additionally, OPDS formats feed and per-shelf OPDS feeds have no routes. These are planned for Phase 30.

---

## 11. Kobo Sync

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Initialization (`/kobo/<token>/v1/initialization`) | ✅ | ✅ | ✅ |
| Library sync | ✅ | ✅ | ✅ |
| Book state sync (reading progress) | ✅ | ✅ | ✅ |
| Bookmark sync | ✅ | ✅ | ✅ |
| Tag sync (`/v1/library/tags` POST/DELETE/PUT) | ✅ | ❌ | ❌ **False positive** |
| Kobo store product endpoints (mocked) | ✅ ~27 routes | ✅ 14 mock endpoints | ✅ Phase 24 |
| Kobo deals / affiliate endpoints | ✅ mocked | ✅ mocked | ✅ Phase 24 |
| Kobo analytics (mocked) | ✅ | ✅ mocked | ✅ Phase 24 |
| Kobo user loyalty / recommendations / wishlist | ✅ mocked | ✅ mocked (GET/POST/DELETE) | ✅ Phase 24 |
| Kobo image serving (`/<uuid>/<w>/<h>/…/image.jpg`) | ✅ | ✅ cover proxy + 1×1 placeholder | ✅ Phase 24 |

**Gap (Phase 30 target):** A post-Phase 24 audit revealed that Kobo tag sync was incorrectly marked ✅. The routes `POST /v1/library/tags`, `DELETE /v1/library/tags/:tag_id`, `PUT /v1/library/tags/:tag_id`, `POST /v1/library/tags/:tag_id/items`, and `DELETE /v1/library/tags/:tag_id/items` are absent from `kobo.rs`. Without these routes, Kobo shelves/collections cannot be created or modified from the device — only read during sync. Planned for Phase 30.

---

## 12. Metadata Provider Search

| Feature | calibre-web | xcalibre-server | Status |
|---------|-------------|-----------------|--------|
| Search metadata providers | ✅ `/metadata/search` | ✅ Open Library + Google Books | ✅ |
| List configured providers | ✅ `/metadata/provider` | ❌ not configurable at runtime | ❌ |
| Configure provider at runtime | ✅ `/metadata/provider` POST | ❌ config.toml only | ❌ |
| LLM-assisted metadata enrichment | ❌ | ✅ classify, validate, quality check | ✅ Unique |
| Enrichment result cache | ❌ | ✅ SQLite cache, 30-day expiry | ✅ Unique |

**Gap:** Provider configuration is static (config.toml); calibre-web allows switching/configuring providers at runtime through the admin UI.

---

## 13. Features Unique to xcalibre-server (Not in calibre-web)

| Feature | Description |
|---------|-------------|
| Collections | Curated, searchable book sets; public/private; shareable |
| Collections RAG surface | `/api/v1/collections/:id/search/chunks` — semantic search within a collection |
| Book chunks (full-text RAG) | Pre-chunked passages for LLM synthesis and semantic search |
| Semantic search | Vector-based search via sqlite-vec embeddings |
| Hybrid search | BM25 + semantic fusion with RRF ranking |
| Cross-encoder reranking | LLM-based result reranking |
| MCP server | Library as a RAG tool surface for AI agents |
| Memory API | `/api/v1/memory` for agents to persist indexed knowledge |
| Goodreads / StoryGraph import | Import ratings and reading state from CSV exports |
| Watch folder | Auto-import books dropped into a configured directory |
| Webhooks | HMAC-SHA256-signed event delivery with configurable retry |
| Prometheus metrics | `/metrics` endpoint + structured JSON logging |
| OpenAPI docs | Auto-generated via utoipa |
| S3 storage backend | Store books on S3-compatible object storage |
| Mobile apps (iOS/Android) | Expo app with offline reading and download queue |
| Multi-library | Multiple independent libraries per user |
| Magic links | Passwordless email login (time-limited, signed) |
| API tokens | Long-lived, scoped tokens for CI/scripting |
| TOTP 2FA | Time-based one-time passwords with backup codes |
| Reading streaks + statistics | Per-user reading charts, top authors, monthly counts |
| Audio streaming | MP3/M4B/OGG with HTTP range-request support |
| xs-migrate CLI | Import existing Calibre library with metadata preservation |
| Vision LLM pass | Schematics, diagrams, and chart extraction for image-heavy pages |
| Cross-document synthesis | 14 output formats with full source attribution |
| Prompt evaluation framework | Fixture-driven testing, model matrix, prompt versioning |

---

## 14. Gaps by Priority

> Items marked ✅ were closed in Phases 23–28. Remaining open items are listed below.

### High — Affects Core Compatibility

1. ✅ ~~**OPDS cover serving**~~ — Closed Phase 23. `/opds/cover/:id`, `/thumb`, `/large` all implemented with LRU cache.
2. ✅ ~~**OpenSearch descriptor (`/opds/osd`)**~~ — Closed Phase 23.
3. ✅ ~~**Kobo mock store endpoints**~~ — Closed Phase 24. 14 mock endpoints + image proxy.
4. ✅ ~~**`/show/<id>/<format>` inline serve**~~ — Closed Phase 25. `GET /api/v1/books/:id/view/:format`.

### Medium — Feature Parity

5. ✅ ~~**Shelf reordering**~~ — Closed Phase 25. `PUT /api/v1/shelves/:id/order`.
6. ✅ ~~**OPDS letter-based browsing**~~ — Closed Phase 23. `/opds/authors/letter/:char` + `/opds/series/letter/:char`.
7. ✅ ~~**OPDS hot/trending feed**~~ — Closed Phase 23. `/opds/hot`.
8. ✅ ~~**OAuth account link/unlink**~~ — Closed Phase 26. Link flow + unlink + lockout guard.
9. ✅ ~~**Book merge with simulation**~~ — Closed Phase 27. Preview + execute with reading progress strategies.
10. **Inline metadata provider config** — Admin cannot switch Open Library / Google Books on or off at runtime; requires server restart. Low operational impact for self-hosted deployments.

### Low — Polish & Admin

11. ✅ ~~**Domain allowlist/blocklist management**~~ — Closed Phase 28. Full CRUD at `/api/v1/admin/domains`.
12. ✅ ~~**Admin log viewer API**~~ — Closed Phase 28. `GET /api/v1/admin/logs`.
13. ✅ ~~**Metadata backup endpoint**~~ — Closed Phase 28. `POST /api/v1/admin/backup`.
14. ✅ ~~**Bulk thumbnail regeneration**~~ — Closed Phase 28. `POST /api/v1/admin/covers/regenerate`.
15. ✅ ~~**Task cancellation endpoint**~~ — Closed Phase 28. `DELETE /api/v1/admin/tasks/:id`.
16. ✅ ~~**OPDS stats + discover**~~ — Closed Phase 23. `/opds/stats` + `/opds/discover`.

### Remaining Open Gaps (Post Phase 28) — Phase 30 Targets

**High — False Positives (were incorrectly marked ✅)**

| # | Gap | calibre-web Routes | Priority |
|---|-----|--------------------|----------|
| 1 | **Kobo tag sync** | `POST /v1/library/tags`, `DELETE/PUT /v1/library/tags/:id`, `POST/DELETE /v1/library/tags/:id/items` | 🔴 High — Kobo shelves can't be created from device |
| 2 | **OPDS category/tag feeds** | `GET /opds/category`, `/opds/category/:id`, `/opds/category/letter/:id` | 🔴 High — e-reader genre browse returns empty |
| 3 | **OPDS read/unread feeds** | `GET /opds/readbooks`, `GET /opds/unreadbooks` | 🔴 High — "Read" shortcut broken on OPDS clients |

**Medium — Undocumented Gaps Found in Audit**

| # | Gap | calibre-web Routes | Priority |
|---|-----|--------------------|----------|
| 4 | **Shelf edit** (rename / toggle public) | `GET+POST /shelf/edit/:id` | 🟡 Medium — users can't rename shelves |
| 5 | **OPDS shelf feed** | `GET /opds/shelfindex`, `/opds/shelf/:id` | 🟡 Medium — shelf browsing from e-reader impossible |
| 6 | **OPDS formats feed** | `GET /opds/formats`, `/opds/formats/:id` | 🟡 Medium — format-based browsing not available |
| 7 | **OPDS book UUID lookup** | `GET /ajax/book/:uuid/:library` | 🟡 Medium — some clients use UUID-based single-book fetch |

**Low — Pre-existing Polish Items**

| # | Gap | Notes |
|---|-----|-------|
| 8 | Inline metadata provider config (runtime switching) | config.toml only; low priority for self-hosted |
| 9 | Log download (file export) | `/admin/logdownload/*` — stream log as file attachment |
| 10 | Admin HTML UI | Intentional — API-only by design |
| 11 | Domain edit-in-place | Delete + re-add workaround works |
| 12 | Scheduled tasks HTML form | API exists; no HTML UI |
| 13 | Bulk display-status toggle | `/ajax/displayselectedbooks` has no API equivalent |
| 14 | Calibre format conversion | Requires Calibre binary; out of scope |

---

## 15. Intentional Divergences (Not Gaps)

These calibre-web features are intentionally absent from xcalibre-server — replaced by better alternatives or outside product scope.

| calibre-web Feature | Why Absent in xcalibre-server |
|--------------------|-------------------------------|
| Remote login tokens | Replaced by magic links (same UX, more secure) |
| Session-based auth | Replaced by JWT + refresh tokens (stateless, API-friendly) |
| Admin HTML UI | xcalibre-server is API-only; admin UI is a separate frontend concern |
| Metadata provider switching UI | Config-file driven; reduces attack surface |
| Built-in news/recipe downloader | Out of product scope |
| USB/MTP device sync | Out of product scope |
| Python plugin system | No plugin system yet (planned) |
| calibredb compatibility mode | `xs-migrate` CLI handles one-time import instead |
