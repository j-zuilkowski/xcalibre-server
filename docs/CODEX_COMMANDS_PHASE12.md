# Codex Desktop App — autolibre Phase 12: Post-v1.0 Polish

## What Phase 12 Builds

Closes the four post-v1.0 gaps identified in the Phase 11 evaluation:

- **Stage 1** — Playwright E2E test suite (reader, search, auth, admin golden paths)
- **Stage 2** — Deployment runbooks (single-instance Docker, HA MariaDB, backup/restore, S3 migration)
- **Stage 3** — FR/DE/ES translation completion + CI coverage check
- **Stage 4** — S3 range request support (audio + large PDF streaming restored on S3 backend)

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
Now add a Playwright E2E test suite for the autolibre web app.

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
        - run: pnpm --filter @autolibre/backend build   # or cargo build --release
        - run: npx playwright install --with-deps chromium
        - name: Start backend (test mode)
          run: |
            AUTOLIBRE_ENV=test ./target/release/autolibre &
            echo "BACKEND_PID=$!" >> $GITHUB_ENV
          env:
            DATABASE_URL: sqlite::memory:
            JWT_SECRET: ci-test-secret-do-not-use
        - name: Run E2E tests
          run: pnpm --filter @autolibre/web test:e2e
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
pnpm --filter @autolibre/web test:e2e
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
Now write comprehensive deployment runbooks for autolibre.

─────────────────────────────────────────
DELIVERABLE 1 — docs/DEPLOY.md
─────────────────────────────────────────

Create docs/DEPLOY.md. Write in a direct, instructional style — commands must be
copy-pasteable. Do not pad with marketing language or motivational headings.

Contents:

  # Deploying autolibre

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
      - Reverse proxy to the autolibre container
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
    Database init SQL: CREATE DATABASE autolibre; GRANT ALL ON autolibre.* TO 'autolibre'@'%';
    Config change: database_url = "mysql://autolibre:password@mariadb:3306/autolibre"

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
    Include compression: | gzip > autolibre-$(date +%Y%m%d).sql.gz

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

  Script reads AUTOLIBRE_BACKUP_DIR, AUTOLIBRE_DB_PATH, AUTOLIBRE_STORAGE_PATH
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
       MariaDB: gunzip < <backup.sql.gz> | mysql autolibre
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
pnpm --filter @autolibre/web build   # no TS errors
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

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/review` — verify no test state leaks across specs, E2E fixtures are minimal, CI job is non-blocking |
| Stage 2 | `/review` — verify backup scripts are idempotent and fail loudly, runbook commands are copy-pasteable and accurate |
| Stage 3 | `/review` — verify 100% key coverage confirmed by CI script, no EN keys missing from any locale |
| Stage 4 | `/review` + `/security-review` — verify Range header parsing rejects malformed input, no path traversal via range, S3 key sanitizer unchanged |

Run `/engineering:deploy-checklist` after Stage 4 before tagging v1.1.
