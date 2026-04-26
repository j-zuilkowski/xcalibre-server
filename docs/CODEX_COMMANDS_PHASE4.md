# Codex Desktop App — calibre-web-rs Phase 4: Search

## What Phase 4 Builds

Upgrades search from the current LIKE-based query to a proper full-text search stack
with an optional Meilisearch tier and LLM-powered semantic search:

- SQLite FTS5 virtual table — fast full-text search, no external service required
- `SearchBackend` trait — routes queries to FTS5 or Meilisearch transparently
- Meilisearch integration — optional, typo-tolerant, upgrades search quality when available
- Semantic search — sqlite-vec + LLM `/v1/embeddings` endpoint, gated behind `llm.enabled`
- Frontend wiring — SearchPage and SearchBar hit the real search endpoint; Semantic tab
  enables dynamically based on backend capability report

## Key Schema Facts (already in 0001_initial.sql — do not recreate)

- `llm_jobs` table: types include `semantic_index`; status: pending/running/completed/failed
- `book_embeddings(book_id PK, model_id, embedding BLOB, created_at)` — stores dense vectors
- `llm_eval_results` table — used in Phase 5, do not touch here
- Current `list_books` uses LIKE on title/author/tag — will be replaced with FTS5 MATCH

## Reference Files

Read these before starting each stage:
- `docs/ARCHITECTURE.md` — search stack decisions (FTS5 fallback, Meilisearch optional,
  sqlite-vec for semantic, graceful degradation pattern)
- `docs/API.md` — existing endpoint contracts; Phase 4 adds `/api/v1/search`,
  `/api/v1/search/suggestions`, `/api/v1/system/search-status`

---

## STAGE 1 — FTS5 Full-Text Search

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md. Now do Stage 1 of Phase 4.

The current list_books query uses LIKE on title/author/tag. Replace it with
SQLite FTS5. Do not change the list_books API contract — the q param still works
the same way for callers; only the implementation changes.

Deliverables:

backend/migrations/sqlite/0002_fts.sql:
  CREATE VIRTUAL TABLE books_fts USING fts5(
    book_id UNINDEXED,
    title,
    authors,
    tags,
    series,
    content='',
    tokenize='unicode61 remove_diacritics 1'
  );
  Populate from existing books on migration.
  Triggers: keep books_fts in sync on INSERT/UPDATE/DELETE to books,
  book_authors, book_tags, book_series (re-build the row's concatenated fields).

backend/migrations/mariadb/: skip — mariadb uses FULLTEXT, out of scope here.

backend/src/db/queries/books.rs — update list_books:
  When params.q is Some and non-empty:
    Use FTS5 MATCH query against books_fts instead of LIKE.
    FTS5 query: sanitize user input (strip special chars except spaces and *),
    append * for prefix matching.
    JOIN books_fts ON books_fts.book_id = b.id.
  When params.q is None or empty: behaviour unchanged (no FTS join).

backend/tests/fts_search.rs — new integration test file:
  test_fts_search_finds_by_title
  test_fts_search_finds_by_author
  test_fts_search_prefix_match
  test_fts_search_empty_query_returns_all

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test fts_search -- --nocapture 2>&1

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file (db/queries/search.rs, api/search.rs).
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 2 — Search API Routes + SearchBackend Trait

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md and docs/API.md. Now do Stage 2 of Phase 4.

Build the SearchBackend trait and the three new API endpoints. These are separate
from the existing GET /api/v1/books — the new search endpoint returns a score field
and is designed for Meilisearch to slot in later without changing the API contract.

Deliverables:

backend/src/search/mod.rs — SearchBackend trait:
  pub struct SearchQuery {
    pub q: String,
    pub author_id: Option<String>,
    pub tag: Option<String>,
    pub language: Option<String>,
    pub format: Option<String>,
    pub page: u32,
    pub page_size: u32,
  }

  pub struct SearchHit {
    pub book_id: String,
    pub score: f32,    // 0.0–1.0; FTS5 uses rank, Meilisearch uses its own score
  }

  pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
  }

  #[async_trait]
  pub trait SearchBackend: Send + Sync {
    async fn search(&self, query: &SearchQuery) -> anyhow::Result<SearchPage>;
    async fn suggest(&self, q: &str, limit: u8) -> anyhow::Result<Vec<String>>;
    async fn is_available(&self) -> bool;
    fn backend_name(&self) -> &'static str;
  }

