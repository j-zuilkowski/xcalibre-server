# Codex Desktop App — xcalibre-server Phase 19: CI/CD and Production Hardening

## What Phase 19 Builds

Phase 17 declared the security posture production-ready. Phase 18 shipped the Merlin memory integration. Phase 19 closes the remaining operational gaps: automated CI/CD, a real E2E test suite, xcalibre-specific security policy, xs-migrate test coverage, and the last open items from STATE.md.

After this phase, xcalibre-server is suitable for rapid iteration by multiple contributors without manual test runs before merge.

- **Stage 1** — GitHub Actions CI pipeline: `cargo test`, `cargo clippy`, `cargo audit`, `vitest`, Docker multi-arch build on every push and PR
- **Stage 2** — Playwright E2E critical path: register → login → upload → search → read → memory ingest
- **Stage 3** — SECURITY.md rewrite: xcalibre-specific vulnerability reporting, CVE history, response SLA
- **Stage 4** — xs-migrate test coverage: import dry-run, real Calibre library fixture, idempotency
- **Stage 5** — Frontend: API token scope selector in the admin panel (creates tokens with read/write/admin scope)
- **Stage 6** — Open items cleanup: translation key, `allow_private_endpoints` namespace, E2E CI promotion, CHANGELOG entry, v2.0 tag

---

## Key Design Decisions

**GitHub Actions over local-only testing:**
The existing quality gate is entirely manual. Any contributor (including future Codex runs) can ship a broken build. The CI pipeline blocks merges on `cargo test`, `cargo clippy -- -D warnings`, `cargo audit`, `pnpm vitest run`, and a Docker build. The Docker multi-arch build (`amd64`, `arm64`, `armv7`) runs on every push to `main` and publishes to `ghcr.io` on tagged releases.

**Playwright over Cypress or custom test harness:**
Playwright is already configured in `package.json` (from Phase 12). No new tooling dependency. The E2E suite is thin: it covers the five critical user journeys (register, login, upload, search, read) and the new memory ingest API. It runs headless in CI against a real `cargo run` server instance with a test SQLite DB.

**E2E test server startup strategy:**
The Playwright config starts the backend with `cargo run` and the web dev server with `pnpm --filter web dev` as `webServer` entries. The backend uses `XCS_DB_URL=sqlite://test_e2e.db` and `XCS_LLM_ENABLED=false`. The DB is wiped before each test run. This is the same approach Playwright recommends for full-stack web apps.

**SECURITY.md is a first-class document:**
The current file is the calibre-web security policy verbatim. It references v0.6.x CVEs and a Google reporting email. This is misleading to security researchers. The rewrite covers: supported versions (current only — self-hosted, single maintainer), reporting channel (GitHub private security advisory), response SLA (acknowledge in 48h, patch in 14 days for Critical/High), and the known-safe-by-design choices (no user-supplied SQL, no eval, no shell exec).

**`allow_private_endpoints` namespace promotion:**
STATE.md notes this as a known issue: the flag lives under `[llm]` but also controls webhook SSRF validation. Promoting it to `[app]` or a `[network]` top-level key requires a config migration note (old key still works, new key takes precedence). This is a one-stage cleanup with no behaviour change.

**API token scope UI:**
The scope enforcement (Phase 17 Stage 10) landed as a backend-only change. The admin panel token creation form shows no scope selector — new tokens get the default `write` scope silently. This stage adds a three-option radio group (Read / Read-Write / Admin) to the `CreateApiTokenModal` component, consistent with the existing token management UI.

---

## Key Schema Changes

No new DB migrations this phase. All existing tables are unchanged.

---

## Reference Files

Read before starting each stage:
- `.github/` — check for existing workflow files (Stage 1)
- `docker/Dockerfile` and `docker/docker-compose.yml` — used by CI build step (Stage 1)
- `apps/web/playwright.config.ts` — existing Playwright config (Stage 2)
- `apps/web/e2e/` — existing E2E test directory, if present (Stage 2)
- `docs/SECURITY.md` — current (calibre-web) content to replace (Stage 3)
- `xs-migrate/src/` — CLI source to understand what needs testing (Stage 4)
- `apps/web/src/` — admin panel components around API token creation (Stage 5)
- `docs/STATE.md` — open items to close (Stage 6)
- `backend/src/config.rs` — `allow_private_endpoints` current location (Stage 6)

