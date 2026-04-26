# Codex Desktop App — xcalibre-server Phase 10: Review Fixes

## What This File Covers

Fixes identified by the post-Stage-1-through-7 code and security review. Stages are ordered
by severity: security/critical first, data integrity second, UX/polish third.

Stages 4 and 5 are reserved for findings from the Phase 10 Stage 5 (readers) and
Stage 8 (format conversion) reviews, which have not yet been conducted.

## Source Reviews

- Code review: Stages 1–4, 6, 7 (2026-04-22)
- Security review: Stages 1–4, 6, 7 (2026-04-22)

---

## STAGE 1 — Security Fixes (High/Critical)

**Priority: Critical/High**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md, backend/src/api/opds.rs, backend/src/api/mod.rs,
backend/src/api/admin.rs, backend/src/middleware/auth.rs,
backend/src/db/queries/books.rs, and backend/src/config.rs.
Fix the following five security issues in order.

─────────────────────────────────────────
FIX 1 — OPDS: Add Authentication + Pass user_id to list_books
─────────────────────────────────────────

Problem A: The OPDS router is mounted at /opds with no auth middleware. The
entire library catalog (titles, authors, descriptions, cover URLs) is publicly
accessible without credentials.

Problem B: build_book_feed passes ListBooksParams with user_id = None, which
causes list_books to skip all per-user tag restriction filters. Books blocked
for a user appear in their OPDS feed.

Fix:

In backend/src/api/opds.rs, update router() to add the auth layer:
  pub fn router(state: AppState) -> Router<AppState> {
      let auth_layer = middleware::from_fn_with_state(
          state.clone(), crate::middleware::auth::require_auth,
      );
      Router::new()
          .route("/", get(opds_root))
          // ... all existing routes ...
          .route_layer(auth_layer)
  }

In backend/src/api/opds.rs, update all handlers that call build_book_feed to
accept Extension(auth_user): Extension<AuthenticatedUser> and pass the user_id
into ListBooksParams:

  async fn catalog_feed(
      State(state): State<AppState>,
      Extension(auth_user): Extension<AuthenticatedUser>,
      Query(params): Query<OpdsPageParams>,
  ) -> Result<Response, AppError> {
      build_book_feed(&state, ListBooksParams {
          user_id: Some(auth_user.user.id.clone()),
          ..Default::default()
      }, params.page, params.page_size).await
  }

Apply the same Extension(auth_user) + user_id pattern to every handler that
calls build_book_feed: catalog_feed, new_books_feed, search_feed,
author_books_feed, series_books_feed, publisher_books_feed,
language_books_feed, and ratings_books_feed.

The browse/navigation feeds (opds_root, authors_feed, series_feed,
publishers_feed, languages_feed, ratings_feed) do not call list_books — they
only return navigation entries — so they do not need user_id but still need
to be behind auth.

Add test to backend/tests/test_opds.rs:
  test_opds_requires_auth — unauthenticated request to /opds returns 401
  test_opds_catalog_respects_tag_restrictions — user with a blocked tag
    does not see books with that tag in the OPDS catalog feed

─────────────────────────────────────────
FIX 2 — OPDS: Remove N+1 Query in build_book_feed
─────────────────────────────────────────

Problem: build_book_feed calls list_books (which returns BookSummary), then
loops over each summary and calls get_book_by_id individually to get formats
and description for push_book_entry. This is 31 DB round-trips per OPDS page.

Fix: Read the current BookSummary struct in backend/src/db/models.rs (or
wherever it is defined) and the current list_books query.

If BookSummary already includes formats and description, update push_book_entry
to accept a BookSummary instead of a Book and remove the get_book_by_id loop.

If BookSummary does not include formats and description, add them:
  - Add formats: Vec<FormatEntry> to BookSummary (FormatEntry = { format, id })
  - Add description: Option<String> to BookSummary
  - Update the list_books SQL query to LEFT JOIN formats and aggregate them
    (or use a subquery) and select description alongside the existing columns.

After the fix, build_book_feed should make exactly 1 DB query per page
regardless of how many books are returned.

Add test to backend/tests/test_opds.rs:
  test_opds_catalog_returns_download_urls — verify that each entry in the
  catalog acquisition feed includes at least one DownloadUrl with a non-empty
  href (confirms formats are present without the N+1).

