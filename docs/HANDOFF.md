# Codex Task: xcalibre-server — Phase 1 Backend Foundation (TDD)

## Objective

Scaffold a new Rust repository at `~/Documents/localProject/xcalibre-server` for a complete
rewrite of calibre-web. This is a TDD project: **write all tests first**, then implement until
they pass. Phase 1 covers the backend foundation — config, database, auth, books CRUD, file
serving, cover pipeline, and security middleware.

Do not modify anything in `~/Documents/localProject/calibre-web` (the existing Python app).

---

## Context

- **Language**: Rust (edition 2021), async via Tokio
- **Web framework**: Axum 0.7
- **Database**: sqlx 0.7 — SQLite by default, MariaDB optional (same codebase, `DATABASE_URL` env var switches)
- **Auth**: JWT (jsonwebtoken crate) + refresh tokens, argon2 password hashing
- **File serving**: tower-http ServeFile with HTTP range request support
- **Security headers**: tower-http middleware
- **Rate limiting**: tower-governor
- **Input validation**: validator crate
- **Image processing**: image crate (cover resize + thumbnail)
- **Config**: config crate reading `config.toml` + environment variable overrides
- **Logging**: tracing + tracing-subscriber
- **Error handling**: thiserror for library errors, anyhow for application errors

### Architecture reference files (read these before writing anything)
All reference docs are in the `docs/` directory of this repo:
- `docs/ARCHITECTURE.md` — tech stack, deployment targets, security, phased build plan
- `docs/SCHEMA.md` — all 18 database tables, SQLite + MariaDB DDL
- `docs/API.md` — all 60+ routes, request/response shapes, TypeScript types
- `docs/DESIGN.md` — UI/UX spec, color system, component library, layout

### Do NOT touch
- Anything in `~/Documents/localProject/calibre-web/`
- `test/` directory (does not exist yet — do not create a top-level test/ dir, tests live in `backend/tests/`)

---

## Repository Structure to Create

```
xcalibre-server/
├── Cargo.toml                        # workspace root
├── package.json                      # pnpm workspace root (placeholder for Phase 3)
├── turbo.json                        # Turborepo config (placeholder)
├── .github/
│   └── workflows/
│       └── ci.yml                    # cargo test + cargo audit + cargo clippy
├── backend/
│   ├── Cargo.toml
│   ├── src/
│   │   ├── main.rs                   # binary entry point
│   │   ├── lib.rs                    # library root (all logic here, main.rs just calls it)
│   │   ├── config.rs                 # config loading + validation
│   │   ├── error.rs                  # AppError type + Into<axum::Response>
│   │   ├── state.rs                  # AppState (db pool, config, optional LlmClient)
│   │   ├── middleware/
│   │   │   ├── mod.rs
│   │   │   ├── auth.rs               # JWT extraction middleware
│   │   │   └── security_headers.rs  # CSP, X-Frame-Options, etc.
│   │   ├── api/
│   │   │   ├── mod.rs               # router assembly
│   │   │   ├── auth.rs              # /auth/* handlers
│   │   │   └── books.rs             # /books/* handlers
│   │   └── db/
│   │       ├── mod.rs
│   │       ├── models.rs            # sqlx structs mirroring schema
│   │       └── queries/
│   │           ├── mod.rs
│   │           ├── auth.rs          # user + token queries
│   │           └── books.rs         # book queries
│   ├── tests/
│   │   ├── common/
│   │   │   └── mod.rs               # shared test helpers (test DB setup, auth helpers)
│   │   ├── test_config.rs
│   │   ├── test_auth.rs
│   │   ├── test_books.rs
│   │   ├── test_file_serving.rs
│   │   └── test_security.rs
│   └── migrations/
│       ├── sqlite/
│       │   └── 0001_initial.sql
│       └── mariadb/
│           └── 0001_initial.sql
├── docker/
│   ├── Dockerfile
│   ├── docker-compose.yml
│   └── Caddyfile
├── tools/
│   └── mcp_server.js                 # MCP server exposing dev tools to Claude Code
├── .claude/
│   └── settings.json                 # Claude Code hooks + permissions allowlist
└── config.example.toml
```

---

## Requirements