---

## STAGE 1 — GitHub Actions CI Pipeline

**Priority: High**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Check whether .github/workflows/ exists. Read docker/Dockerfile and
docker/docker-compose.yml to understand the build process.

Create a complete GitHub Actions CI pipeline that runs on every push and pull
request to main.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Currently all quality gates are manual. A bad push can break the build for
any contributor. CI blocks merges on failures and publishes Docker images
on version tags.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

Create .github/workflows/ci.yml:

  name: CI
  on:
    push:
      branches: [main]
    pull_request:
      branches: [main]

  env:
    CARGO_TERM_COLOR: always

  jobs:
    rust:
      name: Rust (test + lint + audit)
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: dtolnay/rust-toolchain@stable
          with:
            components: clippy
        - uses: Swatinem/rust-cache@v2
        - name: cargo test
          run: cargo test --workspace --locked
        - name: cargo clippy
          run: cargo clippy --workspace --locked -- -D warnings
        - name: cargo audit
          run: |
            cargo install cargo-audit --locked
            cargo audit

    frontend:
      name: Frontend (typecheck + vitest)
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: pnpm/action-setup@v3
          with:
            version: 9
        - uses: actions/setup-node@v4
          with:
            node-version: 20
            cache: pnpm
        - run: pnpm install --frozen-lockfile
        - name: typecheck
          run: pnpm --filter @xs/web exec tsc --noEmit
        - name: vitest
          run: pnpm --filter @xs/web test run

    docker:
      name: Docker build (multi-arch dry-run)
      runs-on: ubuntu-latest
      needs: [rust, frontend]
      steps:
        - uses: actions/checkout@v4
        - uses: docker/setup-qemu-action@v3
        - uses: docker/setup-buildx-action@v3
        - name: Build amd64
          uses: docker/build-push-action@v5
          with:
            context: .
            file: docker/Dockerfile
            platforms: linux/amd64
            push: false
            tags: xcalibre-server:ci

Create .github/workflows/release.yml:

  name: Release
  on:
    push:
      tags: ['v*.*.*']

  jobs:
    publish:
      runs-on: ubuntu-latest
      permissions:
        contents: read
        packages: write
      steps:
        - uses: actions/checkout@v4
        - uses: docker/setup-qemu-action@v3
        - uses: docker/setup-buildx-action@v3
        - uses: docker/login-action@v3
          with:
            registry: ghcr.io
            username: ${{ github.actor }}
            password: ${{ secrets.GITHUB_TOKEN }}
        - name: Extract tag
          id: meta
          uses: docker/metadata-action@v5
          with:
            images: ghcr.io/${{ github.repository }}
            tags: |
              type=semver,pattern={{version}}
              type=semver,pattern={{major}}.{{minor}}
              type=raw,value=latest
        - uses: docker/build-push-action@v5
          with:
            context: .
            file: docker/Dockerfile
            platforms: linux/amd64,linux/arm64,linux/arm/v7
            push: true
            tags: ${{ steps.meta.outputs.tags }}

Note on cargo-audit in CI: check .cargo/audit.toml for existing CVE
suppressions — they must remain in effect or CI will fail on known-ignored
advisories. Verify cargo audit passes locally before pushing.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Commit the workflow files:
  git add .github/workflows/
  git commit -m "ci: add GitHub Actions CI pipeline and release workflow (Phase 19 Stage 1)"

