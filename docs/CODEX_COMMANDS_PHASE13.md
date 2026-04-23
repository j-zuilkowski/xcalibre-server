# Codex Desktop App — autolibre Phase 13: Reader Depth + Observability

## What Phase 13 Builds

Four high-value features identified in the post-v1.1 evaluation:

- **Stage 1** — Reader annotations: highlights + notes (epub.js CFI-based, persisted in DB)
- **Stage 2** — OpenAPI spec generation (utoipa, Swagger UI at `/api/docs`)
- **Stage 3** — Prometheus metrics endpoint (`/metrics`, request counters, latency histograms, job queue depth)
- **Stage 4** — Reading statistics (pages per day, books finished, time spent reading — aggregate queries + stats page)

## Key Design Decisions

**Reader Annotations:**
- epub.js exposes a `rendition.annotations` API for highlight ranges using CFI (Canonical Fragment Identifier) strings — the same coordinate system already used for reading progress
- Annotations stored in a new `book_annotations` table: `(id, user_id, book_id, cfi_range, text, note, color, created_at)`
- Three annotation types: `highlight` (color only), `note` (color + text), `bookmark` (CFI point, no range)
- Annotations are per-user — users never see each other's annotations
- Web reader loads annotations on open and syncs new ones immediately (no offline queue needed for MVP)
- Mobile reader: annotations displayed read-only in Phase 13; editing on mobile is Phase 14

**OpenAPI Spec:**
- `utoipa` crate annotates handlers and types with `#[utoipa::path]` and `#[derive(ToSchema)]`
- Spec served at `GET /api/docs/openapi.json` (unauthenticated — spec itself contains no sensitive data)
- Swagger UI served at `GET /api/docs` via `utoipa-swagger-ui`
- Only REST API routes annotated — OPDS and MCP routes excluded (different audiences)
- JWT bearer auth scheme declared in the spec so Swagger UI's "Authorize" button works

**Prometheus Metrics:**
- `axum-prometheus` crate wraps the existing Axum router with minimal code change
- Metrics endpoint at `GET /metrics` — unauthenticated but should be firewalled (not exposed publicly; document this in DEPLOY.md)
- Custom metrics beyond HTTP defaults: LLM job queue depth, active imports, Meilisearch index lag, DB pool utilization
- Grafana dashboard JSON included in `docker/grafana/autolibre-dashboard.json`

**Reading Statistics:**
- All raw data already exists: `reading_progress` stores CFI + percentage + `last_read_at`; `book_user_state` tracks `is_read`
- New aggregate queries against existing tables — no schema changes
- Stats page in web profile; stats card in mobile profile tab
- Metrics: books read (total, this year, this month), average reading session length, longest streak, formats breakdown, most-read authors/tags

## Key Schema Facts (new tables this phase)

```sql
-- Stage 1 — Reader Annotations (migration 0015)
CREATE TABLE book_annotations (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    book_id     TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    type        TEXT NOT NULL CHECK(type IN ('highlight', 'note', 'bookmark')),
    cfi_range   TEXT NOT NULL,      -- epub.js CFI string (range for highlight/note, point for bookmark)
    highlighted_text TEXT,          -- the selected text (NULL for bookmarks)
    note        TEXT,               -- user's annotation text (NULL for highlights/bookmarks)
    color       TEXT NOT NULL DEFAULT 'yellow' CHECK(color IN ('yellow', 'green', 'blue', 'pink')),
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX idx_annotations_user_book ON book_annotations(user_id, book_id);
```

## Reference Files

Read before starting each stage:
- `backend/src/db/queries/` — query patterns to follow
- `backend/src/api/books.rs` — reading progress handlers to extend for annotations (Stage 1)
- `apps/web/src/features/reader/EpubReader.tsx` — epub.js integration to extend (Stage 1)
- `backend/Cargo.toml` — dependency patterns (Stage 2, 3)
- `backend/src/lib.rs` — middleware and router composition (Stage 2, 3)
- `backend/src/db/queries/books.rs` — query patterns for stats aggregates (Stage 4)
- `apps/web/src/features/profile/ProfilePage.tsx` — stats page home (Stage 4)

