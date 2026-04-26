# Codex Desktop App — calibre-web-rs Phase 5: LLM Classification Features + Agentic RAG Surface

## What Phase 5 Builds

Ports the LLM classification pipeline from the original Python implementation to Rust,
adds a job-queue-backed async classification workflow, wires the UI for tag suggestions,
validation results, and derived works, and adds the text extraction API that makes this
library a tool provider for external agentic RAG systems:

- `ChatClient` — async LM Studio chat completions client using the librarian role config
- Book classification — LLM suggests tags; user confirms or rejects before they take effect
- Metadata validation — LLM checks fields for completeness and accuracy issues
- Content quality check — LLM scores prose quality and flags formatting problems
- Derived works — LLM generates a summary, related titles, and discussion questions
- Library organize — admin-triggered bulk classification queued as background jobs
- Admin Jobs page — real-time view of the LLM job queue with cancel support
- **Text extraction API** — chapter listing and plain-text extraction from EPUB/PDF; no LLM required;
  designed as a tool surface for external agents (LangGraph, smolagents, etc.)

## Key Schema Facts (already in 0001_initial.sql — do not recreate)

- `book_tags(book_id, tag_id, confirmed)` — `confirmed = 0` means LLM suggestion pending user review;
  `confirmed = 1` means accepted and live
- `tags(id, name, source)` — source CHECK includes `'manual'`, `'llm'`, `'calibre_import'`
- `llm_jobs(id, job_type, status, book_id, ...)` — job_type CHECK already covers
  `classify`, `semantic_index`, `quality_check`, `validate_metadata`, `organize`, `derive`
- `llm_jobs.status` CHECK: `pending / running / completed / failed`
- The job runner in `llm/job_runner.rs` currently handles `semantic_index` only —
  Stage 1 extends it to `classify` and Stage 2 extends it to `organize`

## Reference Files

Read these before starting each stage:
- `docs/ARCHITECTURE.md` — LLM graceful degradation rules (10s timeout, silent fallback,
  `Option<ChatClient>` in AppState, `llm.enabled = false` by default, all LLM errors
  silently swallowed and never surfaced to users)
- `docs/API.md` — LLM route contracts for `/llm/health`, `/books/:id/classify`,
  `/books/:id/tags/confirm`, `/books/:id/tags/confirm-all`, `/books/:id/validate`,
  `/books/:id/quality`, `/books/:id/derive`, `/organize`, `/admin/jobs`

---

## STAGE 1 — Chat Client + Classify + Confirm API

**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md and docs/API.md. Now do Stage 1 of Phase 5.

Build the ChatClient and the book classification pipeline. The EmbeddingClient in
llm/embeddings.rs is a useful structural reference — ChatClient follows the same
patterns (10s timeout, silent errors, Option<Self> when disabled) but calls
/v1/chat/completions instead of /v1/embeddings.

Deliverables:

backend/src/llm/chat.rs — ChatClient:
  Fields: endpoint (String), model (String), system_prompt (String), http (reqwest::Client).
  pub fn new(config: &AppConfig) -> Option<Self>
    Return None when config.llm.enabled is false or librarian endpoint is empty.
  pub fn is_configured(&self) -> bool
  pub fn model_id(&self) -> &str
  pub async fn complete(&self, user_message: &str) -> anyhow::Result<String>
    POST /v1/chat/completions. Normalise endpoint: strip trailing slash, append
    /v1/chat/completions (handle paths that already include /v1).
    Body: { "model": self.model, "messages": [
      { "role": "system", "content": self.system_prompt },
      { "role": "user", "content": user_message }
    ]}
    Parse response.choices[0].message.content.
    10-second timeout. Return Err on non-2xx or missing content field.

