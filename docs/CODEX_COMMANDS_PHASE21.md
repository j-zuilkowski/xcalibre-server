# Phase 21 — Metadata Enrichment (Identify)

Fetch cover art, description, and bibliographic metadata from Google Books
and Open Library for any book in the library. Equivalent to Emby's "Identify"
feature. No new database migration needed — ISBNs and external IDs are stored
in the existing `identifiers` table.

TDD rule (non-negotiable): tests are written first and run to confirm failure,
then implementation is written to make them pass. Never write implementation
before the test exists.

Every stage ends with a local git commit.

---

## Stage 1 — Backend: metadata client module

**Paste this into Codex:**

```
Read backend/src/lib.rs and backend/src/llm/chat.rs to understand how
the existing HTTP clients and modules are structured.

─────────────────────────────────────────
STEP 1 — Write unit tests first
─────────────────────────────────────────

Create backend/src/metadata/mod.rs, backend/src/metadata/google_books.rs,
and backend/src/metadata/open_library.rs as empty stubs so the module
compiles. Add pub mod metadata; to backend/src/lib.rs.

In backend/src/metadata/mod.rs define the MetadataCandidate struct:

  #[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
  pub struct MetadataCandidate {
      pub source: String,        // "google_books" or "open_library"
      pub external_id: String,
      pub title: String,
      pub authors: Vec<String>,
      pub description: Option<String>,
      pub publisher: Option<String>,
      pub published_date: Option<String>,
      pub isbn_13: Option<String>,
      pub isbn_10: Option<String>,
      pub thumbnail_url: Option<String>,  // shown in picker UI
      pub cover_url: Option<String>,      // downloaded on apply
  }

Now create backend/src/metadata/google_books.rs and
backend/src/metadata/open_library.rs with the public search function stub:

  pub async fn search(_query: &str) -> anyhow::Result<Vec<MetadataCandidate>> {
      todo!()
  }

Add unit tests at the bottom of each file using the #[cfg(test)] block:

  In google_books.rs:
    #[tokio::test]
    async fn test_google_books_search_returns_vec() {
        // Call search("Dune Herbert") — this will hit the live API in test
        // or return empty on network error. Assert Ok(_) is returned (no panic).
        // Assert result is a Vec (may be empty in sandbox — that is acceptable).
        let result = search("Dune Herbert").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_google_books_strips_edge_curl_param() {
        // Given a thumbnail URL with &edge=curl, assert it is stripped
        let url = "https://books.google.com/thumbnail?zoom=1&edge=curl";
        let cleaned = strip_edge_curl(url);
        assert!(!cleaned.contains("edge=curl"));
    }

    #[test]
    fn test_google_books_upgrades_http_to_https() {
        let url = "http://books.google.com/thumbnail?zoom=1";
        let upgraded = upgrade_to_https(url);
        assert!(upgraded.starts_with("https://"));
    }

  In open_library.rs:
    #[tokio::test]
    async fn test_open_library_search_returns_vec() {
        let result = search("Lord of the Rings Tolkien").await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_library_cover_url_format() {
        let url = cover_url_for_id(8406786);
        assert_eq!(url, "https://covers.openlibrary.org/b/id/8406786-L.jpg");
    }

Run: cargo test --workspace
Confirm the new tests fail with "not yet implemented" (todo!()) or compile
errors (missing helper fns). Expected red state.

─────────────────────────────────────────
STEP 2 — Implement Google Books client
─────────────────────────────────────────

Implement backend/src/metadata/google_books.rs:

  Helper functions (used by tests above):
    fn strip_edge_curl(url: &str) -> String {
        url.replace("&edge=curl", "").replace("edge=curl&", "")
    }
    fn upgrade_to_https(url: &str) -> String {
        if url.starts_with("http://") { url.replacen("http://", "https://", 1) }
        else { url.to_string() }
    }

  pub async fn search(query: &str) -> anyhow::Result<Vec<MetadataCandidate>>:
    URL: https://www.googleapis.com/books/v1/volumes?q=<encoded_query>&maxResults=10&printType=books
    Encode spaces in query as + (simple replace or percent-encode).
    Use reqwest::Client with a 10-second timeout.
    Parse response as serde_json::Value.
    For each item in items array:
      - source = "google_books"
      - external_id = item["id"].as_str()
      - title, authors, description, publisher, publishedDate from volumeInfo
      - industryIdentifiers → isbn_13 (type "ISBN_13"), isbn_10 (type "ISBN_10")
      - thumbnail from imageLinks.thumbnail:
          strip_edge_curl → upgrade_to_https → that is thumbnail_url
          cover_url = thumbnail_url with "zoom=1" replaced by "zoom=3"
    On any HTTP error or parse failure: return Ok(vec![]) — silent fallback.

─────────────────────────────────────────
STEP 3 — Implement Open Library client
─────────────────────────────────────────

Implement backend/src/metadata/open_library.rs:

  fn cover_url_for_id(cover_i: i64) -> String {
      format!("https://covers.openlibrary.org/b/id/{cover_i}-L.jpg")
  }

  pub async fn search(query: &str) -> anyhow::Result<Vec<MetadataCandidate>>:
    URL: https://openlibrary.org/search.json?q=<encoded_query>&limit=10&fields=key,title,author_name,first_publish_year,isbn,cover_i,publisher
    For each doc:
      - source = "open_library"
      - external_id = doc["key"].as_str() (e.g. "/works/OL45804W")
      - published_date = doc["first_publish_year"].as_i64().map(|y| y.to_string())
      - cover: if cover_i present, thumbnail_url and cover_url = cover_url_for_id(cover_i)
        (thumbnail uses -M.jpg, cover uses -L.jpg)
      - isbn: from isbn array — 13-digit entries → isbn_13, 10-digit → isbn_10
      - publisher: first element of publisher array if present
    On HTTP error: return Ok(vec![]).

─────────────────────────────────────────
STEP 4 — Run tests to green
─────────────────────────────────────────

cargo test --workspace
The helper function unit tests must pass. The async search tests may return
empty results in a sandboxed environment — assert only Ok(_), not content.

cargo clippy -- -D warnings

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

git add -A
git commit -m "feat: metadata module — Google Books + Open Library clients (Phase 21 Stage 1)"
```

