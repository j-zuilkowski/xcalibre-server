# Changelog — autolibre (Rust Rewrite)

All notable changes to the autolibre Rust rewrite. Format: `[YYYY-MM-DD] — Commit — Summary`

> **Note:** Earlier versions of this file documented the calibre-web Python predecessor project
> (dependency upgrades, flake8/bandit audit results). That content is no longer relevant.
> This file now tracks the Rust rewrite exclusively.

---

## 2026-04-22 — Phase 11 Stage 3: S3-compatible storage backend

- `StorageBackend` trait made fully async (`put`, `delete`, `get_bytes`)
- `LocalFsStorage` updated with async trait impl; `get_bytes` via `tokio::fs::read`
- New `backend/src/storage_s3.rs` — `S3Storage` using `aws-sdk-s3`; endpoint_url override for MinIO/R2/B2; key sanitization; `resolve()` unsupported (returns `None`)
- Config: `[storage]` + `[storage.s3]` sections in `config.toml`; S3 secret redacted in debug output; backend validated at startup
- Runtime dispatch in file-serving handlers: `ServeFile` for local (range support), `get_bytes` + full response for S3 (range degraded, documented)
- Text extraction falls back to temp-file download when `resolve()` returns `None` (S3 path)
- Cargo deps: `aws-sdk-s3`, `tempfile` added
- `backend/tests/test_storage_s3.rs` — unit tests + `#[ignore]` roundtrip test for real S3/MinIO endpoint
- ARCHITECTURE.md updated: storage backend comparison table, local→S3 migration steps

---

## 2026-04-22 — Phase 11 Stage 2: 2FA/TOTP — setup, login flow, backup codes, admin disable

- Migration 0014: `ALTER TABLE users ADD COLUMN totp_secret TEXT / totp_enabled INTEGER`; `totp_backup_codes` table
- `backend/src/auth/totp.rs` — TOTP crypto (totp-rs), AES-256-GCM encrypted secret, HKDF key derivation, backup code hashing
- Backend routes: TOTP setup/confirm/disable, `POST /auth/totp/verify` for pending-token login step, admin disable at `DELETE /admin/users/:id/totp`
- `totp_pending` JWT enforced in `backend/src/middleware/auth.rs` — cannot access non-TOTP routes until verified
- DB queries in `backend/src/db/queries/totp.rs`
- Shared types/client updated in `packages/shared/src/client.ts` and `packages/shared/src/types.ts`
- Web UI: TOTP step in `LoginPage.tsx`, setup/disable flow in `ProfilePage.tsx`, admin disable in `UsersPage.tsx`
- Mobile: TOTP step added to `apps/mobile/src/app/login.tsx`
- 14 backend integration tests in `backend/tests/test_totp.rs`

---

## 2026-04-22 — Phase 11 Stage 1: Mobile search screen (FTS + semantic)

- Replaced stub `apps/mobile/src/app/(tabs)/search.tsx` with full search screen
- Debounced FTS search via shared API client; pagination support
- Semantic search tab gated by `GET /api/v1/llm/health` — grayed out when LLM disabled
- Result cards with cover/placeholder, title, author, semantic score badge
- Slide-up filter panel (language, format, sort, order) via `@gorhom/bottom-sheet`
- `useDebounce` hook at `apps/mobile/src/hooks/useDebounce.ts`
- `CoverPlaceholder` component at `apps/mobile/src/components/CoverPlaceholder.tsx`
- Test coverage in `apps/mobile/src/tests/SearchScreen.test.tsx`
- Type shims for NativeWind, expo-constants, react-test-renderer
- Supporting fixes: `apps/mobile/src/lib/db.ts`, `apps/mobile/src/app/reader/[id].tsx`, `packages/shared/src/client.ts`

---

## 2026-04-22 — Phase 10 Stage 5 + Stage 6 review fixes

- Review fixes from `/review` pass on Phase 10 Stages 5 and 6