backend/src/llm/classify.rs:
  pub struct TagSuggestion { pub name: String, pub confidence: f32 }
  pub struct ClassifyResult { pub suggestions: Vec<TagSuggestion>, pub model_id: String }

  pub async fn classify_book(
    client: &ChatClient, title: &str, authors: &str, description: &str
  ) -> ClassifyResult
    Build user message:
      "Title: {title}\nAuthors: {authors}\nDescription: {description}\n\n
       Classify this book. Return JSON only:\n
       {\"tags\": [{\"name\": \"...\", \"confidence\": 0.0-1.0}]}\n
       Return 3-8 tags. No prose, no markdown fences."
    Call client.complete(). Parse JSON response.
    If JSON parse fails: try extracting the first { ... } block with a regex.
    On total parse failure: return ClassifyResult with empty suggestions — never Err.

backend/src/db/queries/llm.rs — add:
  pub async fn insert_tag_suggestions(
    db: &SqlitePool, book_id: &str, suggestions: &[TagSuggestion]
  ) -> anyhow::Result<usize>
    For each suggestion:
      INSERT OR IGNORE INTO tags (id, name, source, last_modified) VALUES (..., 'llm', ...)
      INSERT OR IGNORE INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 0)
    Return count of newly inserted book_tags rows.

  pub async fn list_pending_tags(db: &SqlitePool, book_id: &str)
    -> anyhow::Result<Vec<(String, String)>>
    SELECT t.id, t.name FROM book_tags bt JOIN tags t ON t.id = bt.tag_id
    WHERE bt.book_id = ? AND bt.confirmed = 0

  pub async fn confirm_tags(
    db: &SqlitePool, book_id: &str,
    confirm_names: &[String], reject_names: &[String]
  ) -> anyhow::Result<usize>
    UPDATE book_tags SET confirmed = 1 WHERE book_id = ? AND tag_id IN
      (SELECT id FROM tags WHERE name IN {confirm_names})
    DELETE FROM book_tags WHERE book_id = ? AND tag_id IN
      (SELECT id FROM tags WHERE name IN {reject_names})
    Return confirmed row count.

  pub async fn confirm_all_pending_tags(db: &SqlitePool, book_id: &str)
    -> anyhow::Result<usize>
    UPDATE book_tags SET confirmed = 1 WHERE book_id = ? AND confirmed = 0

backend/src/llm/mod.rs — add: pub mod chat; pub mod classify;

backend/src/api/llm.rs — four handlers, wired into api/mod.rs under /api/v1:

  GET /api/v1/llm/health — any auth
    Ping GET /v1/models on the librarian endpoint with a 3-second timeout.
    Response: { "enabled": bool, "librarian": { "available": bool,
      "model_id": String|null, "endpoint": String } }
    When chat_client is None: enabled false, available false.

  GET /api/v1/books/:id/classify — any auth
    503 { "error": "llm_unavailable" } when state.chat_client is None.
    404 when book not found.
    Load title/authors/description from DB. Call classify_book().
    Call insert_tag_suggestions(). Call list_pending_tags() for pending_count.
    Response: { "book_id", "suggestions": [{ "name", "confidence" }],
      "model_id", "pending_count" }

  POST /api/v1/books/:id/tags/confirm — can_edit role
    Body: { "confirm": [String], "reject": [String] }
    Call confirm_tags(). Return updated Book (reuse book detail loader from api/books.rs).

  POST /api/v1/books/:id/tags/confirm-all — can_edit role
    No body. Call confirm_all_pending_tags(). Return updated Book.

AppState — add: pub chat_client: Option<ChatClient>
  Initialise from config in main.rs using ChatClient::new(&config).

backend/tests/llm_classify.rs — new file, use wiremock:
  test_classify_inserts_pending_tags
    Mock POST /v1/chat/completions returning:
      {"choices":[{"message":{"content":"{\"tags\":[{\"name\":\"Science Fiction\",\"confidence\":0.92}]}"}}]}
    Call GET /api/v1/books/:id/classify. Assert book_tags row with confirmed=0 in DB.
  test_confirm_tags_marks_confirmed
    Classify then POST /books/:id/tags/confirm { confirm: ["Science Fiction"], reject: [] }.
    Assert book_tags.confirmed = 1 in DB.
  test_reject_tags_removes_row
    Classify then confirm with reject: ["Science Fiction"]. Assert row deleted from DB.
  test_classify_returns_503_when_disabled
    No LLM config. Assert GET /books/:id/classify returns 503.

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test llm_classify -- --nocapture 2>&1
    cargo test --workspace 2>&1 | tail -20

    If any test fails:
      1. Read the full error output.
      2. Read the relevant handler/query source file.
      3. Fix the implementation or the test. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 2 — Validate + Quality + Derive + Organize + Text Extraction

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/ARCHITECTURE.md and docs/API.md. Now do Stage 2 of Phase 5.