### 1. Workspace + Cargo setup
- Cargo workspace with a single `backend` member crate
- All dependencies pinned in `backend/Cargo.toml`
- `cargo clippy -- -D warnings` must pass with zero warnings
- `cargo audit` must pass (add to CI)

### 2. Configuration (`config.rs`)
- Reads `config.toml` from the working directory (or `CONFIG_PATH` env var)
- Environment variables override file values (prefix: `APP_`)
- On startup, check `config.toml` file permissions — log a warning if world-readable (mode & 0o004 != 0 on Unix)
- Auto-generate a 256-bit JWT secret if `jwt_secret` is not set; write it back to config and log a warning
- `ENABLE_LLM_FEATURES` defaults to `false`
- Required fields: `database_url`, `storage_path`, `base_url`
- `AppConfig` struct must derive `Debug` but redact `jwt_secret` in output

```toml
# config.example.toml
[app]
base_url = "http://localhost:8083"
storage_path = "./storage"

[database]
url = "sqlite://library.db"

[auth]
jwt_secret = ""                  # auto-generated if blank
access_token_ttl_mins = 15
refresh_token_ttl_days = 30
max_login_attempts = 10
lockout_duration_mins = 15

[llm]
enabled = false

[llm.librarian]
endpoint = "http://192.168.0.72:1234/v1"
model = ""
timeout_secs = 10
system_prompt = ""

[llm.architect]
endpoint = "http://localhost:1234/v1"
model = ""
timeout_secs = 10
system_prompt = ""

[limits]
upload_max_bytes = 524288000     # 500MB
rate_limit_per_ip = 200
```

### 3. Database migrations

SQLite migration `0001_initial.sql` must create all 18 tables defined in SCHEMA.md:
`roles`, `users`, `refresh_tokens`, `authors`, `series`, `tags`, `books`, `book_authors`,
`book_tags`, `formats`, `identifiers`, `shelves`, `shelf_books`, `reading_progress`,
`custom_columns`, `book_custom_values`, `llm_jobs`, `llm_eval_results`, `migration_log`,
`audit_log`, `book_embeddings`.

Seed two default roles: `admin` (all permissions) and `user` (can_edit + can_download only).

MariaDB migration must be equivalent — note differences: `TEXT` → `VARCHAR`/`TEXT`,
`INTEGER` booleans → `TINYINT(1)`, no partial indexes.

### 4. Auth routes (`/api/v1/auth/*`)

Implement handlers matching the API contract in API.md:

- `POST /auth/register` — creates first admin; returns 409 if any user already exists
- `POST /auth/login` — validates credentials, checks lockout, issues JWT + refresh token
- `POST /auth/logout` — revokes refresh token
- `POST /auth/refresh` — exchanges refresh token for new pair
- `GET /auth/me` — returns current user (requires valid JWT)
- `PATCH /auth/me/password` — change own password (requires valid JWT + current password)

Account lockout: increment `login_attempts` counter on failure; lock account when
`>= max_login_attempts`; reset on success; `locked_until` timestamp in users table.

### 5. Books CRUD (`/api/v1/books/*`)

- `GET /books` — paginated list with filters (q, author_id, series_id, tag, language, format, sort, order, page, page_size, since)
- `GET /books/:id` — single book with all relations (authors, tags, formats, identifiers, series)
- `POST /books` — multipart upload: accept file, extract metadata (title, author from filename if epub metadata missing), detect format by magic bytes, store file, return Book
- `PATCH /books/:id` — partial update, writes audit_log row for every changed field
- `DELETE /books/:id` — admin only, cascade deletes all formats + files on disk

### 6. File serving

- `GET /books/:id/formats/:format/download` — full file download
- `GET /books/:id/formats/:format/stream` — streaming with HTTP range request support
- `GET /books/:id/cover` — serve cover image
- Path traversal prevention: resolve requested path, verify it starts with `storage_path`
- Directory listing: disabled

### 7. Cover pipeline (ingest)

When a book is uploaded:
1. Extract cover from epub (first image in OPF manifest marked as cover) or PDF (first page render — skip if pdf rendering unavailable, store no cover)
2. Resize to max 400×600px maintaining aspect ratio
3. Generate 100×150px thumbnail
4. Store both under bucketed path: `storage/covers/{first2_of_uuid}/{uuid}.jpg` and `{uuid}.thumb.jpg`
5. Set `books.has_cover = true`, `books.cover_path = relative path`

