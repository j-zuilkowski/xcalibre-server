# Phase 20 — Emby-Style UI Redesign

TDD rule (non-negotiable): for every new component or backend endpoint, write
the test first, run it to confirm failure, implement to make it pass. Never
write implementation before the test exists.

Every stage ends with a local git commit.

---

## Stage 1 — Backend: in-progress endpoint + document_type filter

**Paste this into Codex:**

```
Read backend/src/api/books.rs and backend/src/db/queries/books.rs.

─────────────────────────────────────────
STEP 1 — Write integration tests first
─────────────────────────────────────────

In backend/tests/ (following the TestContext pattern used by existing tests),
add a new test file backend/tests/test_browse.rs with two failing test stubs:

  #[tokio::test]
  async fn test_list_books_by_document_type() {
      // Create two books: one with document_type "Book", one "Reference"
      // GET /api/v1/books?document_type=Reference
      // Assert exactly one result with the Reference book
      todo!()
  }

  #[tokio::test]
  async fn test_in_progress_books_returns_started_books() {
      // Create a book, record reading_progress at 50%
      // GET /api/v1/books/in-progress
      // Assert the book appears in results
      // Create a second book with no reading progress
      // Assert the second book does NOT appear
      todo!()
  }

Run: cargo test --workspace
Confirm both new tests fail (todo!/endpoint not found). Expected red state.

─────────────────────────────────────────
STEP 2 — Implement document_type filter
─────────────────────────────────────────

In backend/src/api/books.rs, find the query extractor struct used by
list_books (the one with sort: Option<String>, order: Option<String>) and add:
  document_type: Option<String>,

Pass it through to ListBooksParams. In backend/src/db/queries/books.rs, add
document_type: Option<String> to ListBooksParams. In the list_books query
builder, add:
  if let Some(dt) = &params.document_type {
      qb.push(" AND b.document_type = ");
      qb.push_bind(dt.as_str());
  }

─────────────────────────────────────────
STEP 3 — Implement GET /api/v1/books/in-progress
─────────────────────────────────────────

Register a new route BEFORE /books/:id (so the literal path wins):
  /api/v1/books/in-progress  GET  list_in_progress_books

Implement the handler — query reading_progress for books where the
authenticated user has percentage > 0 AND percentage < 100, ordered by
updated_at DESC, limit 20. Return Vec<BookSummary>.

Add the utoipa #[utoipa::path(...)] doc comment following nearby handler style.

─────────────────────────────────────────
STEP 4 — Replace todo!() with real assertions, run to green
─────────────────────────────────────────

Fill in the two test bodies with real TestContext-based assertions.
Run: cargo test --workspace
Both tests must pass. All pre-existing tests must still pass.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

cargo clippy -- -D warnings

git add -A
git commit -m "feat: document_type filter + GET /api/v1/books/in-progress (Phase 20 Stage 1)"
```

---

## Stage 2 — Frontend: HomePage

**Paste this into Codex:**