─────────────────────────────────────────
FIX 3 — Proxy Auth: Handle User-Creation Race Condition
─────────────────────────────────────────

Problem: In backend/src/middleware/auth.rs, the proxy auth flow does
find_user_by_username → if not found → create_user. Two concurrent requests
from the same new proxy user can both see None, both call create_user, and the
second fails with a unique constraint violation that maps to AppError::Internal
(HTTP 500 for a legitimate login attempt).

Fix: In the proxy auth handler (authenticate_proxy_user or equivalent), after
calling create_user, catch the unique-constraint error by retrying with
find_user_by_username rather than propagating the error:

  let user = match create_user(&state.db, &username, &email, "user").await {
      Ok(u) => u,
      Err(e) if is_unique_constraint_error(&e) => {
          // Lost the race — another request created the user first
          find_user_by_username(&state.db, &username)
              .await
              .map_err(|_| AppError::Internal)?
              .ok_or(AppError::Internal)?
      }
      Err(_) => return Err(AppError::Internal),
  };

Add is_unique_constraint_error(e: &anyhow::Error) -> bool helper that checks
whether the underlying sqlx error is a UNIQUE constraint violation:
  e.downcast_ref::<sqlx::Error>().map_or(false, |sqlx_err| {
      matches!(sqlx_err, sqlx::Error::Database(db_err) if db_err.is_unique_violation())
  })

Add test to backend/tests/test_proxy_auth.rs:
  test_proxy_auth_concurrent_first_request_succeeds — simulate the race by
  calling authenticate_proxy_user with the same username from two tasks
  concurrently; assert both return Ok (or at worst the second retries cleanly).

─────────────────────────────────────────
FIX 4 — Update Checker: Cache TOCTOU + Hardcode URL
─────────────────────────────────────────

Problem A: The update check cache is read under a read() lock, the lock is
dropped, the network fetch happens (up to 10 seconds), then the write() lock
is acquired. Concurrent admins all see stale cache simultaneously, all launch
parallel GitHub requests, all write.

Problem B: The GitHub releases URL is configurable via XCS_RELEASES_URL
env var, which is inconsistent with the SSRF guard applied to LLM endpoints
and creates an SSRF vector if env vars can be influenced.

Fix A: Replace the read-then-drop-then-write pattern with a Mutex so the
entire check-then-fetch-then-store is atomic per request:

  static CACHE: OnceLock<tokio::sync::Mutex<Option<CachedUpdateCheck>>> =
      OnceLock::new();

  fn cache() -> &'static tokio::sync::Mutex<Option<CachedUpdateCheck>> {
      CACHE.get_or_init(|| tokio::sync::Mutex::new(None))
  }

  // In the handler:
  let mut guard = cache().lock().await;
  if let Some(ref cached) = *guard {
      if cached.fetched_at.elapsed() < Duration::from_secs(3600) {
          return Ok(Json(cached.response.clone()));
      }
  }
  let response = fetch_update_info().await;
  *guard = Some(CachedUpdateCheck { response: response.clone(),
      fetched_at: std::time::Instant::now() });
  Ok(Json(response))

Fix B: Remove the XCS_RELEASES_URL env var override. Hardcode the URL
as a module-level constant:
  const GITHUB_RELEASES_URL: &str =
      "https://api.github.com/repos/xcalibre/xcalibre-server/releases/latest";

─────────────────────────────────────────
FIX 5 — Scheduler: Add Job Deduplication Guard
─────────────────────────────────────────

Problem: run_scheduled_task in backend/src/scheduler.rs enqueues a new LLM
job every time a task fires with no check for whether a prior job of the same
type is still pending or running. An aggressive cron (e.g., * * * * *) creates
an unbounded job queue.

Fix: In run_scheduled_task, before calling the enqueue function, check whether
an existing job of the same type is already in pending or running state:

  let existing = sqlx::query_scalar(
      "SELECT COUNT(*) FROM llm_jobs WHERE job_type = ? AND status IN ('pending', 'running')"
  )
  .bind(&job_type_str)
  .fetch_one(&state.db)
  .await
  .unwrap_or(0_i64);

  if existing > 0 {
      tracing::info!(
          job_type = %job_type_str,
          "skipping scheduled task dispatch — prior job still active"
      );
      return Ok(());
  }