### 8. Security middleware

Applied to all routes via `tower` layer stack:

| Header | Value |
|---|---|
| `X-Content-Type-Options` | `nosniff` |
| `X-Frame-Options` | `DENY` |
| `Referrer-Policy` | `strict-origin-when-cross-origin` |
| `Content-Security-Policy` | `default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'` |
| `Permissions-Policy` | `camera=(), microphone=(), geolocation=()` |

Rate limiting via `tower-governor`:
- Auth routes: 10 req/min per IP
- All other routes: 200 req/min per IP (configurable)

### 9. Docker

`docker/Dockerfile` — multi-stage:
- Stage 1: `rust:1.77-slim` — compile backend + placeholder for frontend build
- Stage 2: `debian:bookworm-slim` — copy binary only
- Exposes port 8083
- Runs as non-root user
- Target image size < 50MB

`docker/docker-compose.yml`:
```yaml
services:
  app:
    build: .
    ports: ["8083:8083"]
    volumes:
      - ./config.toml:/app/config.toml:ro
      - library_data:/app/storage
    environment:
      - APP_DATABASE__URL=sqlite:///app/storage/library.db
    depends_on: [meilisearch]

  meilisearch:
    image: getmeili/meilisearch:latest
    volumes:
      - meili_data:/meili_data

  # caddy:                          # uncomment for HTTPS
  #   image: caddy:2-alpine
  #   ports: ["80:80", "443:443"]
  #   volumes:
  #     - ./docker/Caddyfile:/etc/caddy/Caddyfile
  #     - caddy_data:/data

volumes:
  library_data:
  meili_data:
  # caddy_data:
```

`docker/Caddyfile`:
```
{$APP_DOMAIN:localhost} {
    reverse_proxy app:8083
}
```

### 10. `CLAUDE.md` for the new repo

Create `xcalibre-server/CLAUDE.md` with this exact content — it will be auto-loaded
by Claude Code in every future session on this repo:

```markdown
# xcalibre-server — Claude Context

## Project
Rust rewrite of calibre-web. Self-hosted ebook library manager.
Full architecture: docs/ARCHITECTURE.md
Schema: docs/SCHEMA.md
API contract: docs/API.md
Design spec: docs/DESIGN.md
Skills reference: docs/SKILLS.md

## Stack
- Backend: Rust, Axum 0.7, sqlx 0.7, SQLite default / MariaDB optional
- Frontend: React + Vite + TanStack Router + shadcn/ui (Phase 3)
- Mobile: Expo (Phase 6)
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

## MCP Tools (xcalibre-server-dev server)
Register once: `claude mcp add xcalibre-server-dev node tools/mcp_server.js`
- `run_tests [filter]` — run cargo tests, optionally filtered
- `cargo_check` — fast compile check
- `cargo_clippy` — lint with -D warnings
- `cargo_audit` — CVE check
- `db_query <sql>` — query dev SQLite DB (SELECT/PRAGMA only)
- `list_tables` — list DB tables with row counts
- `run_migrations` — apply sqlx migrations

## Code Style
- Rust edition 2021
- No unwrap() in production code — use ? and proper error types
- AppError implements IntoResponse — all handlers return Result<T, AppError>
- Tests use TestContext::new().await — never share state between tests
```

### 11. CI (`ci.yml`)

```yaml
on: [push, pull_request]
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with: { components: clippy }
      - uses: Swatinem/rust-cache@v2
      - run: cargo test --workspace
      - run: cargo clippy --workspace -- -D warnings
      - run: cargo audit
        continue-on-error: false
```

---

## Claude Code Integration

### `.claude/settings.json` — Hooks + Permissions Allowlist

Create this file at `xcalibre-server/.claude/settings.json`. It configures automatic
quality gates and eliminates permission prompts for routine commands.