---

## Stage 2 — Backend: GET /api/v1/books/{id}/metadata/search

**Paste this into Codex:**

```
Read backend/src/api/books.rs and backend/src/metadata/mod.rs.

─────────────────────────────────────────
STEP 1 — Write integration test first
─────────────────────────────────────────

In backend/tests/ add to an existing test file or create
backend/tests/test_metadata.rs:

  #[tokio::test]
  async fn test_metadata_search_returns_200_for_existing_book() {
      // Create a book via TestContext
      // GET /api/v1/books/<id>/metadata/search?q=Dune
      // Assert 200 and response body is a valid JSON array
      // (may be empty in sandbox — assert status only, not content)
      todo!()
  }

  #[tokio::test]
  async fn test_metadata_search_returns_404_for_unknown_book() {
      // GET /api/v1/books/nonexistent-id/metadata/search?q=anything
      // Assert 404
      todo!()
  }

Run: cargo test --workspace
Confirm both tests fail (endpoint not found). Expected red state.

─────────────────────────────────────────
STEP 2 — Implement the endpoint
─────────────────────────────────────────

In backend/src/api/books.rs, register:
  GET /api/v1/books/:id/metadata/search  →  search_book_metadata

Query extractor:
  #[derive(Deserialize)]
  struct MetadataSearchQuery { q: Option<String> }

Handler logic:
  1. Fetch book — return 404 if not found.
  2. Build query string: use q if provided and non-empty, otherwise
     "<title> <first_author_name>".
  3. Call both clients concurrently:
       let (google, ol) = tokio::join!(
           crate::metadata::google_books::search(&query),
           crate::metadata::open_library::search(&query),
       );
  4. Merge by interleaving (google[0], ol[0], google[1], ol[1], …), up to 20.
  5. Return Json(merged_candidates).

Add utoipa #[utoipa::path(...)] doc comment.

─────────────────────────────────────────
STEP 3 — Fill in tests, run to green
─────────────────────────────────────────

Replace todo!() with real TestContext-based assertions.

Run: cargo test --workspace
Both new tests plus all pre-existing tests must pass.

cargo clippy -- -D warnings

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

git add -A
git commit -m "feat: GET /api/v1/books/{id}/metadata/search (Phase 21 Stage 2)"
```