Then push to a feature branch and open a draft PR to confirm the CI run
triggers. Observe the Actions tab — all three jobs should pass.
If cargo audit fails, check .cargo/audit.toml and add/update suppressions.
```

---

## STAGE 2 — Playwright E2E Critical Path Tests

**Priority: High**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read apps/web/playwright.config.ts and check for any existing E2E tests
under apps/web/e2e/.

Write Playwright E2E tests for the five critical user journeys and verify
they pass against a locally running xcalibre-server.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Playwright is already configured. The E2E job exists in CI but is marked
continue-on-error: true because no test files existed. These tests will
allow it to be promoted to a blocking CI job.

The backend test server uses:
  XCS_DB_URL=sqlite://test_e2e.db
  XCS_LLM_ENABLED=false
  XCS_STORAGE_PATH=./storage_e2e

─────────────────────────────────────────
PLAYWRIGHT CONFIG UPDATE
─────────────────────────────────────────

apps/web/playwright.config.ts — update/confirm webServer entries:

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
  use: {
    baseURL: 'http://localhost:5173',
  }

Add a globalSetup script (apps/web/e2e/global-setup.ts) that:
  1. Deletes test_e2e.db and storage_e2e/ before each run (clean state)
  2. Runs sqlx migrate run against test_e2e.db

─────────────────────────────────────────
TESTS TO IMPLEMENT
─────────────────────────────────────────

apps/web/e2e/critical-path.spec.ts:

  test('register and login', async ({ page }) => {
    // Navigate to /register
    // Fill username, email, password
    // Submit → redirected to /library (empty)
    // Assert library page is visible
    // Navigate to /login and log in again → assert back on /library
  })

  test('upload a book and see it in the library', async ({ page }) => {
    // Login as seeded test user
    // Navigate to /library
    // Click upload button
    // Upload a small test EPUB from e2e/fixtures/test.epub
    // Assert the book card appears in the library grid with title visible
    // Assert cover image loads (no broken img)
  })

  test('search returns results', async ({ page }) => {
    // After upload test (or create book via API in beforeEach)
    // Navigate to /search
    // Type a word that appears in the test EPUB
    // Assert at least one result card appears
    // Assert result has book title and snippet visible
  })

  test('open reader and navigate chapters', async ({ page }) => {
    // After upload, click the book card → book detail
    // Click "Read" button → reader page
    // Assert the EPUB content renders (not a blank iframe)
    // Click "next chapter" if visible → assert page changes
  })

  test('admin creates and revokes an API token', async ({ page }) => {
    // Login as admin
    // Navigate to /admin → API tokens section
    // Click "Create token"
    // Fill name, select scope "Read"
    // Assert token value is shown (one-time display)
    // Click revoke → token disappears from list
  })

  test('memory ingest via API', async ({ request }) => {
    // Use Playwright request fixture (direct HTTP, no browser)
    // POST to http://localhost:8083/api/v1/auth/login → get bearer token
    // POST to http://localhost:8083/api/v1/memory with bearer token
    //   body: { text: "Test memory chunk from E2E", chunk_type: "episodic" }
    // Assert 201 with id
    // GET http://localhost:8083/api/v1/search/chunks?q=Test+memory&source=memory
    // Assert the chunk appears in results
  })

Add apps/web/e2e/fixtures/test.epub — a minimal valid EPUB (use
https://github.com/IDPF/epub3-samples, pick the smallest file, or generate
a 3-chapter dummy EPUB programmatically in the fixture setup).

─────────────────────────────────────────
VISUAL INSPECTION
─────────────────────────────────────────

Run the E2E suite locally:
  pnpm --filter @xs/web exec playwright test --headed

Watch the tests execute in Chromium. Fix any failures before committing.
Screenshot artifacts are saved to apps/web/playwright-report/ on failure.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

pnpm --filter @xs/web exec playwright test
All 6 tests must pass. Kill the test servers.
Commit: "test: Playwright E2E critical path — register, upload, search, read, memory (Phase 19 Stage 2)"
```

---

