# Codex Desktop App — xcalibre-server Phase 19: Local Hardening

## What Phase 19 Builds

Phase 19 closes the remaining local operational gaps: Playwright E2E tests,
xcalibre-specific security policy, xs-migrate test coverage, the API token
scope selector, and configuration cleanup.

GitHub Actions workflows are handled separately in CODEX_COMMANDS_GITHUB_PUBLISH.md.

TDD rule (non-negotiable): tests are written first and run to confirm failure,
then the implementation is written to make them pass. Never write implementation
before the test exists.

---

## STAGE 1 — Playwright E2E Critical Path Tests

Playwright E2E is the deliverable of this stage — there is no prior
implementation to test-drive against. The tests ARE the implementation.
They will fail on first run if the server is not running, which is expected.
Run them headed locally to watch and fix each failure.

**Paste this into Codex:**

```
Read apps/web/playwright.config.ts and check for any existing E2E tests
under apps/web/e2e/.

─────────────────────────────────────────
STEP 1 — Update Playwright config
─────────────────────────────────────────

Update apps/web/playwright.config.ts — confirm or set webServer entries:

  webServer: [
    {
      command: 'XCS_DB_URL=sqlite://test_e2e.db XCS_LLM_ENABLED=false XCS_STORAGE_PATH=./storage_e2e cargo run -p backend',
      url: 'http://localhost:8083/api/v1/health',
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
      cwd: '../..',
    },
    {
      command: 'pnpm --filter @xs/web dev',
      url: 'http://localhost:5173',
      reuseExistingServer: !process.env.CI,
    },
  ],
  use: { baseURL: 'http://localhost:5173' }

Add a globalSetup script apps/web/e2e/global-setup.ts that:
  1. Deletes test_e2e.db and storage_e2e/ before each run
  2. Runs sqlx migrate run against test_e2e.db

─────────────────────────────────────────
STEP 2 — Write E2E tests
─────────────────────────────────────────

Create apps/web/e2e/critical-path.spec.ts with these six tests:

  test('register and login', async ({ page }) => {
    // Navigate to /register
    // Fill username, email, password — submit
    // Assert redirected to /home
    // Navigate to /login, log in again → assert /home
  })

  test('upload a book and see it in the library', async ({ page }) => {
    // Login as seeded test user
    // Navigate to /browse/books
    // Click upload button, upload apps/web/e2e/fixtures/test.epub
    // Assert the book card appears with title visible
    // Assert cover image loads (no broken img src)
  })

  test('search returns results', async ({ page }) => {
    // Navigate to /search
    // Type a word that appears in the test EPUB
    // Assert at least one result card appears with title visible
  })

  test('open reader and navigate chapters', async ({ page }) => {
    // Click a book card → book detail
    // Click "Read" button → reader page
    // Assert EPUB content renders (not a blank iframe)
  })

  test('admin creates and revokes an API token', async ({ page }) => {
    // Login as admin → /admin → API tokens section
    // Click "Create token", fill name, select scope "Read"
    // Assert token value shown (one-time display)
    // Click revoke → token disappears from list
  })

  test('memory ingest via API', async ({ request }) => {
    // POST /api/v1/auth/login → get bearer token
    // POST /api/v1/memory { text: "Test memory chunk from E2E", chunk_type: "episodic" }
    // Assert 201 with id field
    // GET /api/v1/search/chunks?q=Test+memory&source=memory
    // Assert chunk appears in results
  })

Add apps/web/e2e/fixtures/test.epub — a minimal valid 3-chapter EPUB (~5KB).
Generate it programmatically in a fixture helper or download the smallest
available sample EPUB from the epub3-samples repository.

─────────────────────────────────────────
STEP 3 — Run headed and fix failures
─────────────────────────────────────────

  pnpm --filter @xs/web exec playwright test --headed

Watch each test in Chromium. Fix any selector mismatches or timing issues.
Screenshot artifacts appear in apps/web/playwright-report/ on failure.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

pnpm --filter @xs/web exec playwright test
All 6 tests must pass in headless mode.

Commit: "test: Playwright E2E critical path — register, upload, search, read, memory (Phase 19 Stage 1)"
```