```
Read apps/web/src/features/library/LibraryPage.tsx and
apps/web/src/test/handlers.ts to understand existing patterns.

─────────────────────────────────────────
STEP 1 — Write tests first
─────────────────────────────────────────

Create apps/web/src/features/library/HomePage.test.tsx with four failing
test stubs. Do NOT create HomePage.tsx yet.

  test('renders Continue Reading row when in-progress books exist', () => { throw new Error('not implemented') })
  test('hides Continue Reading row when no in-progress books', () => { throw new Error('not implemented') })
  test('always renders Recently Added row', () => { throw new Error('not implemented') })
  test('search hero navigates to /search on submit', () => { throw new Error('not implemented') })

Add MSW handlers to apps/web/src/test/handlers.ts:
  GET /api/v1/books/in-progress — returns [] by default (override per-test)
  GET /api/v1/books (with sort=created_at) — returns a list of BookSummary fixtures
  GET /api/v1/collections — returns []

Run: pnpm --filter @xs/web test run
Confirm the four new tests fail (HomePage does not exist). Expected red state.

─────────────────────────────────────────
STEP 2 — Add listInProgress to ApiClient
─────────────────────────────────────────

Find the ApiClient class in packages/shared/src/ (wherever listBooks and
other methods live). Add:

  async listInProgress(): Promise<BookSummary[]> {
    return this.get('/api/v1/books/in-progress');
  }

─────────────────────────────────────────
STEP 3 — Implement HomePage.tsx
─────────────────────────────────────────

Create apps/web/src/features/library/HomePage.tsx with three rows and a
hero search bar:

Hero search bar: full-width rounded input (h-12), placeholder from
t("home.search_placeholder"). On submit, navigate to /search?q=<value>
using TanStack Router useNavigate.

Row 1 — Continue Reading:
  useQuery calling apiClient.listInProgress()
  Hidden entirely if result is empty array.

Row 2 — Recently Added:
  useQuery calling apiClient.listBooks({ sort: "created_at", order: "desc", page_size: 20 })
  Always rendered.

Row 3 — Collections:
  useQuery calling apiClient.listCollections() (existing method)
  Hidden if empty.

Each row layout: <h2> heading + "See all >" link right-aligned, then a
horizontally scrollable flex container (overflow-x-auto flex gap-4 pb-2)
with fixed-width MediaCard items (w-32 shrink-0 md:w-40).

Use the MediaCard component (to be created in Stage 4). For now, import it
and use BookCard as a placeholder if MediaCard doesn't exist yet — Stage 4
will swap it in.

Add i18n keys to all four locale files:
  "home": {
    "continue_reading": "Continue Reading",
    "recently_added": "Recently Added",
    "collections": "Collections",
    "search_placeholder": "Search books, authors, topics…",
    "see_all": "See all"
  }

─────────────────────────────────────────
STEP 4 — Fill in tests, run to green
─────────────────────────────────────────

Fill in the four test bodies using React Testing Library and MSW.
Per-test MSW handler overrides for Continue Reading: one test uses a
handler that returns one BookSummary, another returns [].

Run: pnpm --filter @xs/web test run
All four new tests plus all pre-existing tests must pass.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

git add -A
git commit -m "feat: HomePage — Continue Reading, Recently Added, Collections rows (Phase 20 Stage 2)"
```

---

## Stage 3 — Frontend: BrowsePage — category grid + alphabetical sidebar

**Paste this into Codex:**

```
Read apps/web/src/features/library/LibraryPage.tsx and
apps/web/src/test/handlers.ts.

─────────────────────────────────────────
STEP 1 — Write tests first
─────────────────────────────────────────

Create apps/web/src/features/library/BrowsePage.test.tsx with three failing
test stubs. Do NOT create BrowsePage.tsx yet.

  test('renders grid of books for the given documentType', () => { throw new Error('not implemented') })
  test('alpha sidebar renders clickable buttons only for letters that have books', () => { throw new Error('not implemented') })
  test('letters with no books render as non-interactive', () => { throw new Error('not implemented') })

Add MSW handler to handlers.ts if not already present:
  GET /api/v1/books (with document_type param) — return fixture books whose
  titles start with "A", "B", and "Z" to test the alpha sidebar logic.

Run: pnpm --filter @xs/web test run
Confirm the three new tests fail. Expected red state.

─────────────────────────────────────────
STEP 2 — Implement BrowsePage.tsx
─────────────────────────────────────────

Create apps/web/src/features/library/BrowsePage.tsx.

Props: type BrowsePageProps = { documentType: string }

Data: useQuery calling
  apiClient.listBooks({ document_type: documentType, sort: "title", order: "asc", page_size: 200 })

Layout:
  Left sidebar (w-10 shrink-0, fixed or sticky):
    Buttons for "#" + A–Z. Each button scrolls to <div id="alpha-<letter>">
    using element.scrollIntoView({ behavior: 'smooth' }). Letters with no
    matching books get opacity-40 and pointer-events-none classes.

  Right content area:
    Books grouped by first letter of title. Each group:
      <div id="alpha-<letter>"> heading + responsive grid
      (grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 gap-4)
    Use existing BookCard for each book.

Add i18n keys to all four locale files:
  "browse": {
    "books": "Books",
    "reference": "Reference",
    "periodicals": "Periodicals",
    "magazines": "Magazines",
    "no_results": "No books in this category yet."
  }

─────────────────────────────────────────
STEP 3 — Fill in tests, run to green
─────────────────────────────────────────

Fill in the three test bodies. For the alpha sidebar tests, use the fixture
that returns books starting with A, B, and Z — assert A/B/Z buttons are
active, C–Y buttons are non-interactive.

Run: pnpm --filter @xs/web test run
All three new tests plus all pre-existing tests must pass.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

git add -A
git commit -m "feat: BrowsePage — category grid + alphabetical sidebar (Phase 20 Stage 3)"
```

