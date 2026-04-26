# Codex Desktop App — xcalibre-server Phase 12: Post-v1.0 Polish

## What Phase 12 Builds

Eight post-v1.0 improvements spanning test coverage, ops, i18n, and backend quality:

- **Stage 1** — Playwright E2E test suite (reader, search, auth, admin golden paths)
- **Stage 2** — Deployment runbooks (single-instance Docker, HA MariaDB, backup/restore, S3 migration)
- **Stage 3** — FR/DE/ES translation completion + CI coverage check
- **Stage 4** — S3 range request support (audio + large PDF streaming restored on S3 backend)
- **Stage 5** — Structured JSON logging + `/health` endpoint (DB + Meilisearch checks)
- **Stage 6** — Rate-limit response headers (X-RateLimit-*, Retry-After on 429)
- **Stage 7** — WebP cover conversion with JPEG fallback (content negotiation)
- **Stage 8** — Global tag management — rename, merge, delete (admin UI + API)

## Key Design Decisions

**E2E Tests (Playwright):**
- Playwright runs against the web SPA only — mobile E2E (Detox) is not in scope for this phase
- Tests run against a real backend started in test mode (SQLite, empty DB, seeded via API calls in test setup)
- Test isolation: each spec file logs in as a fresh user created in `beforeAll` via `POST /auth/register`
- Fixtures: a real EPUB (public domain) checked in at `apps/web/e2e/fixtures/test.epub` for reader tests
- CI: non-blocking initially (failures do not block merge); promote to blocking after 2 clean runs
- Playwright config uses `baseURL` from `PLAYWRIGHT_BASE_URL` env (defaults to `http://localhost:5173`)

**Deployment Runbooks:**
- Two tiers documented: single-instance (SQLite, one Docker container — the common case) and multi-instance (MariaDB, Docker Swarm or Compose replicas)
- `docs/DEPLOY.md` is the primary artifact; `scripts/backup.sh` and `scripts/restore.sh` are companion scripts
- Caddy is the recommended reverse proxy (already in `docker/Caddyfile`); nginx alternative included
- Runbooks do not prescribe cloud providers — all steps are provider-agnostic

**Translation Completion:**
- EN base is the source of truth; FR/DE/ES must have 100% key coverage
- A `scripts/check-translations.ts` script diffs each locale against EN and exits non-zero if any key is missing
- CI job (`i18n-check.yml`) runs the script on every push; fails the PR if coverage drops below 100%
- Machine-translated completions are acceptable as a starting point — flag them with a `# TODO: human review` comment in the JSON (JSON does not have comments; use a parallel `*_todo` key convention — see Stage 3 detail)

**S3 Range Request Support:**
- S3 GetObject natively supports the `Range` header (RFC 7233 `bytes=start-end`)
- The fix: extract the `Range` header from the incoming Axum request and pass it directly to the S3 GetObject call
- S3 returns a 206 Partial Content response with `Content-Range`; proxy that back to the client unchanged
- This restores: epub.js chunk loading, react-pdf page streaming, HTML5 audio seeking, expo-av seeking
- `StorageBackend::get_bytes` is extended to accept an optional byte range; LocalFsStorage implements it with `tokio::fs::File` + `seek`
- The code comment documenting range degradation is removed once this stage is complete

## Key Schema Facts (no new tables this phase)

No DB migrations in Phase 12. All changes are frontend, documentation, config, and backend serving logic only.

## Reference Files

Read before starting each stage:
- `docs/ARCHITECTURE.md` — design constraints
- `apps/web/src/features/auth/LoginPage.tsx` — login flow (Stage 1 auth tests)
- `apps/web/src/features/library/LibraryPage.tsx` — grid (Stage 1 library tests)
- `apps/web/src/features/reader/` — reader implementations (Stage 1 reader tests)
- `apps/web/src/features/search/SearchPage.tsx` — search (Stage 1 search tests)
- `apps/web/src/locales/en.json` — EN base (Stage 3 translation audit)
- `apps/web/src/locales/fr.json`, `de.json`, `es.json` — current starter translations (Stage 3)
- `backend/src/storage.rs` — StorageBackend trait (Stage 4)
- `backend/src/storage_s3.rs` — S3Storage impl (Stage 4)
- `backend/src/api/books.rs` — file-serving handlers (Stage 4)
- `docker/docker-compose.yml`, `docker/Caddyfile` — deployment baseline (Stage 2)

---

## STAGE 1 — Playwright E2E Test Suite

**Priority: High (frontend test coverage is the largest quality gap)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read apps/web/src/features/auth/LoginPage.tsx,
apps/web/src/features/library/LibraryPage.tsx,
apps/web/src/features/reader/,
apps/web/src/features/search/SearchPage.tsx,
apps/web/src/features/admin/,
and apps/web/package.json.
Now add a Playwright E2E test suite for the xcalibre-server web app.

─────────────────────────────────────────
SETUP
─────────────────────────────────────────

apps/web/package.json — add to devDependencies:
  "@playwright/test": "^1.44.0"

Add scripts:
  "test:e2e":         "playwright test"
  "test:e2e:headed":  "playwright test --headed"
  "test:e2e:ui":      "playwright test --ui"

apps/web/playwright.config.ts — new file:

  import { defineConfig, devices } from "@playwright/test";

  export default defineConfig({
    testDir: "./e2e",
    fullyParallel: false,       // sequential — tests share a backend instance
    retries: process.env.CI ? 1 : 0,
    timeout: 30_000,
    expect: { timeout: 10_000 },
    use: {
      baseURL: process.env.PLAYWRIGHT_BASE_URL ?? "http://localhost:5173",
      trace: "on-first-retry",
      screenshot: "only-on-failure",
    },
    projects: [
      { name: "chromium", use: { ...devices["Desktop Chrome"] } },
    ],
    webServer: {
      command: "pnpm dev",
      url: "http://localhost:5173",
      reuseExistingServer: !process.env.CI,
      timeout: 60_000,
    },
  });

