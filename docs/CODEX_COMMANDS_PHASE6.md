# Codex Desktop App — xcalibre-server Phase 6: Mobile App

## What Phase 6 Builds

A native iOS + Android app using Expo (React Native) that consumes the same
`packages/shared` API client and types as the web frontend. The app provides
offline-capable library browsing, book reading, and reading progress sync:

- Expo project at `apps/mobile/` wired into the pnpm + Turborepo monorepo
- Auth: login screen, JWT + refresh token flow via Expo SecureStore
- Library browse: cover-first grid, pull-to-refresh, infinite scroll
- Book detail: metadata, formats, download, AI panel (classify/validate/derive)
- Offline: Expo SQLite mirrors the library subset; `last_modified` drives incremental sync
- File downloads: books stored via expo-file-system for offline reading
- EPUB reader: foliojs-port with reading position (CFI) persisted and synced
- PDF reader: expo-pdf with page position persisted and synced
- EAS build config: development, preview, and production profiles for iOS + Android

## Key Facts

- `packages/shared/src/types.ts` and `packages/shared/src/client.ts` are already built —
  import directly, do not duplicate
- `reading_progress(user_id, book_id, format_id, cfi, page, percentage, last_modified)` —
  CFI for EPUB position, page for PDF; already in schema and backend
- `books.last_modified` drives incremental sync — always pass `?since=<timestamp>` on sync
- Auth pattern: access token (15 min JWT) + refresh token (30 day) stored in Expo SecureStore;
  httpOnly cookies are web-only — mobile must use Authorization header
- NativeWind v4 required — must match Tailwind v3 used in web app (see root package.json)
- All LLM routes return 503 when disabled — AI panel must guard the same way as web

## Reference Files

Read these before starting each stage:
- `docs/ARCHITECTURE.md` — mobile stack decisions, offline depth (Decision D), SecureStore auth
- `docs/API.md` — all route contracts; reading progress routes at `/api/v1/progress/*`
- `docs/DESIGN.md` — color system (zinc palette, teal-600 accent), typography (Inter/Literata)
- `packages/shared/src/types.ts` — all shared types already defined
- `packages/shared/src/client.ts` — all API methods already implemented

---

## STAGE 1 — Expo Setup + Auth + Library Browse

**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, docs/DESIGN.md, packages/shared/src/types.ts,
and packages/shared/src/client.ts. Now do Stage 1 of Phase 6.

Scaffold the Expo mobile app and implement auth + library browse.

Deliverables:

apps/mobile/ — new Expo project, wired into pnpm workspace:
  package.json:
    name: "@xs/mobile"
    extends the root pnpm workspace
    dependencies: expo ~51, expo-router ~3, expo-secure-store, expo-sqlite,
      expo-file-system, nativewind@^4, react-native-safe-area-context,
      react-native-screens, @tanstack/react-query v5, react-native-gesture-handler,
      react-native-reanimated, @expo/vector-icons
    devDependencies: typescript, @types/react, @types/react-native, tailwindcss
  app.json: name "Xcalibre", slug "xcalibre", scheme "xcalibre",
    platforms [ios, android], icon ./assets/icon.png, splash ./assets/splash.png
  tailwind.config.js: content includes app/**/*.tsx, extends zinc/teal-600 theme
    matching web app — import from ../../packages/shared/tailwind.config.base.js if it
    exists, otherwise define inline
  babel.config.js: expo preset + nativewind/babel plugin
  tsconfig.json: extends expo/tsconfig.base, paths alias for @xs/shared

apps/mobile/src/lib/api.ts:
  Import ApiClient from @xs/shared.
  Export a singleton client instance configured from:
    - AsyncStorage or Expo SecureStore for base URL (user-configurable on first launch)
    - getAccessToken() reads from SecureStore key "access_token"
    - Attach Authorization: Bearer <token> header on every request
  Export useApi() hook returning the singleton.

apps/mobile/src/lib/auth.ts:
  saveTokens(access: string, refresh: string) — SecureStore.setItemAsync
  getAccessToken() -> string | null
  getRefreshToken() -> string | null
  clearTokens() — SecureStore.deleteItemAsync both keys
  On 401: call /auth/refresh automatically, save new tokens, retry original request.
    If refresh fails: clearTokens() and navigate to /login.

apps/mobile/src/app/_layout.tsx — root layout:
  Expo Router root layout. QueryClientProvider wrapping the app.
  On mount: check if access token exists in SecureStore.
    If yes → navigate to /(tabs)/library.
    If no → navigate to /login.

apps/mobile/src/app/login.tsx — login screen:
  Email + password TextInput fields (zinc-900 text, zinc-200 border, teal-600 focus ring).
  "Sign In" button (teal-600 background).
  On submit: call client.login(). Save tokens. Navigate to /(tabs)/library.
  Error state: show error message below form.