---

## STAGE 1 — Reader Annotations (Highlights + Notes)

**Priority: Very High (most-requested reader feature)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/api/books.rs (the reading_progress handlers),
backend/src/db/queries/books.rs, apps/web/src/features/reader/EpubReader.tsx,
backend/migrations/sqlite/0014_totp.sql (for migration format reference),
and docs/SCHEMA.md.
Now implement reader annotations: highlights, notes, and bookmarks.

─────────────────────────────────────────
SCHEMA — migration 0015
─────────────────────────────────────────

backend/migrations/sqlite/0015_annotations.sql:

  CREATE TABLE book_annotations (
      id               TEXT PRIMARY KEY,
      user_id          TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      book_id          TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
      type             TEXT NOT NULL CHECK(type IN ('highlight', 'note', 'bookmark')),
      cfi_range        TEXT NOT NULL,
      highlighted_text TEXT,
      note             TEXT,
      color            TEXT NOT NULL DEFAULT 'yellow'
                         CHECK(color IN ('yellow', 'green', 'blue', 'pink')),
      created_at       TEXT NOT NULL,
      updated_at       TEXT NOT NULL
  );
  CREATE INDEX idx_annotations_user_book ON book_annotations(user_id, book_id);

backend/migrations/mariadb/0014_annotations.sql — equivalent MariaDB DDL.

─────────────────────────────────────────
DELIVERABLE 1 — Backend API
─────────────────────────────────────────

backend/src/api/books.rs — add annotation routes:

  GET    /books/:id/annotations         — list all annotations for current user + book
  POST   /books/:id/annotations         — create annotation
  PATCH  /books/:id/annotations/:ann_id — update note text or color
  DELETE /books/:id/annotations/:ann_id — delete annotation

  GET /books/:id/annotations
    Auth: Any authenticated user
    Response: Vec<Annotation> ordered by cfi_range ASC
    Only returns annotations belonging to the current user.

  POST /books/:id/annotations
    Auth: Any authenticated user
    Body:
      {
        "type": "highlight" | "note" | "bookmark",
        "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:128)",
        "highlighted_text": "The text the user selected",  -- required for highlight/note
        "note": "My annotation text",                       -- required for note, null for others
        "color": "yellow"                                   -- optional, default yellow
      }
    Response 201: Annotation

  PATCH /books/:id/annotations/:ann_id
    Body: { "note": "updated text", "color": "green" }  -- all fields optional
    Ownership check: 403 if ann_id belongs to a different user.
    Response 200: updated Annotation

  DELETE /books/:id/annotations/:ann_id
    Ownership check: 403 if not owner.
    Response 204.

backend/src/db/queries/annotations.rs — new file with:
  list_annotations(db, user_id, book_id) -> Vec<Annotation>
  create_annotation(db, NewAnnotation) -> Annotation
  update_annotation(db, ann_id, user_id, patch) -> Option<Annotation>
  delete_annotation(db, ann_id, user_id) -> bool

─────────────────────────────────────────
DELIVERABLE 2 — Web Reader Integration
─────────────────────────────────────────