```json
{
  "permissions": {
    "allow": [
      "Bash(cargo:*)",
      "Bash(cargo check:*)",
      "Bash(cargo test:*)",
      "Bash(cargo clippy:*)",
      "Bash(cargo audit:*)",
      "Bash(cargo build:*)",
      "Bash(cargo fmt:*)",
      "Bash(git status:*)",
      "Bash(git diff:*)",
      "Bash(git log:*)",
      "Bash(git add:*)",
      "Bash(git commit:*)",
      "Bash(docker build:*)",
      "Bash(docker compose:*)",
      "Bash(ls:*)",
      "Bash(mkdir:*)",
      "Bash(cat:*)",
      "Bash(sqlite3:*)"
    ]
  },
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Edit|Write",
        "hooks": [
          {
            "type": "command",
            "command": "cd backend && cargo check --quiet 2>&1 | head -20"
          }
        ]
      },
      {
        "matcher": "Edit",
        "hooks": [
          {
            "type": "command",
            "command": "if echo '$CLAUDE_TOOL_INPUT' | grep -q 'migrations/'; then cd backend && cargo sqlx prepare --check 2>&1 | head -10; fi"
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "cd backend && cargo clippy --quiet 2>&1 | head -30"
          }
        ]
      }
    ]
  }
}
```

**What each hook does:**
- `PostToolUse Edit|Write` → runs `cargo check` after every file edit — catches compile errors immediately
- `PostToolUse Edit` on migrations → runs `cargo sqlx prepare --check` when migration files change
- `Stop` → runs `cargo clippy` at end of every Claude Code session — surfaces warnings before closing

---

### `tools/mcp_server.js` — Dev MCP Server

Exposes development tools to Claude Code as callable MCP tools. Register in Claude Code
settings after scaffolding by running: `claude mcp add xcalibre-server-dev node tools/mcp_server.js`

```javascript
#!/usr/bin/env node
// MCP server for xcalibre-server development tooling
// Exposes cargo, db, and codex controls as Claude Code tools

import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import { execSync, exec } from "child_process";
import { promisify } from "util";
import { z } from "zod";
import path from "path";
import { fileURLToPath } from "url";

const execAsync = promisify(exec);
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const BACKEND = path.join(ROOT, "backend");

const server = new McpServer({
  name: "xcalibre-server-dev",
  version: "1.0.0",
});

// ── run_tests ─────────────────────────────────────────────────────────────────
// Run cargo test — optionally filter to a specific test file or test name
server.tool(
  "run_tests",
  "Run cargo tests. Optionally filter by test file (e.g. 'test_auth') or test name.",
  {
    filter: z.string().optional().describe("Test file or name filter e.g. 'test_auth'"),
    show_output: z.boolean().optional().default(true),
  },
  async ({ filter, show_output }) => {
    const filterArg = filter ? `--test ${filter}` : "--workspace";
    const cmd = `cd ${BACKEND} && cargo test ${filterArg} 2>&1`;
    try {
      const { stdout } = await execAsync(cmd, { timeout: 120000 });
      return { content: [{ type: "text", text: show_output ? stdout : "Tests passed." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

// ── cargo_check ───────────────────────────────────────────────────────────────
// Compile check without running tests — fast feedback
server.tool(
  "cargo_check",
  "Run cargo check to verify the project compiles without errors.",
  {},
  async () => {
    try {
      const { stdout } = await execAsync(`cd ${BACKEND} && cargo check 2>&1`, { timeout: 60000 });
      return { content: [{ type: "text", text: stdout || "cargo check passed." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

// ── cargo_clippy ──────────────────────────────────────────────────────────────
server.tool(
  "cargo_clippy",
  "Run cargo clippy with -D warnings. Returns warnings and errors.",
  {},
  async () => {
    try {
      const { stdout } = await execAsync(
        `cd ${BACKEND} && cargo clippy --workspace -- -D warnings 2>&1`,
        { timeout: 60000 }
      );
      return { content: [{ type: "text", text: stdout || "clippy clean." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

// ── cargo_audit ───────────────────────────────────────────────────────────────
server.tool(
  "cargo_audit",
  "Run cargo audit to check for known CVEs in dependencies.",
  {},
  async () => {
    try {
      const { stdout } = await execAsync(`cd ${ROOT} && cargo audit 2>&1`, { timeout: 60000 });
      return { content: [{ type: "text", text: stdout || "No vulnerabilities found." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

// ── db_query ──────────────────────────────────────────────────────────────────
// Query the dev SQLite DB directly — useful for verifying migrations and seeded data
server.tool(
  "db_query",
  "Run a read-only SQL query against the development SQLite database.",
  {
    sql: z.string().describe("SELECT query to run"),
    db_path: z.string().optional().default("./library.db").describe("Path to SQLite DB"),
  },
  async ({ sql, db_path }) => {
    // Safety: only allow SELECT statements
    const trimmed = sql.trim().toUpperCase();
    if (!trimmed.startsWith("SELECT") && !trimmed.startsWith("PRAGMA")) {
      return {
        content: [{ type: "text", text: "Only SELECT and PRAGMA statements allowed." }],
        isError: true,
      };
    }
    try {
      const { stdout } = await execAsync(
        `sqlite3 -column -header "${path.resolve(ROOT, db_path)}" "${sql.replace(/"/g, '\\"')}"`,
        { timeout: 10000 }
      );
      return { content: [{ type: "text", text: stdout || "(no rows)" }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.message }], isError: true };
    }
  }
);