backend/src/search/fts5.rs — Fts5Backend:
  Implements SearchBackend using books_fts.
  search(): FTS5 MATCH query, JOIN back to books for filters, return hits with
    score = 1.0 - (rank / min_rank) clamped to [0, 1].
  suggest(): SELECT DISTINCT title FROM books_fts WHERE books_fts MATCH '{q}*'
    LIMIT {limit}. Returns titles only (SearchBar shows them as quick results).
  is_available(): always true.
  backend_name(): "fts5".

backend/src/app_state.rs (or wherever AppState lives):
  Add: pub search: Arc<dyn SearchBackend>
  Initialize with Fts5Backend wrapped in Arc.

backend/src/api/search.rs — three new handlers:

  GET /api/v1/search
    Query params: q (required, min 1 char), author_id, tag, language, format,
      page (default 1), page_size (default 24, max 100)
    Auth: required (same as books)
    Response: PaginatedResponse<SearchResultItem>
      SearchResultItem = BookSummary + { score: f32 }
    Implementation: call state.search.search(), then batch-load BookSummary rows
      by the returned book_ids in order.

  GET /api/v1/search/suggestions
    Query params: q (required, min 1 char), limit (default 5, max 10)
    Auth: required
    Response: { suggestions: Vec<String> }
    Implementation: call state.search.suggest()

  GET /api/v1/system/search-status
    Auth: required
    Response:
      { fts: bool, meilisearch: bool, semantic: bool,
        backend: String }  // e.g. "fts5" or "meilisearch"
    Implementation: fts always true; meilisearch = state.search.backend_name() == "meilisearch"
      && state.search.is_available(); semantic = state.llm.is_some() (use None check)

Wire the three routes into api/mod.rs under /api/v1.

backend/tests/search_api.rs — integration tests:
  test_search_returns_matching_books
  test_search_empty_query_returns_400
  test_suggestions_returns_titles
  test_search_status_reports_fts

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test search_api -- --nocapture 2>&1
    cargo test --test fts_search -- --nocapture 2>&1

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file (api/search.rs, search/mod.rs).
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 3 — Meilisearch Integration

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md. Now do Stage 3 of Phase 4.

Add Meilisearch as an optional search tier. When Meilisearch is reachable and
configured, route all search traffic there. Fall back to Fts5Backend silently
when it is not. Never surface Meilisearch errors to users.

Deliverables:

backend/Cargo.toml — add:
  meilisearch-sdk = { version = "0.27", optional = true }
  features = ["meilisearch"]  — gate behind the feature flag

backend/src/search/meili.rs — MeilisearchBackend:
  Implements SearchBackend.
  Constructor: takes base_url: String, api_key: Option<String>.
  On startup: ping the health endpoint (/health). If unreachable, log a warning
    and return None — caller falls back to FTS5.
  search(): POST /indexes/books/search with q, filters, page, hitsPerPage.
    Map Meilisearch hits to SearchHit { book_id, score: hit._rankingScore }.
  suggest(): POST /indexes/books/search with q, limit, attributesToRetrieve: ["title"].
    Return the title strings.
  is_available(): ping /health, cache result for 30 seconds.
  backend_name(): "meilisearch".

  Index document shape (posted on book create/update):
    { id, title, authors: [String], tags: [String], series: Option<String>,
      language: Option<String>, description: Option<String> }

backend/src/search/mod.rs — add:
  pub fn build_search_backend(config: &Config, db: SqlitePool) -> Arc<dyn SearchBackend>
    If config.meilisearch.enabled:
      Try to connect MeilisearchBackend.
      If ping fails: log warning, fall back to Fts5Backend.
    Else: Fts5Backend.

backend/src/config.rs (or equivalent) — add:
  [meilisearch]
  enabled = false
  url = "http://meilisearch:7700"
  api_key = ""        # leave blank for development

backend/src/api/books.rs — hook into book create/update/delete:
  After a successful DB write, call state.search.index_book() (add this method
  to SearchBackend — default impl is a no-op so Fts5Backend doesn't need to
  implement it; MeilisearchBackend does). Fire-and-forget: log errors, never fail.