apps/web/src/features/reader/EpubReader.tsx — extend:

  On reader mount:
    1. Fetch GET /books/:id/annotations
    2. For each annotation, call rendition.annotations.add():
       - type: "highlight" for highlights and notes; ignored for bookmarks
       - cfi: the stored cfi_range
       - data: { id, color, note } (stored as annotation metadata)
       - className: `annotation-${color}` (for CSS color styling)

  On text selection:
    3. rendition.on("selected", (cfiRange, contents)) fires when user selects text
    4. Show a floating context menu above the selection with:
       - Color picker: 4 color swatches (yellow, green, blue, pink)
       - Note icon: opens a small text input inline
       - Bookmark icon: saves a CFI point annotation
       - X: dismiss without saving
    5. On color swatch click: POST /books/:id/annotations with type "highlight"
    6. On note submit: POST /books/:id/annotations with type "note" and note text
    7. On success: rendition.annotations.add() to show immediately (optimistic)

  On highlight click:
    8. rendition.on("markClicked", (cfiRange, data)) fires when a highlight is clicked
    9. Show a tooltip with:
       - The annotation note (if any)
       - Color picker to change color (PATCH)
       - Edit note button (for notes)
       - Delete button (DELETE + rendition.annotations.remove(cfiRange))

  CSS for annotation colors (inject into epub.js iframe via rendition.themes):
    .annotation-yellow { background: rgba(255, 235, 59, 0.4); }
    .annotation-green  { background: rgba(76, 175, 80, 0.3); }
    .annotation-blue   { background: rgba(33, 150, 243, 0.3); }
    .annotation-pink   { background: rgba(233, 30, 99, 0.3); }

  Annotations panel in TOC slide-in:
    Add a second tab to the existing TOC panel: "Chapters" | "Annotations"
    Annotations tab: list of all annotations for this book, grouped by chapter.
    Click an annotation → rendition.display(cfi_range) to navigate to it.

─────────────────────────────────────────
DELIVERABLE 3 — Mobile (Read-Only)
─────────────────────────────────────────

