# Codex Desktop App — calibre-web-rs Phase 8: MCP Server

## What Phase 8 Builds

Exposes the library as a first-class tool provider for external agentic AI systems
(Claude Code, Claude Desktop, LangGraph, smolagents). The REST API built in Phases 1–5
is the foundation; Phase 8 adds an MCP transport layer that lets any MCP-compatible
agent call library functions as tools — no HTTP client required on the agent side.

- **API token system** — long-lived admin-generated tokens stored as SHA256 hashes;
  used by MCP clients instead of short-lived JWTs
- **MCP server binary** — `calibre-mcp` Rust binary using the `rmcp` crate;
  stdio transport for Claude Code/Desktop, SSE transport for web-based agents
- **Five library tools**: `search_books`, `get_book_metadata`, `list_chapters`,
  `get_book_text`, `semantic_search` — backed by direct DB calls, not HTTP self-calls
- **Integration documentation** — how to connect Claude Code, Claude Desktop,
  LangGraph, and smolagents to the library

## Key Design Decisions

- MCP server is a **separate binary** (`calibre-mcp`) in the same Cargo workspace,
  sharing all library code with the web server binary — no HTTP round-trips
- Tools call DB query functions directly (same sqlx pool) — not the REST API
- API tokens are stored as `SHA256(token)` — plain token is shown once at generation
  and never stored; admin can list and revoke by name
- stdio transport: launched by Claude Code / Claude Desktop as a subprocess
- SSE transport: serves `GET /mcp/sse` on the same Axum port — for LangGraph and
  other HTTP-based agent frameworks
- `semantic_search` tool returns 503-equivalent when `llm.enabled = false` —
  same graceful degradation as the REST API

## Key Schema Facts

- `api_tokens` table does not yet exist — Stage 1 adds it via migration
- All existing DB query modules (`db/queries/books.rs`, `db/queries/llm.rs`) are
  reused directly — no duplication
- `book_embeddings` table (from Phase 4) backs the `semantic_search` tool

## Reference Files

Read these before starting each stage:
- `docs/ARCHITECTURE.md` — Phase 8 spec, agent tool surface, MCP auth decision
- `docs/API.md` — Content API contracts for chapters/text; search route contracts
- `backend/src/db/queries/books.rs` — query functions the tools will call
- `backend/src/llm/` — semantic search and embedding patterns
- `tools/mcp_server.js` — the existing dev-tooling MCP server (Node.js) —
  useful as a structural reference for tool shape, but Phase 8 is Rust

---

## STAGE 1 — API Token System

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md and backend/src/api/admin.rs. Now do Stage 1 of Phase 8.

Add a long-lived API token system for MCP clients. Tokens are admin-generated,
stored as SHA256 hashes, and accepted in Authorization: Bearer headers as an
alternative to JWTs.

Deliverables:

backend/migrations/sqlite/0005_api_tokens.sql:
  CREATE TABLE api_tokens (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL UNIQUE,        -- human label e.g. "claude-desktop"
    token_hash   TEXT NOT NULL UNIQUE,        -- SHA256(token) hex string
    created_by   TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   TEXT NOT NULL,
    last_used_at TEXT                         -- updated on each authenticated request
  );

backend/migrations/mariadb/0004_api_tokens.sql — equivalent MariaDB DDL.

backend/src/db/queries/api_tokens.rs — new file:
  pub struct ApiToken { pub id: String, pub name: String, pub created_at: String,
    pub last_used_at: Option<String> }  -- token_hash never returned to callers

  pub async fn create_token(db: &SqlitePool, name: &str, token_hash: &str,
    created_by: &str) -> anyhow::Result<ApiToken>
  pub async fn find_by_hash(db: &SqlitePool, token_hash: &str)
    -> anyhow::Result<Option<ApiToken>>
  pub async fn touch_last_used(db: &SqlitePool, id: &str) -> anyhow::Result<()>
  pub async fn list_tokens(db: &SqlitePool, created_by: &str)
    -> anyhow::Result<Vec<ApiToken>>
  pub async fn delete_token(db: &SqlitePool, id: &str, created_by: &str)
    -> anyhow::Result<bool>  -- false if not found or not owned by caller

Register in backend/src/db/queries/mod.rs: pub mod api_tokens;