---

## STAGE 2 — SECURITY.md Rewrite

Documentation only — no code, TDD not applicable.

**Paste this into Codex:**

```
Read docs/SECURITY.md. It currently contains calibre-web's policy — out of
date and referencing wrong CVEs and contact details. Replace it entirely.

Write docs/SECURITY.md:

  # Security Policy

  ## Supported Versions

  | Version | Supported |
  |---------|-----------|
  | Latest (main branch) | ✅ |
  | All prior releases | ❌ (self-hosted; update to latest) |

  xcalibre-server is self-hosted software with a single active release line.
  Security fixes ship in patch releases. Older versions do not receive
  backported patches — operators should update.

  ## Reporting a Vulnerability

  **Do not open a public GitHub issue for security vulnerabilities.**

  Report via GitHub private security advisories (once the repository is public):
  → https://github.com/<org>/xcalibre-server/security/advisories/new

  Until the repository is public, report directly to the maintainer by email.

  Include: description, steps to reproduce, impact assessment, suggested fix.

  ## Response SLA

  | Severity | Acknowledge | Patch release |
  |----------|-------------|---------------|
  | Critical | 24 hours    | 7 days        |
  | High     | 48 hours    | 14 days       |
  | Medium   | 7 days      | 30 days       |
  | Low/Info | 14 days     | Next minor    |

  ## Scope

  In scope: auth bypass, privilege escalation, injection (SQL/command/SSRF/
  prompt), path traversal, sensitive data exposure, cryptographic weaknesses.

  Out of scope: physical access, social engineering, attacks requiring existing
  admin access, unauthenticated DoS, theoretical issues without demonstrated impact.

  ## Known Design Decisions

  The following are intentional and not considered vulnerabilities:

  - CSP includes 'unsafe-inline' on script-src and style-src to support
    epub.js rendering and shadcn/ui dynamic styles.
  - LLM endpoint URLs are operator-configured and trusted (not user-supplied).
    SSRF protection requires allow_private_endpoints = true for local model
    servers — this is an explicit operator opt-in.
  - The OPDS feed is unauthenticated by default. Enable opds.require_auth = true
    in config.toml to require authentication.

  ## Dependency Audit

  cargo audit is run before every release. Suppressed advisories are documented
  in .cargo/audit.toml with justification comments.

Confirm no calibre-web references remain in the file.
Commit: "docs: rewrite SECURITY.md with xcalibre-server-specific policy (Phase 19 Stage 2)"
```

---

## STAGE 3 — xs-migrate Test Coverage

xs-migrate already exists. Tests are written first; any failures reveal bugs
to fix in the implementation before the stage is done.

**Paste this into Codex:**

```
Read xs-migrate/src/ in full to understand the CLI structure and import stages.
Check xs-migrate/tests/ — if any tests already exist, read them too.

─────────────────────────────────────────
STEP 1 — Write tests first
─────────────────────────────────────────

Create xs-migrate/tests/test_import.rs with four tests. Write all four
test function stubs first — use todo!() as the body so they compile but
clearly fail:

  #[tokio::test]
  async fn test_dry_run_reads_calibre_metadata_without_writing() { todo!() }

  #[tokio::test]
  async fn test_import_maps_calibre_fields_correctly() { todo!() }

  #[tokio::test]
  async fn test_import_idempotent_second_run_does_not_duplicate() { todo!() }

  #[tokio::test]
  async fn test_import_skips_missing_files_gracefully() { todo!() }

Run: cargo test -p xs-migrate -- --nocapture
Confirm all four fail with "not yet implemented". This is the expected red state.

─────────────────────────────────────────
STEP 2 — Build the fixture
─────────────────────────────────────────

Create xs-migrate/tests/fixtures/mod.rs (or a setup fn in test_import.rs)
that programmatically builds a minimal Calibre metadata.db using rusqlite:
  - 3 books with title, author, and tags
  - 2 authors
  - 5 tags
  - One book references an EPUB path that does not exist on disk

─────────────────────────────────────────
STEP 3 — Implement each test
─────────────────────────────────────────

Replace the todo!() bodies with real test logic:

  test_dry_run_reads_calibre_metadata_without_writing:
    - Build fixture DB
    - Run xs-migrate with --dry-run flag
    - Assert: exit code 0, no books written to target DB,
      stdout lists the 3 books that would be imported

  test_import_maps_calibre_fields_correctly:
    - Build fixture DB, run import against a fresh test xcalibre-server DB
    - For each imported book assert: title matches, authors non-empty,
      tags imported, file path recorded

  test_import_idempotent_second_run_does_not_duplicate:
    - Import fixture once → assert 3 books (minus the missing-file book = 2)
    - Import same fixture again
    - Assert: still the same count (no duplicates)

  test_import_skips_missing_files_gracefully:
    - Import fixture that includes a book with a non-existent EPUB path
    - Assert: import completes without panic, missing book is skipped
      with a warning in stderr, other books are imported

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

cargo test -p xs-migrate -- --nocapture
All four tests must pass (green).
cargo clippy -p xs-migrate -- -D warnings

Commit: "test: xs-migrate import — dry run, field mapping, idempotency, missing files (Phase 19 Stage 3)"
```