Add four more LLM features following the exact same pattern as classify_book in
llm/classify.rs: build a user message, call ChatClient::complete(), parse JSON
leniently, return a typed struct with sensible defaults on parse failure. LLM
errors must never propagate to the API caller.

Also add the text extraction API — this has NO LLM dependency and must work
regardless of llm.enabled. It is the foundational content surface for agentic RAG.

Deliverables:

backend/src/llm/validate.rs:
  pub struct ValidationIssue { pub field: String, pub severity: String,
    pub message: String, pub suggestion: Option<String> }
  pub struct ValidationResult { pub severity: String, pub issues: Vec<ValidationIssue>,
    pub model_id: String }
  pub async fn validate_book(client: &ChatClient, title: &str, authors: &str,
    description: &str, language: Option<&str>) -> ValidationResult
    Prompt: ask LLM to check for missing/thin description, missing author, dubious
    language code. Return JSON matching the struct shape above.
    On parse failure: severity "ok", empty issues.

backend/src/llm/quality.rs:
  pub struct QualityIssue { pub issue_type: String, pub severity: String,
    pub message: String }
  pub struct QualityResult { pub score: f32, pub issues: Vec<QualityIssue>,
    pub model_id: String }
  pub async fn check_quality(client: &ChatClient, title: &str,
    description: &str) -> QualityResult
    Prompt: ask LLM to score prose quality 0.0–1.0 and list formatting or content issues.
    On parse failure: score 0.5, empty issues.

backend/src/llm/derive.rs:
  pub struct DeriveResult { pub summary: String, pub related_titles: Vec<String>,
    pub discussion_questions: Vec<String>, pub model_id: String }
  pub async fn derive_book(client: &ChatClient, title: &str, authors: &str,
    description: &str) -> DeriveResult
    Prompt: ask for a one-paragraph summary, 3–5 related titles, 3–5 discussion
    questions. On parse failure: empty strings/vecs.

backend/src/llm/classify_type.rs — document type classification (runs synchronously at ingest):
  pub enum DocumentType { Novel, Textbook, Reference, Magazine, Datasheet, Comic, Unknown }
  impl DocumentType {
    pub fn as_str(&self) -> &'static str  // "novel", "textbook", etc.
    pub fn from_str(s: &str) -> Self      // parse DB value; unrecognised → Unknown
  }

  pub async fn classify_document_type(
    client: &ChatClient, title: &str, authors: &str, description: &str
  ) -> DocumentType
    Prompt: "Classify this book into exactly one category: novel, textbook, reference,
      magazine, datasheet, comic, or unknown. Title: {title}. Authors: {authors}.
      Description: {description}. Reply with the single category word only."
    Call client.complete(). Trim response, match to DocumentType.
    On any failure or unrecognised value: return DocumentType::Unknown — never Err.

backend/src/api/books.rs — update POST /books (upload) handler:
  After metadata extraction and before writing to DB:
    If state.chat_client is Some(client):
      Call classify_document_type(client, title, authors, description).await
      Store result as document_type on the new book record.
    Else: document_type = DocumentType::Unknown.
  This call is synchronous and within the upload handler — it has the same 10s timeout
  as all other LLM calls. On timeout: fall through to Unknown, never fail the upload.

backend/src/llm/mod.rs — add: pub mod validate; pub mod quality; pub mod derive; pub mod classify_type;