// ── list_tables ───────────────────────────────────────────────────────────────
server.tool(
  "list_tables",
  "List all tables in the development SQLite database with row counts.",
  {
    db_path: z.string().optional().default("./library.db"),
  },
  async ({ db_path }) => {
    try {
      const { stdout } = await execAsync(
        `sqlite3 "${path.resolve(ROOT, db_path)}" ".tables"`,
        { timeout: 10000 }
      );
      return { content: [{ type: "text", text: stdout || "(no tables — run migrations first)" }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.message }], isError: true };
    }
  }
);

// ── run_migrations ────────────────────────────────────────────────────────────
server.tool(
  "run_migrations",
  "Run sqlx migrations against the development database.",
  {
    db_url: z.string().optional().default("sqlite://./library.db"),
  },
  async ({ db_url }) => {
    try {
      const { stdout } = await execAsync(
        `cd ${BACKEND} && DATABASE_URL="${db_url}" cargo sqlx migrate run 2>&1`,
        { timeout: 60000 }
      );
      return { content: [{ type: "text", text: stdout || "Migrations applied." }] };
    } catch (err) {
      return { content: [{ type: "text", text: err.stdout || err.message }], isError: true };
    }
  }
);

// ── start ─────────────────────────────────────────────────────────────────────
const transport = new StdioServerTransport();
await server.connect(transport);
```

**`tools/package.json`** (required by the MCP server):
```json
{
  "name": "xs-dev-mcp",
  "version": "1.0.0",
  "type": "module",
  "dependencies": {
    "@modelcontextprotocol/sdk": "^1.0.0",
    "zod": "^3.22.0"
  }
}
```

After scaffolding, register the MCP server with:
```bash
cd ~/Documents/localProject/xcalibre-server/tools && npm install
claude mcp add xcalibre-server-dev node tools/mcp_server.js
```

---

## TDD Order — Write Tests First

**All test files must be written before any implementation.** Tests should compile
(with `#[ignore]` on tests that need unimplemented handlers) but fail when run.
Remove `#[ignore]` as each feature is implemented.

### Testing Crates (add to `backend/Cargo.toml` under `[dev-dependencies]`)

```toml
[dev-dependencies]
axum-test = "14"          # HTTP integration testing for Axum — provides TestServer
tokio = { version = "1", features = ["full", "test-util"] }
tempfile = "3"            # temporary directories for storage in tests
serde_json = "1"          # JSON assertion helpers
pretty_assertions = "1"   # better diff output on assert_eq! failures
fake = { version = "2", features = ["derive"] }  # test data generation
```

### `tests/common/mod.rs` — Complete Specification

This file is the foundation of the entire test suite. Define it exactly as follows:

```rust
use axum_test::TestServer;
use sqlx::SqlitePool;
use tempfile::TempDir;
use std::sync::Arc;

/// Shared test context — one per test function via TestContext::new().await
/// Holds its own in-memory DB and temp storage dir — fully isolated between tests.
pub struct TestContext {
    pub db: SqlitePool,
    pub storage: TempDir,          // auto-deleted when TestContext drops
    pub server: TestServer,        // axum-test server wrapping the full app
}

impl TestContext {
    /// Spin up a fully isolated app instance for one test.
    /// - In-memory SQLite (":memory:" — no file, no cleanup needed)
    /// - Runs all migrations from backend/migrations/sqlite/
    /// - Seeds default roles (admin, user)
    /// - Temp storage directory for book files and covers
    /// - Returns a TestServer backed by the full Axum router
    pub async fn new() -> Self { todo!() }

    /// Seed the DB with a test admin user.
    /// Returns (User, plain_password) — password is "Test1234!" unless overridden.
    pub async fn create_admin(&self) -> (User, String) { todo!() }

    /// Seed the DB with a test regular user (role: "user").
    /// Returns (User, plain_password).
    pub async fn create_user(&self) -> (User, String) { todo!() }

    /// POST /api/v1/auth/login and return the access token string.
    /// Panics if login fails — test setup error, not the subject under test.
    pub async fn login(&self, username: &str, password: &str) -> String { todo!() }

    /// Convenience: create_admin() then login() — returns access token.
    pub async fn admin_token(&self) -> String { todo!() }

    /// Convenience: create_user() then login() — returns access token.
    pub async fn user_token(&self) -> String { todo!() }

    /// Seed the DB with a minimal Book record (no file on disk).
    /// Returns the created Book. Authors, tags, series all empty by default.
    pub async fn create_book(&self, title: &str, author: &str) -> Book { todo!() }

    /// Place a real file in the storage directory and seed the DB record.
    /// format: "EPUB" | "PDF" | "MOBI"
    /// Returns the created Book with one Format.
    pub async fn create_book_with_file(&self, title: &str, format: &str) -> (Book, PathBuf) { todo!() }
}

/// Minimal valid epub bytes for upload tests — just enough magic bytes + structure
/// to pass magic byte detection. Not a real readable epub.
pub fn minimal_epub_bytes() -> Vec<u8> {
    // PK\x03\x04 ZIP magic + mimetype file = valid epub magic bytes
    todo!()
}

/// Minimal valid PDF bytes for upload tests.
pub fn minimal_pdf_bytes() -> Vec<u8> {
    // %PDF-1.4 header
    todo!()
}

/// Assert a response has the given HTTP status, printing the body on failure.
#[macro_export]
macro_rules! assert_status {
    ($response:expr, $status:expr) => {
        let status = $response.status_code();
        if status != $status {
            let body = $response.text();
            panic!("Expected status {} got {}: {}", $status, status, body);
        }
    };
}

/// Assert a JSON response body contains a field with an expected value.
#[macro_export]
macro_rules! assert_json_field {
    ($response:expr, $field:expr, $value:expr) => {
        let json: serde_json::Value = $response.json();
        assert_eq!(json[$field], $value, "Field '{}' mismatch", $field);
    };
}
```

**Key rules for test isolation:**
- Every test calls `TestContext::new().await` — never share a `TestContext` between tests
- `#[tokio::test]` on every async test function
- Never use `#[tokio::test(flavor = "multi_thread")]` unless a specific test requires it
- Never use global `once_cell` or `lazy_static` for DB pools in tests
- `TempDir` in `TestContext.storage` is automatically deleted when the context drops — no manual cleanup

**Pattern for every test:**
```rust
#[tokio::test]
async fn test_example() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx.server
        .get("/api/v1/books")
        .add_header("Authorization", format!("Bearer {}", token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 0);
}
```

### Test Fixture Files

Place in `backend/tests/fixtures/`:

```
backend/tests/fixtures/
├── minimal.epub       # valid epub: PK magic + META-INF/container.xml + OPF with title/author
├── minimal.pdf        # valid PDF: %PDF-1.4 header + minimal xref table
├── minimal.mobi       # valid MOBI: PalmDOC magic bytes (0x00 0x00 0x00...)
├── fake.epub          # file with .epub extension but PDF magic bytes (tests magic byte rejection)
└── cover.jpg          # 200x300 JPEG — used for cover upload tests
```

These must be real binary files that pass magic byte detection, not empty files.
`minimal.epub` must contain a valid `META-INF/container.xml` pointing to a minimal OPF
with `<dc:title>Test Book</dc:title>` and `<dc:creator>Test Author</dc:creator>`.

### `tests/test_config.rs`
```rust
test_config_loads_from_file()
test_env_vars_override_file()
test_missing_required_fields_error()
test_jwt_secret_autogenerated_when_blank()
test_world_readable_config_logs_warning()
test_llm_disabled_by_default()
test_debug_output_redacts_jwt_secret()
```