apps/web/e2e/helpers/auth.ts — test helpers:

  import { Page, request } from "@playwright/test";

  const API = process.env.PLAYWRIGHT_API_URL ?? "http://localhost:3000";

  export async function createUser(username: string, password: string) {
    const ctx = await request.newContext();
    await ctx.post(`${API}/api/v1/auth/register`, {
      data: { username, password, email: `${username}@test.local` },
    });
    await ctx.dispose();
  }

  export async function login(page: Page, username: string, password: string) {
    await page.goto("/login");
    await page.getByLabel("Username").fill(username);
    await page.getByLabel("Password").fill(password);
    await page.getByRole("button", { name: "Sign in" }).click();
    await page.waitForURL("**/library");
  }

  export async function loginAsAdmin(page: Page) {
    // Assumes default admin credentials from config.toml test seed
    await login(page, "admin", process.env.E2E_ADMIN_PASSWORD ?? "testpassword");
  }

apps/web/e2e/fixtures/test.epub — check in a small public-domain EPUB.
  Use "The Yellow Wallpaper" by Charlotte Perkins Gilman (Project Gutenberg, public domain).
  Keep under 200KB — no large fixtures in the repo.

─────────────────────────────────────────
DELIVERABLE 1 — Auth Tests
─────────────────────────────────────────

apps/web/e2e/auth.spec.ts:

  test("login with valid credentials navigates to library")
    - goto /login
    - fill username + password
    - click Sign in
    - assert URL is /library

  test("login with wrong password shows error")
    - goto /login
    - fill correct username + wrong password
    - click Sign in
    - assert page contains "Invalid username or password" (do not assert URL change)

  test("logout clears session and redirects to login")
    - login as admin
    - open user avatar menu
    - click Sign out
    - assert URL is /login
    - reload page
    - assert still on /login (tokens cleared)

  test("unauthenticated access to library redirects to login")
    - clear storage (page.evaluate(() => localStorage.clear()))
    - goto /library
    - assert URL is /login

─────────────────────────────────────────
DELIVERABLE 2 — Library Tests
─────────────────────────────────────────

apps/web/e2e/library.spec.ts:

  beforeAll: login as admin. Upload test.epub via POST /api/v1/books (multipart).
  beforeEach: page.goto("/library")

  test("library grid renders at least one book card")
    - assert page has role="img" with name matching uploaded book title
      (or a CoverPlaceholder with first letter)

  test("filter chip opens filter panel and filters results")
    - click "Format" filter chip
    - select EPUB
    - click Apply
    - assert URL contains format=epub (or results visibly update)

  test("sort dropdown changes book order")
    - click sort dropdown
    - select "Date added"
    - assert no JS errors in console (page.on("console") check)

  test("grid/list toggle switches to list view")
    - click list toggle button
    - assert a <table> or list container is visible (not the card grid)

  test("clicking a book card navigates to book detail")
    - click first book card body (not the Read button)
    - assert URL matches /books/:id

─────────────────────────────────────────
DELIVERABLE 3 — Reader Tests
─────────────────────────────────────────

apps/web/e2e/reader.spec.ts:

  beforeAll: login as admin. Upload test.epub.
  beforeEach: navigate to the uploaded book's detail page; click Read.

  test("EPUB reader opens and displays content")
    - assert page has no visible error
    - assert an iframe or epub.js container is visible
    - assert URL contains /reader/

  test("reader toolbar fades in on mouse move")
    - move mouse to center of page (page.mouse.move)
    - assert a toolbar or progress element becomes visible
    - wait 4 seconds
    - assert toolbar is no longer visible (opacity: 0 or hidden)

  test("reading progress is saved and shown on return to library")
    - open reader, trigger a page turn (keyboard ArrowRight or click right edge)
    - navigate back to library
    - assert a progress bar (teal bar at bottom of card cover) is visible on the book card

  test("reader settings panel opens on gear icon click")
    - move mouse to trigger toolbar
    - click the settings (gear) icon
    - assert a settings panel is visible containing font size or theme controls

  test("TOC panel opens on menu icon click")
    - move mouse to trigger toolbar
    - click the TOC (hamburger) icon
    - assert a chapter list panel is visible

─────────────────────────────────────────
DELIVERABLE 4 — Search Tests
─────────────────────────────────────────

apps/web/e2e/search.spec.ts:

  beforeAll: login as admin. Upload test.epub (title: "The Yellow Wallpaper").

  test("FTS search returns results matching query")
    - click search input in top bar
    - type "Yellow Wallpaper"
    - wait for dropdown results or navigate to search page
    - assert a result containing "Yellow Wallpaper" is visible

  test("empty search shows no results state")
    - goto /search
    - type "xyzzy_no_match_12345" in search input
    - wait 600ms (debounce)
    - assert "No results" message is visible

  test("semantic tab is grayed when LLM is disabled")
    - goto /search
    - assert the "AI Semantic" tab has aria-disabled="true" or a reduced opacity class
    - click the grayed tab
    - assert a tooltip or alert message references AI features being disabled

  test("clicking a search result navigates to book detail")
    - search "Yellow Wallpaper"
    - click first result card
    - assert URL matches /books/:id

─────────────────────────────────────────
DELIVERABLE 5 — Admin Tests
─────────────────────────────────────────

apps/web/e2e/admin.spec.ts:

  beforeAll: loginAsAdmin(page)

  test("admin panel is accessible from user avatar menu")
    - click user avatar in top bar
    - assert "Admin Panel" link is visible
    - click "Admin Panel"
    - assert URL is /admin or /admin/dashboard

  test("users table lists at least the admin user")
    - goto /admin/users
    - assert a row containing "admin" is visible

  test("create user inline and verify it appears in table")
    - goto /admin/users
    - click "Add user" or equivalent button
    - fill in username "e2e-test-user" + password + role User
    - submit
    - assert a row containing "e2e-test-user" appears in the table

  test("delete user removes them from the table")
    - (continue from create user test or re-create)
    - click Delete on "e2e-test-user" row
    - confirm the destructive dialog
    - assert "e2e-test-user" is no longer in the table

  test("import page renders with drag-drop zone and dry run toggle")
    - goto /admin/import
    - assert a file drop zone is visible
    - assert a "Dry run" toggle is visible

─────────────────────────────────────────
CI WORKFLOW
─────────────────────────────────────────