---

## Stage 4 — Frontend: MediaCard — compact cover card for scroll rows

**Paste this into Codex:**

```
Read apps/web/src/features/library/BookCard.tsx and
apps/web/src/features/library/CoverPlaceholder.tsx.

─────────────────────────────────────────
STEP 1 — Write tests first
─────────────────────────────────────────

Create apps/web/src/features/library/MediaCard.test.tsx with four failing
test stubs. Do NOT create MediaCard.tsx yet.

  test('renders cover image when book.has_cover is true', () => { throw new Error('not implemented') })
  test('renders CoverPlaceholder when book.has_cover is false', () => { throw new Error('not implemented') })
  test('renders progress bar when progressPercentage > 0', () => { throw new Error('not implemented') })
  test('progress bar is absent when progressPercentage is 0', () => { throw new Error('not implemented') })

Run: pnpm --filter @xs/web test run
Confirm the four new tests fail. Expected red state.

─────────────────────────────────────────
STEP 2 — Implement MediaCard.tsx
─────────────────────────────────────────

Create apps/web/src/features/library/MediaCard.tsx.

Props:
  type MediaCardProps = {
    book: BookSummary;
    progressPercentage?: number;
  };

Layout (cover-dominant, minimal):
  - Entire card is an <a href="/books/<id>"> (tabIndex={0}, keyboard Enter handler)
  - Cover: aspect-[2/3] w-full rounded-xl shadow-md object-cover, or CoverPlaceholder
  - Progress strip: if progressPercentage > 0, an absolutely positioned h-1
    strip at the bottom of the cover container, bg-teal-500,
    width = progressPercentage + '%'. Always visible (not hover-only).
  - Title: line-clamp-2 text-xs font-semibold text-zinc-900 mt-2
  - No author line, no overlay buttons, no archive button

─────────────────────────────────────────
STEP 3 — Fill in tests, run to green
─────────────────────────────────────────

Fill in the four test bodies using React Testing Library and the existing
BookSummary fixture from apps/web/src/test/fixtures.ts.

Run: pnpm --filter @xs/web test run
All four new tests plus all pre-existing tests must pass.

─────────────────────────────────────────
STEP 4 — Swap BookCard for MediaCard in HomePage
─────────────────────────────────────────

Update apps/web/src/features/library/HomePage.tsx — replace the BookCard
imports in the Continue Reading and Recently Added rows with MediaCard.
BrowsePage continues to use BookCard (full hover overlay card).

Re-run tests. All must still pass.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

git add -A
git commit -m "feat: MediaCard — compact cover card for home scroll rows (Phase 20 Stage 4)"
```

---

## Stage 5 — Frontend: AppShell nav + routes + search page

**Paste this into Codex:**

