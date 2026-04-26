# Codex Desktop App — calibre-web-rs Phase 3: Web Frontend

## What Phase 3 Builds

A Vite + React SPA that consumes the Phase 1 backend API. The full UI/UX spec
is in docs/DESIGN.md — read it before starting any stage. Key surfaces:

- Library cover grid with filters, sort, and pagination
- Book detail with expandable sections and format download
- Full-screen epub and PDF reader with reading progress sync
- Inline + full-page search
- Admin panel (users, roles, import, jobs)
- PWA (installable, offline shell)

The monorepo adds two new workspaces alongside the Rust crates:
- `packages/shared/` — TypeScript types + API client (reused by Phase 6 mobile)
- `apps/web/` — Vite React SPA

---

## Tech Stack (do not deviate)

| Concern | Choice |
|---|---|
| Bundler | Vite 5 |
| Framework | React 18 + TypeScript |
| Routing | TanStack Router (file-based) |
| Data fetching | TanStack Query v5 |
| UI components | shadcn/ui — **copied into repo**, not a runtime dep |
| Styling | Tailwind CSS v3, zinc palette, teal-600 accent |
| UI font | Inter (local, no CDN) |
| Reader font | Literata (local, no CDN) |
| Epub reader | epub.js |
| PDF reader | PDF.js (pdfjs-dist) |
| PWA | vite-plugin-pwa |
| Testing | Vitest + React Testing Library + MSW (API mocking) |
| Package manager | pnpm (workspace) |
| Monorepo | Turborepo |

---

## Monorepo Structure

```
calibre-web-rs/
├── Cargo.toml              (existing Rust workspace)
├── package.json            (pnpm workspace root)
├── pnpm-workspace.yaml
├── turbo.json
├── apps/
│   └── web/                (Vite React SPA)
│       ├── package.json
│       ├── vite.config.ts
│       ├── tailwind.config.ts
│       ├── src/
│       │   ├── main.tsx
│       │   ├── router.tsx
│       │   ├── components/
│       │   │   └── ui/     (shadcn/ui copies)
│       │   ├── features/   (auth/, library/, reader/, search/, admin/)
│       │   └── lib/        (query-client.ts, auth-store.ts)
│       └── __tests__/
└── packages/
    └── shared/
        ├── package.json
        └── src/
            ├── types.ts    (Book, BookSummary, User, Role, etc.)
            ├── client.ts   (API client)
            └── index.ts
```

---

## Design Reference (read before each stage)

docs/DESIGN.md contains the full spec. Key decisions to know:

**Colors**: zinc neutrals + `teal-600` accent (light) / `teal-400` (dark)
**Typography**: Inter for UI, Literata for reader body text
**Sidebar**: Collapsed icon-only 48px wide, expands to 200px on hover
**Book card rest state**: cover + title + author only, no badges
**Book card hover**: dark overlay, Read + Download buttons, teal progress bar (3px)
**Cover placeholder**: deterministic muted background color from title hash + large
serif initial letter, same 2:3 aspect ratio as covers
**Reader**: full-screen, chrome fades in on mouse move, auto-hides after 3s

---

## API Base URL

In development: `http://localhost:8083`
Configured via `VITE_API_URL` env var.
All requests include `Authorization: Bearer <access_token>` except login/register.
On 401, attempt one silent refresh using the stored refresh_token, then redirect
to login if refresh also fails.

---

## STAGE 1 — Monorepo Scaffold + packages/shared + All Test Stubs

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/DESIGN.md and docs/HANDOFF.md.

Scaffold the Phase 3 monorepo and write ALL test files as stubs. No component
implementation yet — tests should import and call but use todo stubs.

Deliverables:

Root (calibre-web-rs/):
- package.json — pnpm workspace root with scripts: dev, build, test, lint
- pnpm-workspace.yaml — includes apps/*, packages/*
- turbo.json — pipeline: build depends on ^build, test depends on build

packages/shared/:
- package.json — name: @calibre/shared, private, vitest configured
- tsconfig.json
- src/types.ts — ALL TypeScript types matching the API:
    Book, BookSummary, AuthorRef, SeriesRef, TagRef, FormatRef, Identifier,
    ReadingProgress, Shelf, User, Role,
    PaginatedResponse<T>, ApiError,
    LoginRequest, LoginResponse, RefreshResponse, RegisterRequest
- src/client.ts — ApiClient class:
    constructor(baseUrl: string, getToken: () => string | null,
                onUnauthorized: () => void)
    Methods (all return Promise, throw ApiError on failure):
      login(req: LoginRequest): Promise<LoginResponse>
      register(req: RegisterRequest): Promise<User>
      refresh(refreshToken: string): Promise<RefreshResponse>
      logout(refreshToken: string): Promise<void>
      me(): Promise<User>
      changePassword(current: string, next: string): Promise<void>
      listBooks(params: ListBooksParams): Promise<PaginatedResponse<BookSummary>>
      getBook(id: string): Promise<Book>
      uploadBook(file: File, metadata?: object): Promise<Book>
      patchBook(id: string, patch: object): Promise<Book>
      deleteBook(id: string): Promise<void>
      coverUrl(bookId: string): string
      downloadUrl(bookId: string, format: string): string
      streamUrl(bookId: string, format: string): string
- src/index.ts — re-exports everything
- src/__tests__/client.test.ts — test stubs for:
    test_login_sends_correct_request
    test_login_returns_tokens
    test_refresh_on_401
    test_list_books_builds_correct_url
    test_get_book_returns_book
    test_api_error_on_non_ok_response

apps/web/:
- package.json — depends on @calibre/shared, all deps listed in tech stack
- vite.config.ts — react plugin, path aliases (@/ → src/), proxy /api → localhost:8083
- tailwind.config.ts — zinc base, teal-600 accent, Inter + Literata font families
- tsconfig.json
- index.html — Inter + Literata fonts loaded from /fonts/ (local)
- src/main.tsx — React root
- src/router.tsx — TanStack Router stub with placeholder routes
- src/lib/query-client.ts — TanStack Query client config
- src/lib/auth-store.ts — zustand store: access_token, refresh_token, user, setAuth, clearAuth
- src/__tests__/auth-store.test.ts — stubs
- src/__tests__/cover-placeholder.test.ts — stubs

When done, run:
  pnpm install 2>&1 | tail -10
  pnpm --filter @calibre/shared typecheck 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if clean.**

---

## STAGE 2 — Auth UI

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/DESIGN.md. Now do Stage 2 of Phase 3.

Implement the authentication UI and token management. Make all auth tests pass.

Deliverables:

packages/shared/src/client.ts — implement login, register, refresh, logout, me,
changePassword fully. On any 401 response: attempt one silent refresh, retry
the original request. If refresh fails, call onUnauthorized().

apps/web/src/lib/auth-store.ts — zustand store (fully implemented):
  { access_token, refresh_token, user, setAuth(LoginResponse), clearAuth() }
  Persists to localStorage. Reads on init.

apps/web/src/features/auth/:
  LoginPage.tsx — centered card, username + password fields, submit button,
    error message on failure. On success: setAuth(), navigate to /
  RegisterPage.tsx — shown only when no users exist (backend returns 201 on
    first registration, 409 after). Same layout as login.
  ProtectedRoute.tsx — wraps routes that require auth. Redirects to /login
    if no access_token. Attempts silent refresh before redirecting.

apps/web/src/router.tsx — wire routes:
  /login → LoginPage (public)
  /register → RegisterPage (public)
  / → protected, redirect to /library
  /library → protected (stub)
  /books/:id → protected (stub)

apps/web/src/__tests__/auth-store.test.ts — remove stubs, implement:
  test_set_auth_persists_to_storage
  test_clear_auth_removes_from_storage
  test_auth_restored_on_init

packages/shared/src/__tests__/client.test.ts — remove stubs, implement using MSW:
  test_login_sends_correct_request
  test_login_returns_tokens
  test_refresh_on_401
  test_list_books_builds_correct_url
  test_get_book_returns_book
  test_api_error_on_non_ok_response

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter @calibre/shared test -- --reporter=verbose 2>&1
    pnpm --filter web test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component or client source file.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    pnpm --filter @xs/web dev &
    @Computer Use — open http://localhost:5173 in the in-app browser
    Verify:
      - /login — form renders, validation states look correct
      - /register — form renders correctly
    Kill the dev server: kill %1

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 3 — Library Grid

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/DESIGN.md carefully — the "Library View" and "Book Card" sections
have the full spec. Now do Stage 3 of Phase 3.

First fix this type mismatch before writing any components:
In packages/shared/src/types.ts, BookSummary currently picks tags, formats,
pubdate, created_at, and indexed_at from Book, but the backend list endpoint
does not return these fields. Fix BookSummary to only include:
  id, title, sort_title, authors, series, series_index, cover_url, has_cover,
  language, rating, last_modified

Then implement the library cover grid.

Deliverables:

apps/web/src/features/library/LibraryPage.tsx:
  - TanStack Query: useQuery(['books', params], () => apiClient.listBooks(params))
  - Responsive cover grid: 2 cols mobile → 4 tablet → 6-8 desktop (Tailwind grid)
  - Toolbar: filter chips (Author, Series, Tag, Language, Format) +
    sort dropdown (Title, Author, Date Added, Rating) + Grid/List toggle
  - Pagination (page controls, updates query params)
  - Empty state: message + call to action when no books

apps/web/src/features/library/BookCard.tsx:
  - Rest state: cover image (2:3 aspect ratio, object-cover) + title + author.
    Nothing else.
  - Hover state: semi-transparent dark overlay fades in over cover.
    Two icon buttons appear: Read (→ /books/:id/read/:format) and Download.
    Reading progress bar: 3px teal-600, flush bottom of cover, only if
    percentage > 0.
  - Cover image: src={apiClient.coverUrl(book.id)}, lazy loading, has_cover check
  - Falls back to CoverPlaceholder when has_cover is false

apps/web/src/features/library/CoverPlaceholder.tsx:
  - Hash the book title string to one of 8 muted background colors
    (zinc/teal palette — no bright colors)
  - Render first letter of title, centered, large Literata font, light on dark
  - Exact same 2:3 aspect ratio as real covers (use aspect-[2/3] Tailwind class)
  - Pure client-side — no server round-trip

apps/web/src/features/library/BookListRow.tsx:
  - List view row: small cover thumbnail + title + author + series + format badges

apps/web/src/__tests__/BookCard.test.tsx:
  test_shows_cover_image_when_has_cover_true
  test_shows_placeholder_when_has_cover_false
  test_progress_bar_visible_when_progress_nonzero
  test_progress_bar_hidden_when_no_progress

apps/web/src/__tests__/CoverPlaceholder.test.tsx:
  test_renders_first_letter_of_title
  test_same_title_always_same_color

apps/web/src/__tests__/LibraryPage.test.tsx:
  test_renders_book_cards
  test_empty_state_when_no_books
  test_filter_updates_query

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter web test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component source file.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    pnpm --filter @xs/web dev &
    @Computer Use — open http://localhost:5173 in the in-app browser
    Verify:
      - /library — cover grid renders, empty state shows when no books
      - /library — hover a card and confirm action buttons appear
      - /library — filter chips and sort dropdown visible and functional
      - /library — pagination controls present when total > page_size
    Kill the dev server: kill %1

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 4 — Book Detail

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/DESIGN.md — the "Book Detail" section has the full spec. Now do
Stage 4 of Phase 3.

Implement the book detail page with all expandable sections.

Deliverables:

apps/web/src/features/library/BookDetailPage.tsx:
  Zone 1 — Hero:
    - Large cover image (or CoverPlaceholder)
    - Title, authors, series + book number
    - Star rating display (0-10 mapped to 0-5 stars)
    - [ Read ] primary button → navigates to /books/:id/read/:format
    - [ Download ▾ ] dropdown → lists available formats with file size, download link
    - ••• menu (visible to can_edit / admin only):
        Edit metadata, Replace cover, Delete book

  Zone 2 — Metadata strip:
    Language · Year · Tags · Formats (clicking a tag filters the library)

  Zone 3 — Expandable sections (all collapsed by default, shadcn Collapsible):
    Description | Formats | Identifiers | Series | History (admin only)

apps/web/src/__tests__/BookDetailPage.test.tsx — tests:
  test_shows_book_title_and_author
  test_download_dropdown_lists_formats
  test_edit_menu_hidden_from_non_editors
  test_expandable_sections_toggle
  test_tag_click_navigates_to_filtered_library

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter web test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component source file.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    pnpm --filter @xs/web dev &
    @Computer Use — open http://localhost:5173 in the in-app browser
    Verify:
      - /books/1 — title, authors, cover image, star rating display correctly
      - /books/1 — collapsible Description section opens and shows text
      - /books/1 — collapsible Formats section opens and shows download links
      - /books/1 — Read button present and links to reader
      - /books/1 — tag chips render and are clickable
      - /books/1 — series info displays correctly
    Kill the dev server: kill %1

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 5 — Reader (Epub + PDF)

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/DESIGN.md — the "Reader" section has the full spec. Now do Stage 5
of Phase 3. This is the most complex stage — take care with the epub.js
and PDF.js integrations.

Implement the full-screen reader for EPUB and PDF formats.

Deliverables:

apps/web/src/features/reader/:
  ReaderPage.tsx — route /books/:id/read/:format:
    - Detects format type: EPUB → EpubReader, PDF → PdfReader
    - Full-screen, hides all other chrome
    - On mount: load book info, resume reading progress (GET /reading-progress/:id)
    - On progress change: debounced save (PATCH /reading-progress/:id)
    - Keyboard: left/right arrows for page turns (epub), scroll (pdf)

  EpubReader.tsx — epub.js integration:
    - Load from streamUrl (streaming endpoint with range request support)
    - Renders into a container div, epub.js manages pagination
    - Reader toolbar (fades in on mouse move, auto-hides after 3s):
        ← back to book detail | Title · Author | ⚙ settings | ☰ TOC
        Thin progress bar at bottom (percentage)
    - Settings panel (slides in from right, shadcn Sheet):
        Font: Literata / Inter radio
        Font size: slider (14-24px)
        Line height: slider
        Margin: slider
        Theme: Light / Sepia / Dark
    - TOC panel (slides in from left, shadcn Sheet)
    - Settings persisted to localStorage per user

  PdfReader.tsx — PDF.js integration:
    - Load from streamUrl
    - Page-by-page rendering into canvas
    - Same toolbar as epub reader
    - Progress = current page / total pages

apps/web/src/__tests__/ReaderPage.test.tsx — tests:
  test_epub_reader_renders_for_epub_format
  test_pdf_reader_renders_for_pdf_format
  test_reader_saves_progress_on_advance
  test_reader_restores_progress_on_load
  test_toolbar_fades_in_on_mouse_move
  test_settings_panel_opens_on_settings_click

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter web test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component source file.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    pnpm --filter @xs/web dev &
    @Computer Use — open http://localhost:5173 in the in-app browser
    Verify:
      - /books/1/read/epub — reader fills the viewport, toolbar visible
      - /books/1/read/epub — toolbar fades after a few seconds of no movement
      - /books/1/read/epub — move mouse to restore toolbar
      - /books/1/read/pdf — PDF renders pages correctly
    Kill the dev server: kill %1

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 6 — Search + Admin Panel

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/DESIGN.md — the "Search" and "Admin Panel" sections. Now do Stage 6.

Implement search and the admin panel.

Deliverables:

apps/web/src/features/search/:
  SearchBar.tsx — top bar search input:
    - Expands on focus to a dropdown
    - Shows: last 5 recent searches + first 5 matching books as mini-cards
    - "See all results →" link
  SearchPage.tsx — route /search:
    - Same cover grid as library, filtered by query
    - Two tabs: Library (full-text) | Semantic (grayed with tooltip if unavailable)
    - Filter chips + sort same as library

apps/web/src/features/admin/:
  AdminLayout.tsx — full-page replacement (not a modal), sidebar nav
  DashboardPage.tsx — stats cards: total books, users, storage used, LLM status
  UsersPage.tsx — table: username, role, active, last login. Inline CRUD.
  ImportPage.tsx — file upload or path input, dry-run toggle, progress log
  JobsPage.tsx — LLM job queue table, cancel pending jobs

apps/web/src/components/AppShell.tsx — wire the full layout:
  Top bar: logo + SearchBar + user avatar menu (Profile, Theme toggle,
    Sign out, Admin Panel link if admin)
  Sidebar: collapsed 48px icons, expands to 200px on hover. Items:
    Library, Search, Shelves. Never shows admin items.
  Main content area renders child routes

apps/web/src/__tests__/SearchPage.test.tsx — key tests
apps/web/src/__tests__/AdminUsersPage.test.tsx — key tests

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter web test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component source file.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    pnpm --filter @xs/web dev &
    @Computer Use — open http://localhost:5173 in the in-app browser
    Verify:
      - /search?q=dune — results render with title, author, cover
      - /search?q=dune — semantic tab visible; enable/disable based on backend status
      - /search?q=dune — score badges show on each result card
      - /admin/users — user table renders with role badges and action buttons
      - /admin/jobs — job queue renders with status indicators
      - /admin/import — file input and upload button present
    Kill the dev server: kill %1

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 7 — PWA + Backend Static Serving + Final Checks

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 7 of Phase 3 — wire everything together
and make all CI checks pass.

Deliverables:

apps/web/vite.config.ts — add vite-plugin-pwa:
  manifest: name, short_name, theme_color (#0d9488 = teal-600), icons
  workbox: cache shell + assets, network-first for /api/*

backend/src/api/mod.rs — serve the built web app:
  Add a fallback route that serves apps/web/dist/index.html for all non-API
  GET requests (SPA client-side routing). Use tower-http ServeDir for static
  assets at /assets/.

docker/Dockerfile — add frontend build stage:
  FROM node:20-slim AS web-builder
  Install pnpm, run pnpm install + pnpm --filter web build
  Copy apps/web/dist into the runtime image at /app/web/

docker/docker-compose.yml — no changes needed (Caddy stage already commented)

Update docker/Caddyfile — add gzip, cache headers for /assets/*

When done, run ALL of these and show the complete output:
  pnpm --filter @calibre/shared test -- --reporter=verbose 2>&1
  pnpm --filter web test -- --reporter=verbose 2>&1
  pnpm --filter web build 2>&1 | tail -20
  cargo test --workspace 2>&1 | tail -20
  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude runs /review + /engineering:deploy-checklist → proceed to Stage 8.**

---

## STAGE 8 — Complete RTL Frontend Test Suite

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read localProject/TEST_SPEC.md in full before writing any code.
Now complete Stage 8 of Phase 3: implement the full React Testing Library
test suite for all components built in Stages 1–7.

SETUP — write apps/web/vitest.config.ts with this exact content:
  import { defineConfig } from "vitest/config";
  export default defineConfig({
    test: {
      environment: "jsdom",
      globals: true,
      setupFiles: ["./src/test/setup.ts"],
      include: ["src/**/*.test.{ts,tsx}"],
      exclude: ["e2e/**", "node_modules/**"],
      pool: "forks",
      poolOptions: { forks: { execArgv: ["--max-old-space-size=4096"] } },
    },
  });

  The include/exclude is required — without it vitest picks up e2e/ Playwright
  specs and crashes the worker. The heap flag prevents OOM on large test runs.