---

## Stage 3 — Backend: POST /api/v1/books/{id}/metadata/apply

**Paste this into Codex:**

```
Read backend/src/api/books.rs — find how cover upload and patch_book are
handled in existing endpoints to follow the same pattern exactly.

─────────────────────────────────────────
STEP 1 — Write integration tests first
─────────────────────────────────────────

Add to backend/tests/test_metadata.rs:

  #[tokio::test]
  async fn test_metadata_apply_updates_title_and_description() {
      // Create a book via TestContext
      // POST /api/v1/books/<id>/metadata/apply
      //   body: { source: "google_books", external_id: "abc", title: "New Title",
      //           description: "A great book.", authors: null, publisher: null,
      //           published_date: null, isbn_13: null, isbn_10: null, cover_url: null }
      // Assert 200 and returned book has title "New Title" and description set
      todo!()
  }

  #[tokio::test]
  async fn test_metadata_apply_stores_external_id_as_identifier() {
      // Create a book, apply metadata with source "google_books", external_id "vol123"
      // GET /api/v1/books/<id>
      // Assert identifiers array contains { id_type: "google_books", value: "vol123" }
      todo!()
  }

  #[tokio::test]
  async fn test_metadata_apply_requires_can_edit_permission() {
      // Create a read-only user (no can_edit)
      // POST /api/v1/books/<id>/metadata/apply as that user
      // Assert 403
      todo!()
  }

Run: cargo test --workspace
Confirm all three fail. Expected red state.

─────────────────────────────────────────
STEP 2 — Implement the endpoint
─────────────────────────────────────────

In backend/src/api/books.rs, register:
  POST /api/v1/books/:id/metadata/apply  →  apply_book_metadata

Request body:
  #[derive(Deserialize)]
  struct ApplyMetadataBody {
      source: String,
      external_id: String,
      title: Option<String>,
      authors: Option<Vec<String>>,
      description: Option<String>,
      publisher: Option<String>,
      published_date: Option<String>,
      isbn_13: Option<String>,
      isbn_10: Option<String>,
      cover_url: Option<String>,
  }

Handler logic:
  1. Check can_edit permission via role_permissions_for_user — return 403 if lacking.
  2. Verify book exists — 404 if not.
  3. Patch text fields via queries::books::patch_book with PatchBookInput
     (title, description, pubdate from published_date).
  4. Update publisher if provided (use the existing update_book_publisher fn;
     if it is not pub, add a pub wrapper in queries/books.rs).
  5. Upsert external ID into identifiers table:
       INSERT INTO identifiers (id, book_id, id_type, value) VALUES (?,?,?,?)
       ON CONFLICT(book_id, id_type) DO UPDATE SET value = excluded.value
     id_type = body.source.as_str() (e.g. "google_books" or "open_library").
  6. Upsert isbn_13 and isbn_10 similarly (id_type = "isbn_13" / "isbn_10").
  7. Download cover if cover_url provided:
       Use reqwest with 15s timeout. On success, store via state.storage.
       Follow the exact same pattern as the existing cover upload handler.
       On failure, log and continue — do not fail the whole request.
       If cover downloaded successfully, UPDATE books SET has_cover = 1.
  8. Return Json(updated_book) from get_book().

let now = chrono::Utc::now().to_rfc3339();

Add utoipa doc comment.

─────────────────────────────────────────
STEP 3 — Fill in tests, run to green
─────────────────────────────────────────

Replace todo!() with real TestContext-based assertions.

Run: cargo test --workspace
All three new tests plus all pre-existing tests must pass.

cargo clippy -- -D warnings

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

git add -A
git commit -m "feat: POST /api/v1/books/{id}/metadata/apply — cover + metadata write-back (Phase 21 Stage 3)"
```

