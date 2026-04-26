# Codex Desktop App — calibre-web-rs Phase 2: Calibre Migration CLI

## What Phase 2 Builds

A standalone `calibre-migrate` binary that reads an existing Calibre library
(`metadata.db` + book files) and imports it into calibre-web-rs. This is how
users move their existing library into the new system.

Key behaviours:
- `--dry-run` — show what would be imported, write nothing
- Idempotent — safe to re-run, skips already-imported books
- Validation report — counts of imported / skipped / failed with reasons
- Covers — copies Calibre cover images into calibre-web-rs bucketed storage

---

## Before You Start — One-Time Setup (Terminal)

Docs are already in `calibre-web-rs/docs/` from Phase 1. No extra setup needed.
Open the Codex desktop app and point it at:
```
~/Documents/localProject/calibre-web-rs
```

---

## How Calibre Stores Data

Codex needs this to implement the reader correctly.

### Calibre Schema (metadata.db)

Key tables:
```
books          — id, title, sort, author_sort, pubdate, series_index, rating, flags, has_cover, last_modified
authors        — id, name, sort
books_authors_link — book, author
series         — id, name, sort
books_series_link  — book, series
tags           — id, name
books_tags_link    — book, tag
publishers     — id, name, sort
books_publishers_link — book, publisher
ratings        — id, rating  (stored 0–10, maps to our 0–10)
books_ratings_link — book, rating
comments       — id, book, text  (book description)
identifiers    — id, book, type, val  (isbn, isbn13, asin, etc.)
data           — id, book, format, uncompressed_size, name
```

### Calibre File Path Convention

Given a book record, the file lives at:
```
<library_path>/<author_dir>/<book_dir>/<data.name>.<data.format.lower()>
```
Where:
- `author_dir` = `books.author_sort` sanitized (Calibre replaces bad chars with `_`)
- `book_dir`   = `"<books.title> (<books.id>)"` sanitized

Calibre cover images live at:
```
<library_path>/<author_dir>/<book_dir>/cover.jpg
```
`books.has_cover` is 1 when this file exists.

### Idempotency Key

Use `identifiers.type = 'calibre_id'` with `val = books.id` to detect
already-imported books. Insert this identifier on import.

---

## STAGE 1 — Scaffold + Write All Tests

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/HANDOFF.md for project conventions, layout, and test patterns.

Now scaffold Phase 2: the calibre-migrate binary crate. Write ALL test files
with #[ignore] or todo!() bodies — no implementation yet.

Deliverables:
- calibre-migrate/Cargo.toml — binary crate, add to workspace Cargo.toml
- calibre-migrate/src/main.rs — CLI entrypoint stub (clap args: --source,
  --target-db, --target-storage, --dry-run, --report-path)
- calibre-migrate/src/lib.rs — module declarations only
- calibre-migrate/src/calibre/mod.rs — stub
- calibre-migrate/src/calibre/reader.rs — stub
- calibre-migrate/src/calibre/schema.rs — Calibre model structs:
  CalibreBook, CalibreAuthor, CalibeSeries, CalibreTag, CalibreFormat,
  CalibreIdentifier, CalibreComment
- calibre-migrate/src/import/mod.rs — stub
- calibre-migrate/src/import/pipeline.rs — stub
- calibre-migrate/src/import/covers.rs — stub
- calibre-migrate/src/report.rs — MigrationReport struct stub
- calibre-migrate/tests/common/mod.rs — helpers:
    create_calibre_fixture_db() — builds a minimal in-memory Calibre
    metadata.db with 3 books (one with cover, one with ISBN, one mobi),
    calibre_fixture_library_dir() — temp dir with fake book files and cover.jpg
- calibre-migrate/tests/test_reader.rs — all test stubs:
    test_reader_loads_books
    test_reader_loads_authors
    test_reader_loads_formats
    test_reader_loads_identifiers
    test_reader_loads_comments
    test_reader_loads_series
    test_reader_loads_tags
- calibre-migrate/tests/test_import.rs — all test stubs:
    test_import_creates_book_in_target_db
    test_import_copies_book_file_to_storage
    test_import_copies_cover_to_storage
    test_import_skips_duplicate_calibre_id
    test_import_missing_file_is_skipped_not_fatal
    test_import_multiple_authors
    test_import_multiple_formats
    test_import_identifiers_preserved
- calibre-migrate/tests/test_dryrun.rs — all test stubs:
    test_dryrun_writes_nothing_to_db
    test_dryrun_writes_nothing_to_storage
    test_dryrun_report_shows_expected_counts
    test_report_counts_skipped_books
    test_report_counts_failed_books

When done, run:
  cargo check --workspace 2>&1 | head -30
  ls calibre-migrate/tests/
  git diff --stat