Create apps/web/src/test/setup.ts (MSW server bootstrap):
  import "@testing-library/jest-dom"
  import { afterAll, afterEach, beforeAll } from "vitest"
  import { setupServer } from "msw/node"
  import { handlers } from "./handlers"

  export const server = setupServer(...handlers)
  beforeAll(() => server.listen({ onUnhandledRequest: "warn" }))
  afterEach(() => server.resetHandlers())
  afterAll(() => server.close())

Create apps/web/src/test/handlers.ts — default happy-path MSW handlers:
  GET  /api/v1/auth/providers   → makeAuthProviders()
  POST /api/v1/auth/login       → makeAuthSession() (branch: body.username==="totp"
                                    → { totp_required:true, totp_token:"totp-token" })
  POST /api/v1/auth/register    → makeAuthSession().user, status 201
  POST /api/v1/auth/totp/verify → makeAuthSession()
  POST /api/v1/auth/totp/verify-backup → makeAuthSession()
  POST /api/v1/auth/refresh     → { access_token:"new-token", refresh_token:"new-refresh" }
  GET  /api/v1/auth/me          → makeUser()   ← NOT /users/me; apiClient.me() calls /auth/me
  GET  /api/v1/auth/totp/setup  → { secret_base32:"JBSWY3DPEHPK3PXP", otpauth_uri:"..." }
                                    ← must be GET, not POST; apiClient.setupTotp() uses GET
  POST /api/v1/auth/totp/confirm → { backup_codes: ["ABC12345"] }
  POST /api/v1/auth/totp/disable → 204
  GET  /api/v1/books            → { items:[], total:0, page:1, page_size:24 }
  GET  /api/v1/books/:id        → makeBook({ id: params.id })
  PATCH /api/v1/books/:id       → makeBook({ id: params.id, ...requestBody })
  DELETE /api/v1/books/:id      → 204
  POST /api/v1/books/:id/archive → 204
  GET  /api/v1/books/:id/progress → makeProgress()
  PATCH /api/v1/books/:id/progress → makeProgress()
  GET  /api/v1/books/:id/annotations → [makeAnnotation()]
  POST /api/v1/books/:id/annotations → makeAnnotation(requestBody)
  PATCH /api/v1/books/:id/annotations/:annotationId → makeAnnotation({...})
  DELETE /api/v1/books/:id/annotations/:annotationId → 204
  GET  /api/v1/shelves          → [makeShelf()]
  POST /api/v1/shelves          → makeShelf({ id:"shelf-2", ...requestBody }), status 201
  DELETE /api/v1/shelves/:id    → 204
  GET  /api/v1/shelves/:id/books → { items:[makeBookSummary()], total:1, page:1, page_size:100 }
  DELETE /api/v1/shelves/:id/books/:bookId → 204
  GET  /api/v1/collections      → [makeCollection()]
  GET  /api/v1/libraries        → [makeLibrary()]
  GET  /api/v1/search/status    → { fts:true, meilisearch:true, semantic:true, backend:"meilisearch" }
  GET  /api/v1/search           → if q="" return empty; if q="error" return 500;
                                    else return [makeBookSummary({title:"Dune"}), makeBookSummary({id:"2",title:"Children of Dune"})]
                                    ← default returns results for any non-empty q; tests that
                                       assert a no-results state MUST add a server.use() override
                                       returning { items:[], total:0 } for their specific query
  GET  /api/v1/search/semantic  → { items:[makeBookSummary({title:"Dune",...})], total:1, ... }
  GET  /api/v1/admin/users      → [makeAdminUser()]
  GET  /api/v1/admin/roles      → [makeAdminUser().role]
  POST /api/v1/admin/users      → makeAdminUser({...requestBody}), status 201
  PATCH /api/v1/admin/users/:id → makeAdminUser({...})
  DELETE /api/v1/admin/users/:id → 204
  POST /api/v1/admin/users/:id/reset-password → 204
  POST /api/v1/admin/users/:id/totp/disable → 204
  GET  /api/v1/admin/jobs       → { items:[makeJob()], total:1, page:1, page_size:25 }
  DELETE /api/v1/admin/jobs/:id → 204
  POST /api/v1/admin/import/bulk → { job_id:"job-1" }, status 201
  GET  /api/v1/admin/import/:id → makeImportStatus()
  GET  /api/v1/auth/me          → makeUser()   (duplicate of above — keep only once)

  CRITICAL HANDLER GOTCHAS (these cause silent test timeouts if wrong):
  - apiClient.me() calls GET /api/v1/auth/me — not /api/v1/users/me
  - apiClient.setupTotp() calls GET /api/v1/auth/totp/setup — not POST
  - Tests overriding the "me" endpoint to seed totp_enabled:true must override
    /api/v1/auth/me, not /api/v1/users/me