---

## Stage 4 — Frontend: IdentifyModal component

**Paste this into Codex:**

```
Read apps/web/src/components/ui/Dialog.tsx and
apps/web/src/test/handlers.ts.

─────────────────────────────────────────
STEP 1 — Add types and ApiClient methods
─────────────────────────────────────────

In packages/shared/src/types.ts (or wherever BookSummary and Book are
defined), add:

  export type MetadataCandidate = {
    source: string;
    external_id: string;
    title: string;
    authors: string[];
    description: string | null;
    publisher: string | null;
    published_date: string | null;
    isbn_13: string | null;
    isbn_10: string | null;
    thumbnail_url: string | null;
    cover_url: string | null;
  };

  export type ApplyMetadataBody = {
    source: string;
    external_id: string;
    title?: string;
    authors?: string[];
    description?: string;
    publisher?: string;
    published_date?: string;
    isbn_13?: string;
    isbn_10?: string;
    cover_url?: string;
  };

In the ApiClient class, add:
  async searchBookMetadata(bookId: string, q: string): Promise<MetadataCandidate[]>
    → GET /api/v1/books/<bookId>/metadata/search?q=<q>

  async applyBookMetadata(bookId: string, body: ApplyMetadataBody): Promise<Book>
    → POST /api/v1/books/<bookId>/metadata/apply with JSON body

─────────────────────────────────────────
STEP 2 — Write tests first
─────────────────────────────────────────

Add MSW handlers to apps/web/src/test/handlers.ts:

  http.get('/api/v1/books/:id/metadata/search', () =>
    HttpResponse.json([
      {
        source: 'google_books', external_id: 'vol123',
        title: 'Identified Book', authors: ['Test Author'],
        description: 'A found book.', publisher: 'Publisher',
        published_date: '2020', isbn_13: null, isbn_10: null,
        thumbnail_url: null, cover_url: null,
      },
    ])
  ),
  http.post('/api/v1/books/:id/metadata/apply', () =>
    HttpResponse.json({ /* use existing book fixture */ })
  ),

Create apps/web/src/features/library/IdentifyModal.test.tsx with four
failing test stubs. Do NOT create IdentifyModal.tsx yet.

  test('renders with search query prefilled from book title and author', () => { throw new Error('not implemented') })
  test('clicking Search triggers searchBookMetadata API call', () => { throw new Error('not implemented') })
  test('candidate list renders after search response', () => { throw new Error('not implemented') })
  test('clicking Apply calls applyBookMetadata and invokes onApplied', () => { throw new Error('not implemented') })

Run: pnpm --filter @xs/web test run
Confirm all four fail. Expected red state.

─────────────────────────────────────────
STEP 3 — Implement IdentifyModal.tsx
─────────────────────────────────────────

Create apps/web/src/features/library/IdentifyModal.tsx.

Props:
  type IdentifyModalProps = {
    book: Book;
    onClose: () => void;
    onApplied: () => void;
  };

State: query (prefilled: book.title + " " + first author name), candidates,
isSearching, applyingId.

Phase 1 — Search form (inside Dialog):
  Text input for query, "Search" button.
  On submit: call apiClient.searchBookMetadata(book.id, query), set candidates.
  Show spinner during search.

Phase 2 — Results list:
  For each candidate:
    - 40×60 thumbnail (gray box if no thumbnail_url)
    - Title + authors + year in middle
    - Source badge: "Google Books" or "Open Library"
    - "Apply" button — on click: applyBookMetadata with all candidate fields,
      then onApplied() then onClose(). Show loading state during apply.

Use the existing Dialog component from apps/web/src/components/ui/Dialog.tsx.

Add i18n keys to all four locale files:
  "identify": {
    "title": "Identify Book",
    "search_label": "Search query",
    "search_button": "Search",
    "searching": "Searching…",
    "no_results": "No results found. Try a different query.",
    "apply": "Apply",
    "applying": "Applying…",
    "source_google": "Google Books",
    "source_open_library": "Open Library"
  }

─────────────────────────────────────────
STEP 4 — Fill in tests, run to green
─────────────────────────────────────────

Fill in the four test bodies using React Testing Library, MSW handlers, and
the existing Book fixture from apps/web/src/test/fixtures.ts.

Run: pnpm --filter @xs/web test run
All four new tests plus all pre-existing tests must pass.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

git add -A
git commit -m "feat: IdentifyModal — Google Books + Open Library metadata picker (Phase 21 Stage 4)"
```

