# Project State — autolibre (Rust Rewrite)

_Last updated: 2026-04-22_

> **Note:** Earlier versions of this file described the calibre-web Python predecessor project (audit results, dependency upgrades, flake8/bandit findings). That content is no longer relevant. The Rust rewrite is the active project.

---

## Overall Status: Phase 11 Complete

All planned phases and open items are complete. The project is feature-complete: mobile search (Stage 1), 2FA/TOTP (Stage 2), and S3-compatible storage backend (Stage 3) all shipped.

---

## Phase Completion Summary

| Phase | Description | Status |
|---|---|---|
| Phase 1 | Backend foundation (auth, books CRUD, file serving, security) | ✅ Complete |
| Phase 2 | `autolibre-migrate` CLI (Calibre import) | ✅ Complete |
| Phase 3 | React SPA (library grid, reader, admin panel) | ✅ Complete |
| Phase 4 | Search (FTS5 + Meilisearch + semantic/sqlite-vec) | ✅ Complete |
| Phase 5 | LLM features + Agentic RAG surface | ✅ Complete |
| Phase 6 | Mobile (Expo — iOS + Android) | ✅ Complete |
| Phase 7 | Hardening (multi-arch Docker, OWASP audit, benchmarks) | ✅ Complete |
| Phase 8 | MCP server (stdio + SSE transports) | ✅ Complete |
| Phase 9 | Feature parity (OPDS, OAuth, LDAP, Kobo sync, multi-library) | ✅ Complete |
| Phase 10 | Extended features (per-user state, OPDS feeds, i18n, scheduled tasks) | ✅ Complete |
| Phase 11 | Open items: mobile search, 2FA/TOTP, S3 storage backend | ✅ Complete |

---

## Database Migrations

| Migration | Contents | Status |
|---|---|---|
| `0001_initial.sql` | 21 base tables | ✅ Applied |
| `0002_fts.sql` | FTS5 virtual table + sync triggers | ✅ Applied |
| `0003_document_type.sql` | `document_type` column on `books` | ✅ Applied |
| `0004_security_hardening.sql` | Lockout columns, audit log indexes | ✅ Applied |
| `0005_api_tokens.sql` | `api_tokens` table | ✅ Applied |
| `0006_email_settings.sql` | `email_settings` singleton table | ✅ Applied |
| `0007_oauth_accounts.sql` | `oauth_accounts` table | ✅ Applied |
| `0008_kobo.sql` | `kobo_devices`, `kobo_reading_state` tables | ✅ Applied |
| `0009_libraries.sql` | `libraries` table; `library_id` on `books`; `default_library_id` on `users` | ✅ Applied |
| `0010_book_user_state.sql` | `book_user_state` table | ✅ Applied |
| `0011_download_history.sql` | `download_history` table | ✅ Applied |
| `0012_user_tag_restrictions.sql` | `user_tag_restrictions` table | ✅ Applied |
| `0013_scheduled_tasks.sql` | `scheduled_tasks` table | ✅ Applied |
| `0014_totp.sql` | `totp_backup_codes` table; `totp_secret`/`totp_enabled` on `users` | ✅ Applied |

Total: **33 tables** across SQLite and MariaDB migration sets (including 2 new TOTP columns on `users`).

---

## Quality Gates (last verified: 2026-04-22)

| Check | Status |
|---|---|
| `cargo test --workspace` | All integration tests passing |
| `cargo clippy -- -D warnings` | Zero warnings |
| `cargo audit` | Zero CVEs |
| Multi-arch Docker build (amd64/arm64/armv7) | ✅ Passing in CI |
| Criterion benchmarks | Non-blocking CI job |

---

## Local Dev Environment

| Item | Value |
|---|---|
| LM Studio (local) | `localhost:1234` |
| LM Studio (remote) | `192.168.0.72:1234` — phi-3-mini |
| Meilisearch | Optional; FTS5 fallback active when not running |
| SQLite dev DB | `./library.db` (created by migrations) |

---

## Open Items

None blocking. See [ARCHITECTURE.md](ARCHITECTURE.md) for the full build plan history.