backend/src/ingest/text.rs — text extraction (no LLM dependency):
  pub struct Chapter { pub index: u32, pub title: String, pub word_count: usize }

  pub fn list_chapters(path: &Path, format: &str) -> anyhow::Result<Vec<Chapter>>
    EPUB: unzip → parse META-INF/container.xml → load OPF → read spine items in order
      → for each spine item: parse HTML, extract <title> or first <h1>/<h2> as chapter title,
        count words in text content. Return ordered Vec<Chapter>.
    PDF: count pages, group into 5-page chunks, title each "Pages N–M".
    On any failure: return Ok(vec![]) — never Err to callers.

  pub fn extract_text(path: &Path, format: &str, chapter: Option<u32>) -> anyhow::Result<String>
    EPUB: unzip → find spine item at index `chapter` (or all items if None)
      → parse HTML with a lightweight HTML stripper (no external parser needed —
        strip tags via regex `<[^>]+>`, decode common HTML entities, normalize whitespace)
      → concatenate chapters with "\n\n---\n\n" separator when chapter is None.
    PDF: extract text from the page range corresponding to chapter N (5 pages per group),
      or all pages when chapter is None.
    On any failure: return Ok(String::new()) — never Err to callers.

  Register text.rs in backend/src/ingest/mod.rs: pub mod text;

backend/src/api/books.rs — add two handlers, wire into api/mod.rs under /api/v1:

  GET /api/v1/books/:id/chapters — any auth, NO llm guard
    Load book from DB. 404 when not found.
    Find best extractable format: prefer EPUB, fall back to PDF.
    422 { "error": "no_extractable_format" } when neither EPUB nor PDF exists.
    Resolve format file path via StorageBackend. Call list_chapters().
    Response: { "book_id": str, "format": str, "chapters": [{ "index": u32, "title": str, "word_count": usize }] }

  GET /api/v1/books/:id/text — any auth, NO llm guard
    Query param: chapter (optional u32).
    Load book from DB. 404 when not found.
    Find best extractable format. 422 when none.
    Resolve path. Call extract_text(path, format, chapter).
    Response: { "book_id": str, "format": str, "chapter": u32|null, "text": str, "word_count": usize }
    word_count is len of text.split_whitespace().

backend/src/db/queries/llm.rs — add:
  pub async fn enqueue_classify_job(db: &SqlitePool, book_id: &str)
    -> anyhow::Result<bool>
    Same idempotent pattern as enqueue_semantic_index_job — skip if a pending or
    running classify job already exists for this book. Return true if inserted.

  pub async fn enqueue_organize_job(db: &SqlitePool) -> anyhow::Result<String>
    INSERT if no pending or running organize job exists (book_id = NULL).
    Return the job_id of the existing or newly created job.

backend/src/api/llm.rs — add four handlers:
  GET /api/v1/books/:id/validate — any auth, 503 when LLM disabled, 404 when not found
    Call validate_book(). Response: { book_id, severity, issues, model_id }
  GET /api/v1/books/:id/quality — any auth, same guards
    Call check_quality(). Response: { book_id, score, issues, model_id }
  GET /api/v1/books/:id/derive — any auth, same guards
    Call derive_book(). Response: { book_id, summary, related_titles,
      discussion_questions, model_id }
  POST /api/v1/organize — Admin role
    Call enqueue_organize_job(). Response 202: { "job_id": "..." }

backend/src/llm/job_runner.rs — extend the job dispatch:
  Add a job_type field to the existing job struct (currently only semantic_index).
  Match on job.job_type:
    "semantic_index" => existing handler (unchanged)
    "classify" => load book document, call classify_book(), call insert_tag_suggestions(),
      mark job completed or failed
    "organize" => SELECT book ids that have no pending/running/completed classify job;
      call enqueue_classify_job for each, batch max 50; mark organize job completed
    other => tracing::warn!(job_type = other, "unknown job type, skipping")