Create apps/web/src/test/render.tsx — renderWithProviders helper:
  Wraps in: QueryClientProvider (retry: false) + RouterProvider (memory history)
  + seeded authStore (access_token: "test-token").
  Signature: renderWithProviders(ui, { initialPath = "/library", authenticated = true } = {})
  Read apps/web/src/main.tsx and apps/web/src/router.tsx first to match
  the exact providers the real app uses.

AUTH TESTS — implement every test from TEST_SPEC.md for:
  src/features/auth/LoginPage.test.tsx
  src/features/auth/RegisterPage.test.tsx
  src/features/auth/ProtectedRoute.test.tsx

  Note: LoginPage and RegisterPage are public routes. Render with just
  QueryClientProvider + RouterProvider (not renderWithProviders which seeds auth).

LIBRARY TESTS — implement every test from TEST_SPEC.md for:
  src/features/library/BookCard.test.tsx       (render BookCard directly, no router)
  src/features/library/LibraryPage.test.tsx    (renderWithProviders initialPath="/library")
  src/features/library/BookDetailPage.test.tsx (renderWithProviders initialPath="/books/1")
  src/features/library/ShelvesPage.test.tsx    (renderWithProviders initialPath="/shelves")

READER TESTS — implement every test from TEST_SPEC.md for:
  src/features/reader/ReaderPage.test.tsx

  Mock all sub-readers with vi.mock():
    vi.mock("../reader/EpubReader", () => ({ EpubReader: () => <div data-testid="epub-reader" /> }))
    // same for PdfReader, ComicReader, DjvuReader