.github/workflows/e2e.yml:

  name: E2E Tests
  on:
    push:
      branches: [main]
    pull_request:

  jobs:
    e2e:
      runs-on: ubuntu-latest
      continue-on-error: true   # non-blocking initially
      steps:
        - uses: actions/checkout@v4
        - uses: actions/setup-node@v4
          with: { node-version: "20" }
        - run: pnpm install
        - run: pnpm --filter @xs/backend build   # or cargo build --release
        - run: npx playwright install --with-deps chromium
        - name: Start backend (test mode)
          run: |
            XCS_ENV=test ./target/release/xcalibre-server &
            echo "BACKEND_PID=$!" >> $GITHUB_ENV
          env:
            DATABASE_URL: sqlite::memory:
            JWT_SECRET: ci-test-secret-do-not-use
        - name: Run E2E tests
          run: pnpm --filter @xs/web test:e2e
          env:
            PLAYWRIGHT_BASE_URL: http://localhost:5173
            PLAYWRIGHT_API_URL: http://localhost:3000
        - uses: actions/upload-artifact@v4
          if: failure()
          with:
            name: playwright-report
            path: apps/web/playwright-report/

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cd apps/web && npx playwright install chromium
pnpm --filter @xs/web test:e2e
git add apps/web/e2e/ apps/web/playwright.config.ts apps/web/package.json .github/workflows/e2e.yml
git commit -m "Phase 12 Stage 1: Playwright E2E suite (auth, library, reader, search, admin)"
```

---

## STAGE 2 — Deployment Runbooks

**Priority: High (missing docs are a barrier to adoption)**
**Blocks: nothing. Blocked by: nothing.**
**Model: claude-opus-4-7**

**Paste this into Codex:**

```
Read docker/docker-compose.yml, docker/docker-compose.production.yml, docker/Caddyfile,
docs/ARCHITECTURE.md (the storage backend and security sections),
backend/src/config.rs, and config.example.toml.
Now write comprehensive deployment runbooks for xcalibre-server.

─────────────────────────────────────────
DELIVERABLE 1 — docs/DEPLOY.md
─────────────────────────────────────────

Create docs/DEPLOY.md. Write in a direct, instructional style — commands must be
copy-pasteable. Do not pad with marketing language or motivational headings.

Contents:

  # Deploying xcalibre-server

  ## Prerequisites
  - Docker 24+ and Docker Compose v2
  - A domain name pointing at your server (for TLS)
  - 1GB RAM minimum; 4GB recommended for Meilisearch

  ## Tier 1: Single Instance (SQLite) — Recommended for < 5 users

  This is the standard self-hosted deployment. One container, SQLite DB, local
  filesystem or S3 for book files. Easiest to operate.

  ### Quick start

    Step-by-step: clone the repo, copy config.example.toml, set at minimum:
      - jwt_secret (random 32+ char string — provide generation command)
      - admin_password
      - library_name
      - storage_path
    Then: docker compose up -d

    Include the exact docker-compose.yml command and expected output.

  ### Caddy reverse proxy (TLS)

    Provide a complete working Caddyfile for a single domain with automatic
    HTTPS via Let's Encrypt. Include:
      - Reverse proxy to the xcalibre-server container
      - Static file serving bypass for /covers/ (performance optimization)
      - Compression enabled

    Nginx alternative: provide an equivalent nginx.conf snippet.

  ### Enabling Meilisearch (optional)

    The app works without Meilisearch (falls back to SQLite FTS5).
    To enable: uncomment the Meilisearch service in docker-compose.yml,
    set meilisearch.enabled = true in config.toml.
    Include the environment variables for the Meilisearch master key.

  ### Enabling S3 storage (optional)

    Provide exact config.toml snippet for [storage] and [storage.s3].
    Include endpoint_url examples for MinIO, Cloudflare R2, Backblaze B2.

    One-time migration from local filesystem to S3:
      1. Stop the server
      2. aws s3 sync {storage_path}/ s3://{bucket}/ --delete
      3. Update config.toml: backend = "s3"
      4. Restart the server
      5. Verify a book download works

  ## Tier 2: Multi-Instance (MariaDB) — For multi-user or HA setups

  When to use this: more than ~20 concurrent users, or when you need zero-downtime
  deploys (multiple app replicas with a shared DB).

  ### MariaDB setup

    Provide Docker Compose snippet adding a MariaDB service.
    Database init SQL: CREATE DATABASE xcalibre-server; GRANT ALL ON xcalibre-server.* TO 'xcalibre-server'@'%';
    Config change: database_url = "mysql://xcalibre-server:password@mariadb:3306/xcalibre-server"

    Connection pool tuning: provide recommended max_connections value.

  ### Multiple app replicas

    Docker Compose deploy replicas: 2 snippet.
    Note: all replicas must share:
      - The same config.toml (JWT secret must match across replicas)
      - The same storage backend (use S3 — local filesystem does not work with multiple replicas)
    Include a warning: SQLite cannot be used with multiple replicas.

  ### Health check endpoint

    GET /health returns 200 {"status":"ok"} — use this for load balancer health checks.
    Provide Caddy upstream health check snippet.

  ## Backup and Restore

  ### SQLite backup
    The recommended approach: SQLite online backup via the .backup command.
    Provide the exact command:
      sqlite3 library.db ".backup backup-$(date +%Y%m%d-%H%M%S).db"
    
    Automate with a cron job (provide crontab line).
    Include: book files (storage_path) must also be backed up — rsync command example.

  ### MariaDB backup
    mysqldump command with --single-transaction flag.
    Include compression: | gzip > xcalibre-server-$(date +%Y%m%d).sql.gz

  ### Restore procedure
    Step-by-step restore for both SQLite and MariaDB.
    Include: stop server → restore DB → restore book files → start server → verify.

  ### S3 backup (book files)
    If using S3, book files are durable by design.
    Recommend enabling S3 versioning on the bucket.
    Include: aws s3api put-bucket-versioning command.

  ## Upgrade Procedure

    1. Pull new image: docker compose pull
    2. Check CHANGELOG.md for migration notes
    3. Migrations run automatically on startup — no manual step required
    4. Restart: docker compose up -d
    5. Verify: docker compose logs -f | head -50

    Include: downgrade is not supported — always back up before upgrading.

  ## Troubleshooting

  | Symptom | Likely Cause | Fix |
  |---|---|---|
  | "Database locked" errors | SQLite WAL file not cleaned up | Restart the container; check for stuck processes |
  | Covers not loading | storage_path not mounted in Docker | Add volume mount in docker-compose.yml |
  | OPDS feeds returning 401 | OPDS auth not configured | Check opds.require_auth in config.toml |
  | Meilisearch not indexing | MEILI_MASTER_KEY mismatch | Must match between app config and Meilisearch container |
  | LDAP auth failing | LDAP server unreachable | App falls back to local auth; check ldap.host and network |
  | S3 uploads failing | Credentials or bucket policy | Check access_key, secret_key, and bucket IAM policy |