apps/mobile/src/app/(tabs)/_layout.tsx — tab navigator:
  Three tabs: Library (book icon), Search (search icon), Profile (person icon).

apps/mobile/src/app/(tabs)/library.tsx — library browse screen:
  useInfiniteQuery calling client.listBooks({ page, page_size: 30 }).
  FlashList (or FlatList) of BookCard components, 2-column grid.
  Pull-to-refresh: invalidate query.
  Loading: skeleton placeholders (zinc-200 animated shimmer).
  Empty state: "Your library is empty" with teal-600 icon.

apps/mobile/src/components/BookCard.tsx:
  Props: book: BookSummary.
  Cover image (expo-image for caching). Falls back to zinc-200 placeholder.
  Title (2-line truncation, zinc-900, Inter semibold 14px).
  Primary author (zinc-500, 12px).
  Tap → navigate to /book/[id].

apps/mobile/src/app/book/[id].tsx — book detail screen:
  useQuery calling client.getBook(id).
  Header: cover (120×180), title, authors, series badge if present.
  Metadata section: language, rating stars, document_type badge, tags as chips.
  Formats section: list of available formats with file size. "Download" button per format.
  "Read" button (teal-600, full width) — enabled when a downloaded file exists.
  AI section: same three-tab panel as web (Classify / Validate / Derive).
    Only render when getLlmHealth() returns enabled: true.
    Reuse same API methods from packages/shared client.

apps/mobile/src/__tests__/LibraryScreen.test.tsx:
  test_library_renders_book_cards — mock listBooks returning 2 books, assert both titles render
  test_library_pull_to_refresh — simulate pull-to-refresh, assert query invalidated
  test_empty_library_shows_state — mock empty list, assert empty state text visible

apps/mobile/src/__tests__/LoginScreen.test.tsx:
  test_login_success_navigates — mock client.login success, assert navigation to library
  test_login_error_shows_message — mock client.login throwing 401, assert error text rendered

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter @xs/mobile test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component/hook source.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    npx expo start --simulator &
    @Computer Use — open the iPhone 17 Pro simulator (iOS 26.4)
    Verify:
      - Login screen renders with username/password fields and a Sign In button
      - Signing in with test credentials navigates to the library grid
      - Library grid shows cover thumbnails with title and author below each card
      - Pull-to-refresh triggers a reload spinner
    Kill the Expo process when done.

When done, run:
  pnpm turbo build --filter=@xs/mobile 2>&1 | tail -20
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 2 — Offline Sync + Downloads

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md and docs/API.md. Now do Stage 2 of Phase 6.

Add offline library sync via Expo SQLite and book file downloads via expo-file-system.

Deliverables:

apps/mobile/src/lib/db.ts — local SQLite database:
  Open DB: SQLite.openDatabaseAsync("xcalibre_local.db").
  runMigrations(): CREATE TABLE IF NOT EXISTS:
    local_books (id TEXT PK, title TEXT, sort_title TEXT, authors_json TEXT,
      cover_url TEXT, has_cover INTEGER, language TEXT, rating INTEGER,
      document_type TEXT, series_json TEXT, last_modified TEXT, synced_at TEXT)
    local_sync_state (key TEXT PK, value TEXT)
    local_downloads (book_id TEXT, format TEXT, local_path TEXT, size_bytes INTEGER,
      downloaded_at TEXT, PRIMARY KEY (book_id, format))
  Export: db singleton, runMigrations()

apps/mobile/src/lib/sync.ts:
  syncLibrary(client: ApiClient, db: SQLiteDatabase):
    Read last_sync_at from local_sync_state.
    Call client.listBooks({ since: last_sync_at, page_size: 200 }) — paginate until
      all pages fetched.
    Upsert each book into local_books (INSERT OR REPLACE).
    Write new last_sync_at = now() to local_sync_state.
    Return { synced: number, total: number }.
  On network error: return { synced: 0, total: 0 } — never throw.

apps/mobile/src/lib/downloads.ts:
  downloadBook(client: ApiClient, db: SQLiteDatabase,
    bookId: string, format: string) -> { localPath: string }
    Build download URL: {baseUrl}/api/v1/books/{bookId}/formats/{format}/download
    Use expo-file-system FileSystem.downloadAsync() to
      documentDirectory + "books/{bookId}.{format.toLowerCase()}"
    On success: INSERT OR REPLACE into local_downloads.
    On failure: throw with message — caller handles UI feedback.
  getLocalPath(db: SQLiteDatabase, bookId: string, format: string)
    -> string | null
    SELECT local_path FROM local_downloads WHERE book_id=? AND format=?
  deleteDownload(db: SQLiteDatabase, bookId: string, format: string)
    FileSystem.deleteAsync(localPath, { idempotent: true })
    DELETE FROM local_downloads WHERE book_id=? AND format=?