Apply this check for every task_type branch in run_scheduled_task.

Also add a maximum scheduled-task count guard in create_scheduled_task
(admin.rs): reject with 400 if COUNT(*) FROM scheduled_tasks >= 50.

Add test to backend/tests/test_scheduled_tasks.rs:
  test_scheduler_skips_dispatch_when_job_already_active — seed a pending
    llm_job of type classify_all, fire the scheduler, assert no second job
    was created.
  test_create_scheduled_task_limit — create 50 tasks, assert the 51st
    returns 400.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/
git commit -m "Phase 10 fixes Stage 1: OPDS auth, N+1, proxy race, update cache, scheduler dedup"
```

---

## STAGE 2 — Data Integrity Fixes

**Priority: High/Medium**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/db/queries/books.rs, backend/src/db/queries/book_user_state.rs,
backend/migrations/sqlite/0011_download_history.sql,
backend/src/api/books.rs, and backend/src/scheduler.rs.
Fix the following four data integrity issues in order.

─────────────────────────────────────────
FIX 1 — Atomic set_read / set_archived
─────────────────────────────────────────

Problem: set_read and set_archived in backend/src/db/queries/book_user_state.rs
each do a SELECT (get_state) then an UPDATE (upsert_state). A concurrent
request changing the other flag between the read and write silently resets it.
Example: device A sets is_read=true while device B concurrently sets
is_archived=true — one will win and reset the other's change.

Fix: Replace both functions with single partial-update upserts that only touch
the intended column:

  pub async fn set_read(db: &SqlitePool, user_id: &str, book_id: &str, is_read: bool) -> anyhow::Result<()> {
      let now = Utc::now().to_rfc3339();
      sqlx::query(
          r#"
          INSERT INTO book_user_state (user_id, book_id, is_read, is_archived, updated_at)
          VALUES (?, ?, ?, 0, ?)
          ON CONFLICT(user_id, book_id) DO UPDATE SET
              is_read = excluded.is_read,
              updated_at = excluded.updated_at
          "#,
      )
      .bind(user_id)
      .bind(book_id)
      .bind(is_read as i64)
      .bind(&now)
      .execute(db)
      .await?;
      Ok(())
  }

  pub async fn set_archived(db: &SqlitePool, user_id: &str, book_id: &str, is_archived: bool) -> anyhow::Result<()> {
      let now = Utc::now().to_rfc3339();
      sqlx::query(
          r#"
          INSERT INTO book_user_state (user_id, book_id, is_read, is_archived, updated_at)
          VALUES (?, ?, 0, ?, ?)
          ON CONFLICT(user_id, book_id) DO UPDATE SET
              is_archived = excluded.is_archived,
              updated_at = excluded.updated_at
          "#,
      )
      .bind(user_id)
      .bind(book_id)
      .bind(is_archived as i64)
      .bind(&now)
      .execute(db)
      .await?;
      Ok(())
  }

Remove the get_state pre-read from both functions entirely.

Add test to backend/tests/test_book_user_state.rs:
  test_set_read_does_not_reset_archived — set is_archived=true, then
    set is_read=true; assert is_archived is still true in the final state.
  test_set_archived_does_not_reset_read — mirror test.

─────────────────────────────────────────
FIX 2 — download_history: Add FK + Fix Count/Items Mismatch
─────────────────────────────────────────

Problem A: download_history.book_id has no REFERENCES books(id) constraint.
When a book is deleted, its download_history rows survive. The list query uses
INNER JOIN books, so those orphan rows are excluded from items but still
counted in the COUNT(*) total. This causes pagination to show phantom empty
pages.

Problem B: The count query and item query disagree — count uses
COUNT(*) FROM download_history WHERE user_id = ? but items use an INNER JOIN
that silently drops orphan rows.

Fix A: Create a new migration backend/migrations/sqlite/0014_download_history_fk.sql:
  -- Add FK with SET NULL so history is preserved but marked as deleted
  -- SQLite does not support ADD CONSTRAINT after CREATE TABLE, so recreate:
  CREATE TABLE download_history_new (
      id            TEXT PRIMARY KEY,
      user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      book_id       TEXT REFERENCES books(id) ON DELETE SET NULL,
      format        TEXT NOT NULL,
      downloaded_at TEXT NOT NULL
  );
  INSERT INTO download_history_new SELECT * FROM download_history;
  DROP TABLE download_history;
  ALTER TABLE download_history_new RENAME TO download_history;
  CREATE INDEX idx_download_history_user ON download_history(user_id);
  CREATE INDEX idx_download_history_book ON download_history(book_id);

backend/migrations/mariadb/0013_download_history_fk.sql — equivalent DDL
using ALTER TABLE download_history MODIFY COLUMN book_id VARCHAR(36)
REFERENCES books(id) ON DELETE SET NULL.

Fix B: In backend/src/db/queries/download_history.rs, update both the count
query and item query to use LEFT JOIN (not INNER JOIN) and handle NULL book_id:
  -- Count query:
  SELECT COUNT(*) FROM download_history WHERE user_id = ?
  -- Item query:
  SELECT dh.id, dh.book_id, COALESCE(b.title, '[Deleted book]') AS title,
         dh.format, dh.downloaded_at
  FROM download_history dh
  LEFT JOIN books b ON b.id = dh.book_id
  WHERE dh.user_id = ?
  ORDER BY dh.downloaded_at DESC
  LIMIT ? OFFSET ?

Update the DownloadHistoryEntry struct to handle title as a String (never
Option — use the COALESCE default).

Add test to backend/tests/test_download_history.rs:
  test_download_history_shows_deleted_book_as_placeholder — record a download,
    delete the book, fetch history; assert the entry is still present with
    title = "[Deleted book]" and total count matches items count.

─────────────────────────────────────────
FIX 3 — merge_books: Reassign download_history + book_custom_values
─────────────────────────────────────────

Problem A: The merge_books transaction in backend/src/db/queries/books.rs
does not reassign download_history rows. After merge the duplicate's history
entries become orphans (or NULLs after Fix 2 above) associated with a
deleted book.

Problem B: book_custom_values for the duplicate are silently deleted via ON
DELETE CASCADE when the duplicate book is deleted. If the primary has no value
for a given column but the duplicate does, that information is lost.

Fix: Inside the merge_books transaction, add two steps immediately before the
DELETE FROM books step:

Step A — Reassign download_history:
  sqlx::query("UPDATE download_history SET book_id = ? WHERE book_id = ?")
      .bind(primary_id)
      .bind(duplicate_id)
      .execute(tx.as_mut())
      .await
      .context("reassign download history")?;

Step B — Merge custom column values (primary wins on conflict):
  sqlx::query(
      r#"
      INSERT OR IGNORE INTO book_custom_values
          (id, book_id, column_id, value_text, value_int, value_float, value_bool)
      SELECT
          lower(hex(randomblob(16))), ?, column_id,
          value_text, value_int, value_float, value_bool
      FROM book_custom_values
      WHERE book_id = ?
        AND column_id NOT IN (
            SELECT column_id FROM book_custom_values WHERE book_id = ?
        )
      "#,
  )
  .bind(primary_id)
  .bind(duplicate_id)
  .bind(primary_id)
  .execute(tx.as_mut())
  .await
  .context("merge custom column values")?;

Add tests to backend/tests/test_merge.rs:
  test_merge_reassigns_download_history — seed download history for the
    duplicate book; after merge, assert history entries have primary_id as
    book_id.
  test_merge_transfers_custom_values_when_primary_has_none — seed a custom
    value on the duplicate for a column the primary doesn't have; after merge,
    assert primary has that value.
  test_merge_primary_custom_value_wins_on_conflict — seed different values
    for the same column on both books; after merge, assert primary's value
    is kept.

─────────────────────────────────────────
FIX 4 — Scheduler: Fix Schedule Drift
─────────────────────────────────────────

Problem: After a task fires, mark_scheduled_task_ran in backend/src/scheduler.rs
calls next_run_at_for_cron(&task.cron_expr, Utc::now()). Using Utc::now()
causes drift — if the tick was delayed (e.g., system under load for 10 minutes),
the next fire time is calculated from "now" rather than from the originally
scheduled time. A "0 2 * * *" task firing at 2:10 AM schedules its next run
at 2:10+1d instead of 2:00+1d.

Fix: In mark_scheduled_task_ran (or wherever next_run_at is computed after
dispatch), parse task.next_run_at as a DateTime<Utc> and use it as the base
for the cron computation:

  let scheduled_base = task.next_run_at
      .as_deref()
      .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
      .map(|dt| dt.with_timezone(&Utc))
      .unwrap_or_else(Utc::now);

  let next_run_at = next_run_at_for_cron(&task.cron_expr, scheduled_base)
      .map(|dt| dt.to_rfc3339())
      .unwrap_or_else(|| Utc::now().to_rfc3339());

Add test to backend/tests/test_scheduled_tasks.rs:
  test_scheduler_no_drift — create a task with cron "0 2 * * *" whose
    next_run_at is "2026-01-01T02:00:00Z"; simulate a delayed dispatch at
    "2026-01-01T02:10:00Z"; assert the computed next_run_at is
    "2026-01-02T02:00:00Z" (not "2026-01-02T02:10:00Z").

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/
git commit -m "Phase 10 fixes Stage 2: atomic state flags, download history FK, merge completeness, scheduler drift"
```