─────────────────────────────────────────
DELIVERABLE 2 — scripts/backup.sh
─────────────────────────────────────────

Create scripts/backup.sh — a production-ready backup script:

  #!/usr/bin/env bash
  set -euo pipefail

  Usage:
    ./scripts/backup.sh [--db-only] [--files-only]

  Default (no flags): back up both DB and book files.

  Script reads XCS_BACKUP_DIR, XCS_DB_PATH, XCS_STORAGE_PATH
  from environment (or defaults from config.toml if readable).

  SQLite mode:
    sqlite3 "$DB_PATH" ".backup ${BACKUP_DIR}/db-$(date +%Y%m%d-%H%M%S).db"

  MariaDB mode (detected if DATABASE_URL starts with "mysql://"):
    Parse host/user/pass/db from DATABASE_URL
    Run mysqldump --single-transaction | gzip > ${BACKUP_DIR}/db-$(date +%Y%m%d-%H%M%S).sql.gz

  Book files:
    rsync -a --delete "${STORAGE_PATH}/" "${BACKUP_DIR}/files/"

  Output: print summary of what was backed up and where.
  On error: print clear message; exit non-zero.

─────────────────────────────────────────
DELIVERABLE 3 — scripts/restore.sh
─────────────────────────────────────────

Create scripts/restore.sh:

  Usage:
    ./scripts/restore.sh <backup-db-file> [--files-dir <path>]

  Steps:
    1. Confirm the target DB file does not exist (or prompt to overwrite)
    2. SQLite: cp <backup.db> <target.db>
       MariaDB: gunzip < <backup.sql.gz> | mysql xcalibre-server
    3. If --files-dir provided: rsync -a <path>/ <STORAGE_PATH>/
    4. Print: "Restore complete. Start the server with: docker compose up -d"

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
bash scripts/backup.sh --help   # check help text prints cleanly
git add docs/DEPLOY.md scripts/backup.sh scripts/restore.sh
git commit -m "Phase 12 Stage 2: deployment runbooks, backup/restore scripts"
```

---

## STAGE 3 — FR/DE/ES Translation Completion

**Priority: Medium**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read apps/web/src/locales/en.json,
apps/web/src/locales/fr.json,
apps/web/src/locales/de.json,
and apps/web/src/locales/es.json.
Now complete the FR/DE/ES translations and add a CI coverage check.

─────────────────────────────────────────
DELIVERABLE 1 — Complete FR/DE/ES locale files
─────────────────────────────────────────

For each of fr.json, de.json, es.json:

  1. Collect every key that exists in en.json but is missing from the locale file.
  2. Provide machine-translated values for every missing key.
     Quality bar: idiomatic, not word-for-word. Library/book management context.
     For UI strings (button labels, placeholders) prefer concise phrasing.
  3. Do NOT change keys that already have translations — only add missing ones.
  4. Preserve the exact JSON structure and key nesting of en.json.
  5. Keep the same key order as en.json for easier diffing in future.

  Result: fr.json, de.json, es.json each have 100% key coverage vs en.json.

─────────────────────────────────────────
DELIVERABLE 2 — Translation coverage checker
─────────────────────────────────────────

scripts/check-translations.ts — new file (runs with tsx or ts-node):

  #!/usr/bin/env tsx
  import en from "../apps/web/src/locales/en.json";
  import fr from "../apps/web/src/locales/fr.json";
  import de from "../apps/web/src/locales/de.json";
  import es from "../apps/web/src/locales/es.json";

  // Recursively collect all dot-notation key paths from a nested object
  function collectKeys(obj: Record<string, unknown>, prefix = ""): string[] {
    return Object.entries(obj).flatMap(([k, v]) => {
      const path = prefix ? `${prefix}.${k}` : k;
      return typeof v === "object" && v !== null
        ? collectKeys(v as Record<string, unknown>, path)
        : [path];
    });
  }

  const enKeys = new Set(collectKeys(en));
  const locales: [string, typeof fr][] = [["fr", fr], ["de", de], ["es", es]];
  let exitCode = 0;

  for (const [code, locale] of locales) {
    const localeKeys = new Set(collectKeys(locale));
    const missing = [...enKeys].filter((k) => !localeKeys.has(k));
    const extra   = [...localeKeys].filter((k) => !enKeys.has(k));

    if (missing.length > 0) {
      console.error(`[${code}] Missing ${missing.length} key(s):`);
      missing.forEach((k) => console.error(`  - ${k}`));
      exitCode = 1;
    }
    if (extra.length > 0) {
      console.warn(`[${code}] ${extra.length} key(s) not in EN (orphaned):`);
      extra.forEach((k) => console.warn(`  + ${k}`));
    }
    if (missing.length === 0 && extra.length === 0) {
      console.log(`[${code}] ✓ 100% coverage`);
    }
  }
  process.exit(exitCode);

Add to root package.json scripts:
  "check:i18n": "tsx scripts/check-translations.ts"

─────────────────────────────────────────
DELIVERABLE 3 — CI workflow
─────────────────────────────────────────

.github/workflows/i18n-check.yml:

  name: i18n Coverage
  on:
    push:
      paths:
        - "apps/web/src/locales/**"
        - "scripts/check-translations.ts"
    pull_request:
      paths:
        - "apps/web/src/locales/**"

  jobs:
    i18n:
      runs-on: ubuntu-latest
      steps:
        - uses: actions/checkout@v4
        - uses: actions/setup-node@v4
          with: { node-version: "20" }
        - run: npm install -g tsx
        - run: tsx scripts/check-translations.ts

  This workflow is BLOCKING — a PR that drops translation coverage below 100%
  will fail CI.

─────────────────────────────────────────
DELIVERABLE 4 — Contributing guide (translation section)
─────────────────────────────────────────

docs/CONTRIBUTING.md — add (or create if absent) a "Translations" section:

  ## Adding or Improving Translations

  ### Adding a new locale
  1. Copy `apps/web/src/locales/en.json` to `apps/web/src/locales/{code}.json`
  2. Translate all values (keep all keys unchanged)
  3. Register the locale in `apps/web/src/i18n.ts` (add to the `resources` map)
  4. Add the locale to the picker options in `apps/web/src/features/profile/ProfilePage.tsx`
  5. Run `pnpm check:i18n` to verify 100% coverage
  6. Open a PR with the new locale file

  ### Fixing an existing translation
  1. Edit the value in `apps/web/src/locales/{code}.json`
  2. Run `pnpm check:i18n` to verify no keys were accidentally removed
  3. Open a PR

  The CI `i18n-check` workflow enforces that every locale has 100% key coverage
  vs the EN base at all times. Orphaned keys (in a locale but not in EN) are
  reported as warnings but do not fail CI.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
pnpm check:i18n   # must print ✓ 100% coverage for fr, de, es
pnpm --filter @xs/web build   # no TS errors
git add apps/web/src/locales/ scripts/check-translations.ts .github/workflows/i18n-check.yml docs/CONTRIBUTING.md
git commit -m "Phase 12 Stage 3: complete FR/DE/ES translations, add i18n CI coverage check"
```