```

**Paste output here → Claude reviews → proceed if clean.**

---

## STAGE 2 — Calibre DB Reader

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 2 of Phase 2.

Implement the Calibre database reader. Make all tests in test_reader.rs pass.

The Calibre schema is documented in CODEX_COMMANDS_PHASE2.md at the repo root
(or in docs/ if copied there). Key tables: books, authors, books_authors_link,
series, books_series_link, tags, books_tags_link, ratings, books_ratings_link,
comments, identifiers, data.

Deliverables:
- calibre-migrate/src/calibre/schema.rs — fully populated structs:
    CalibreBook { id, title, sort, author_sort, pubdate, series_index, rating,
                  has_cover, last_modified }
    CalibreAuthor { id, name, sort }
    CalibeSeries { id, name, sort }
    CalibreTag { id, name }
    CalibreFormat { id, book_id, format, name, uncompressed_size }
    CalibreIdentifier { id, book_id, id_type, value }
    CalibreComment { id, book_id, text }
    CalibreEntry — aggregated single book with all relations loaded
- calibre-migrate/src/calibre/reader.rs — CalibreReader struct:
    fn open(library_path: &Path) -> anyhow::Result<Self>
      opens <library_path>/metadata.db as read-only SQLite
    fn read_all_entries(&self) -> anyhow::Result<Vec<CalibreEntry>>
      loads all books with relations in a single efficient pass
    fn file_path(&self, entry: &CalibreEntry, format: &CalibreFormat) -> PathBuf
      constructs: library_path/author_sort_sanitized/title_id_sanitized/name.ext
    fn cover_path(&self, entry: &CalibreEntry) -> Option<PathBuf>
      constructs cover.jpg path, returns None if has_cover = 0

Remove #[ignore] from test_reader.rs tests as you implement each.

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test test_reader -- --nocapture 2>&1

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file (calibre/reader.rs, calibre/schema.rs).
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 3 — Import Pipeline + Cover Handling

**Model: GPT-5.3-Codex, High effort**

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 3 of Phase 2.

Implement the import pipeline and cover handling. Make all tests in
test_import.rs pass.

Deliverables:
- calibre-migrate/src/import/pipeline.rs — ImportPipeline struct:
    fn new(target_db: SqlitePool, storage: LocalFs, dry_run: bool) -> Self
    async fn run(&self, entries: Vec<CalibreEntry>, reader: &CalibreReader)
      -> anyhow::Result<MigrationReport>

  Per-book logic (in order):
  1. Check identifiers for type='calibre_id', val=entry.id — skip if found
  2. Validate at least one format file exists on disk — skip book if none found,
     record as failed with reason
  3. If dry_run: increment counters, continue — do not write anything
  4. Copy each format file to storage: books/{first2_of_uuid}/{uuid}.ext
  5. If has_cover and cover.jpg exists: copy and resize to
     covers/{first2_of_uuid}/{book_id}.jpg + .thumb.jpg (400x600, 100x150)
     using the same image crate logic as the main backend
  6. Insert book, authors (get_or_create), tags (get_or_create), series,
     identifiers, formats into target DB in a single transaction
  7. Insert identifier type='calibre_id' val=<calibre book id> for idempotency

- calibre-migrate/src/import/covers.rs — cover copy + resize logic
  (reuse render_cover_variants pattern from backend/src/api/books.rs)

- calibre-migrate/src/report.rs — MigrationReport:
    total: usize
    imported: usize
    skipped: usize  (already imported)
    failed: usize
    failures: Vec<FailureRecord { calibre_id, title, reason }>
    fn print_summary(&self)
    fn to_json(&self) -> String

Remove #[ignore] from test_import.rs tests as you implement each.

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test test_import -- --nocapture 2>&1

    If any test fails:
      1. Read the full error output.
      2. Read the relevant source file (import/pipeline.rs, import/covers.rs).
      3. Fix the implementation. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 4 — Dry-Run, Reporting, Final Checks

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/HANDOFF.md. Now do Stage 4 of Phase 2.

Wire everything into the CLI binary, implement dry-run, and make all tests in
test_dryrun.rs pass. Then run all CI checks.

Deliverables:
- calibre-migrate/src/main.rs — fully implemented:
    clap args:
      --source <path>         Calibre library directory (contains metadata.db)
      --target-db <url>       Target DB URL (e.g. sqlite:///app/storage/library.db)
      --target-storage <path> Target storage root directory
      --dry-run               Report what would be imported, write nothing
      --report-path <path>    Optional: write JSON report to this file

    Execution flow:
      1. Load target DB (run sqlx migrations)
      2. Open CalibreReader on --source
      3. Read all entries
      4. Run ImportPipeline with dry_run flag
      5. Print summary via MigrationReport::print_summary()
      6. If --report-path given, write JSON report
      7. Exit 0 on success, 1 if any failures

Remove #[ignore] from test_dryrun.rs tests as you implement each.

When done, run ALL of these and show the complete output:
  cargo test --workspace 2>&1
  cargo clippy --workspace -- -D warnings 2>&1
  cargo audit 2>&1
  git diff --stat
```

**Paste output here → Claude runs /review + /engineering:deploy-checklist → Phase 2 complete.**

---

## Review Checkpoints

| After stage | Skill | Purpose |
|---|---|---|
| Stage 1 | `/review` | Structure, test harness shape |
| Stage 2 | `/review` | Reader correctness, Calibre schema mapping |
| Stage 3 | `/review` + `/simplify` | Import logic, cover handling, idempotency |
| Stage 4 | `/review` + `/engineering:deploy-checklist` | CLI wiring, final CI pass |

---

## If Codex Gets Stuck or a Test Fails

Paste the error output back into Codex with:

```
The following test is failing. Diagnose the root cause and fix it.
Do not work around it — fix the underlying issue.

[paste error output]
```

If still stuck, paste the error here (to Claude) and run /engineering:debug.