---

## STAGE 3 — UX, Translation, and Polish Fixes

**Priority: Medium/Low**
**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read apps/web/public/locales/, apps/web/src/features/admin/ScheduledTasksPage.tsx,
apps/web/src/features/admin/DashboardPage.tsx, backend/src/api/opds.rs,
and backend/src/api/admin.rs. Fix the following issues in order.

─────────────────────────────────────────
FIX 1 — Missing and Mismatched Translation Keys
─────────────────────────────────────────

Problem A: t("common.ready") is used in DashboardPage.tsx but the key "ready"
is missing from the "common" section in all four locale files (en, fr, de, es).
With parseMissingKeyHandler returning the key, the UI renders the raw string
"common.ready".

Problem B: admin.scheduled_tasks key is missing from de, fr, and es locale
files. Any sidebar nav or page title that uses this key renders the raw key.

Problem C: book.unarchive key exists in de, fr, and es but not in en. Either
the EN locale is missing the key, or the key is unused and should be removed
from the other locales.

Fix:

In apps/web/public/locales/en/translation.json:
  - Add under "common": { "ready": "Ready" }
  - Confirm book.unarchive exists; if not, add: "unarchive": "Unarchive"
  - Confirm admin.scheduled_tasks exists; if not, add: "scheduled_tasks": "Scheduled tasks"

