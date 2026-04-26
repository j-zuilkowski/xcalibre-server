# Project State — xcalibre-server (Rust Rewrite)

_Last updated: 2026-04-24_

> **Note:** Earlier versions of this file described the calibre-web Python predecessor project (audit results, dependency upgrades, flake8/bandit findings). That content is no longer relevant. The Rust rewrite is the active project.

---

## Overall Status: Phase 17 Complete

All 18 security remediation stages shipped. The codebase is clean against the full post-Phase 16 review (two independent audit passes). No open findings.

---

## Phase Completion Summary

| Phase | Description | Status |
|---|---|---|
| Phase 1 | Backend foundation (auth, books CRUD, file serving, security) | ✅ Complete |
| Phase 2 | `xs-migrate` CLI (Calibre import) | ✅ Complete |
| Phase 3 | React SPA (library grid, reader, admin panel) | ✅ Complete |
| Phase 4 | Search (FTS5 + Meilisearch + semantic/sqlite-vec) | ✅ Complete |
| Phase 5 | LLM features + Agentic RAG surface | ✅ Complete |
| Phase 6 | Mobile (Expo — iOS + Android) | ✅ Complete |
| Phase 7 | Hardening (multi-arch Docker, OWASP audit, benchmarks) | ✅ Complete |
| Phase 8 | MCP server (stdio + SSE transports) | ✅ Complete |
| Phase 9 | Feature parity (OPDS, OAuth, LDAP, Kobo sync, multi-library) | ✅ Complete |
| Phase 10 | Extended features (per-user state, OPDS feeds, i18n, scheduled tasks) | ✅ Complete |
| Phase 11 | Open items: mobile search, 2FA/TOTP, S3 storage backend | ✅ Complete |
| Phase 12 | Post-v1.0 polish (E2E tests, ops, i18n, backend quality) | ✅ Complete |
| Phase 13 | Reader depth + observability (annotations, OpenAPI, metrics, stats) | ✅ Complete |
| Phase 14 | Import, author management, webhooks, mobile downloads, a11y, photos | ✅ Complete |
| Phase 15 | Cross-document synthesis engine (chunking, hybrid retrieval, collections) | ✅ Complete |
| Phase 16 | Security remediation (14 findings from post-Phase 15 review) | ✅ Complete |
| Phase 17 | Security remediation II (18 findings from post-Phase 16 review — final) | ✅ Complete |

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
| `0015_annotations.sql` | `book_annotations` table | ✅ Applied |
| `0016_goodreads_import.sql` | `import_logs` table | ✅ Applied |
| `0017_author_profiles.sql` | `author_profiles` table | ✅ Applied |
| `0018_webhooks.sql` | `webhooks` + `webhook_deliveries` tables | ✅ Applied |
| `0019_chunks.sql` | `book_chunks` table | ✅ Applied |
| `0020_collections.sql` | `collections` + `collection_books` tables | ✅ Applied |
| `0021_chunks_fts.sql` | `book_chunks_fts` FTS5 virtual table + triggers | ✅ Applied |
| `0022_collections_idx.sql` | `idx_collections_owner_id` index on `collections` | ✅ Applied |
| `0023_chunks_idx.sql` | `idx_book_chunks_created_at` index on `book_chunks` | ✅ Applied |
| `0024_session_type.sql` | `session_type` discriminator on `sessions` | ✅ Applied |
| `0025_api_token_expiry.sql` | `expires_at` column on `api_tokens` | ✅ Applied |
| `0026_api_token_scope.sql` | `scope` column on `api_tokens` | ✅ Applied |

Total: **42 tables, 26 migrations** across SQLite and MariaDB migration sets.

---

## Quality Gates (last verified: 2026-04-24)

| Check | Status |
|---|---|
| `cargo test --workspace` | All integration tests passing |
| `cargo clippy -- -D warnings` | Zero warnings |
| `cargo audit` | Zero CVEs |
| Multi-arch Docker build (amd64/arm64/armv7) | ✅ Passing in CI |
| Criterion benchmarks | Non-blocking CI job |

_Last verified: 2026-04-24 (Phase 17 complete)_

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

- Orphaned translation key `book.unarchive` in locale files — not in EN base; clean up when EN key is added
- E2E Playwright suite: Chromium sandbox blocked on macOS CI; promote CI job from `continue-on-error: true` to blocking after 2 clean runs on a real CI host
- `llm_features.rs` wiremock tests fail in sandbox (mock HTTP port bind blocked) — environment constraint only; passes on real CI
- `%2e%2e` in storage paths is treated as a literal Normal component by `Path::components()` — safe for S3 keys; if URL-decoded input ever reaches storage paths, add `percent_decode` before sanitization
- `allow_private_endpoints` config flag lives under the `llm` namespace but is also used for webhook SSRF validation — consider promoting to a top-level config key in a future polish pass
- API token scope is enforced but not yet surfaced in the frontend admin panel — token creation UI shows no scope selector

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full build plan history.