backend/src/middleware/auth.rs — extend JWT extraction to also accept API tokens:
  After failing JWT validation, check if the bearer token is a known API token:
    hash = SHA256(bearer_value) as hex string
    call find_by_hash(). If found: touch_last_used(), treat as authenticated with
    a synthetic Claims that has role "user" and user_id = token.created_by.
    If not found: return 401 as before.
  Use sha2 crate for SHA256 — add to backend/Cargo.toml if not present.

backend/src/api/admin.rs — add three token management routes:
  POST /api/v1/admin/tokens — Admin role
    Body: { "name": string }
    Generate 32 cryptographically random bytes (rand crate), encode as hex (64 chars).
    Hash with SHA256. Store hash. Return the plain token ONCE:
      { "id": string, "name": string, "token": string, "created_at": string }
    The plain token is never stored and cannot be retrieved again.
  GET /api/v1/admin/tokens — Admin role
    List tokens created by the current admin (never include token_hash).
    Response: { "items": ApiToken[] }
  DELETE /api/v1/admin/tokens/:id — Admin role
    Delete token by id (only if created by current admin).
    404 when not found. 204 on success.

Wire new routes into api/mod.rs under /api/v1/admin.

backend/tests/test_api_tokens.rs — new file:
  test_create_token_returns_plain_token — POST /admin/tokens, assert response
    has "token" field of length 64, assert token_hash NOT in response
  test_token_authenticates_requests — create token, use plain token as Bearer,
    call GET /api/v1/books, assert 200 (not 401)
  test_list_tokens_excludes_hash — GET /admin/tokens, assert no "token_hash" key
    in any item
  test_delete_token_revokes_auth — create token, delete it, use plain token,
    assert 401
  test_tokens_require_admin — non-admin user cannot call POST /admin/tokens, assert 403

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test test_api_tokens -- --nocapture 2>&1
    cargo test --workspace 2>&1 | tail -20

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file (api/admin.rs, middleware/auth.rs).
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 2 — MCP Server Binary + Tools

**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, docs/API.md, backend/src/db/queries/books.rs,
and backend/src/db/queries/llm.rs. Now do Stage 2 of Phase 8.

Build the calibre-mcp binary — a standalone MCP server that exposes five
library tools via stdio and SSE transports. Tools call DB functions directly
(same sqlx pool), never the REST API.

Deliverables:

Cargo.toml (workspace root) — add calibre-mcp as a workspace member:
  members = ["backend", "calibre-migrate", "calibre-mcp"]

calibre-mcp/Cargo.toml:
  [package] name = "calibre-mcp" version = "0.1.0" edition = "2021"
  [dependencies]:
    rmcp = { version = "0.1", features = ["server", "transport-stdio",
      "transport-sse-server"] }
    backend = { path = "../backend" }
    tokio = { version = "1", features = ["full"] }
    sqlx = { version = "0.7", features = ["sqlite", "runtime-tokio"] }
    serde = { version = "1", features = ["derive"] }
    serde_json = "1"
    anyhow = "1"
    tracing = "1"
    tracing-subscriber = "1"
    config = "0.14"
  Note: verify rmcp version on crates.io before writing — use the latest stable.

calibre-mcp/src/main.rs:
  Parse args: --transport stdio (default) | --transport sse --port <port>
  Load AppConfig from config.toml (reuse backend::config::AppConfig).
  Connect to SqlitePool (same DATABASE_URL).
  Run validate_llm_endpoint() on configured LLM endpoints (reuse from backend).
  Match transport:
    stdio → run_stdio_server(db, config)
    sse   → run_sse_server(db, config, port)