Update apps/mobile/src/app/(tabs)/library.tsx:
  On mount: call syncLibrary() in background — do not block render.
  Show sync indicator (small spinner in header) while sync is running.
  When offline (no network): read from local_books instead of API.
    Use NetInfo to detect connectivity.

Update apps/mobile/src/app/book/[id].tsx:
  "Download" button calls downloadBook(). Show progress indicator during download.
  After download: button changes to "Downloaded ✓" + "Delete" option.
  "Read" button: enabled when getLocalPath returns non-null for EPUB or PDF.

apps/mobile/src/__tests__/Sync.test.ts:
  test_sync_upserts_books — mock client.listBooks, call syncLibrary,
    assert local_books rows created in in-memory SQLite
  test_sync_incremental — sync once, then sync again with since param,
    assert second call passes since= query param
  test_sync_survives_network_error — mock client throwing, assert syncLibrary
    returns { synced: 0 } without throwing

apps/mobile/src/__tests__/Downloads.test.ts:
  test_download_stores_path — mock FileSystem.downloadAsync, call downloadBook,
    assert local_downloads row created
  test_get_local_path_returns_null_when_not_downloaded — no row, assert null
  test_delete_removes_file_and_row — mock FileSystem.deleteAsync, call deleteDownload,
    assert row deleted

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter @xs/mobile test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component/hook source.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    npx expo start --simulator &
    @Computer Use — open the iPhone 17 Pro simulator (iOS 26.4)
    Verify:
      - Downloaded books show a local badge or indicator on the card
      - Tapping a downloaded book opens it without a network request
      - Download progress indicator appears while a book is downloading
      - Sync runs on app resume and updates the library grid
    Kill the Expo process when done.

When done, run:
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 3 — EPUB + PDF Readers + Reading Progress

**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/API.md and apps/mobile/src/lib/. Now do Stage 3 of Phase 6.

Add EPUB and PDF reading with reading position persistence and server sync.

Deliverables:

apps/mobile/src/app/reader/[id].tsx — reader entry point:
  Query param: format ("EPUB" or "PDF").
  Get local file path from getLocalPath(db, id, format).
  If no local file: show error "Book not downloaded" with back button.
  Route to EpubReaderScreen or PdfReaderScreen based on format.

apps/mobile/src/features/reader/EpubReaderScreen.tsx:
  Use foliojs-port (react-native-foliojs or equivalent Expo-compatible epub renderer).
  On load: fetch reading progress from server (GET /api/v1/progress/:bookId).
    If cfi exists: restore position.
  On page change: debounce 2s, then:
    Save progress locally (local_sync_state key "progress_{bookId}").
    POST /api/v1/progress/:bookId { cfi, percentage } — fire and forget.
  Full-screen layout. StatusBar hidden. Swipe left/right to turn pages.
  Tap center: toggle header (back button + title) with 300ms fade.
  Font toggle: Inter / Literata (stored in SecureStore "reader_font").
  Night mode toggle: invert colors in reader view (stored in SecureStore "reader_night").

apps/mobile/src/features/reader/PdfReaderScreen.tsx:
  Use expo-pdf (expo-document-viewer or react-native-pdf).
  On load: fetch reading progress, restore page number.
  On page change: debounce 2s, save + sync same pattern as EPUB.
  Full-screen. Page indicator overlay (tap center to show/hide).

apps/mobile/src/lib/progress.ts:
  saveProgress(client, db, bookId, formatId, data: { cfi?: string, page?: number,
    percentage: number })
    Write to local_sync_state. POST to server. Swallow errors silently.
  loadProgress(client, bookId) -> { cfi?: string, page?: number, percentage: number } | null
    GET /api/v1/progress/:bookId. Return null on any error.

Update apps/mobile/src/app/book/[id].tsx:
  "Read" button now navigates to /reader/[id]?format=EPUB (prefer EPUB over PDF).
  Show reading progress percentage under the Read button if > 0%.

apps/mobile/src/__tests__/Progress.test.ts:
  test_save_progress_posts_to_server — mock client POST, call saveProgress,
    assert POST called with correct body
  test_save_progress_survives_network_error — mock client throwing, call saveProgress,
    assert no throw
  test_load_progress_returns_null_on_error — mock client throwing, assert null returned

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter @xs/mobile test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component/hook source.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    npx expo start --simulator &
    @Computer Use — open the iPhone 17 Pro simulator (iOS 26.4)
    Verify:
      - Opening an EPUB book launches the reader with rendered text
      - Swipe left/right navigates between pages
      - Reading position persists — close and reopen the book, confirm it reopens at the same page
      - Opening a PDF book renders the first page correctly
      - PDF page position persists on close and reopen
    Kill the Expo process when done.