ADMIN + PROFILE + SEARCH TESTS — implement every test from TEST_SPEC.md for:
  src/features/admin/UsersPage.test.tsx    (seed authStore role:"admin")
  src/features/admin/ImportPage.test.tsx   (seed authStore role:"admin")
  src/features/admin/JobsPage.test.tsx     (seed authStore role:"admin")
  src/features/profile/ProfilePage.test.tsx
  src/features/search/SearchPage.test.tsx

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter @xs/web test -- --reporter=verbose 2>&1

    If exit code is non-zero (any failures):
      For EACH failing test:
        1. Read the full error output for that test.
        2. Read the component source file the test is exercising.
        3. Determine root cause:
           - Wrong async pattern → use findByRole/findByText instead of getByRole/getByText,
             or wrap synchronous DOM checks in await waitFor(() => { ... })
           - Wrong endpoint in handler → read packages/shared/src/client.ts to find the
             exact URL and method the ApiClient uses, then fix the MSW handler to match
           - Missing translation key → read the component to find the t("...") call,
             then add the key to apps/web/public/locales/en/translation.json (and fr/de/es)
           - Default handler returns results for non-empty q → any test asserting a
             no-results state must add server.use() override returning { items:[], total:0 }
             for its specific query value before rendering
           - Wrong assertion → read the rendered DOM, fix the assertion to match reality
        4. Fix the test or the handler. Never skip, comment out, or .skip a failing test.
      Go back to LOOP.

    If exit code is 0 (all tests pass): exit loop and commit.