backend/tests/llm_features.rs — new file:
  test_validate_returns_issues — mock LLM returning issues JSON, assert response has issues array
  test_derive_returns_content — mock LLM, assert summary field is non-empty string
  test_organize_enqueues_job — POST /organize, assert 202, assert job row in DB
  test_organize_idempotent — POST /organize twice, assert only one pending organize job in DB
  test_classify_job_runner — enqueue a classify job, run process_pending_jobs_once,
    assert book_tags rows were created in DB
  test_list_chapters_epub — create_book_with_file("minimal.epub"), GET /books/:id/chapters,
    assert at least one chapter returned with non-empty title and word_count > 0
  test_get_text_full_epub — GET /books/:id/text (no chapter param), assert text field is
    non-empty string, word_count matches text.split_whitespace().count()
  test_get_text_single_chapter — GET /books/:id/text?chapter=0, assert response chapter=0
    and text is shorter than or equal to full-book text
  test_text_works_when_llm_disabled — configure no LLM, GET /books/:id/text, assert 200
    (not 503) — text extraction must never be gated behind llm.enabled
  test_chapters_returns_422_no_extractable_format — create book with only MOBI format,
    GET /books/:id/chapters, assert 422 with error="no_extractable_format"
  test_upload_sets_document_type — mock ChatClient returning "textbook", upload minimal.epub,
    assert returned Book has document_type="textbook"
  test_upload_document_type_defaults_unknown_when_llm_disabled — no LLM config, upload
    minimal.epub, assert document_type="unknown"
  test_upload_document_type_defaults_unknown_on_llm_timeout — mock ChatClient with 11s delay
    (exceeds 10s timeout), upload, assert document_type="unknown" and upload succeeds

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test llm_features -- --nocapture 2>&1
    cargo test --workspace 2>&1 | tail -20

    If any test fails:
      1. Read the full error output.
      2. Read the relevant handler/query source file.
      3. Fix the implementation or the test. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 3 — Admin Jobs API

**Model: GPT-5.4-Mini**

**Paste this into Codex:**