calibre-mcp/src/tools/mod.rs — define the five tools as an rmcp ServerHandler:

  search_books:
    Description: "Search the library by title, author, tag, series, or full-text query.
      Returns a paginated list of matching books with metadata."
    Parameters:
      q: string (optional) — full-text search query
      author: string (optional) — filter by author name (partial match)
      tags: string (optional) — comma-separated tag names
      document_type: string (optional) — novel|textbook|reference|magazine|datasheet|comic
      page: number (optional, default 1)
      page_size: number (optional, default 20, max 50)
    Implementation: call backend::db::queries::books::list_books() with mapped filters.
    Returns: JSON array of BookSummary objects with total count.

  get_book_metadata:
    Description: "Get full metadata for a single book including authors, tags, series,
      formats, and identifiers."
    Parameters:
      book_id: string (required)
    Implementation: call backend::db::queries::books::get_book().
    Returns: full Book JSON object. Error if not found.

  list_chapters:
    Description: "List the chapters of a book with titles and word counts.
      Requires the book to have an EPUB or PDF format."
    Parameters:
      book_id: string (required)
    Implementation: load format path via books query, call
      backend::ingest::text::list_chapters().
    Returns: { book_id, format, chapters: [{index, title, word_count}] }
    Error: "no_extractable_format" when book has no EPUB or PDF.

  get_book_text:
    Description: "Extract plain text from a book. Optionally request a single chapter
      by index (0-based, matching list_chapters output). Returns full book text
      when chapter is omitted. No LLM required."
    Parameters:
      book_id: string (required)
      chapter: number (optional) — 0-based chapter index
    Implementation: call backend::ingest::text::extract_text().
    Returns: { book_id, format, chapter, text, word_count }

  semantic_search:
    Description: "Search the library by semantic meaning using vector embeddings.
      Requires LLM features to be enabled (llm.enabled = true in config)."
    Parameters:
      query: string (required)
      limit: number (optional, default 10, max 50)
    Implementation: call backend::db::queries::llm::semantic_search() (or equivalent
      sqlite-vec query from Phase 4 semantic search module).
    Returns: JSON array of { book_id, title, authors, score } sorted by relevance.
    Error when llm.enabled = false: return tool error "semantic_search_unavailable:
      LLM features are disabled. Enable llm.enabled in config.toml."

calibre-mcp/src/transport/stdio.rs:
  pub async fn run_stdio_server(db: SqlitePool, config: AppConfig)
    Use rmcp StdioServerTransport. Register the tools handler.
    Log to stderr only (stdout is the MCP channel for stdio transport).

calibre-mcp/src/transport/sse.rs:
  pub async fn run_sse_server(db: SqlitePool, config: AppConfig, port: u16)
    Bind Axum router with rmcp SSE handler at GET /mcp/sse and POST /mcp/message.
    Authenticate: require Authorization: Bearer <api_token> on the SSE endpoint.
      Use the same SHA256 hash lookup as the web server middleware.
    Log connection events at INFO level.

calibre-mcp/tests/test_mcp_tools.rs — integration tests:
  test_search_books_returns_results — seed 3 books, call search_books tool with
    q="fiction", assert results is an array
  test_get_book_metadata_returns_full_record — seed book with author + tag,
    call get_book_metadata, assert authors array non-empty
  test_get_book_text_no_llm_required — seed book with EPUB fixture,
    call get_book_text with no chapter param, assert text field non-empty,
    assert tool does not error when llm.enabled=false
  test_semantic_search_error_when_disabled — llm.enabled=false, call semantic_search,
    assert tool returns error containing "semantic_search_unavailable"
  test_list_chapters_returns_spine — seed book with minimal.epub fixture,
    call list_chapters, assert at least one chapter returned

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test test_mcp_tools -- --nocapture 2>&1
    cargo test --workspace 2>&1 | tail -20

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file (mcp/tools.rs, mcp/server.rs).
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  cargo build --release -p calibre-mcp 2>&1 | tail -10
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 3 — Integration Documentation

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md and calibre-mcp/src/main.rs. Now do Stage 3 of Phase 8.

Write integration documentation and update project metadata.

Deliverables:

docs/MCP.md — complete integration guide:

  # MCP Server Integration

  calibre-web-rs exposes your library as an MCP (Model Context Protocol) tool provider.
  Any MCP-compatible agent can search your library, read book metadata, extract chapter
  text, and run semantic search — as native tool calls.

  ## Available Tools

  | Tool | Description | LLM Required |
  |---|---|---|
  | search_books | Full-text + filtered library search | No |
  | get_book_metadata | Complete metadata for one book | No |
  | list_chapters | Table of contents from EPUB or PDF | No |
  | get_book_text | Plain text extraction, full or by chapter | No |
  | semantic_search | Vector similarity search | Yes (llm.enabled=true) |

  ## Setup: Generate an API Token

  Before connecting any client, generate a long-lived API token:
  ```bash
  curl -X POST https://your-library/api/v1/admin/tokens \
    -H "Authorization: Bearer <your-jwt>" \
    -H "Content-Type: application/json" \
    -d '{"name": "claude-desktop"}'
  ```
  Save the returned `token` value — it is shown only once.

  ## Claude Code Integration

  Add to your Claude Code MCP config:
  ```bash
  claude mcp add calibre-library calibre-mcp \
    --env CALIBRE_DB_URL=sqlite:///path/to/library.db \
    --env CALIBRE_CONFIG=/path/to/config.toml
  ```
  Or add manually to `.claude/settings.json`:
  ```json
  {
    "mcpServers": {
      "calibre-library": {
        "command": "/path/to/calibre-mcp",
        "args": ["--transport", "stdio"],
        "env": {
          "CALIBRE_CONFIG": "/path/to/config.toml"
        }
      }
    }
  }
  ```

  ## Claude Desktop Integration

  Add to `~/Library/Application Support/Claude/claude_desktop_config.json`:
  ```json
  {
    "mcpServers": {
      "calibre-library": {
        "command": "/path/to/calibre-mcp",
        "args": ["--transport", "stdio"],
        "env": {
          "CALIBRE_CONFIG": "/path/to/config.toml"
        }
      }
    }
  }
  ```

  ## LangGraph / HTTP Agent Integration

  Start the SSE transport:
  ```bash
  calibre-mcp --transport sse --port 8084
  ```
  Connect from LangGraph:
  ```python
  from langchain_mcp_adapters.client import MultiServerMCPClient
  client = MultiServerMCPClient({
    "calibre": {
      "url": "http://localhost:8084/mcp/sse",
      "transport": "sse",
      "headers": {"Authorization": "Bearer <api-token>"}
    }
  })
  tools = await client.get_tools()
  ```

  ## smolagents Integration

  ```python
  from smolagents import ToolCollection, CodeAgent, HfApiModel
  tools = ToolCollection.from_mcp(
    {"url": "http://localhost:8084/mcp/sse",
     "headers": {"Authorization": "Bearer <api-token>"}}
  )
  agent = CodeAgent(tools=[*tools.tools], model=HfApiModel())
  agent.run("Find all textbooks about machine learning in my library")
  ```

  ## Example Agentic Query

  Once connected, an agent can run multi-step library queries:
  1. `search_books(tags="machine-learning", document_type="textbook")` → get book IDs
  2. `list_chapters(book_id=<id>)` → identify relevant chapters
  3. `get_book_text(book_id=<id>, chapter=3)` → retrieve the chapter text
  4. Agent synthesizes an answer from retrieved passages

docs/ARCHITECTURE.md — mark Phase 8 complete:
  Update the Phase 8 checklist replacing [ ] with [x] for all items.
  Add a "Completed" note to the phase header.

CLAUDE.md (calibre-web-rs root) — add to MCP Tools section:
  ## Library MCP Server (for agents)
  Build: cargo build --release -p calibre-mcp
  Register with Claude Code:
    claude mcp add calibre-library ./target/release/calibre-mcp \
      --env CALIBRE_CONFIG=./config.toml
  Exposes: search_books, get_book_metadata, list_chapters, get_book_text,
    semantic_search

When done, run:
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## Review Checkpoints

| After stage | What to verify |
|---|---|
| Stage 1 | Plain token returned once and not stored; token hash lookup used in middleware; delete only works for token owner; 5/5 tests passing |
| Stage 2 | Tools call DB functions directly (not HTTP); get_book_text works when llm.enabled=false; semantic_search returns tool error (not panic) when disabled; calibre-mcp binary builds release |
| Stage 3 | docs/MCP.md covers all four client types; CLAUDE.md registration command uses correct binary path; ARCHITECTURE.md Phase 8 marked complete |

## If Codex Gets Stuck or a Test Fails

```
The following is failing. Diagnose the root cause and fix it.
Do not work around it — fix the underlying issue.

[paste error output]
```

## Commit Sequence

```bash
# After Stage 1
git add -A && git commit -m "Phase 8 Stage 1: API token system, admin CRUD, token middleware, 5/5 tests passing"

# After Stage 2
git add -A && git commit -m "Phase 8 Stage 2: calibre-mcp binary, 5 library tools, stdio + SSE transports, tests passing"

# After Stage 3
git add -A && git commit -m "Phase 8 Stage 3: MCP integration docs, CLAUDE.md update, Phase 8 complete"
```