---

## Stage 5 — Frontend: wire Identify into BookDetailPage

**Paste this into Codex:**

```
Read apps/web/src/features/library/BookDetailPage.tsx and its test file
apps/web/src/features/library/BookDetailPage.test.tsx.

─────────────────────────────────────────
STEP 1 — Write test first
─────────────────────────────────────────

In apps/web/src/features/library/BookDetailPage.test.tsx, add one failing
test:

  test('Identify button is visible for admin users and opens the modal', () => {
    throw new Error('not implemented')
  })

Run: pnpm --filter @xs/web test run
Confirm it fails. Expected red state.

─────────────────────────────────────────
STEP 2 — Wire IdentifyModal into BookDetailPage
─────────────────────────────────────────

In BookDetailPage.tsx:
  - Add useState: const [identifyOpen, setIdentifyOpen] = useState(false)
  - Find the action buttons area (alongside Edit, Download, etc.)
  - Add an "Identify…" button that only renders if the current user has
    can_edit permission (use whatever role-check pattern BookDetailPage
    already uses for edit-gated actions)
  - Below the action buttons, conditionally render:
      {identifyOpen && book ? (
        <IdentifyModal
          book={book}
          onClose={() => setIdentifyOpen(false)}
          onApplied={() => {
            void queryClient.invalidateQueries({ queryKey: ["book", bookId] });
          }}
        />
      ) : null}

book must be the fully hydrated Book type (not BookSummary).

─────────────────────────────────────────
STEP 3 — Fill in test, run to green
─────────────────────────────────────────

Fill in the test: render BookDetailPage with an admin user fixture and a
book fixture. Assert the "Identify…" button is present. Click it — assert
the modal opens (the "Identify Book" heading is visible).

Run: pnpm --filter @xs/web test run
All tests must pass.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

git add -A
git commit -m "feat: Identify button wired into BookDetailPage (Phase 21 Stage 5)"
```

---

## Stage 6 — Integration, STATE.md, final commit

**Paste this into Codex:**

```
Perform final integration and cleanup for Phase 21.

─────────────────────────────────────────
STEP 1 — Full test suite
─────────────────────────────────────────

cargo test --workspace        (all Rust tests must pass)
cargo clippy -- -D warnings   (zero warnings)
pnpm --filter @xs/web build   (no TypeScript errors)
pnpm --filter @xs/web test run (all vitest tests must pass)

─────────────────────────────────────────
STEP 2 — Locale sweep
─────────────────────────────────────────

Verify all four locale files contain every identify.* key added in Stage 4.
Add any missing keys to non-English locales using the English value as placeholder.

─────────────────────────────────────────
STEP 3 — STATE.md update
─────────────────────────────────────────

Update docs/STATE.md:
  - Overall status: Phase 21 Complete
  - Phase 21 row: ✅ Complete
  - Add to Open Items:
    "Metadata: Google Books free tier is 1,000 req/day per IP — add optional
     google_books_api_key config field under [metadata] for higher quota"
    "Metadata i18n: identify.* locale keys use English placeholder for FR/DE/ES"
  - Update Last updated date to today

─────────────────────────────────────────
STEP 4 — Commit and tag
─────────────────────────────────────────

git add -A
git commit -m "chore: Phase 21 complete — locale sweep, STATE.md update"
git tag -a v2.2.0 -m "Phase 21 complete — Google Books + Open Library Identify feature"
```

---

_Phase 21 complete when all 6 stages are committed, `cargo test --workspace`
and `pnpm --filter @xs/web test run` both pass, and v2.2.0 is tagged._