Commit when all tests are green:
  cd /Users/jonzuilkowski/Documents/localProject/xcalibre-server
  git add apps/web/src/test/ \
          apps/web/src/features/auth/*.test.tsx \
          apps/web/src/features/library/*.test.tsx \
          apps/web/src/features/reader/ReaderPage.test.tsx \
          apps/web/src/features/admin/*.test.tsx \
          apps/web/src/features/profile/*.test.tsx \
          apps/web/src/features/search/*.test.tsx \
          apps/web/public/locales/
  git commit -m "test(phase3): complete RTL frontend test suite"
```

**Paste output here → Claude reviews → Phase 3 complete.**

---

## Review Checkpoints

| After stage | Skill | Purpose |
|---|---|---|
| Stage 1 | `/review` | Monorepo structure, shared types completeness |
| Stage 2 | `/review` | Token refresh logic, protected route correctness |
| Stage 3 | `/review` + `/simplify` | Grid performance, cover placeholder, hover states |
| Stage 4 | `/review` | Permission checks, expandable sections |
| Stage 5 | `/review` | epub.js/PDF.js wiring, progress sync correctness |
| Stage 6 | `/review` + `/simplify` | Search debounce, admin permission gates |
| Stage 7 | `/review` + `/engineering:deploy-checklist` | PWA manifest, static serving, Docker build |
| Stage 8 | `/review` | RTL test suite completeness, MSW handler coverage |

---

## If Codex Gets Stuck or a Test Fails

```
The following test is failing. Diagnose the root cause and fix it.
Do not work around it — fix the underlying issue.

[paste error output]
```

If still stuck, paste here and run /engineering:debug.