```
Read docs/API.md. Now do Stage 3 of Phase 5.

Add three admin-only endpoints for viewing and managing the LLM job queue. Check
whether backend/src/api/admin.rs already exists — if so, add to it; do not create
a duplicate file.

Deliverables:

backend/src/db/queries/llm.rs — add:
  pub struct JobRow {
    pub id: String, pub job_type: String, pub status: String,
    pub book_id: Option<String>, pub book_title: Option<String>,
    pub created_at: String, pub started_at: Option<String>,
    pub completed_at: Option<String>, pub error_text: Option<String>,
  }

  pub async fn list_jobs(db: &SqlitePool, status: Option<&str>,
    job_type: Option<&str>, page: u32, page_size: u32)
    -> anyhow::Result<(Vec<JobRow>, i64)>
    SELECT lj.*, b.title AS book_title FROM llm_jobs lj
    LEFT JOIN books b ON b.id = lj.book_id
    WHERE (status = ? if Some) AND (job_type = ? if Some)
    ORDER BY lj.created_at DESC LIMIT ? OFFSET ?
    Also return total count for pagination.

  pub async fn get_job(db: &SqlitePool, job_id: &str)
    -> anyhow::Result<Option<JobRow>>

  pub async fn cancel_job(db: &SqlitePool, job_id: &str) -> anyhow::Result<bool>
    UPDATE llm_jobs SET status='failed', error_text='cancelled by admin',
      completed_at=now WHERE id=? AND status='pending'
    Return true if a row was affected (false = not found or not pending).

backend/src/api/admin.rs — add three handlers (Admin role required for all):
  GET /api/v1/admin/jobs
    Query params: status (optional), job_type (optional),
      page (default 1), page_size (default 20, max 100).
    Response: PaginatedResponse<JobRow>

  GET /api/v1/admin/jobs/:id
    404 when not found.
    Response: JobRow

  DELETE /api/v1/admin/jobs/:id
    404 when not found.
    409 { "error": "conflict", "message": "Job is not in pending status" }
      when the job exists but status != pending.
    On success: 204 No Content.

Wire all three routes into api/mod.rs under /api/v1/admin.

backend/tests/admin_jobs.rs — new file:
  test_list_jobs_empty — no jobs in DB, assert response items: [], total: 0
  test_list_jobs_filtered_by_status — insert pending + completed job, filter by
    status=pending, assert exactly 1 result returned
  test_get_job_not_found — GET /admin/jobs/{random-uuid}, assert 404
  test_cancel_pending_job — insert pending job, DELETE it, assert 204,
    assert DB row now has status=failed and error_text='cancelled by admin'
  test_cancel_non_pending_job — insert running job, DELETE it, assert 409

TDD BUILD LOOP — do not stop until all tests pass:

  LOOP:
    cargo test --test admin_jobs -- --nocapture 2>&1
    cargo test --workspace 2>&1 | tail -20

    If any test fails:
      1. Read the full error output.
      2. Read the relevant handler/query source file.
      3. Fix the implementation or the test. Never skip a failing test.
      Go back to LOOP.

    If all tests pass: exit loop.

  cargo clippy --workspace -- -D warnings 2>&1
  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## STAGE 4 — Frontend LLM UI

**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read docs/DESIGN.md. Now do Stage 4 of Phase 5.

Wire the frontend to the LLM endpoints built in Stages 1–3. Add a three-tab AI
panel to BookDetailPage and a new Admin Jobs page. Match the existing zinc/teal-600
Tailwind style throughout — no new design patterns.

Deliverables:

packages/shared/src/types.ts — add:
  TagSuggestion = { name: string; confidence: number }
  ClassifyResult = { book_id: string; suggestions: TagSuggestion[];
    model_id: string; pending_count: number }
  ValidationIssue = { field: string; severity: 'warning'|'error';
    message: string; suggestion: string|null }
  ValidationResult = { book_id: string; severity: 'ok'|'warning'|'error';
    issues: ValidationIssue[]; model_id: string }
  DeriveResult = { book_id: string; summary: string; related_titles: string[];
    discussion_questions: string[]; model_id: string }
  LlmHealth = { enabled: boolean; librarian: { available: boolean;
    model_id: string|null; endpoint: string } }
  AdminJob = { id: string; job_type: string;
    status: 'pending'|'running'|'completed'|'failed';
    book_id: string|null; book_title: string|null; created_at: string;
    started_at: string|null; completed_at: string|null; error_text: string|null }
  Chapter = { index: number; title: string; word_count: number }
  BookChapters = { book_id: string; format: string; chapters: Chapter[] }
  BookText = { book_id: string; format: string; chapter: number|null;
    text: string; word_count: number }

packages/shared/src/client.ts — add:
  classifyBook(bookId: string): Promise<ClassifyResult>
    GET /api/v1/books/:id/classify
  confirmTags(bookId: string, confirm: string[], reject: string[]): Promise<Book>
    POST /api/v1/books/:id/tags/confirm
  confirmAllTags(bookId: string): Promise<Book>
    POST /api/v1/books/:id/tags/confirm-all
  validateBook(bookId: string): Promise<ValidationResult>
    GET /api/v1/books/:id/validate
  deriveBook(bookId: string): Promise<DeriveResult>
    GET /api/v1/books/:id/derive
  getLlmHealth(): Promise<LlmHealth>
    GET /api/v1/llm/health
  listAdminJobs(params: { status?: string; job_type?: string; page?: number;
    page_size?: number }): Promise<PaginatedResponse<AdminJob>>
    GET /api/v1/admin/jobs
  cancelAdminJob(jobId: string): Promise<void>
    DELETE /api/v1/admin/jobs/:id
  listChapters(bookId: string): Promise<BookChapters>
    GET /api/v1/books/:id/chapters
  getBookText(bookId: string, chapter?: number): Promise<BookText>
    GET /api/v1/books/:id/text (append ?chapter=N when chapter is provided)

apps/web/src/features/library/BookDetailPage.tsx — add AI panel:
  Add a collapsible "AI" section (same Collapsible component already used in the file)
  below the existing metadata zones. Only render it when getLlmHealth() returns
  enabled: true (useQuery, staleTime: 60_000). The section has three tabs:
  Classify, Validate, Derive.

  Classify tab:
    "Classify" button — useMutation calling classifyBook(bookId).
    Loading: spinner. Error 503: show "LLM unavailable" text.
    On success: render pending suggestions as teal chips with confidence label "(92%)".
    Each pending chip has Confirm and Reject icon buttons inline.
    "Confirm All" button below the pending chip group.
    Confirmed tags appear in the main tag list (they are real book_tags rows with confirmed=1).

  Validate tab:
    "Validate" button — useMutation calling validateBook(bookId).
    On success: severity badge (ok=green, warning=amber, error=red).
    Each issue as a card: field name, message, suggestion if present.
    Empty state when issues is []: "No issues found" with a green checkmark.

  Derive tab:
    "Generate" button — useMutation calling deriveBook(bookId).
    On success: summary as a paragraph, related_titles as a bulleted list,
    discussion_questions as a numbered list.
    Loading and error states follow the same pattern as other tabs.

apps/web/src/features/admin/AdminJobsPage.tsx — new file:
  Table columns: Job ID (first 8 chars), Type, Status badge, Book (title or
    "Library-wide" when book_title is null), Created, Duration, Actions.
  Status badges: pending=amber pill, running=blue pill with spinner, completed=green,
    failed=red.
  Filter bar above table: status select and job_type select (both optional, clear = all).
  Pagination: same pattern as LibraryPage.
  Cancel button visible only for pending rows. Calls cancelAdminJob(id) then
    invalidates the jobs query.
  refetchInterval: 5000 while any item in the list has status "running".

Add AdminJobsPage to the admin router alongside existing admin pages.

apps/web/src/__tests__/BookDetailLlm.test.tsx — new file:
  test_classify_shows_suggestions
    Mock classifyBook returning one suggestion. Assert chip renders with name and confidence.
  test_confirm_tag_calls_api
    Click the confirm button on a suggestion chip. Assert confirmTags was called with
    the correct book ID and tag name.
  test_llm_panel_hidden_when_disabled
    Mock getLlmHealth returning enabled: false. Assert the AI section is not in the DOM.

apps/web/src/__tests__/AdminJobsPage.test.tsx — new file:
  test_jobs_table_renders
    Mock listAdminJobs returning two rows. Assert both render in the table.
  test_cancel_job_calls_api
    Click the cancel button on a pending row. Assert cancelAdminJob was called with
    the correct job ID.
  test_running_job_shows_spinner
    Mock a running-status job. Assert the status cell contains a spinner element.

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
      - /books/1 — AI panel visible below metadata (classify, validate, derive tabs)
      - /books/1 — classify tab shows suggested tags with confirm/reject buttons
      - /books/1 — AI panel hidden entirely when LLM is disabled
      - /admin/jobs — job queue table renders with status column (pending/running/completed)
      - /admin/jobs — running jobs show a spinner in the status cell
      - /admin/jobs — cancel button present on pending rows
    Kill the dev server: kill %1

  git diff --stat
```