---

## STAGE 4 — S3 Range Request Support

**Priority: Medium (restores audio + large PDF streaming on S3 backend)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/storage.rs, backend/src/storage_s3.rs, backend/src/api/books.rs,
backend/tests/test_storage_s3.rs, and backend/Cargo.toml.
Now add range request support to the S3 storage backend.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

The current S3 serving path (in books.rs) calls get_bytes() and returns the
full file body. This means:
  - epub.js chunk requests get the entire file instead of the requested range
  - expo-av audio seeking downloads the full file before playing
  - Large PDF page streaming stalls on initial load

S3's GetObject API natively supports the HTTP Range header (RFC 7233):
  .range("bytes=0-65535")  → returns 206 Partial Content with Content-Range header

The fix: extend get_bytes to accept an optional byte range, pass it to S3 GetObject,
and proxy the S3 206 response back to the client.

LocalFsStorage can also support this via tokio::fs::File + seek + take, avoiding
a regression from any callers that now use get_bytes instead of ServeFile.

─────────────────────────────────────────
TRAIT CHANGE
─────────────────────────────────────────

backend/src/storage.rs — extend StorageBackend:

  /// Range is inclusive on both ends: Some((0, 65535)) means "bytes 0–65535".
  /// None means "return the entire file."
  async fn get_range(
    &self,
    relative_path: &str,
    range: Option<(u64, u64)>,
  ) -> anyhow::Result<GetRangeResult>;

  pub struct GetRangeResult {
    pub bytes: Bytes,
    pub content_range: Option<String>,  // e.g. "bytes 0-65535/1048576"
    pub total_length: u64,
    pub partial: bool,                  // true = 206, false = 200
  }

Keep get_bytes as a convenience wrapper:
  async fn get_bytes(&self, relative_path: &str) -> anyhow::Result<Bytes> {
    Ok(self.get_range(relative_path, None).await?.bytes)
  }
  -- default impl so existing callers don't need to change

─────────────────────────────────────────
LOCALFSSTORAGE IMPLEMENTATION
─────────────────────────────────────────

backend/src/storage.rs — implement get_range for LocalFsStorage:

  async fn get_range(
    &self,
    relative_path: &str,
    range: Option<(u64, u64)>,
  ) -> anyhow::Result<GetRangeResult> {
    let path = self.resolve(relative_path)?;
    let mut file = tokio::fs::File::open(&path).await?;
    let total_length = file.metadata().await?.len();

    match range {
      None => {
        let bytes = tokio::fs::read(&path).await?;
        Ok(GetRangeResult {
          bytes: Bytes::from(bytes),
          content_range: None,
          total_length,
          partial: false,
        })
      }
      Some((start, end)) => {
        use tokio::io::{AsyncReadExt, AsyncSeekExt};
        let end = end.min(total_length.saturating_sub(1));
        let len = (end - start + 1) as usize;
        file.seek(std::io::SeekFrom::Start(start)).await?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf).await?;
        Ok(GetRangeResult {
          bytes: Bytes::from(buf),
          content_range: Some(format!("bytes {start}-{end}/{total_length}")),
          total_length,
          partial: true,
        })
      }
    }
  }

─────────────────────────────────────────
S3STORAGE IMPLEMENTATION
─────────────────────────────────────────

backend/src/storage_s3.rs — implement get_range for S3Storage:

  async fn get_range(
    &self,
    relative_path: &str,
    range: Option<(u64, u64)>,
  ) -> anyhow::Result<GetRangeResult> {
    let key = self.s3_key(relative_path);
    let mut req = self.client
      .get_object()
      .bucket(&self.bucket)
      .key(&key);

    if let Some((start, end)) = range {
      req = req.range(format!("bytes={start}-{end}"));
    }

    let resp = req.send().await
      .with_context(|| format!("S3 GetObject {key}"))?;

    let total_length = resp.content_length().unwrap_or(0) as u64;
    let content_range = resp.content_range().map(|s| s.to_string());
    let partial = content_range.is_some();
    let bytes = resp.body.collect().await?.into_bytes();

    Ok(GetRangeResult {
      bytes,
      content_range,
      total_length,
      partial,
    })
  }

─────────────────────────────────────────
UPDATING HANDLERS
─────────────────────────────────────────