### `tests/test_auth.rs`
```rust
test_register_first_user_becomes_admin()
test_register_fails_if_users_exist()
test_login_success_returns_tokens()
test_login_wrong_password_returns_401()
test_login_nonexistent_user_returns_401()
test_login_lockout_after_max_attempts()
test_login_lockout_resets_after_duration()
test_refresh_token_returns_new_pair()
test_refresh_token_revoked_after_use()
test_refresh_invalid_token_returns_401()
test_logout_revokes_refresh_token()
test_me_returns_current_user()
test_me_without_token_returns_401()
test_me_with_expired_token_returns_401()
test_change_password_success()
test_change_password_wrong_current_returns_400()
```

### `tests/test_books.rs`
```rust
test_list_books_empty_library()
test_list_books_pagination()
test_list_books_filter_by_author()
test_list_books_filter_by_tag()
test_list_books_sort_by_title()
test_list_books_since_returns_only_modified()
test_get_book_returns_full_relations()
test_get_book_not_found_returns_404()
test_upload_epub_extracts_metadata()
test_upload_epub_extracts_cover()
test_upload_pdf_no_cover_ok()
test_upload_unknown_format_returns_422()
test_upload_magic_bytes_mismatch_returns_422()
test_upload_duplicate_isbn_returns_409()
test_upload_requires_upload_permission()
test_patch_book_updates_fields()
test_patch_book_writes_audit_log()
test_patch_book_not_found_returns_404()
test_delete_book_removes_files()
test_delete_book_requires_admin()
```

### `tests/test_file_serving.rs`
```rust
test_download_returns_full_file()
test_stream_supports_range_requests()
test_stream_partial_content_206()
test_cover_returns_image()
test_cover_missing_returns_404()
test_path_traversal_rejected()
test_download_requires_auth()
test_download_requires_download_permission()
```

### `tests/test_security.rs`
```rust
test_security_headers_present_on_all_responses()
test_x_content_type_options_nosniff()
test_x_frame_options_deny()
test_csp_header_present()
test_permissions_policy_present()
test_rate_limit_auth_after_10_requests()
test_rate_limit_resets_after_window()
test_upload_over_size_limit_returns_413()
```

---

## Acceptance Criteria

- [ ] `cargo test --workspace` passes with zero failures
- [ ] `cargo clippy --workspace -- -D warnings` passes with zero warnings
- [ ] `cargo audit` passes with zero vulnerabilities
- [ ] All 18 schema tables present in SQLite migration
- [ ] Auth lockout triggers after configured max attempts
- [ ] JWT secret auto-generated and persisted if blank in config
- [ ] HTTP range requests return `206 Partial Content` with correct `Content-Range` header
- [ ] Path traversal attempt (`../../etc/passwd`) returns `400` or `403`
- [ ] All 5 security headers present on every response
- [ ] Docker image builds successfully and app starts
- [ ] `POST /api/v1/auth/register` returns `409` if called twice

---

## Additional Notes

- Use `uuid::Uuid::new_v4().to_string()` for all IDs — TEXT in SQLite
- All timestamps: `chrono::Utc::now().to_rfc3339()` — stored as TEXT in SQLite
- `AppError` must implement `IntoResponse` for Axum — map to correct HTTP status + JSON body matching API.md error shape
- Magic byte detection: epub starts with `PK\x03\x04` (ZIP); PDF starts with `%PDF`; use first 8 bytes
- Cover extraction from epub: parse `META-INF/container.xml` → find OPF → find `<item properties="cover-image">` — skip gracefully if not found
- The `since` query param on `GET /books` enables mobile sync — return only books where `last_modified > since`
- `audit_log` rows must be written in the same DB transaction as the metadata change they record
- sqlx `query!` macros require `DATABASE_URL` set at compile time — use `sqlx::query_as!` with SQLite for tests
- For test isolation: each integration test gets its own in-memory SQLite DB via `test_db()` helper — never share state between tests
- Run `sqlx migrate run` before the test app starts in `common::test_app()`

---

## Codex Model Recommendation

Use `codex --model o4-mini` for scaffolding and test writing.
Use `codex --model o3` for complex implementation tasks (auth lockout, cover pipeline, range requests).
