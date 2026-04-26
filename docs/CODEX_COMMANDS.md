# Codex Desktop App — calibre-web-rs Phase 1

## How This Works

The Codex desktop app reads files directly from your Mac, runs terminal commands
(cargo, git, etc.), and writes files — all from a chat interface. You send it
stage prompts as messages. It does the work. You review at each checkpoint.

No API key, no bash paste-list. Just copy/paste the prompt for each stage.

---

## Before You Start — One-Time Setup (do this in Terminal)

This creates the new repo and copies all reference docs in so Codex can read them:

```bash
cd ~/Documents/localProject
mkdir -p calibre-web-rs/docs
cd calibre-web-rs
git init

cp ../calibre-web/ARCHITECTURE.md docs/
cp ../calibre-web/SCHEMA.md       docs/
cp ../calibre-web/API.md          docs/
cp ../calibre-web/DESIGN.md       docs/
cp ../calibre-web/HANDOFF.md      docs/
cp ../calibre-web/SKILLS.md       docs/
```

Then open the Codex desktop app and **point it at:**
```
~/Documents/localProject/calibre-web-rs
```

---

## How to Use Codex Desktop at Each Stage

1. Copy the prompt for that stage (below)
2. Paste it into the Codex chat
3. Let Codex work — it will read files, create files, run cargo commands
4. When it finishes, ask it to run the checkpoint commands (also below)
5. Copy the checkpoint output and paste it here (to Claude) for review
6. Proceed to the next stage only after review passes

---

## STAGE 1 — Scaffold + Write All Tests

**Paste this into Codex:**

```
Read docs/HANDOFF.md in full before doing anything else. Also read
docs/ARCHITECTURE.md, docs/SCHEMA.md, docs/API.md, and docs/DESIGN.md.

Then do Stage 1 only:

Create the full repository scaffold and write ALL test files. Do not implement
any handlers or business logic yet. Tests should compile but all be marked
#[ignore] or have todo!() bodies.

Deliverables:
- Cargo.toml (workspace root)
- backend/Cargo.toml with all dependencies from HANDOFF.md
- Full directory structure exactly as shown in HANDOFF.md
- backend/tests/common/mod.rs — implement ALL helpers fully: TestContext,
  create_admin, create_user, login, admin_token, user_token, create_book,
  create_book_with_file, minimal_epub_bytes, minimal_pdf_bytes,
  assert_status! macro, assert_json_field! macro
- backend/tests/fixtures/ — all 5 fixture files: minimal.epub, minimal.pdf,
  minimal.mobi, fake.epub, cover.jpg as real binary files
- backend/tests/test_config.rs — all test functions, bodies todo!() or #[ignore]
- backend/tests/test_auth.rs — all test functions, bodies todo!() or #[ignore]
- backend/tests/test_books.rs — all test functions, bodies todo!() or #[ignore]
- backend/tests/test_file_serving.rs — all test functions, bodies todo!() or #[ignore]
- backend/tests/test_security.rs — all test functions, bodies todo!() or #[ignore]
- backend/src/lib.rs and backend/src/main.rs (stubs only — enough to compile)
- config.example.toml
- .github/workflows/ci.yml
- .gitignore
- .claude/settings.json — hooks and permissions allowlist per HANDOFF.md spec
- tools/mcp_server.js — MCP server per HANDOFF.md spec
- tools/package.json
- CLAUDE.md — per the CLAUDE.md scaffold in HANDOFF.md

When done, run these commands and show me the output:
  cargo check 2>&1 | head -40
  cargo test --test test_config 2>&1 | head -20
  ls backend/tests/fixtures/
  ls .claude/
  ls tools/
  git diff --stat
```

**When Codex finishes — paste the output here (to Claude) with the message:**
> "Stage 1 done — here is the output, please review"

---

## STAGE 2 — Config + Database Migrations

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 2.

Implement config loading and database migrations. Make all tests in
test_config.rs pass. Remove #[ignore] from config tests as you implement each.

Deliverables:
- backend/src/config.rs — full implementation per HANDOFF.md requirements
- backend/migrations/sqlite/0001_initial.sql — all 18 tables from docs/SCHEMA.md
- backend/migrations/mariadb/0001_initial.sql — equivalent MariaDB migration
- backend/src/state.rs — AppState struct with db pool and config
- Update tests/common/mod.rs test_db() helper to run migrations

When done, run these and show me the output:
  cargo test --test test_config -- --nocapture 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 3 — Auth Routes

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 3.

Implement all auth routes and JWT middleware. Make all tests in test_auth.rs
pass. Remove #[ignore] from auth tests as you implement each.