**Paste output here → Claude reviews → proceed if passing.**

---

## Review Checkpoints

| After stage | What to verify |
|---|---|
| Stage 1 | ChatClient calls /v1/chat/completions; classify inserts confirmed=0 rows; 503 when LLM disabled |
| Stage 2 | validate/quality/derive return correct shapes; organize enqueues idempotently; job runner dispatches classify and organize; GET /books/:id/text returns 200 even when llm.enabled=false; GET /books/:id/chapters returns spine items from EPUB; upload sets document_type from LLM or falls back to "unknown" without failing |
| Stage 3 | Jobs list paginates; book_title LEFT JOIN is null for organize jobs; cancel returns 409 on non-pending |
| Stage 4 | AI panel absent when LLM disabled; suggestion chips show confidence; Admin Jobs table polls while running; chapter/text client methods exist in packages/shared |

## If Codex Gets Stuck or a Test Fails

```
The following test is failing. Diagnose the root cause and fix it.
Do not work around it — fix the underlying issue.

[paste error output]
```

## Commit Sequence

```bash
# After Stage 1
git add -A && git commit -m "Phase 5 Stage 1: ChatClient, classify pipeline, confirm/reject routes, 4/4 tests passing"

# After Stage 2
git add -A && git commit -m "Phase 5 Stage 2: validate/quality/derive/organize, job runner extended, tests passing"

# After Stage 3
git add -A && git commit -m "Phase 5 Stage 3: admin jobs list/detail/cancel API, 5/5 tests passing"

# After Stage 4
git add -A && git commit -m "Phase 5 Stage 4: AI panel on BookDetailPage, AdminJobsPage, frontend tests passing"
```