apps/mobile/src/features/reader/EpubReaderMobile.tsx — display existing annotations:

  On mount: fetch GET /books/:id/annotations
  Render highlights as colored backgrounds using the foliojs-port annotation API
  (or equivalent for the mobile epub renderer in use).

  Display-only for Phase 13 — no creation or editing on mobile.
  Add a comment: "TODO Phase 14: annotation creation on mobile"

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_annotations.rs:
  test_create_highlight_returns_201
  test_create_note_requires_note_text
  test_create_bookmark_accepts_null_highlighted_text
  test_list_annotations_only_returns_own
  test_update_annotation_changes_color
  test_update_annotation_owned_by_other_user_returns_403
  test_delete_annotation_returns_204
  test_delete_annotation_owned_by_other_user_returns_403
  test_annotations_cascade_delete_on_book_delete

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @autolibre/web build
pnpm --filter @autolibre/mobile exec tsc --noEmit
git add backend/ apps/web/src/features/reader/ apps/mobile/
git commit -m "Phase 13 Stage 1: reader annotations — highlights, notes, bookmarks (web + mobile read-only)"
```

---

## STAGE 2 — OpenAPI Spec Generation

**Priority: High (enables SDK generation, community integrations, interactive docs)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/api/mod.rs, backend/src/lib.rs, backend/Cargo.toml,
backend/src/api/books.rs (first 100 lines for handler signature patterns),
and backend/src/api/auth.rs (first 80 lines).
Now add OpenAPI spec generation using utoipa.

─────────────────────────────────────────
DEPENDENCIES
─────────────────────────────────────────

backend/Cargo.toml — add:
  utoipa             = { version = "4", features = ["axum_extras", "chrono", "uuid"] }
  utoipa-swagger-ui  = { version = "7", features = ["axum"] }

─────────────────────────────────────────
DELIVERABLE 1 — Annotate types with ToSchema
─────────────────────────────────────────

Priority types to annotate (start with these; others can be added incrementally):
  Book, BookSummary, Author, Tag, Series, Format, Identifier
  User, Role, Permission
  ReadingProgress, BookAnnotation, BookUserState
  LlmJob, ScheduledTask
  PaginatedResponse<T>
  AppError (as an error response schema)

Add #[derive(utoipa::ToSchema)] to each struct in backend/src/db/queries/*.rs
and the relevant type files.

─────────────────────────────────────────
DELIVERABLE 2 — Annotate priority handlers
─────────────────────────────────────────

Annotate the following handler groups with #[utoipa::path(...)]:

  Auth:    POST /auth/login, POST /auth/refresh, POST /auth/logout
           POST /auth/totp/verify, GET /auth/totp/setup, POST /auth/totp/confirm
  Books:   GET /books, GET /books/:id, POST /books, PATCH /books/:id, DELETE /books/:id
           GET /books/:id/cover, GET /books/:id/formats/:format/download
           GET /books/:id/progress, PUT /books/:id/progress
           GET /books/:id/annotations, POST /books/:id/annotations
  Search:  GET /search, GET /search/semantic
  Shelves: GET /shelves, POST /shelves, GET /shelves/:id,
           POST /shelves/:id/books, DELETE /shelves/:id/books/:book_id
  Users:   GET /users/me, PATCH /users/me
  Health:  GET /health

  For each handler: document summary, description (one line), request body schema,
  response schema (200 and common error codes: 400, 401, 403, 404, 422, 429).

─────────────────────────────────────────
DELIVERABLE 3 — OpenApiDoc struct and routes
─────────────────────────────────────────

backend/src/api/docs.rs — new file:

  use utoipa::OpenApi;
  use utoipa_swagger_ui::SwaggerUi;

  #[derive(OpenApi)]
  #[openapi(
    info(
      title = "autolibre API",
      version = env!("CARGO_PKG_VERSION"),
      description = "Self-hosted ebook library manager — REST API",
      license(name = "MIT"),
    ),
    components(schemas(
      Book, BookSummary, Author, Tag, /* ... all annotated types ... */
    )),
    security(("bearer_auth" = [])),
    tags(
      (name = "auth", description = "Authentication and session management"),
      (name = "books", description = "Book library management"),
      (name = "search", description = "Full-text and semantic search"),
      (name = "shelves", description = "Personal reading lists"),
      (name = "reader", description = "Reading progress and annotations"),
    )
  )]
  pub struct ApiDoc;

  pub fn openapi_routes() -> Router {
    Router::new()
      .merge(SwaggerUi::new("/api/docs")
        .url("/api/docs/openapi.json", ApiDoc::openapi()))
  }

backend/src/lib.rs — merge openapi_routes() into the main router.

The /api/docs and /api/docs/openapi.json routes require NO authentication.
The spec itself contains no sensitive data — it describes the API shape only.

─────────────────────────────────────────
DELIVERABLE 4 — Bearer auth scheme
─────────────────────────────────────────

Declare the security scheme so Swagger UI's "Authorize" button works:

  #[openapi(
    modifiers(&SecurityAddon),
    ...
  )]

  struct SecurityAddon;
  impl Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
      let components = openapi.components.as_mut().unwrap();
      components.add_security_scheme(
        "bearer_auth",
        SecurityScheme::Http(
          HttpBuilder::new()
            .scheme(HttpAuthScheme::Bearer)
            .bearer_format("JWT")
            .build()
        )
      );
    }
  }

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_openapi.rs:
  test_openapi_json_endpoint_returns_200
  test_openapi_json_is_valid_json
  test_openapi_json_contains_books_path
  test_openapi_json_requires_no_auth
  test_swagger_ui_returns_200

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Manual: open http://localhost:3000/api/docs in browser
# Manual: click "Authorize", enter a JWT, try GET /books — should return real data
git add backend/
git commit -m "Phase 13 Stage 2: OpenAPI spec (utoipa) + Swagger UI at /api/docs"
```

---

## STAGE 3 — Prometheus Metrics

**Priority: High (essential for multi-user ops and capacity planning)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/lib.rs, backend/Cargo.toml, backend/src/api/admin.rs
(the jobs and system endpoints), backend/src/scheduler.rs,
and docker/docker-compose.yml.
Now add a Prometheus metrics endpoint and a companion Grafana dashboard.