When done, run:
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 4 — EAS Build Config + Polish

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read apps/mobile/app.json. Now do Stage 4 of Phase 6.

Add EAS build configuration and final app polish.

Deliverables:

apps/mobile/eas.json:
  {
    "cli": { "version": ">= 7.0.0" },
    "build": {
      "development": {
        "developmentClient": true,
        "distribution": "internal",
        "ios": { "simulator": true }
      },
      "preview": {
        "distribution": "internal",
        "ios": { "simulator": false },
        "android": { "buildType": "apk" }
      },
      "production": {
        "ios": { "buildType": "release" },
        "android": { "buildType": "aab" }
      }
    },
    "submit": {
      "production": {}
    }
  }

apps/mobile/app.json updates:
  ios.bundleIdentifier: "com.xcalibre.library"
  android.package: "com.xcalibre.library"
  android.versionCode: 1
  version: "1.0.0"
  plugins: ["expo-router", "expo-secure-store", "expo-sqlite", "expo-file-system"]

apps/mobile/src/app/(tabs)/profile.tsx — profile screen:
  Show current user (useQuery calling client.getMe()).
  Server URL setting: TextInput + "Save" — persists to SecureStore "server_url".
    On save: reinitialise API client with new URL.
  "Sign Out" button: clearTokens() then navigate to /login.
  App version display (from app.json via expo-constants).

apps/mobile/assets/ — placeholder assets:
  icon.png — 1024×1024 zinc-900 background with white book icon (SVG converted)
  splash.png — 2048×2048 zinc-950 background with teal-600 "Xcalibre" wordmark centered
  adaptive-icon.png — 1024×1024 for Android adaptive icon
  Note: create simple solid-color placeholders if image generation is not available —
    the build must not fail due to missing assets.

apps/mobile/src/__tests__/ProfileScreen.test.tsx:
  test_profile_shows_username — mock getMe, assert username renders
  test_signout_clears_tokens_and_navigates — tap sign out, assert clearTokens called
    and navigation to /login triggered
  test_server_url_saves_to_secure_store — enter URL, tap save, assert SecureStore.setItemAsync
    called with key "server_url"

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter @xs/mobile test -- --reporter=verbose 2>&1

    If any test fails:
      1. Read the full error for that test.
      2. Read the component/hook source.
      3. Fix the test if the assertion was wrong, fix the source if the
         behavior was wrong. Never skip or .skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  VISUAL INSPECTION (after tests pass):
    npx expo start --simulator &
    @Computer Use — open the iPhone 17 Pro simulator (iOS 26.4)
    Verify:
      - App feels polished end-to-end: login → library → book detail → reader
      - No layout overflow or clipped text on iPhone 17 Pro screen size
      - Dark mode renders correctly throughout (library, detail, reader)
      - Book detail AI panel shows classify/validate/derive tabs (requires LLM enabled)
      - App icon and splash screen display correctly on launch
    Kill the Expo process when done.

When done, run:
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## Review Checkpoints

| After stage | What to verify |
|---|---|
| Stage 1 | Login screen uses SecureStore; library grid renders BookCard with cover; book detail shows document_type badge; AI panel absent when LLM disabled |
| Stage 2 | syncLibrary passes `since=` on incremental sync; download stores file at deterministic path; offline mode reads local_books when NetInfo offline |
| Stage 3 | EPUB reader restores CFI position on open; progress POST is fire-and-forget (errors swallowed); PDF reader tracks page not CFI |
| Stage 4 | eas.json has development/preview/production profiles; profile screen server URL persists to SecureStore; all 3 platform asset files present |

## If Codex Gets Stuck or a Test Fails

```
The following test is failing. Diagnose the root cause and fix it.
Do not work around it — fix the underlying issue.

[paste error output]
```

## Commit Sequence

```bash
# After Stage 1
git add -A && git commit -m "Phase 6 Stage 1: Expo setup, auth, library browse, book detail, 5/5 tests passing"

# After Stage 2
git add -A && git commit -m "Phase 6 Stage 2: offline sync, file downloads, incremental sync, 6/6 tests passing"

# After Stage 3
git add -A && git commit -m "Phase 6 Stage 3: EPUB + PDF readers, reading progress sync, 3/3 tests passing"

# After Stage 4
git add -A && git commit -m "Phase 6 Stage 4: EAS build config, profile screen, app assets, tests passing"
```