In apps/web/public/locales/fr/translation.json:
  - Add under "common": { "ready": "Prêt" }
  - Add under "admin": { "scheduled_tasks": "Tâches planifiées" }
  - Confirm book.unarchive; if missing add: "unarchive": "Désarchiver"

In apps/web/public/locales/de/translation.json:
  - Add under "common": { "ready": "Bereit" }
  - Add under "admin": { "scheduled_tasks": "Geplante Aufgaben" }
  - Confirm book.unarchive; if missing add: "unarchive": "Archivierung aufheben"

In apps/web/public/locales/es/translation.json:
  - Add under "common": { "ready": "Listo" }
  - Add under "admin": { "scheduled_tasks": "Tareas programadas" }
  - Confirm book.unarchive; if missing add: "unarchive": "Desarchivar"

Also mirror the same keys in the mobile locale files under
apps/mobile/src/locales/ for each language.

─────────────────────────────────────────
FIX 2 — ScheduledTasksPage: Add Error States
─────────────────────────────────────────

Problem: apps/web/src/features/admin/ScheduledTasksPage.tsx never checks
tasksQuery.isError. Mutation errors (create, update, delete) are also not
surfaced to the user — failures are silent.

Fix:

Add an error banner at the top of the page when tasksQuery.isError is true:
  {tasksQuery.isError && (
    <div className="rounded border border-red-500 bg-red-950 px-4 py-3 text-red-300">
      {t("errors.load_failed")}
    </div>
  )}

For createMutation, updateMutation, and deleteMutation, add onError callbacks
that set a local error state string, and render it as an inline error message
below the relevant form or button. Clear the error on the next successful
mutation.

Add "errors.load_failed" to all four locale files if not already present.
  en: "Failed to load data. Please refresh."
  fr: "Échec du chargement. Veuillez actualiser."
  de: "Laden fehlgeschlagen. Bitte aktualisieren."
  es: "Error al cargar. Por favor, actualice."