─────────────────────────────────────────
DEPENDENCIES
─────────────────────────────────────────

backend/Cargo.toml — add:
  axum-prometheus = "0.7"
  metrics         = "0.23"
  metrics-exporter-prometheus = "0.15"

─────────────────────────────────────────
DELIVERABLE 1 — HTTP metrics middleware
─────────────────────────────────────────

backend/src/lib.rs — add axum-prometheus middleware to the router:

  use axum_prometheus::PrometheusMetricLayer;

  let (prometheus_layer, metrics_handle) = PrometheusMetricLayer::pair();

  // Add to the router stack (before auth middleware):
  let app = Router::new()
    ...
    .layer(prometheus_layer);

  // Expose /metrics endpoint (no auth — firewall at reverse proxy level):
  let app = app.route("/metrics", get(move || async move {
    metrics_handle.render()
  }));

  This provides out-of-the-box:
    axum_http_requests_total{method, endpoint, status}
    axum_http_requests_duration_seconds{method, endpoint, status} (histogram)
    axum_http_requests_pending{method, endpoint}

─────────────────────────────────────────
DELIVERABLE 2 — Custom application metrics
─────────────────────────────────────────

backend/src/metrics.rs — new file defining custom metric names:

  pub const LLM_JOBS_QUEUED:   &str = "autolibre_llm_jobs_queued";
  pub const LLM_JOBS_RUNNING:  &str = "autolibre_llm_jobs_running";
  pub const LLM_JOBS_FAILED:   &str = "autolibre_llm_jobs_failed_total";
  pub const IMPORT_JOBS_ACTIVE: &str = "autolibre_import_jobs_active";
  pub const SEARCH_INDEX_LAG:  &str = "autolibre_search_unindexed_books";
  pub const DB_POOL_SIZE:      &str = "autolibre_db_pool_connections";
  pub const STORAGE_BYTES:     &str = "autolibre_storage_bytes_total";

Instrument these at the following call sites:
  - LLM job enqueue/complete/fail → increment/decrement LLM_JOBS_QUEUED, LLM_JOBS_RUNNING
  - Import job start/finish → increment/decrement IMPORT_JOBS_ACTIVE
  - Scheduler loop → gauge SEARCH_INDEX_LAG by querying COUNT(*) WHERE indexed_at IS NULL
  - AppState::new → gauge DB_POOL_SIZE from sqlx pool size

Use the `metrics` crate macros:
  metrics::gauge!(LLM_JOBS_QUEUED, count as f64);
  metrics::counter!(LLM_JOBS_FAILED);
  metrics::histogram!("autolibre_import_duration_seconds", duration_secs);

─────────────────────────────────────────
DELIVERABLE 3 — Security note for /metrics
─────────────────────────────────────────

The /metrics endpoint is unauthenticated (Prometheus scrapes it without auth by default).
It must NOT be exposed publicly.

Add to docs/DEPLOY.md (Caddy section):
  Block /metrics from public access:
    @metrics path /metrics
    respond @metrics 403

  Or restrict to internal network only:
    @metrics {
      path /metrics
      not remote_ip 10.0.0.0/8 172.16.0.0/12 192.168.0.0/16
    }
    respond @metrics 403

─────────────────────────────────────────
DELIVERABLE 4 — Grafana dashboard
─────────────────────────────────────────

docker/grafana/autolibre-dashboard.json — Grafana dashboard JSON with panels:

  Row 1: HTTP Overview
    - Request rate (req/s) — rate(axum_http_requests_total[1m])
    - Error rate (5xx %) — rate(axum_http_requests_total{status=~"5.."}[1m])
    - P50/P95/P99 latency — histogram_quantile() on duration metric

  Row 2: Library Activity
    - LLM jobs queued/running (gauge)
    - Active imports (gauge)
    - Unindexed books (search index lag)

  Row 3: Infrastructure
    - DB pool utilization
    - Storage bytes used