---

## STAGE 4 — Frontend: API Token Scope Selector

**Paste this into Codex:**

```
Read apps/web/src/ — find the admin panel component that handles API token
creation. Read the existing token creation form, its test file, and
apps/web/src/test/handlers.ts.

─────────────────────────────────────────
STEP 1 — Write tests first
─────────────────────────────────────────

In the existing token creation component test file, add the following
failing tests BEFORE touching any component code:

  test('scope radio group renders with Read, Read-Write, and Admin options')
  test('Admin option is disabled when current user is not an admin')
  test('submitting the form with Read selected calls API with scope: "read"')
  test('token list displays a scope badge for each token')

Add MSW handlers in apps/web/src/test/handlers.ts for:
  POST /api/v1/auth/tokens — returns a token with a scope field

Run: pnpm --filter @xs/web test run
Confirm the four new tests fail (the component does not yet have scope UI).
This is the expected red state.

─────────────────────────────────────────
STEP 2 — Implement the scope selector
─────────────────────────────────────────

Now modify the token creation modal/form component to add:

  A RadioGroup with three options:
    value="read"  label="Read"       description="Query books and metadata only"
    value="write" label="Read-Write" description="Full access to user resources"
    value="admin" label="Admin"      description="Admin panel access (admin users only)"

  Rules:
    - Default: "write"
    - "Admin" option is disabled if the current user is not an admin role
    - On submit, include scope in the request body

Add scope badges to the token list:
  "Read" → blue badge, "Read-Write" → green badge, "Admin" → red badge

Add i18n keys to all four locale files:
  "token": {
    "scope_read": "Read",
    "scope_write": "Read-Write",
    "scope_admin": "Admin",
    "scope_read_desc": "Query books and metadata only",
    "scope_write_desc": "Full access to user resources",
    "scope_admin_desc": "Admin panel access (admin users only)"
  }

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

pnpm --filter @xs/web test run
All four new tests plus all pre-existing tests must pass (green).

Visual check:
  pnpm --filter @xs/web dev
  Open http://localhost:5173/admin, navigate to API tokens, click "Create token".
  Confirm scope radio group appears. Select "Read" → confirm badge shows "Read".

Commit: "feat: API token scope selector in admin panel (Phase 19 Stage 4)"
```

---

## STAGE 5 — Open Items Cleanup and v2.0 Tag

**Paste this into Codex:**

