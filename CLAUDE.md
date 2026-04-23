# autolibre — Claude Context

## Project
Rust rewrite of calibre-web. Self-hosted ebook library manager.
Full architecture: docs/ARCHITECTURE.md
Schema: docs/SCHEMA.md
API contract: docs/API.md
Design spec: docs/DESIGN.md
Skills reference: docs/SKILLS.md

## Status
Phase 11 Stage 1 complete (mobile search screen). Remaining open items: 2FA/TOTP (Stage 2), S3 storage backend (Stage 3).

## Stack
- Backend: Rust, Axum 0.7, sqlx 0.7, SQLite default / MariaDB optional
- Frontend: React + Vite + TanStack Router + shadcn/ui + react-i18next (EN/FR/DE/ES)
- Mobile: Expo (iOS + Android) — complete
- Search: Meilisearch + sqlite-vec embeddings

## Key Paths
- `backend/src/` — Axum app (api/, db/, middleware/)
- `backend/migrations/` — sqlx migrations (sqlite/ and mariadb/)
- `backend/tests/` — integration tests (TDD — tests written before implementation)
- `docker/` — Dockerfile, docker-compose.yml, Caddyfile
- `evals/fixtures/` — LLM prompt eval fixtures

## Non-Negotiable Constraints
- TDD: tests written first, implementation makes them pass
- `cargo clippy -- -D warnings` must pass at zero warnings
- `cargo audit` must pass at zero vulnerabilities
- All LLM calls: 10s timeout, silent fallback, never surface errors to users
- `ENABLE_LLM_FEATURES = false` by default
- No secrets hardcoded — config.toml + env var overrides only
- Path traversal prevention on all file serving routes
- All 5 security headers on every response

## Skills Workflow (use at every Codex checkpoint)
- After every Codex stage: `/review` (engineering:code-review)
- After Stage 3 auth and Stage 4 books: `/review` + `/simplify` in parallel
- After Stage 6 security: `/review` + `/security-review` in parallel
- After Stage 7 docker: `/engineering:deploy-checklist`
- On failing tests: `/engineering:debug`
- Start of any new session: `/engineering:standup` to reorient on progress
- Full skills reference: docs/SKILLS.md

## MCP Tools (autolibre-dev server)
Register once: `claude mcp add autolibre-dev node tools/mcp_server.js`
- `run_tests [filter]` — run cargo tests, optionally filtered
- `cargo_check` — fast compile check
- `cargo_clippy` — lint with -D warnings
- `cargo_audit` — CVE check
- `db_query <sql>` — query dev SQLite DB (SELECT/PRAGMA only)
- `list_tables` — list DB tables with row counts
- `run_migrations` — apply sqlx migrations

## Library MCP Server (for agents)
Build: `cargo build --release -p autolibre-mcp`
Register with Claude Code:
`claude mcp add autolibre-library ./target/release/autolibre-mcp --env CONFIG_PATH=./config.toml`
Exposes: `search_books`, `get_book_metadata`, `list_chapters`, `get_book_text`, `semantic_search`

## Code Style
- Rust edition 2021
- No unwrap() in production code — use ? and proper error types
- AppError implements IntoResponse — all handlers return Result<T, AppError>
- Tests use TestContext::new().await — never share state between tests