Deliverables:
- backend/src/api/auth.rs — all 6 auth handlers
- backend/src/middleware/auth.rs — JWT extraction and validation middleware
- backend/src/db/queries/auth.rs — all user and token DB queries
- Account lockout logic (login_attempts, locked_until columns)
- Update tests/common/mod.rs login() and auth_header() helpers

When done, run these and show me the output:
  cargo test --test test_auth -- --nocapture 2>&1
  git diff --stat
```

**Paste output here → Claude runs /review + /simplify in parallel → proceed if passing.**

---

## STAGE 4 — Books CRUD

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 4.

Implement books CRUD API and the ingest pipeline. Cover extraction is Stage 5 —
skip it here. Make all non-cover tests in test_books.rs pass.

Deliverables:
- backend/src/api/books.rs — GET /books, GET /books/:id, POST /books,
  PATCH /books/:id, DELETE /books/:id
- backend/src/db/queries/books.rs — pagination and filter logic
- Multipart upload: magic byte detection, format validation, file storage
- Duplicate ISBN detection
- audit_log writes on PATCH in the same DB transaction as the update
- StorageBackend trait with LocalFs implementation

Cover-related tests may remain #[ignore] until Stage 5.

When done, run these and show me the output:
  cargo test --test test_books -- --nocapture 2>&1
  git diff --stat
```

**Paste output here → Claude runs /review + /simplify in parallel → proceed if passing.**

---

## STAGE 5 — File Serving + Cover Pipeline

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 5.

Implement file serving with HTTP range request support and the cover pipeline.
Make all tests in test_file_serving.rs pass, and remove #[ignore] from any
remaining cover-related tests in test_books.rs.

Deliverables:
- backend/src/api/books.rs — add download, stream (range requests), cover routes
- Cover extraction from epub (container.xml → OPF → cover-image item)
- Cover resize to 400x600 max + 100x150 thumbnail using the image crate
- Bucketed storage path: storage/covers/{first2_of_uuid}/{uuid}.jpg and .thumb.jpg
- Path traversal prevention: canonicalize path, assert it starts with storage_path
- tower-http ServeFile for streaming with range request support

When done, run these and show me the output:
  cargo test --test test_file_serving -- --nocapture 2>&1
  cargo test --test test_books -- --nocapture 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 6 — Security Middleware

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 6.

Implement security headers middleware and rate limiting. Make all tests in
test_security.rs pass.

Deliverables:
- backend/src/middleware/security_headers.rs — tower layer with all 5 headers:
  X-Content-Type-Options, X-Frame-Options, Referrer-Policy,
  Content-Security-Policy, Permissions-Policy
- tower-governor rate limiting: 10 req/min on /auth/* routes, 200 req/min global
- Upload size enforcement: return 413 if Content-Length exceeds config limit
- Wire all middleware into the Axum router in backend/src/api/mod.rs

When done, run these and show me the output:
  cargo test --test test_security -- --nocapture 2>&1
  git diff --stat
```

**Paste output here → Claude runs /review + /security-review in parallel → proceed if passing.**

---

## STAGE 7 — Docker + Final Checks

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 7.

Write the Docker setup and make sure all CI checks pass with zero issues.

Deliverables:
- docker/Dockerfile — multi-stage build, non-root user, target size under 50MB
- docker/docker-compose.yml — app + meilisearch + commented-out caddy service
- docker/Caddyfile

When done, run all of these and show me the complete output:
  cargo test --workspace 2>&1
  cargo clippy --workspace -- -D warnings 2>&1
  cargo audit 2>&1
  docker build -f docker/Dockerfile -t calibre-web-rs:dev . 2>&1 | tail -20
  git diff --stat
```

**Paste output here → Claude runs /review + /engineering:deploy-checklist → Phase 1 complete.**

---

## After Phase 1 — Register the MCP Server (Terminal)

Run this once in Terminal after Stage 1 is complete:

```bash
cd ~/Documents/localProject/calibre-web-rs/tools
npm install
cd ..
claude mcp add calibre-dev node tools/mcp_server.js
```

Then open `calibre-web-rs` in Claude Code for all future sessions — hooks,
MCP tools, and CLAUDE.md all activate automatically.

---

## If Codex Gets Stuck or a Test Fails

Paste the error output back into Codex with:

```
The following test is failing. Diagnose the root cause and fix it.
Do not work around it — fix the underlying issue.

[paste error output]
```

If still stuck after one attempt, paste the error here (to Claude) and
run /engineering:debug for diagnosis before trying Codex again.