backend/src/api/books.rs — update download_format, stream_format, and
  cover-serving handlers:

  1. Extract the Range header from the Axum request:
     let range_header = headers
       .get(axum::http::header::RANGE)
       .and_then(|v| v.to_str().ok())
       .map(|s| s.to_string());

  2. Parse the Range header into Option<(u64, u64)>:

     fn parse_range(range_str: &str, _total: u64) -> Option<(u64, u64)> {
       // Parse "bytes=start-end" — handle open-ended ranges: "bytes=0-" → (0, u64::MAX)
       let s = range_str.strip_prefix("bytes=")?.trim();
       let (start, end) = s.split_once('-')?;
       let start: u64 = start.trim().parse().ok()?;
       let end: u64 = if end.trim().is_empty() {
         u64::MAX     // open-ended; S3 and LocalFs will clamp to file size
       } else {
         end.trim().parse().ok()?
       };
       Some((start, end))
     }

  3. Replace the current runtime dispatch block with:

     let range = range_header.as_deref().and_then(|s| parse_range(s, 0));

     let path_result = state.storage.resolve(&format_file.path);
     match path_result {
       Ok(local_path) if range.is_none() => {
         // LocalFs, no range — use ServeFile (most efficient for full-file serving)
         ServeFile::new(&local_path).call(req).await
           .map_err(|e| AppError::Internal(e.to_string()))
       }
       _ => {
         // LocalFs with range, or S3 (any range) — use get_range
         let result = state.storage.get_range(&format_file.path, range).await
           .map_err(|e| AppError::Internal(e.to_string()))?;

         let status = if result.partial {
           axum::http::StatusCode::PARTIAL_CONTENT   // 206
         } else {
           axum::http::StatusCode::OK                // 200
         };

         let mut builder = Response::builder()
           .status(status)
           .header("Content-Type", &mime_type)
           .header("Content-Length", result.bytes.len().to_string())
           .header("Accept-Ranges", "bytes");

         if let Some(cr) = &result.content_range {
           builder = builder.header("Content-Range", cr);
         }

         builder
           .body(axum::body::Body::from(result.bytes))
           .map_err(|e| AppError::Internal(e.to_string()))
       }
     }

  4. Remove the "S3 serving does not support range requests" code comment added
     in Phase 11 Stage 3 — it is no longer accurate.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_storage_s3.rs — add to existing test file:

  test_local_storage_get_range_returns_partial_bytes
    - put a file with 1024 bytes of known content
    - get_range("file", Some((0, 511)))
    - assert bytes.len() == 512
    - assert content_range == "bytes 0-511/1024"
    - assert partial == true

  test_local_storage_get_range_open_end
    - put a file with 100 bytes
    - get_range("file", Some((50, u64::MAX)))
    - assert bytes.len() == 50
    - assert partial == true

  test_local_storage_get_range_none_returns_full
    - put a file with known content
    - get_range("file", None)
    - assert partial == false
    - assert content_range == None

  test_s3_get_range_passes_range_header (requires real S3 — #[ignore])
    - put a 4096-byte file to S3
    - get_range("file", Some((0, 1023)))
    - assert bytes.len() == 1024
    - assert partial == true

backend/tests/test_file_serving.rs — add:

  test_download_returns_206_for_range_request
    - upload a book with a known format
    - GET /api/v1/books/:id/formats/epub/download with Range: bytes=0-1023
    - assert status 206
    - assert response has Content-Range header
    - assert response body length == 1024

  test_stream_returns_206_for_range_request
    - same as above but for /stream endpoint

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Manual: open a large PDF in the reader and confirm page-by-page loading is
# not downloading the full file (check Network tab in browser DevTools for 206 responses)
git add backend/
git commit -m "Phase 12 Stage 4: S3 range request support — audio + PDF streaming restored"
```

---

## STAGE 5 — Structured JSON Logging + `/health` Endpoint

**Priority: High (ops table-stakes)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/lib.rs, backend/Cargo.toml, backend/src/api/admin.rs,
backend/src/db/queries/mod.rs, and backend/src/api/search.rs.
Now add structured JSON logging and a proper health check endpoint.

─────────────────────────────────────────
DELIVERABLE 1 — JSON logging
─────────────────────────────────────────

backend/Cargo.toml — update tracing-subscriber:
  tracing-subscriber = { version = "0.3", features = ["env-filter", "fmt", "json"] }

backend/src/lib.rs — replace the current tracing_subscriber::fmt() init:

  let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string());
  match log_format.as_str() {
    "text" => {
      tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    }
    _ => {
      tracing_subscriber::fmt()
        .json()
        .with_env_filter(EnvFilter::from_default_env())
        .with_current_span(true)
        .with_span_list(false)
        .init();
    }
  }

  Default is JSON. Set LOG_FORMAT=text for local development readability.
  Document this in config.example.toml as a comment: # LOG_FORMAT=text for human-readable output

All existing tracing::info!/warn!/error! call sites are unchanged — only the
subscriber format changes.

─────────────────────────────────────────
DELIVERABLE 2 — GET /health endpoint
─────────────────────────────────────────

backend/src/api/mod.rs — add health route (no auth required):
  router.route("/health", get(health_handler))

backend/src/api/health.rs — new file:

  use axum::Json;
  use serde::Serialize;
  use crate::AppState;

  #[derive(Serialize)]
  pub struct HealthResponse {
    status: &'static str,
    version: &'static str,
    db: ComponentStatus,
    search: ComponentStatus,
  }

  #[derive(Serialize)]
  pub struct ComponentStatus {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
  }

  pub async fn health_handler(
    State(state): State<Arc<AppState>>,
  ) -> (StatusCode, Json<HealthResponse>) {
    // DB check: run a trivial query
    let db_status = match sqlx::query("SELECT 1").fetch_one(&state.db).await {
      Ok(_) => ComponentStatus { status: "ok", error: None },
      Err(e) => ComponentStatus { status: "degraded", error: Some(e.to_string()) },
    };

    // Meilisearch check: GET /health on the Meilisearch client (if enabled)
    let search_status = if state.config.meilisearch.enabled {
      match state.meili.health().await {
        Ok(_) => ComponentStatus { status: "ok", error: None },
        Err(e) => ComponentStatus { status: "degraded", error: Some(e.to_string()) },
      }
    } else {
      ComponentStatus { status: "disabled", error: None }
    };

    let overall_status = if db_status.status == "ok" { "ok" } else { "degraded" };
    let http_status = if overall_status == "ok" {
      StatusCode::OK
    } else {
      StatusCode::SERVICE_UNAVAILABLE
    };

    (http_status, Json(HealthResponse {
      status: overall_status,
      version: env!("CARGO_PKG_VERSION"),
      db: db_status,
      search: search_status,
    }))
  }

  Response shape:
    200 OK:
    {
      "status": "ok",
      "version": "1.1.0",
      "db": { "status": "ok" },
      "search": { "status": "ok" }
    }

    503 Service Unavailable (DB down):
    {
      "status": "degraded",
      "version": "1.1.0",
      "db": { "status": "degraded", "error": "no connection available" },
      "search": { "status": "ok" }
    }

  Note: search degraded does NOT cause 503 — the app is usable without Meilisearch.
  Only DB degraded causes 503.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_health.rs:
  test_health_returns_200_with_ok_status
  test_health_includes_version_string
  test_health_reports_search_disabled_when_meilisearch_not_configured
  test_health_requires_no_auth

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Manual: curl http://localhost:3000/health | jq
# Manual: LOG_FORMAT=text cargo run — confirm human-readable output
git add backend/
git commit -m "Phase 12 Stage 5: JSON logging, /health endpoint (DB + Meilisearch checks)"
```

---

## STAGE 6 — Rate-Limit Response Headers

**Priority: High (API consumers get 429 with no retry guidance)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/middleware/security_headers.rs, backend/src/lib.rs,
and backend/Cargo.toml.
Now add X-RateLimit-* headers to rate-limited responses.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

tower_governor is already used for rate limiting. It enforces limits but returns
a bare 429 response with no headers. Clients (Kobo sync, API token consumers,
mobile app) cannot tell how long to back off.

Standard rate-limit headers to add:
  X-RateLimit-Limit: 10          — requests allowed per window
  X-RateLimit-Remaining: 3       — requests remaining this window
  X-RateLimit-Reset: 1713820800  — Unix timestamp when the window resets
  Retry-After: 47                — seconds until next request is allowed (on 429 only)

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

backend/src/middleware/security_headers.rs — add a tower middleware layer that
injects rate-limit headers on every response from rate-limited routes:

  Use tower_governor's GovernorConfig to expose the quota and remaining count.
  If tower_governor exposes the current state via an extension, extract it in a
  middleware and set the headers accordingly.

  If tower_governor does not expose per-request remaining count:
    - Set X-RateLimit-Limit with the configured burst size
    - Set X-RateLimit-Reset to the next round window boundary (current time + window_seconds)
    - Omit X-RateLimit-Remaining (better to omit than to lie)
    - On 429: set Retry-After to window_seconds

  Add the headers to ALL responses from rate-limited route groups, not just 429s.
  This lets clients implement proactive backoff.

  The global_rate_limit_layer and auth_rate_limit_layer wrappers in lib.rs should
  each inject the appropriate limit value (global vs auth — they have different quotas).

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_rate_limit.rs (add to existing if present, else create):
  test_auth_endpoint_returns_ratelimit_headers
    — POST /auth/login with valid creds; assert response has X-RateLimit-Limit header

  test_429_response_includes_retry_after
    — exceed the auth rate limit; assert 429 response contains Retry-After header

  test_retry_after_value_is_positive_integer
    — parse Retry-After value; assert it is a number > 0

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Manual: curl -i -X POST http://localhost:3000/api/v1/auth/login | grep -i ratelimit
git add backend/
git commit -m "Phase 12 Stage 6: X-RateLimit-* and Retry-After headers on rate-limited routes"
```

---

## STAGE 7 — WebP Cover Conversion

**Priority: High (mobile grid load time — covers are the most-fetched asset)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/api/books.rs (the cover upload and render_cover_variants sections),
backend/Cargo.toml, and backend/src/storage.rs.
Now add WebP output to cover image processing.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

render_cover_variants() currently generates:
  - Full size: JPEG at 400×600, stored as covers/{ab}/{id}.jpg
  - Thumbnail: JPEG at 100×150, stored as covers/{ab}/{id}.thumb.jpg

The image crate (already in Cargo.toml at version 0.25) has WebP encoding via the
webp feature flag. WebP achieves equivalent visual quality at 25–35% smaller file
size — meaningful for a grid of 50–200 cover images loaded on a mobile connection.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

backend/Cargo.toml — enable WebP:
  image = { version = "0.25", features = ["jpeg", "png", "webp", "gif"] }

backend/src/api/books.rs — update render_cover_variants():

  Generate four files per book (two formats × two sizes):
    covers/{ab}/{id}.jpg        — existing JPEG full size (keep for compatibility)
    covers/{ab}/{id}.thumb.jpg  — existing JPEG thumbnail (keep for compatibility)
    covers/{ab}/{id}.webp       — new WebP full size (same 400×600 dimensions)
    covers/{ab}/{id}.thumb.webp — new WebP thumbnail (same 100×150 dimensions)

  WebP encoding:
    use image::codecs::webp::WebPEncoder;
    let mut webp_bytes: Vec<u8> = Vec::new();
    let encoder = WebPEncoder::new_lossless(&mut webp_bytes);
    // or lossy: WebPEncoder::new_with_quality(&mut webp_bytes, 85.0)
    img.write_with_encoder(encoder)?;

  Use lossy WebP at quality 82 for thumbnails, 85 for full size.
  These values balance file size vs. visual quality for cover art.

Cover-serving handler — add content negotiation:
  Check the Accept header from the request.
  If Accept contains "image/webp":
    Try to serve the .webp variant first.
    Fall back to .jpg if the .webp file does not exist (covers uploaded before this
    change only have .jpg).
  Otherwise: serve .jpg as before.

  This is backward-compatible: old covers served as JPEG; new covers served as
  WebP to clients that support it (all modern browsers, all mobile platforms).

  Example handler logic:
    let wants_webp = headers
      .get(ACCEPT)
      .and_then(|v| v.to_str().ok())
      .map(|s| s.contains("image/webp"))
      .unwrap_or(false);

    let cover_path = if wants_webp {
      let webp_path = format!("covers/{}/{}.webp", bucket, book_id);
      if state.storage.resolve(&webp_path).is_ok() { webp_path } else { jpg_path }
    } else {
      jpg_path
    };

─────────────────────────────────────────
MIGRATION NOTE
─────────────────────────────────────────

Existing covers are JPEG only. WebP variants are generated only on:
  - New cover upload (POST /books/:id/cover)
  - Cover replace
  - Book ingest (if cover is extracted from the file)

No bulk backfill migration is required — old covers fall back to JPEG gracefully.
A future admin action ("Regenerate all covers") can backfill WebP variants if desired.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_covers.rs (add to existing):
  test_cover_upload_generates_webp_variants
    — upload a cover; assert .webp and .thumb.webp files exist in storage

  test_cover_serve_returns_webp_when_accepted
    — GET /books/:id/cover with Accept: image/webp; assert Content-Type: image/webp

  test_cover_serve_falls_back_to_jpeg_when_webp_not_accepted
    — GET /books/:id/cover without Accept: image/webp; assert Content-Type: image/jpeg

  test_cover_serve_falls_back_to_jpeg_when_webp_missing
    — manually delete .webp file; GET with Accept: image/webp; assert Content-Type: image/jpeg

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Manual: upload a cover; compare file sizes of .jpg vs .webp in storage
# Manual: open Chrome DevTools Network tab; verify cover requests return image/webp
git add backend/
git commit -m "Phase 12 Stage 7: WebP cover conversion with JPEG fallback (content negotiation)"
```

---

## STAGE 8 — Global Tag Management

**Priority: High (library hygiene — tags accumulate duplicates over time)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/api/admin.rs, backend/src/db/queries/tags.rs,
backend/src/db/queries/mod.rs, docs/API.md (the tags section),
and backend/tests/ for any existing tag tests.
Now add global tag management routes for admin users.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Tags currently accumulate duplicates: "sci-fi", "Sci-Fi", "science fiction",
"Science Fiction" are treated as four different tags. The only existing tag
routes are a search endpoint and per-book tag editing. There are no admin routes
to rename, merge, or delete tags globally.

─────────────────────────────────────────
SCHEMA (no migration needed)
─────────────────────────────────────────

All operations work on the existing `tags` and `book_tags` tables:
  tags:      id TEXT PK, name TEXT UNIQUE, source TEXT, created_at TEXT
  book_tags: book_id TEXT, tag_id TEXT, confirmed INTEGER, source TEXT

─────────────────────────────────────────
DELIVERABLE 1 — API routes (Admin only)
─────────────────────────────────────────

backend/src/api/admin.rs — add to the admin router:

  GET    /admin/tags                  — list all tags with book counts
  PATCH  /admin/tags/:id              — rename a tag
  DELETE /admin/tags/:id              — delete a tag (removes from all books)
  POST   /admin/tags/:id/merge        — merge tag into another tag

Route details:

  GET /admin/tags
    Query params: q (search), page, page_size
    Response: PaginatedResponse<TagWithCount>
      TagWithCount: { id, name, source, book_count, confirmed_count }
    Order: by book_count DESC by default (most-used first)

  PATCH /admin/tags/:id
    Body: { "name": "Science Fiction" }
    - Validate new name is non-empty and not already taken by another tag
    - UPDATE tags SET name = ? WHERE id = ?
    - Return 200 updated Tag
    - On duplicate name: 409 { "error": "tag_name_conflict" }

  DELETE /admin/tags/:id
    - DELETE FROM book_tags WHERE tag_id = ?
    - DELETE FROM tags WHERE id = ?
    - Return 204 No Content
    - On not found: 404

  POST /admin/tags/:id/merge
    Body: { "into_tag_id": "uuid-of-target-tag" }
    Merges source tag (id) into target tag (into_tag_id):
    1. For each book that has the source tag but NOT the target tag:
       INSERT INTO book_tags (book_id, tag_id, confirmed, source)
       VALUES (book_id, into_tag_id, confirmed, source)
    2. DELETE FROM book_tags WHERE tag_id = source_id
    3. DELETE FROM tags WHERE id = source_id
    Return 200: { "merged_book_count": N, "target_tag": Tag }
    Both steps in a single DB transaction — atomic merge.

─────────────────────────────────────────
DELIVERABLE 2 — DB queries
─────────────────────────────────────────

backend/src/db/queries/tags.rs — add:

  pub async fn list_tags_with_counts(db, q, page, page_size) -> PaginatedResponse<TagWithCount>
    SELECT t.id, t.name, t.source,
           COUNT(bt.book_id) AS book_count,
           SUM(CASE WHEN bt.confirmed = 1 THEN 1 ELSE 0 END) AS confirmed_count
    FROM tags t
    LEFT JOIN book_tags bt ON bt.tag_id = t.id
    WHERE t.name LIKE '%' || ? || '%'
    GROUP BY t.id
    ORDER BY book_count DESC
    LIMIT ? OFFSET ?

  pub async fn rename_tag(db, tag_id, new_name) -> Result<Tag, AppError>
  pub async fn delete_tag(db, tag_id) -> Result<(), AppError>
  pub async fn merge_tags(db, source_id, target_id) -> Result<usize, AppError>
    (returns count of books updated)

─────────────────────────────────────────
DELIVERABLE 3 — Admin UI
─────────────────────────────────────────

apps/web/src/features/admin/TagsPage.tsx — new page:

  Table columns: Name | Books | Confirmed | Actions
  Actions per row:
    - Rename (inline edit — click name to edit in place, Enter to save)
    - Merge (opens a combobox to pick the target tag, then confirms)
    - Delete (confirmation dialog — "Remove this tag from N books?")

  Search input above table (debounced, filters by name).
  Pagination at bottom.

  Add "Tags" to the admin sidebar nav (between Roles and Import).

Add route to TanStack Router: /admin/tags

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_tag_management.rs:
  test_list_tags_returns_book_counts
  test_rename_tag_updates_name
  test_rename_tag_conflicts_with_existing_name_returns_409
  test_delete_tag_removes_from_all_books
  test_delete_nonexistent_tag_returns_404
  test_merge_tag_moves_books_to_target
  test_merge_tag_does_not_duplicate_on_books_that_already_have_target
  test_merge_is_atomic_source_deleted_after_merge

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/web/src/features/admin/TagsPage.tsx
git commit -m "Phase 12 Stage 8: global tag rename, merge, delete — admin tag management"
```

---

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/review` — verify no test state leaks across specs, E2E fixtures are minimal, CI job is non-blocking |
| Stage 2 | `/review` — verify backup scripts are idempotent and fail loudly, runbook commands are copy-pasteable and accurate |
| Stage 3 | `/review` — verify 100% key coverage confirmed by CI script, no EN keys missing from any locale |
| Stage 4 | `/review` + `/security-review` — verify Range header parsing rejects malformed input, no path traversal via range, S3 key sanitizer unchanged |
| Stage 5 | `/review` — verify health endpoint doesn't leak internal error details, JSON logging doesn't log secrets |
| Stage 6 | `/review` + `/security-review` — verify rate-limit headers don't expose internal state, Retry-After is always a positive integer |
| Stage 7 | `/review` — verify WebP fallback is correct, no broken cover-serve on pre-existing JPEG-only books |
| Stage 8 | `/review` — verify merge is atomic (transaction), duplicate suppression on merge is correct, admin-only enforcement |

Run `/engineering:deploy-checklist` after Stage 8 before tagging v1.1.