## STAGE 3 — SECURITY.md Rewrite

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read docs/SECURITY.md (current content is calibre-web's policy — out of date).
Replace it entirely with an xcalibre-server-specific security policy.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

docs/SECURITY.md — full replacement:

  # Security Policy

  ## Supported Versions

  | Version | Supported |
  |---------|-----------|
  | Latest (main branch) | ✅ |
  | All prior releases | ❌ (self-hosted; update to latest) |

  xcalibre-server is self-hosted software with a single active release line.
  Security fixes ship in patch releases on the main branch. Older versions
  do not receive backported patches — operators should update.

  ## Reporting a Vulnerability

  **Do not open a public GitHub issue for security vulnerabilities.**

  Report via GitHub private security advisories:
  → https://github.com/<org>/xcalibre-server/security/advisories/new

  Include:
  - Description of the vulnerability and affected component
  - Steps to reproduce (minimal reproduction preferred)
  - Impact assessment (what can an attacker do?)
  - Your suggested fix (optional but appreciated)

  ## Response SLA

  | Severity | Acknowledge | Patch release |
  |----------|-------------|---------------|
  | Critical | 24 hours    | 7 days        |
  | High     | 48 hours    | 14 days       |
  | Medium   | 7 days      | 30 days       |
  | Low/Info | 14 days     | Next minor    |

  ## Scope

  In scope:
  - Authentication bypass, privilege escalation
  - Injection (SQL, command, SSRF, prompt injection in LLM paths)
  - Path traversal in file serving or storage backends
  - Sensitive data exposure (credentials, session tokens, PII)
  - Cryptographic weaknesses (weak algorithms, predictable tokens)

  Out of scope:
  - Vulnerabilities requiring physical access to the server
  - Social engineering of the server operator
  - Attacks requiring the attacker to already have admin access
  - Denial-of-service via resource exhaustion without authentication
  - Theoretical issues without demonstrated impact

  ## Known Design Decisions

  The following are intentional and not considered vulnerabilities:

  - `Content-Security-Policy` includes `'unsafe-inline'` on script-src and
    style-src to support epub.js rendering and shadcn/ui dynamic styles.
  - LLM endpoint URLs are operator-configured and trusted (not user-supplied).
    SSRF protection for these endpoints requires `allow_private_endpoints = true`
    for local model servers — this is an explicit opt-in by the operator.
  - The OPDS feed is unauthenticated by default. Operators who require auth
    on OPDS should enable `opds.require_auth = true` in config.toml.

  ## Dependency Audit

  `cargo audit` is run in CI on every commit. Known advisories that are
  suppressed for xcalibre-server are documented in `.cargo/audit.toml` with
  justification comments.

Replace the placeholder <org> with the actual GitHub org/user path once known.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Review the document for accuracy. Verify the GitHub advisory link format
is correct for the actual repo URL.
Commit: "docs: rewrite SECURITY.md with xcalibre-server-specific policy (Phase 19 Stage 3)"
```

---

## STAGE 4 — xs-migrate Test Coverage

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read xs-migrate/src/ in full to understand the CLI structure, import stages,
and what the current test coverage looks like.

Add integration tests for the xs-migrate Calibre import tool.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

xs-migrate is the Calibre library import CLI. It has four stages:
  1. Read Calibre metadata.db (SQLite)
  2. Map Calibre schema to xcalibre-server schema
  3. Upload books via POST /api/v1/books (multipart)
  4. Verify import (count checks, spot checks)

Currently there are no tests in xs-migrate/tests/. A bug in the import
path can silently destroy a user's library metadata. This is the highest-risk
untested path in the project.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

Create xs-migrate/tests/test_import.rs with:

  test_dry_run_reads_calibre_metadata_without_writing
    - Create a minimal Calibre metadata.db fixture in xs-migrate/tests/fixtures/
      (SQLite with calibre's schema — 3 books, 2 authors, 5 tags)
    - Run xs-migrate with --dry-run flag
    - Assert: exit code 0, no books written to xcalibre-server DB,
      output lists the 3 books that would be imported

  test_import_maps_calibre_fields_correctly
    - Use the fixture DB
    - Run against a test xcalibre-server instance (TestContext or a real
      running server — pick whichever pattern matches the project)
    - For each imported book, verify:
        - title matches
        - authors array is not empty
        - tags are imported
        - file path is recorded

  test_import_idempotent_second_run_does_not_duplicate
    - Import the fixture DB once → 3 books
    - Import the same fixture DB again
    - Assert: still 3 books (no duplicates)

  test_import_skips_missing_files_gracefully
    - Fixture DB references an EPUB path that does not exist on disk
    - Run import
    - Assert: import completes, missing-file book is skipped with a warning,
      other books are imported successfully

Create xs-migrate/tests/fixtures/metadata.db — minimal Calibre SQLite DB.
Use sqlx::SqlitePool or rusqlite to build it programmatically in a
#[fixture] or setup function rather than committing a binary blob.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

cargo test -p xs-migrate -- --nocapture
cargo clippy -p xs-migrate -- -D warnings
Commit: "test: xs-migrate import — dry run, field mapping, idempotency, missing files (Phase 19 Stage 4)"
```

---

## STAGE 5 — Frontend: API Token Scope Selector

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read apps/web/src/ — find the admin panel component that handles API token
creation (likely AdminPage.tsx or ApiTokensPanel.tsx or similar).
Read the existing token creation form and how it calls the API.

Add a scope radio group (Read / Read-Write / Admin) to the token creation form.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Phase 17 Stage 10 added scope enforcement to the backend (read | write | admin)
but the admin panel creates tokens silently defaulting to "write". An admin
has no way to create a read-only token for a home-automation integration
without modifying the API directly.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

Find the API token creation modal/form component. Add:

  A RadioGroup with three options:
    value="read"  label="Read"       description="Query books and metadata only"
    value="write" label="Read-Write" description="Full access to user resources"
    value="admin" label="Admin"      description="Admin panel access (admin users only)"

  - Default selection: "write" (matches existing API default)
  - "Admin" option is disabled with a tooltip if the current user is not an admin
  - On form submit, include scope in the request body:
      POST /api/v1/auth/tokens { name, expires_in_days?, scope }

Add a scope badge to the existing token list display:
  - "Read" → blue badge
  - "Read-Write" → green badge
  - "Admin" → red badge

Write a test in the component test file:
  - Render the token creation form as admin → all three options enabled
  - Render as non-admin → Admin option is disabled
  - Select "Read" and submit → API called with { scope: "read" }
  - Token list shows scope badges

─────────────────────────────────────────
VISUAL INSPECTION
─────────────────────────────────────────

After tests pass:
  pnpm --filter @xs/web dev &
  @Computer Use — open http://localhost:5173/admin in the browser
  Navigate to the API tokens section.
  Click "Create token" — verify scope radio group appears.
  Select "Read" → verify badge shows "Read" after creation.
  Kill the dev server.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

pnpm --filter @xs/web test run
cargo clippy -- -D warnings (no Rust changes expected)
Commit: "feat: API token scope selector in admin panel (Phase 19 Stage 5)"
```

---

## STAGE 6 — Open Items Cleanup and v2.0 Tag

**Priority: Low**
**Blocks: nothing. Blocked by: Stages 1–5.**
**Model: local**

**Paste this into Codex:**

```
Read docs/STATE.md (Open Items section), backend/src/config.rs
(allow_private_endpoints location), and apps/web/src/ for the stale
translation key.

Close out the remaining open items from STATE.md and prepare the v2.0 tag.

─────────────────────────────────────────
DELIVERABLE 1 — Translation key cleanup
─────────────────────────────────────────

Find the orphaned `book.unarchive` key in the locale files under
apps/web/src/i18n/ (or wherever translation JSON files live).

Check whether `book.unarchive` exists in the EN base file. If it does not:
  - If the key is used in code: add it to EN and all other locales (copy from
    `book.archive` as a starting point, change "Archive" to "Unarchive")
  - If the key is NOT used in code: delete it from all locale files

Run `pnpm run check:i18n` to confirm no orphaned or missing keys remain.

─────────────────────────────────────────
DELIVERABLE 2 — allow_private_endpoints namespace
─────────────────────────────────────────

Read backend/src/config.rs. The `allow_private_endpoints` field currently
lives under LlmSection but is also referenced by webhook SSRF validation.

Add a `[network]` top-level config section:

  #[derive(Clone, Debug, Default, Serialize, Deserialize)]
  #[serde(default)]
  pub struct NetworkSection {
      /// Allow LLM and webhook endpoints on private/loopback addresses.
      /// Set to true for local model servers (LM Studio, Ollama) or
      /// webhooks targeting internal services.
      pub allow_private_endpoints: bool,
  }

  Add to AppConfig: pub network: NetworkSection,

Update validation logic to prefer `config.network.allow_private_endpoints`
over `config.llm.allow_private_endpoints`, with backwards-compatible fallback:

  fn effective_allow_private(config: &AppConfig) -> bool {
      config.network.allow_private_endpoints || config.llm.allow_private_endpoints
  }

Update all callers to use `effective_allow_private(&config)`.

Update config.example.toml:
  [network]
  # allow_private_endpoints = false
  # Set to true to allow LLM endpoints and webhook targets on private/loopback
  # addresses. Required for local model servers (LM Studio, Ollama, etc.) or
  # internal webhook targets. Also configurable as llm.allow_private_endpoints
  # for backwards compatibility.

Update docs/DEPLOY.md — add a note about the new key in a "Configuration
Reference" or "Upgrading" section.

Run cargo test --workspace and cargo clippy -- -D warnings.

─────────────────────────────────────────
DELIVERABLE 3 — E2E CI promotion
─────────────────────────────────────────

Update .github/workflows/ci.yml — find the E2E job (from Stage 1 or existing)
and remove continue-on-error: true now that the E2E suite passes:

  e2e:
    name: E2E (Playwright)
    runs-on: ubuntu-latest
    needs: [rust, frontend]
    # continue-on-error: true  ← remove this line
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v3
        with:
          version: 9
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: pnpm
      - run: pnpm install --frozen-lockfile
      - name: Install Playwright browsers
        run: pnpm --filter @xs/web exec playwright install --with-deps chromium
      - name: Run E2E tests
        run: pnpm --filter @xs/web exec playwright test
        env:
          CI: true

─────────────────────────────────────────
DELIVERABLE 4 — CHANGELOG entry
─────────────────────────────────────────

Update docs/CHANGELOG.md — add a Phase 18 + Phase 19 entry:

  ## [2.0.0] — YYYY-MM-DD

  ### Added
  - Memory API: POST /api/v1/memory, DELETE /api/v1/memory/{id}
  - /search/chunks?source=memory|all for unified RAG retrieval
  - embedding_model config field — split embedding and chat model configuration
  - GitHub Actions CI pipeline (cargo test, clippy, audit, vitest, Docker build)
  - Playwright E2E test suite — register, upload, search, read, memory ingest
  - API token scope selector in admin panel (read/read-write/admin)
  - xs-migrate test coverage — dry run, field mapping, idempotency

  ### Changed
  - allow_private_endpoints promoted to [network] top-level config section
    (llm.allow_private_endpoints still works for backwards compatibility)

  ### Security
  - SECURITY.md rewritten with xcalibre-specific advisory process and SLA

─────────────────────────────────────────
DELIVERABLE 5 — STATE.md update
─────────────────────────────────────────

Update docs/STATE.md:
  - Header: "Phase 19 Complete"
  - Add Phase 19 row to completion table
  - Close all Open Items that are resolved by this phase
  - Update "Last verified" date

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

cargo test --workspace
cargo clippy -- -D warnings
pnpm --filter @xs/web test run
pnpm run check:i18n

Commit all changes:
  git commit -m "chore: Phase 19 cleanup — translation keys, allow_private_endpoints namespace, E2E CI ungate, CHANGELOG (Phase 19 Stage 6)"

Tag v2.0.0:
  git tag -a v2.0.0 -m "Phase 19 complete — CI/CD, E2E, SECURITY.md, xs-migrate tests, frontend scope UI"
```

---

## Post-Phase-19 Checklist

After all 6 stages are committed:

- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] `cargo audit` — zero CVEs
- [ ] `cargo test -p xs-migrate` — xs-migrate tests pass
- [ ] `pnpm --filter @xs/web test run` — all vitest tests pass
- [ ] `pnpm run check:i18n` — no orphaned or missing keys
- [ ] `pnpm --filter @xs/web exec playwright test` — all 6 E2E tests pass
- [ ] GitHub Actions CI passes on a pushed branch (all three jobs green)
- [ ] GitHub Actions release job dry-run: verify multi-arch build steps
- [ ] `docs/SECURITY.md` — no calibre-web references remain
- [ ] `docs/CHANGELOG.md` — v2.0.0 entry present
- [ ] `docs/STATE.md` — Phase 19 complete, all open items resolved or updated
- [ ] `config.example.toml` — `[network]` section present with `allow_private_endpoints`
- [ ] Tag `v2.0.0` locally and push tag

## Phase Summary

| Stage | Area | Priority |
|---|---|---|
| 1 | GitHub Actions CI pipeline + release workflow | 🔴 High |
| 2 | Playwright E2E — 6 critical path tests | 🔴 High |
| 3 | SECURITY.md rewrite | 🟠 Medium |
| 4 | xs-migrate test coverage | 🟠 Medium |
| 5 | Frontend: API token scope selector | 🟠 Medium |
| 6 | Open items cleanup + CHANGELOG + v2.0.0 tag | 🟢 Low |