```
Read apps/web/src/components/AppShell.tsx and apps/web/src/router.tsx.

─────────────────────────────────────────
STEP 1 — Write tests first
─────────────────────────────────────────

In apps/web/src/__tests__/ (or alongside AppShell), add to the AppShell
test file (or create one if it doesn't exist) two failing tests:

  test('sidebar contains a Home nav link', () => { throw new Error('not implemented') })
  test('sidebar contains Browse category links', () => { throw new Error('not implemented') })

Run: pnpm --filter @xs/web test run
Confirm these fail. Expected red state.

─────────────────────────────────────────
STEP 2 — Update AppShell nav
─────────────────────────────────────────

In apps/web/src/components/AppShell.tsx, replace the nav items array with:

  [
    { to: "/home",               label: t("nav.home"),           icon: "⌂" },
    { to: "/browse/books",       label: t("browse.books"),       icon: "B" },
    { to: "/browse/reference",   label: t("browse.reference"),   icon: "R" },
    { to: "/browse/periodicals", label: t("browse.periodicals"), icon: "P" },
    { to: "/browse/magazines",   label: t("browse.magazines"),   icon: "M" },
    { to: "/shelves",            label: t("nav.shelves"),        icon: "H" },
    { to: "/search",             label: t("nav.search"),         icon: "S" },
    { to: "/downloads",          label: t("nav.downloads"),      icon: "D" },
  ]

Add "nav.home": "Home" to all four locale files.

─────────────────────────────────────────
STEP 3 — Add routes to router.tsx
─────────────────────────────────────────

In apps/web/src/router.tsx:

Import HomePage and BrowsePage.

Add homeRoute:
  const homeRoute = createRoute({
    getParentRoute: () => protectedRoute,
    path: "home",
    component: HomePage,
  });

Add four browse routes (inline arrow wrapper for documentType prop):
  const browseBookRoute = createRoute({
    getParentRoute: () => protectedRoute,
    path: "browse/books",
    component: () => <BrowsePage documentType="Book" />,
  });
  // repeat for reference ("Reference"), periodicals ("Periodical"), magazines ("Magazine")

Add index redirect route (/ → /home):
  import { useEffect } from "react";
  const indexRoute = createRoute({
    getParentRoute: () => protectedRoute,
    path: "/",
    component: () => {
      const navigate = useNavigate();
      useEffect(() => { void navigate({ to: "/home", replace: true }); }, [navigate]);
      return null;
    },
  });

Add homeRoute, indexRoute, and all four browseRoutes to routeTree.addChildren([...]).
Keep the existing /library route intact.

─────────────────────────────────────────
STEP 4 — Search page grid refresh
─────────────────────────────────────────

In apps/web/src/features/search/SearchPage.tsx, update the results grid
class to match BrowsePage:
  grid grid-cols-3 sm:grid-cols-4 md:grid-cols-5 lg:grid-cols-6 gap-4

─────────────────────────────────────────
STEP 5 — Fill in tests, run to green
─────────────────────────────────────────

Fill in the two AppShell nav tests. Run:
  pnpm --filter @xs/web test run
All tests must pass.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

pnpm --filter @xs/web build
Fix any TypeScript errors (missing imports in router.tsx, etc.).

git add -A
git commit -m "feat: AppShell nav + home/browse routes + search grid refresh (Phase 20 Stage 5)"
```

---

## Stage 6 — Wire-up, locale sweep, STATE.md, final commit

**Paste this into Codex:**

```
Perform final integration and cleanup for Phase 20.

─────────────────────────────────────────
STEP 1 — Full test run
─────────────────────────────────────────

cargo test --workspace         (all Rust tests must pass)
cargo clippy -- -D warnings    (zero warnings)
pnpm --filter @xs/web test run (all vitest tests must pass)

─────────────────────────────────────────
STEP 2 — Locale sweep
─────────────────────────────────────────

Verify all four locale files contain every key introduced in Stages 2–5:
  home.continue_reading, home.recently_added, home.collections,
  home.search_placeholder, home.see_all,
  browse.books, browse.reference, browse.periodicals, browse.magazines,
  browse.no_results, nav.home

For non-English locales (fr, de, es): if a translation is not yet available,
use the English value as a placeholder with a TODO comment in the JSON.

─────────────────────────────────────────
STEP 3 — STATE.md update
─────────────────────────────────────────

Update docs/STATE.md:
  - Overall status: Phase 20 Complete
  - Phase 20 row: ✅ Complete
  - Update Last updated date to today

─────────────────────────────────────────
STEP 4 — Commit and tag
─────────────────────────────────────────

git add -A
git commit -m "chore: Phase 20 complete — locale sweep, STATE.md update"
git tag -a v2.1.0 -m "Phase 20 complete — Emby-style UI redesign"
```

---

_Phase 20 complete when all 6 stages are committed, `cargo test --workspace`
and `pnpm --filter @xs/web test run` both pass, and v2.1.0 is tagged._