docker/docker-compose.yml — add Meilisearch service:
  meilisearch:
    image: getmeili/meilisearch:v1.7
    environment:
      - MEILI_MASTER_KEY=${MEILI_MASTER_KEY:-development}
    volumes:
      - meili_data:/meili_data
    ports:
      - "7700:7700"
  Add meili_data to volumes section.

backend/tests/meili_search.rs — integration tests (use mockito or wiremock for HTTP):
  test_meili_backend_falls_back_when_unreachable
  test_meili_backend_routes_search_when_available
  test_book_indexed_on_create
  test_book_removed_on_delete

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test meili_search -- --nocapture 2>&1
    cargo test --workspace 2>&1 | tail -20

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file (search/meilisearch.rs).
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 4 — Semantic Search

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md. Now do Stage 4 of Phase 4.

Add semantic search using sqlite-vec for vector storage and the configured LLM
endpoint for embedding generation. The book_embeddings table and llm_jobs table
(with job_type = 'semantic_index') already exist in the schema — do not recreate
them.

This entire feature is gated behind config.llm.enabled = true. When disabled,
GET /api/v1/search/semantic returns 503 with { error: "llm_unavailable" }.

Deliverables:

backend/Cargo.toml — add:
  sqlite-vec = "0.1"       # SQLite extension, loaded at runtime

backend/src/db/mod.rs (or pool setup) — load sqlite-vec extension on connection:
  conn.load_extension(sqlite_vec::load, None)?;
  Guard with #[cfg(feature = "sqlite-vec")] or a runtime check.

backend/src/llm/embeddings.rs — EmbeddingClient:
  async fn embed(text: &str) -> anyhow::Result<Vec<f32>>
    POST {config.llm.librarian.endpoint}/v1/embeddings
      body: { "input": text, "model": config.llm.librarian.model }
    Parse response.data[0].embedding (OpenAI-compatible shape).
    Timeout: 10s (same as all LLM calls).
    On error: return Err — caller handles gracefully.

backend/src/search/semantic.rs — SemanticSearch:
  async fn index_book(book_id, title, authors, description) -> anyhow::Result<()>
    1. Concatenate: "{title} by {authors}. {description}"
    2. Call EmbeddingClient::embed()
    3. Upsert into book_embeddings (book_id, model_id, embedding as BLOB, created_at)
    4. Update llm_jobs record to completed / failed

  async fn search_semantic(query: &str, page, page_size) -> anyhow::Result<SearchPage>
    1. Embed the query string
    2. SELECT book_id, vec_distance_cosine(embedding, ?) AS distance
       FROM book_embeddings ORDER BY distance LIMIT {page_size} OFFSET {(page-1)*page_size}
    3. Convert distance to score: 1.0 - distance
    4. Return SearchPage

backend/src/api/search.rs — add:

  GET /api/v1/search/semantic
    Query params: q (required), page (default 1), page_size (default 24, max 50)
    Auth: required
    503 when llm disabled or embedding model not configured
    Response: PaginatedResponse<SearchResultItem> (same shape as /api/v1/search)

backend/src/api/books.rs — on book create/update:
  If llm.enabled: enqueue a semantic_index llm_jobs record (status = 'pending').

backend/src/llm/job_runner.rs — background worker:
  Poll llm_jobs WHERE job_type = 'semantic_index' AND status = 'pending'
    every 30 seconds (tokio::time::interval).
  Claim one job at a time: UPDATE SET status='running', started_at=now.
  Call SemanticSearch::index_book().
  Update to completed or failed + error_text.
  Max 3 concurrent jobs (tokio::sync::Semaphore).

Wire the job runner into main.rs with tokio::spawn.

backend/tests/semantic_search.rs:
  test_semantic_index_stores_embedding
  test_semantic_search_returns_ranked_results
  test_semantic_search_disabled_returns_503
  test_job_runner_processes_pending_jobs

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test semantic_search -- --nocapture 2>&1
    cargo test --workspace 2>&1 | tail -20

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file (search/semantic.rs, llm/job_runner.rs).
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 5 — Frontend Wiring

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/DESIGN.md. Now do Stage 5 of Phase 4.