─────────────────────────────────────────
FIX 3 — OPDS: Consistent 404 for Unknown Publisher
─────────────────────────────────────────

Problem: author_books_feed and series_books_feed return 404 for unknown IDs
but publisher_books_feed returns an empty 200 for unknown publishers. This is
inconsistent behaviour that confuses OPDS clients.

Publishers are stored as free-text (not a separate table), so there is no
publisher ID to look up. The simplest fix is to match the empty-result-is-200
behaviour and remove the strict NotFound checks from author and series feeds too,
making all feeds consistent.

Read the current author_books_feed and series_books_feed handlers. If they
do a DB existence check and return NotFound for unknown IDs, replace that with:
if the book list is empty after the filtered query, return an empty acquisition
feed (not 404). This matches OPDS-PS 1.2 behaviour (empty feeds are valid).

If the author/series ID checks are needed for URL validation (preventing
garbage IDs from hitting the DB), keep the check but change the response from
AppError::NotFound to an empty OPDS acquisition feed.

Choose one approach and apply it consistently to all browse acquisition feed
handlers: author, series, publisher, language, rating.

─────────────────────────────────────────
FIX 4 — list_libraries: Remove N+1 for Book Counts
─────────────────────────────────────────

Problem: list_libraries in backend/src/api/admin.rs (or the query helper it
calls) fetches all libraries then calls count_books_in_library once per library.
For an admin with N libraries this is N+1 queries.

Fix: In backend/src/db/queries/libraries.rs, replace the per-library count
with a single aggregation query joined to the library list:

  SELECT l.id, l.name, l.calibre_db_path, l.created_at, l.updated_at,
         COUNT(b.id) AS book_count
  FROM libraries l
  LEFT JOIN books b ON b.library_id = l.id
  GROUP BY l.id

Update the Library struct to include book_count: i64.
Remove the separate count_books_in_library call from the list handler.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @xs/web build
git add backend/ apps/
git commit -m "Phase 10 fixes Stage 3: translation keys, error states, OPDS consistency, library N+1"
```

---

## STAGE 4 — Stage 5 (Readers) Review Findings

**Priority: TBD after Stage 5 review**
**Model: TBD**

> ⚠️ This stage is a placeholder. Run `/review` and `/security-review` after
> Phase 10 Stage 5 (DJVU, audio, MOBI/AZW3 readers) is implemented, then
> populate this section with findings before running the Codex prompt.

**Known areas to watch in Stage 5:**
- DJVU: client-side WASM library — ensure no XSS via untrusted DJVU content
- Audio: range request handling — confirm Content-Range header is correct for all audio formats
- MOBI/AZW3 to-epub route: must reuse `safe_storage_path` path traversal guard
- MOBI conversion ZIP output: verify mimetype entry is stored uncompressed (EPUB spec requires this)
- EpubReader `streamUrl` prop: ensure user-supplied URL cannot be used to SSRF internal endpoints

---

## STAGE 5 — Stage 8 (Format Conversion) Review Findings

**Priority: TBD after Stage 8 review**
**Model: TBD**

> ⚠️ This stage is a placeholder. Run `/review` and `/security-review` after
> Phase 10 Stage 8 (server-side format conversion via ebook-convert) is
> implemented, then populate this section with findings before running the
> Codex prompt.

**Known areas to watch in Stage 8:**
- Shell injection: ebook-convert args must come from the allowlist, never user input
- Temp directory cleanup: must be unconditional even on panic (use TempDir drop guard)
- Process timeout: kill_on_drop(true) must be set; verify it fires on future drop
- SSRF via capabilities endpoint: GET /system/capabilities must not leak internal paths
- Conversion output: ensure the streamed file is the output not the input
- File size limit: a 500MB EPUB converting to MOBI could temporarily double disk usage

---

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/security-review` — re-verify OPDS auth, proxy header, cache fix |
| Stage 2 | `/review` — verify transaction ordering in merge, migration correctness |
| Stage 3 | `/review` — verify all 4 locale files complete, no raw keys in UI |
| Stage 4 | Populated after Phase 10 Stage 5 review |
| Stage 5 | Populated after Phase 10 Stage 8 review |