---

## 2026-04-21 — Phase 10 Stage 5: Extended Format Support + RAG

- DJVU reader — server-side page extraction, `DjvuReader.tsx`
- Audio streaming — MP3/M4B/OGG support via range-request stream endpoint
- MOBI/AZW3 reader — server-side conversion to HTML
- RAG text extraction extended to DJVU and MOBI formats
- `document_type` CHECK constraint extended with `'audiobook'`

---

## 2026-04-21 — Phase 10 Stage 7: Scheduled Tasks UI + Update Checker

- `scheduled_tasks` table (migration 0013)
- Scheduler runs inside the Axum process — polls `next_run_at`
- Admin UI: scheduled tasks list, create/edit/delete, last/next run display
- In-app update checker: `GET /admin/system/updates` — compares against GitHub releases API
- Admin dashboard banner when a newer release is available

---

## 2026-04-20 — Phase 10 Stage 6: i18n Framework

- `react-i18next` integrated into web app
- Starter translations: EN (base), FR, DE, ES
- `GET /locale` endpoint — list available locales
- User locale preference stored in DB; falls back to browser `Accept-Language`
- Locale picker in profile settings

---

## 2026-04-20 — Phase 10 Stage 4: Merge Duplicates + Custom Columns UI

- Shared client/types in `packages/shared` for merge and custom columns
- `POST /admin/books/merge` — merge duplicate books (keep target, absorb source formats/tags)
- Custom columns browser in Admin panel
- Bulk metadata edit extended to support custom column values

---

## 2026-04-19 — Phase 10 Stage 3: Per-User Tag Restrictions + Proxy Auth

- `user_tag_restrictions` table (migration 0012)
- `GET/PUT /users/me/tag-restrictions` — per-user allow/block tag filters at browse time
- Proxy authentication: `X-Remote-User` header support with configurable trusted proxy list
- Proxy auth creates local user on first match (same flow as OAuth)

---

## 2026-04-18 — Phase 10 Stage 2: Extended OPDS Feeds

- OPDS browse feeds for publisher, language, and ratings (`/opds/publishers`, `/opds/languages`, `/opds/ratings/:rating`)
- Publisher stored in `books.flags` JSON column; accessed via `json_extract`
- All feeds OPDS-PS 1.2 compliant; download links remain token-gated

---

## 2026-04-17 — Phase 10 Stage 1: Per-User Read/Unread + Download History

- `book_user_state` table (migration 0010): per-user `is_read` + `is_archived`
- `download_history` table (migration 0011)
- `GET/PUT /books/:id/state` endpoints
- `GET /users/me/downloads` — paginated download history
- Library grid badge for unread status

---

## 2026-04-15 — Phase 7 Stage 3: Criterion Benchmarks

- Criterion benchmark suite for hot query paths
- `PERFORMANCE.md` documenting results
- Benchmark CI job (non-blocking)

---

## 2026-04-14 — Phase 7 Stage 2: OWASP Hardening

- CORS policy tightened
- CSP tuned for epub.js `unsafe-inline` requirement
- SSRF guard at startup for LLM endpoint configuration
- Audit log coverage expanded
- `SECURITY.md` documenting full OWASP findings

---

## 2026-04-13 — Phase 7 Stage 1: Multi-Arch Docker

- Multi-stage Dockerfile producing `linux/amd64`, `linux/arm64`, `linux/arm/v7` images
- `docker.yml` CI workflow for image builds
- Production-grade `docker-compose.yml` with Meilisearch + optional Caddy

---

## Earlier Phases (Phases 1–6 and Phase 9)

Full build plan and phase-by-phase feature list: [ARCHITECTURE.md](ARCHITECTURE.md)
Per-phase Codex commands: `CODEX_COMMANDS.md`, `CODEX_COMMANDS_PHASE2.md` through `CODEX_COMMANDS_PHASE10.md`