Wire the frontend to the real search endpoints built in Stages 1–4. The SearchPage
and SearchBar currently use apiClient.listBooks() — replace them with dedicated
search methods. Enable the Semantic tab dynamically based on what the backend reports.

Deliverables:

packages/shared/src/types.ts — add:
  SearchResultItem = BookSummary & { score?: number }
  SearchSuggestionsResponse = { suggestions: string[] }
  SearchStatusResponse = { fts: boolean; meilisearch: boolean; semantic: boolean; backend: string }

packages/shared/src/client.ts — add three methods:
  search(params: SearchQuery): Promise<PaginatedResponse<SearchResultItem>>
    GET /api/v1/search with all filter params

  searchSuggestions(q: string, limit?: number): Promise<SearchSuggestionsResponse>
    GET /api/v1/search/suggestions?q=...&limit=...

  getSearchStatus(): Promise<SearchStatusResponse>
    GET /api/v1/system/search-status

apps/web/src/features/search/SearchPage.tsx — update:
  Replace useQuery calling apiClient.listBooks() with apiClient.search().
  Add a second useQuery for apiClient.getSearchStatus() (staleTime: 60_000).
  Enable the Semantic tab button when searchStatus.semantic === true.
  When Semantic tab is active: call apiClient.search() with { semantic: true }
    (add semantic?: boolean to SearchQuery — backend ignores it if not semantic route,
    or route to /api/v1/search/semantic separately).
  Score badge: if result.score exists and > 0, show a small teal chip "Match X%"
    next to the book title in list view.

apps/web/src/features/search/SearchBar.tsx — update:
  Replace the useQuery calling apiClient.listBooks({ q, page_size: 5 })
  with apiClient.searchSuggestions(query, 5).
  Render the suggestions array as quick-result chips (text only, not mini-cards).
  Keep the existing mini-card rendering for book results from listBooks — use
  suggestions only for the "Recent searches"-style text chips.

apps/web/src/__tests__/SearchPage.test.tsx — update/add:
  test_search_page_renders_books_for_query  (existing — now hits apiClient.search)
  test_semantic_tab_disabled_when_unavailable  (existing — verify still passes)
  test_semantic_tab_enabled_when_available
  test_score_badge_shown_when_score_present

apps/web/src/__tests__/SearchBar.test.tsx — new file:
  test_suggestions_appear_on_input
  test_commit_search_navigates_to_search_page

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    pnpm --filter @calibre/shared test -- --reporter=verbose 2>&1
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
      - /search?q=dune — results render with score badges on each card
      - /search?q=dune — Semantic tab visible; clicking it shows semantic results
      - /search?q=dune — semantic tab disabled/hidden when backend reports semantic:false
      - /library — confirm filter still works after FTS5 migration
    Kill the dev server: kill %1

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## Review Checkpoints

| After stage | What to verify |
|---|---|
| Stage 1 | FTS5 migration applies cleanly; MATCH replaces LIKE; prefix search works |
| Stage 2 | SearchBackend trait compiles; all three endpoints return correct shapes |
| Stage 3 | Meilisearch fallback works when service is down; index sync on book write |
| Stage 4 | Embedding stored as BLOB; cosine distance query returns ranked results; 503 when LLM disabled |
| Stage 5 | Semantic tab toggles; suggestions render; score badge appears |

## If Codex Gets Stuck or a Test Fails

```
The following test is failing. Diagnose the root cause and fix it.
Do not work around it — fix the underlying issue.

[paste error output]
```

## Commit Sequence

```bash
# After Stage 1
git add -A && git commit -m "Phase 4 Stage 1: FTS5 virtual table, replace LIKE with MATCH, 4/4 tests passing"

# After Stage 2
git add -A && git commit -m "Phase 4 Stage 2: SearchBackend trait, Fts5Backend, search/suggestions/status routes, tests passing"

# After Stage 3
git add -A && git commit -m "Phase 4 Stage 3: Meilisearch integration, fallback to FTS5, docker-compose updated, tests passing"

# After Stage 4
git add -A && git commit -m "Phase 4 Stage 4: sqlite-vec semantic search, embedding job runner, LLM gated, tests passing"

# After Stage 5
git add -A && git commit -m "Phase 4 Stage 5: frontend wired to search API, semantic tab dynamic, suggestions live, tests passing"
```