```
Read docs/STATE.md (Open Items section), backend/src/config.rs, and
apps/web/src/ for the stale translation key.

─────────────────────────────────────────
DELIVERABLE 1 — allow_private_endpoints namespace
─────────────────────────────────────────

Read backend/src/config.rs. The allow_private_endpoints field lives under
LlmSection but also controls webhook SSRF validation.

WRITE THE TEST FIRST. In the relevant backend integration test file, add:

  #[tokio::test]
  async fn test_effective_allow_private_prefers_network_section() {
      // Build AppConfig with network.allow_private_endpoints = true
      //   and llm.allow_private_endpoints = false
      // Assert effective_allow_private() returns true
  }

  #[tokio::test]
  async fn test_effective_allow_private_falls_back_to_llm_section() {
      // Build AppConfig with network.allow_private_endpoints = false
      //   and llm.allow_private_endpoints = true
      // Assert effective_allow_private() returns true
  }

Run cargo test --workspace — confirm both new tests fail (function not found).

Now implement:

  Add NetworkSection to config.rs:
    #[derive(Clone, Debug, Default, Serialize, Deserialize)]
    #[serde(default)]
    pub struct NetworkSection {
        pub allow_private_endpoints: bool,
    }
  Add to AppConfig: pub network: NetworkSection,

  Add function:
    pub fn effective_allow_private(config: &AppConfig) -> bool {
        config.network.allow_private_endpoints || config.llm.allow_private_endpoints
    }

  Update all callers to use effective_allow_private(&config).

  Update config.example.toml — add after [app]:
    [network]
    # allow_private_endpoints = false
    # Set to true to allow LLM endpoints and webhook targets on
    # private/loopback addresses. Also configurable as
    # llm.allow_private_endpoints for backwards compatibility.

Run cargo test --workspace — both new tests plus all existing tests must pass.
Run cargo clippy -- -D warnings.

─────────────────────────────────────────
DELIVERABLE 2 — Translation key cleanup
─────────────────────────────────────────

Check whether book.unarchive exists in the EN locale file.
Grep all source files for uses of "book.unarchive":
  grep -r "book.unarchive" apps/web/src/

If used in code: add the key to EN and all other locale files.
If NOT used in code: delete it from all locale files.

─────────────────────────────────────────
DELIVERABLE 3 — CHANGELOG
─────────────────────────────────────────

Create docs/CHANGELOG.md if it does not exist. Add or prepend:

  ## [2.0.0] — YYYY-MM-DD

  ### Added
  - Memory API: POST /api/v1/memory, DELETE /api/v1/memory/{id}
  - /search/chunks?source=memory|all for unified RAG retrieval
  - embedding_model config field — split embedding and chat model configuration
  - Playwright E2E test suite — register, upload, search, read, memory ingest
  - API token scope selector in admin panel (read / read-write / admin)
  - xs-migrate test coverage — dry run, field mapping, idempotency

  ### Changed
  - allow_private_endpoints promoted to [network] top-level config section
    (llm.allow_private_endpoints still works for backwards compatibility)

  ### Security
  - SECURITY.md rewritten with xcalibre-specific advisory process and SLA

─────────────────────────────────────────
DELIVERABLE 4 — STATE.md update
─────────────────────────────────────────

Update docs/STATE.md:
  - Overall status: Phase 19 Complete
  - Phase 19 row: ✅ Complete
  - Close resolved Open Items: book.unarchive, allow_private_endpoints namespace,
    E2E Playwright suite
  - Update Last verified date to today

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

cargo test --workspace
cargo clippy -- -D warnings
pnpm --filter @xs/web test run
pnpm --filter @xs/web exec playwright test

Commit: "chore: Phase 19 cleanup — config namespace, translation key, CHANGELOG, v2.0 tag"
git tag -a v2.0.0 -m "Phase 19 complete — E2E, SECURITY.md, xs-migrate tests, scope UI, config cleanup"
```

---

## Post-Phase-19 Checklist (local)

- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] `cargo audit` — zero CVEs
- [ ] `cargo test -p xs-migrate` — xs-migrate tests pass
- [ ] `pnpm --filter @xs/web test run` — all vitest tests pass
- [ ] `pnpm --filter @xs/web exec playwright test` — all 6 E2E tests pass
- [ ] `docs/SECURITY.md` — no calibre-web references remain
- [ ] `docs/CHANGELOG.md` — v2.0.0 entry present
- [ ] `docs/STATE.md` — Phase 19 complete
- [ ] `config.example.toml` — `[network]` section present
- [ ] Tag `v2.0.0` exists locally