docker/docker-compose.yml — add optional Prometheus + Grafana services (commented out):
  prometheus:
    image: prom/prometheus:latest
    volumes:
      - ./docker/prometheus.yml:/etc/prometheus/prometheus.yml
  grafana:
    image: grafana/grafana:latest
    environment:
      - GF_SECURITY_ADMIN_PASSWORD=admin

docker/prometheus.yml:
  scrape_configs:
    - job_name: autolibre
      static_configs:
        - targets: ["autolibre:3000"]
      metrics_path: /metrics

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_metrics.rs:
  test_metrics_endpoint_returns_200
  test_metrics_endpoint_returns_prometheus_format
  test_metrics_requires_no_auth
  test_metrics_contains_http_requests_total

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Manual: curl http://localhost:3000/metrics | grep autolibre
# Manual: docker compose up prometheus grafana; open Grafana; import dashboard JSON
git add backend/ docker/
git commit -m "Phase 13 Stage 3: Prometheus metrics endpoint + Grafana dashboard"
```

---

## STAGE 4 — Reading Statistics

**Priority: High (engagement feature — all raw data already exists)**
**Blocks: nothing. Blocked by: nothing.**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read backend/src/db/queries/books.rs, backend/src/api/books.rs,
apps/web/src/features/profile/ProfilePage.tsx,
and apps/mobile/src/app/(tabs)/library.tsx.
Now add reading statistics — aggregate queries on existing data and a stats UI.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

No new DB tables needed. All data already exists:
  reading_progress: (user_id, book_id, progress_pct, cfi, last_read_at, updated_at)
  book_user_state:  (user_id, book_id, is_read, is_archived, read_at)
  formats:          (book_id, format)

─────────────────────────────────────────
DELIVERABLE 1 — Backend stats API
─────────────────────────────────────────

backend/src/api/users.rs — add route:
  GET /users/me/stats

  No query params required. Returns stats for the authenticated user.

  Response shape:
  {
    "total_books_read": 42,
    "books_read_this_year": 12,
    "books_read_this_month": 3,
    "books_in_progress": 5,
    "total_reading_sessions": 87,
    "reading_streak_days": 14,         -- consecutive days with reading_progress.updated_at
    "longest_streak_days": 31,
    "average_progress_per_session": 4.2, -- percentage points
    "formats_read": {
      "epub": 35, "pdf": 5, "mobi": 2
    },
    "top_tags": [
      { "name": "Fiction", "count": 18 },
      { "name": "Science Fiction", "count": 9 }
    ],
    "top_authors": [
      { "name": "Terry Pratchett", "count": 7 }
    ],
    "monthly_books": [                  -- last 12 months
      { "month": "2026-04", "count": 3 },
      { "month": "2026-03", "count": 1 }
    ]
  }

backend/src/db/queries/stats.rs — new file:

  pub async fn get_user_stats(db, user_id) -> UserStats

  Implement with a series of efficient queries (not one giant join):

  total_books_read:
    SELECT COUNT(*) FROM book_user_state WHERE user_id = ? AND is_read = 1

  books_read_this_year / this_month:
    SELECT COUNT(*) FROM book_user_state WHERE user_id = ? AND is_read = 1
    AND read_at >= '2026-01-01'  -- (substitute actual year/month boundaries)

  books_in_progress:
    SELECT COUNT(DISTINCT book_id) FROM reading_progress
    WHERE user_id = ? AND progress_pct > 0 AND progress_pct < 100

  reading_streak_days:
    SELECT DISTINCT DATE(updated_at) FROM reading_progress WHERE user_id = ?
    ORDER BY DATE(updated_at) DESC
    -- then compute streak in Rust by iterating the sorted date list

  formats_read:
    SELECT f.format, COUNT(DISTINCT bt.book_id)
    FROM book_user_state bus
    JOIN formats f ON f.book_id = bus.book_id
    WHERE bus.user_id = ? AND bus.is_read = 1
    GROUP BY f.format

  top_tags:
    SELECT t.name, COUNT(*) AS cnt
    FROM book_user_state bus
    JOIN book_tags bt ON bt.book_id = bus.book_id AND bt.confirmed = 1
    JOIN tags t ON t.id = bt.tag_id
    WHERE bus.user_id = ? AND bus.is_read = 1
    GROUP BY t.id ORDER BY cnt DESC LIMIT 5

  monthly_books:
    SELECT strftime('%Y-%m', read_at) AS month, COUNT(*) AS count
    FROM book_user_state WHERE user_id = ? AND is_read = 1
    AND read_at >= date('now', '-12 months')
    GROUP BY month ORDER BY month

─────────────────────────────────────────
DELIVERABLE 2 — Web stats page
─────────────────────────────────────────

apps/web/src/features/profile/StatsPage.tsx — new page:

  Accessible from Profile settings sidebar: "Reading Stats" link.
  Route: /profile/stats

  Layout (responsive, card-based):
    Row 1: 4 stat cards side by side
      ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────┐
      │  42          │ │  12          │ │  🔥 14 days  │ │  5 in       │
      │  Books read  │ │  This year   │ │  Streak      │ │  progress   │
      └─────────────┘ └─────────────┘ └─────────────┘ └─────────────┘

    Row 2: Monthly bar chart (last 12 months)
      Use a simple SVG bar chart — no external charting library.
      Each bar: month label below, book count as bar height, count label above.

    Row 3: Two side-by-side panels
      Left:  Top authors (ranked list with book count)
      Right: Top tags (ranked list with book count)

    Row 4: Formats breakdown
      Horizontal pill bar showing EPUB/PDF/MOBI/etc. proportions.

  Use the existing teal accent color for chart bars and highlights.
  All text via i18next keys (add to EN/FR/DE/ES).

─────────────────────────────────────────
DELIVERABLE 3 — Mobile stats card
─────────────────────────────────────────

apps/mobile/src/app/(tabs)/profile.tsx — add a stats summary card:

  Position: below user info, above settings options.

  Card contents:
    [Book icon] 42 books read   [Flame icon] 14-day streak   [Clock icon] 5 in progress

  Tapping the card navigates to a full stats screen:
    apps/mobile/src/app/stats.tsx

  Stats screen (mobile):
    - Same 4 stat numbers at top
    - Top 3 authors and top 3 tags (abbreviated — no chart)
    - Formats breakdown as text: "EPUB: 35, PDF: 5, MOBI: 2"
    - No bar chart on mobile (Phase 14 if desired)

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_stats.rs:
  test_stats_returns_zero_for_new_user
  test_total_books_read_counts_is_read_books
  test_books_read_this_year_excludes_prior_years
  test_streak_is_zero_with_no_activity
  test_streak_counts_consecutive_days
  test_streak_resets_on_gap
  test_monthly_books_covers_last_12_months
  test_top_tags_ordered_by_count
  test_formats_read_groups_by_format

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @autolibre/web build
pnpm --filter @autolibre/mobile exec tsc --noEmit
git add backend/ apps/web/src/features/profile/ apps/mobile/src/app/
git commit -m "Phase 13 Stage 4: reading statistics — streak, monthly books, top authors/tags"
```

---

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/review` + `/security-review` — verify ownership checks on all annotation routes, CFI strings not used in SQL queries, no XSS from highlighted_text |
| Stage 2 | `/review` — verify /api/docs is unauthenticated, no secrets in spec, bearer scheme correct |
| Stage 3 | `/review` + `/security-review` — verify /metrics is firewalled in DEPLOY.md, custom metrics don't leak user data |
| Stage 4 | `/review` — verify stats queries are efficient (no full table scans), streak computation handles timezone correctly |

Run `/engineering:deploy-checklist` after Stage 4 before tagging v1.2.
