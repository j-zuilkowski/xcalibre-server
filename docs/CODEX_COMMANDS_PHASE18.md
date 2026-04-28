# Codex Desktop App — xcalibre-server Phase 18: Merlin Memory Integration

## What Phase 18 Builds

xcalibre-server gains a lightweight RAG memory store for Merlin. Merlin writes episodic session summaries and extracted facts here at session end; xcalibre-server embeds and indexes them alongside book chunks so that at query time the same `GET /api/v1/search/chunks` endpoint can return both book knowledge and prior session memory, merged by Reciprocal Rank Fusion.

This phase is pure backend — no frontend changes.

- **Stage 1** — Migration 0028: `memory_chunks` table + `memory_chunks_fts` FTS5 virtual table + sync triggers (SQLite + MariaDB)
- **Stage 2** — `db/queries/memory_chunks.rs`: insert, get, delete, FTS5 keyword search, sqlite-vec semantic search
- **Stage 3** — `POST /api/v1/memory` + `DELETE /api/v1/memory/{id}` handlers (lightweight ingest, synchronous, no job queue)
- **Stage 4** — Extend `GET /api/v1/search/chunks` with `?source=books|memory|all` filter; add `source` field to `ChunkResult`
- **Stage 5** — Tests in `backend/tests/test_memory.rs` (ingest, delete, source filtering, auth, embedding fallback)
- **Stage 6** — Docs: `API.md`, `ARCHITECTURE.md`, `STATE.md`, `config.example.toml` verification

### Prerequisites (already shipped — do not re-implement)

The `embedding_model` optional config field was added as a hotfix prior to this phase:
- `LlmSection.embedding_model: Option<String>` in `backend/src/config.rs`
- `EmbeddingClient` prefers `embedding_model` over `librarian.model` when set
- `APP_LLM_EMBEDDING_MODEL` env var override wired in config loading
- `config.example.toml` documents the field

Verify these are present before starting Stage 1. If missing, implement them first.

---

## Key Design Decisions

**Why a separate `memory_chunks` table (not reusing `book_chunks`):**
`book_chunks` has columns tightly coupled to the EPUB pipeline: `book_id`, `chapter_index`, `chunk_index`, `heading_path`. Memory chunks have none of these — they have `session_id` and `project_path`. Forcing memory into book_chunks would require nullable columns on both sides and break the compile-time query safety that sqlx macros provide. A separate table keeps both schemas clean and allows independent indexing strategies.

**Why the ingest is synchronous (no job queue):**
Memory writes from Merlin happen at session end and at specific idle-fire points. Merlin expects confirmation before moving on (it uses the returned chunk ID for deduplication on retry). The job queue is designed for long-running EPUB processing; a 100-word memory write embeds in <500ms on LM Studio. Async queuing would add complexity for no benefit at this scale.

**Embedding fallback on missing LLM:**
If `llm.enabled = false` or the embedding endpoint is not configured, `POST /api/v1/memory` still succeeds — the chunk is stored with `embedding = NULL` and `model_id = ""`. FTS5 keyword search still works. Semantic search returns no results for null-embedding chunks. This is the same pattern as book_chunks, which also allows null embeddings when LLM features are off.

**The `source` filter defaults to `"books"` for backwards compatibility:**
Existing Merlin callers and the web frontend currently call `GET /api/v1/search/chunks` without a `source` parameter. Defaulting to `"books"` means no behaviour change for existing clients. Merlin's `RAGTools.buildEnrichedMessage` will explicitly pass `source=all` to get unified retrieval.

**`project_path` scoping:**
Memory chunks are tagged with the filesystem path of the project they were generated from (e.g. `/Users/jon/Documents/localProject/xcalibre-server`). Merlin passes this at ingest and at query time. The semantic search query filters by `project_path` when provided, preventing memory from one project leaking into another. Pass `project_path = null` to search all memory (used for user-level factual memory that spans projects).

**RRF merge for `source=all`:**
When `source=all`, the handler runs two independent searches (book chunks + memory chunks) then merges by Reciprocal Rank Fusion — the same algorithm already used for hybrid book chunk search. Each result carries a `source` field (`"books"` or `"memory"`) so the caller can distinguish them. The combined list is re-ranked by the merged RRF score, not interleaved in source order.

---

## Key Schema Changes

| Migration | Contents |
|---|---|
| `0028_memory_chunks.sql` | `memory_chunks` table, 3 indexes, `memory_chunks_fts` FTS5 virtual table, 3 sync triggers |

Matching MariaDB migration must be created at `backend/migrations/mariadb/0028_memory_chunks.sql`. MariaDB uses FULLTEXT index instead of FTS5.

No existing tables are modified.

---

## Reference Files

Read before starting each stage:
- `backend/migrations/sqlite/0019_chunks.sql` — `book_chunks` table (pattern to follow for `memory_chunks`)
- `backend/migrations/sqlite/0021_chunks_fts.sql` — FTS5 virtual table + sync triggers (pattern to follow)
- `backend/src/db/queries/book_chunks.rs` — query patterns for chunk insert, search, semantic search
- `backend/src/search/semantic.rs` — `vec_distance_cosine` usage pattern for embedding search
- `backend/src/api/search.rs` — `ChunkSearchQuery`, `ChunkResult`, hybrid search logic, RRF merge
- `backend/src/llm/embeddings.rs` — `EmbeddingClient.embed()` — the ingest path calls this
- `backend/src/config.rs` — `LlmSection.embedding_model` (prerequisite field)
- `backend/src/db/mod.rs` — `maybe_register_sqlite_vec()` — sqlite-vec is already registered at startup
- `backend/tests/test_hybrid_search.rs` — existing chunk search test patterns to follow

---

## STAGE 1 — Migration 0028: memory_chunks Table

**Priority: Must ship first — all other stages depend on this schema**
**Blocks: Stages 2–5. Blocked by: nothing.**
**Model: local**

**Paste this into Codex:**

```
Read backend/migrations/sqlite/0019_chunks.sql (book_chunks table) and
backend/migrations/sqlite/0021_chunks_fts.sql (FTS5 setup + triggers).

Create migration 0028 for a memory_chunks table that follows the same
structural pattern, adapted for Merlin RAG memory chunks.

─────────────────────────────────────────
DELIVERABLE — SQLite
─────────────────────────────────────────

backend/migrations/sqlite/0028_memory_chunks.sql:

  CREATE TABLE IF NOT EXISTS memory_chunks (
      id           TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
      session_id   TEXT,
      project_path TEXT,
      chunk_type   TEXT    NOT NULL DEFAULT 'episodic',
      text         TEXT    NOT NULL,
      tags         TEXT,
      model_id     TEXT    NOT NULL DEFAULT '',
      embedding    BLOB,
      created_at   INTEGER NOT NULL DEFAULT (unixepoch())
  );

  CREATE INDEX IF NOT EXISTS idx_memory_chunks_session_id
      ON memory_chunks(session_id);

  CREATE INDEX IF NOT EXISTS idx_memory_chunks_project_path
      ON memory_chunks(project_path);

  CREATE INDEX IF NOT EXISTS idx_memory_chunks_created_at
      ON memory_chunks(created_at);

  CREATE VIRTUAL TABLE IF NOT EXISTS memory_chunks_fts USING fts5(
      text,
      content     = 'memory_chunks',
      content_rowid = 'rowid',
      tokenize    = 'unicode61 remove_diacritics 2'
  );

  CREATE TRIGGER IF NOT EXISTS memory_chunks_fts_ai
      AFTER INSERT ON memory_chunks BEGIN
          INSERT INTO memory_chunks_fts(rowid, text)
          VALUES (new.rowid, new.text);
      END;

  CREATE TRIGGER IF NOT EXISTS memory_chunks_fts_ad
      AFTER DELETE ON memory_chunks BEGIN
          INSERT INTO memory_chunks_fts(memory_chunks_fts, rowid, text)
          VALUES ('delete', old.rowid, old.text);
      END;

  CREATE TRIGGER IF NOT EXISTS memory_chunks_fts_au
      AFTER UPDATE ON memory_chunks BEGIN
          INSERT INTO memory_chunks_fts(memory_chunks_fts, rowid, text)
          VALUES ('delete', old.rowid, old.text);
          INSERT INTO memory_chunks_fts(rowid, text)
          VALUES (new.rowid, new.text);
      END;

─────────────────────────────────────────
DELIVERABLE — MariaDB
─────────────────────────────────────────

backend/migrations/mariadb/0028_memory_chunks.sql:

  CREATE TABLE IF NOT EXISTS memory_chunks (
      id           VARCHAR(32)  NOT NULL DEFAULT '',
      session_id   VARCHAR(255),
      project_path VARCHAR(1024),
      chunk_type   VARCHAR(32)  NOT NULL DEFAULT 'episodic',
      text         LONGTEXT     NOT NULL,
      tags         TEXT,
      model_id     VARCHAR(255) NOT NULL DEFAULT '',
      embedding    LONGBLOB,
      created_at   BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
      PRIMARY KEY (id),
      INDEX idx_memory_chunks_session_id (session_id),
      INDEX idx_memory_chunks_project_path (project_path(255)),
      INDEX idx_memory_chunks_created_at (created_at),
      FULLTEXT INDEX memory_chunks_fts (text)
  ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Run cargo sqlx migrate run (SQLite) and verify:
  SELECT name FROM sqlite_master WHERE type='table' AND name='memory_chunks';
  SELECT name FROM sqlite_master WHERE type='table' AND name='memory_chunks_fts';
  PRAGMA index_list('memory_chunks');

Run cargo test --workspace to confirm no regressions in existing tests.
Commit: "feat: migration 0028 — memory_chunks table + FTS5 index (Phase 18 Stage 1)"
```

---

## STAGE 2 — `db/queries/memory_chunks.rs`

**Priority: High**
**Blocks: Stages 3–5. Blocked by: Stage 1.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/db/queries/book_chunks.rs (insert, semantic search patterns)
and backend/src/search/semantic.rs (vec_distance_cosine usage).

Create backend/src/db/queries/memory_chunks.rs with the full query set
for the memory_chunks table added in Stage 1.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

memory_chunks follows the same DB patterns as book_chunks:
  - Insert: store text + optional embedding BLOB
  - Delete: by id
  - FTS5 keyword search: via memory_chunks_fts virtual table
  - Semantic search: vec_distance_cosine on the embedding BLOB column,
    filtered by model_id to prevent cross-model comparison

The embedding BLOB is a little-endian f32 array, identical encoding to
book_chunks. Use the same serialize_embedding() helper.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

backend/src/db/queries/memory_chunks.rs — implement:

  pub struct MemoryChunk {
      pub id: String,
      pub session_id: Option<String>,
      pub project_path: Option<String>,
      pub chunk_type: String,
      pub text: String,
      pub tags: Option<String>,  // JSON-encoded Vec<String>
      pub model_id: String,
      pub created_at: i64,
  }

  pub struct InsertMemoryChunkParams<'a> {
      pub id: &'a str,
      pub session_id: Option<&'a str>,
      pub project_path: Option<&'a str>,
      pub chunk_type: &'a str,
      pub text: &'a str,
      pub tags: Option<&'a str>,  // pre-serialized JSON
      pub model_id: &'a str,
      pub embedding: Option<&'a [u8]>,  // None when LLM unavailable
  }

  pub async fn insert_memory_chunk(
      pool: &SqlitePool,
      params: &InsertMemoryChunkParams<'_>,
  ) -> sqlx::Result<MemoryChunk>

  pub async fn get_memory_chunk(
      pool: &SqlitePool,
      id: &str,
  ) -> sqlx::Result<Option<MemoryChunk>>

  pub async fn delete_memory_chunk(
      pool: &SqlitePool,
      id: &str,
  ) -> sqlx::Result<bool>   // true if a row was deleted

  pub struct MemoryChunkSearchResult {
      pub id: String,
      pub session_id: Option<String>,
      pub project_path: Option<String>,
      pub chunk_type: String,
      pub text: String,
      pub tags: Option<String>,
      pub score: f32,
  }

  pub async fn search_memory_chunks_fts(
      pool: &SqlitePool,
      q: &str,
      limit: u32,
      project_path: Option<&str>,
  ) -> sqlx::Result<Vec<MemoryChunkSearchResult>>

  // FTS5 query:
  //   SELECT mc.id, mc.session_id, mc.project_path, mc.chunk_type, mc.text, mc.tags,
  //          rank AS score
  //   FROM memory_chunks_fts
  //   JOIN memory_chunks mc ON mc.rowid = memory_chunks_fts.rowid
  //   WHERE memory_chunks_fts MATCH ?
  //     AND (? IS NULL OR mc.project_path = ?)
  //   ORDER BY rank LIMIT ?

  pub async fn search_memory_chunks_semantic(
      pool: &SqlitePool,
      query_embedding: &[u8],
      limit: u32,
      model_id: &str,
      project_path: Option<&str>,
  ) -> sqlx::Result<Vec<MemoryChunkSearchResult>>

  // Semantic query:
  //   SELECT id, session_id, project_path, chunk_type, text, tags,
  //          vec_distance_cosine(embedding, ?) AS score
  //   FROM memory_chunks
  //   WHERE model_id = ?
  //     AND embedding IS NOT NULL
  //     AND (? IS NULL OR project_path = ?)
  //   ORDER BY score ASC LIMIT ?

Register the module in backend/src/db/queries/mod.rs:
  pub mod memory_chunks;

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Run cargo check -p backend and cargo clippy -- -D warnings.
Do not write tests yet — Stage 5 covers all integration tests.
Commit: "feat: db/queries/memory_chunks — insert, delete, fts5 + semantic search (Phase 18 Stage 2)"
```

---

## STAGE 3 — `POST /api/v1/memory` + `DELETE /api/v1/memory/{id}`

**Priority: High**
**Blocks: Stage 5. Blocked by: Stage 2.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/api/search.rs (how chunk search handlers are structured),
backend/src/llm/embeddings.rs (EmbeddingClient.embed),
backend/src/db/queries/memory_chunks.rs (just written in Stage 2),
and backend/src/api/mod.rs (router assembly).

Create backend/src/api/memory.rs with two handlers. Register them in the router.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

This is a lightweight ingest path. The EPUB pipeline is heavyweight and
asynchronous. Memory writes must be fast and synchronous — Merlin waits for
the returned chunk ID before moving on.

Flow for POST /api/v1/memory:
  1. Parse request body (text, session_id, project_path, chunk_type, tags)
  2. Generate id = ulid or random hex (consistent with existing IDs)
  3. Call EmbeddingClient.embed(&text) — if LLM is disabled or fails, use
     embedding = None, model_id = "" (silent fallback, never surface to caller)
  4. Call db::queries::memory_chunks::insert_memory_chunk(...)
  5. Return 201 Created with { "id": "...", "created_at": ... }

Auth: requires a valid Bearer token (same as all other /api/v1 routes).
No admin role required — any authenticated user can write memory.

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

backend/src/api/memory.rs — create with:

  pub fn router(state: AppState) -> Router<AppState>
  — registers:
      POST   /api/v1/memory        → ingest_memory_chunk
      DELETE /api/v1/memory/:id    → delete_memory_chunk

  Request body for POST:
    #[derive(Deserialize, utoipa::ToSchema)]
    pub struct IngestMemoryChunkRequest {
        pub text: String,
        pub session_id: Option<String>,
        pub project_path: Option<String>,
        #[serde(default = "default_chunk_type")]
        pub chunk_type: String,    // "episodic" | "factual"
        pub tags: Option<Vec<String>>,
    }

  Response body for POST (201):
    #[derive(Serialize, utoipa::ToSchema)]
    pub struct IngestMemoryChunkResponse {
        pub id: String,
        pub created_at: i64,
    }

  Validation:
  - text must not be empty; return 422 if blank
  - chunk_type must be "episodic" or "factual"; return 422 otherwise
  - text length capped at 32,768 chars; return 422 with clear message if exceeded

  Embedding:
  - If state.config.llm.enabled && embedding_client.is_configured():
      match embedding_client.embed(&body.text).await {
          Ok(vec) => (Some(serialize_embedding(&vec)), embedding_client.model_id().to_string())
          Err(_)  => (None, String::new())  // silent fallback
      }
  - Else: (None, String::new())

  DELETE handler:
  - Fetch the chunk first; return 404 if not found
  - Delete and return 204 No Content

Add memory::router(state.clone()) to the router in backend/src/api/mod.rs.

Add utoipa #[utoipa::path(...)] annotations to both handlers so they appear
in the OpenAPI spec at /api/v1/docs.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Run cargo check -p backend and cargo clippy -- -D warnings.
Commit: "feat: POST/DELETE /api/v1/memory handlers (Phase 18 Stage 3)"
```

---

## STAGE 4 — Extend `/search/chunks` with `?source` Filter

**Priority: High**
**Blocks: Stage 5. Blocked by: Stages 2–3.**
**Model: local**

**Paste this into Codex:**

```
Read backend/src/api/search.rs in full — focusing on:
  - ChunkSearchQuery struct (query parameters)
  - ChunkResult struct (response body)
  - The hybrid search handler that calls FTS5 + semantic search + RRF merge
  - The existing book_chunks search path

Extend GET /api/v1/search/chunks to support ?source=books|memory|all.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

Currently GET /api/v1/search/chunks searches book_chunks only. Merlin needs
to retrieve both book knowledge and prior session memory in one call.

The handler runs:
  1. FTS5 keyword search over book_chunks_fts
  2. Semantic search over book_chunks embeddings (if embedding available)
  3. Merge results by RRF

With source=memory or source=all, it should also:
  1. FTS5 keyword search over memory_chunks_fts
  2. Semantic search over memory_chunks embeddings
  3. Merge all results by RRF

─────────────────────────────────────────
DELIVERABLE
─────────────────────────────────────────

In backend/src/api/search.rs:

1. Add `source` field to ChunkSearchQuery:
     pub source: Option<String>,   // "books" | "memory" | "all"; default "books"

2. Add `source` field to ChunkResult:
     pub source: String,   // "books" | "memory"
   Existing results always get source = "books". New memory results get "memory".

3. Add `project_path` field to ChunkSearchQuery (optional, for memory scoping):
     pub project_path: Option<String>,

4. In the handler body, parse `source`:
     let source = query.source.as_deref().unwrap_or("books");

   Validate source is one of the three accepted values; return 422 otherwise.

5. Book chunk search: run when source == "books" || source == "all"
   (unchanged from current implementation)

6. Memory chunk search: run when source == "memory" || source == "all"
     // FTS5
     let memory_fts_hits = db::queries::memory_chunks::search_memory_chunks_fts(
         &state.db, &query.q, limit * 2, query.project_path.as_deref()
     ).await?;

     // Semantic (only if query embedding available)
     let memory_semantic_hits = if let Some(emb) = &query_embedding {
         db::queries::memory_chunks::search_memory_chunks_semantic(
             &state.db, emb, limit * 2, &model_id, query.project_path.as_deref()
         ).await.unwrap_or_default()
     } else {
         vec![]
     };

     // Build ChunkResults from memory hits (source = "memory")

7. RRF merge: merge all ChunkResult lists (book FTS + book semantic + memory
   FTS + memory semantic) into a single ranked list, take top `limit` results.
   Memory results carry source = "memory"; book results carry source = "books".

8. Update OpenAPI annotations on the handler to document the new params.

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Run cargo check -p backend and cargo clippy -- -D warnings.
Commit: "feat: /search/chunks ?source filter — unified book + memory retrieval (Phase 18 Stage 4)"
```

---

## STAGE 5 — Tests: `backend/tests/test_memory.rs`

**Priority: Must complete before commit**
**Blocks: nothing. Blocked by: Stages 1–4 (TDD: write tests before implementation if preferred).**
**Model: local**

**Paste this into Codex:**

```
Read backend/tests/test_hybrid_search.rs and backend/tests/test_books.rs
for TestContext patterns (upload, auth, helper methods).

Create backend/tests/test_memory.rs covering the full memory API surface.

─────────────────────────────────────────
TESTS TO IMPLEMENT
─────────────────────────────────────────

All tests use TestContext::new().await. LLM is disabled (mock:// endpoint
or llm.enabled=false) — embedding tests use the mock embedding path.

  test_memory_ingest_returns_201_with_id
    - POST /api/v1/memory with text="Test memory chunk"
    - Assert 201
    - Assert response body has { "id": <non-empty string>, "created_at": <int> }

  test_memory_ingest_stores_chunk_in_db
    - POST /api/v1/memory
    - Use db_query via TestContext to SELECT from memory_chunks WHERE id = ?
    - Assert row exists with correct text, chunk_type, session_id

  test_memory_ingest_without_llm_stores_null_embedding
    - LLM disabled in test config
    - POST /api/v1/memory
    - SELECT embedding FROM memory_chunks WHERE id = ?
    - Assert embedding IS NULL
    - Assert 201 was returned (fallback, not failure)

  test_memory_ingest_validates_empty_text
    - POST /api/v1/memory with text=""
    - Assert 422

  test_memory_ingest_validates_chunk_type
    - POST /api/v1/memory with chunk_type="invalid"
    - Assert 422

  test_memory_ingest_validates_text_length
    - POST /api/v1/memory with text = "x".repeat(33_000)
    - Assert 422

  test_memory_delete_returns_204
    - POST /api/v1/memory → get id
    - DELETE /api/v1/memory/{id}
    - Assert 204
    - SELECT from memory_chunks WHERE id = ? → 0 rows

  test_memory_delete_nonexistent_returns_404
    - DELETE /api/v1/memory/nonexistent-id
    - Assert 404

  test_memory_requires_auth
    - POST /api/v1/memory without bearer token → 401
    - DELETE /api/v1/memory/any-id without bearer token → 401

  test_search_chunks_source_memory_returns_memory_chunks
    - Ingest a memory chunk with text "dragon scales in mythology"
    - GET /api/v1/search/chunks?q=dragon+scales&source=memory
    - Assert the memory chunk appears in results
    - Assert result has source="memory"

  test_search_chunks_source_books_excludes_memory
    - Ingest a memory chunk with text="unique_memory_phrase_xyz"
    - GET /api/v1/search/chunks?q=unique_memory_phrase_xyz&source=books
    - Assert results do NOT contain the memory chunk

  test_search_chunks_source_all_returns_both
    - Upload a book whose chunk text contains "combined_search_term_abc"
    - Ingest a memory chunk with text="combined_search_term_abc"
    - GET /api/v1/search/chunks?q=combined_search_term_abc&source=all
    - Assert results contain at least one source="books" result and
      at least one source="memory" result

  test_search_chunks_invalid_source_returns_422
    - GET /api/v1/search/chunks?q=test&source=invalid
    - Assert 422

  test_search_chunks_project_path_filter
    - Ingest chunk A with project_path="/project/alpha"
    - Ingest chunk B with project_path="/project/beta"
    - GET /api/v1/search/chunks?q=test&source=memory&project_path=/project/alpha
    - Assert only chunk A appears (B is excluded by project_path)

  test_embedding_model_config_field_exists
    - Construct AppConfig with embedding_model = Some("nomic-embed-text-v1.5")
    - Construct EmbeddingClient from config
    - Assert EmbeddingClient.model_id() == "nomic-embed-text-v1.5"
    - Construct AppConfig with embedding_model = None, librarian.model = "phi-3-mini"
    - Assert EmbeddingClient.model_id() == "phi-3-mini" (fallback to librarian.model)

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

cargo test memory -- --nocapture
cargo test search -- --nocapture
cargo clippy -- -D warnings

All tests must pass. Fix any issues in Stages 1–4 before committing tests.
Commit: "test: memory API — ingest, delete, source filter, auth, embedding fallback (Phase 18 Stage 5)"
```

---

## STAGE 6 — Docs: API.md, ARCHITECTURE.md, STATE.md

**Priority: Medium**
**Blocks: nothing. Blocked by: Stages 1–5 complete.**
**Model: local**

**Paste this into Codex:**

```
Read docs/API.md, docs/ARCHITECTURE.md, and docs/STATE.md.

Update all three to reflect Phase 18 additions.

─────────────────────────────────────────
DELIVERABLE — docs/API.md
─────────────────────────────────────────

Add a new section "Memory API" with:

  POST /api/v1/memory
    Auth: Bearer
    Body: { text, session_id?, project_path?, chunk_type?, tags? }
    Returns: 201 { id, created_at }
    Errors: 422 (empty text, invalid chunk_type, text too long)

  DELETE /api/v1/memory/{id}
    Auth: Bearer
    Returns: 204 No Content
    Errors: 404

  GET /api/v1/search/chunks (update existing entry)
    Add new query params:
      source: "books" | "memory" | "all" (default: "books")
      project_path: string (optional; scopes memory results to a project path)
    ChunkResult shape: add source field ("books" | "memory")

─────────────────────────────────────────
DELIVERABLE — docs/ARCHITECTURE.md
─────────────────────────────────────────

Add a "Merlin RAG Memory Integration" section covering:
  - Purpose: persistent episodic + factual memory for Merlin agents
  - memory_chunks table schema (id, session_id, project_path, chunk_type,
    text, tags, model_id, embedding, created_at)
  - Ingest path: POST /api/v1/memory → embed → store (synchronous)
  - Retrieval path: GET /search/chunks?source=all → book+memory RRF merge
  - Embedding model split: llm.embedding_model for embeddings (nomic-embed-text-v1.5),
    llm.librarian.model for chat (phi-3-mini-4k-instruct)
  - project_path scoping: how multi-project isolation works

─────────────────────────────────────────
DELIVERABLE — docs/STATE.md
─────────────────────────────────────────

1. Update header to "Phase 18 Complete"
2. Add Phase 18 row to the Phase Completion Summary table
3. Add migration 0027 and 0028 rows to the migrations table:
     | 0027_book_user_state_book_id_idx.sql | idx_book_user_state_book_id index | ✅ Applied |
     | 0028_memory_chunks.sql | memory_chunks table + FTS5 + indexes | ✅ Applied |
4. Update "Total" line to: "44 tables, 28 migrations"
5. Update Open Items: remove any items resolved this phase; add:
     - "Merlin-side: XcalibreClient.writeMemoryChunk() and MemoryEngine
       integration not yet implemented (xcalibre-server side is complete)"

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────

Review docs for accuracy. No code changes — docs only.
Commit: "docs: Phase 18 — memory API, architecture section, STATE.md (Phase 18 Stage 6)"
```

---

## Post-Phase-18 Checklist

After all 6 stages are committed:

- [ ] `cargo test --workspace` — all tests pass
- [ ] `cargo clippy -- -D warnings` — zero warnings
- [ ] `cargo audit` — zero CVEs
- [ ] `cargo sqlx migrate run` — migration 0028 applied
- [ ] `SELECT name FROM sqlite_master WHERE type='table' AND name='memory_chunks'` — present
- [ ] `SELECT name FROM sqlite_master WHERE type='table' AND name='memory_chunks_fts'` — present
- [ ] `PRAGMA index_list('memory_chunks')` — 3 indexes present
- [ ] `POST /api/v1/memory` with `curl` → 201 response with id
- [ ] `GET /api/v1/search/chunks?q=test&source=memory` → 200 (empty results OK)
- [ ] `GET /api/v1/search/chunks?q=test&source=invalid` → 422
- [ ] `GET /api/v1/docs` → OpenAPI spec includes `/api/v1/memory` endpoints
- [ ] `docs/STATE.md` — Phase 18 complete, migration 0028 listed, table count updated
- [ ] Tag `v1.5.0` locally: `git tag -a v1.5.0 -m "Phase 18: Merlin memory integration"`

## Phase Summary

| Stage | Area | Priority |
|---|---|---|
| 1 | Migration 0028: memory_chunks table + FTS5 + triggers | 🔴 Must first |
| 2 | db/queries/memory_chunks: CRUD + FTS5 + semantic search | 🔴 High |
| 3 | POST + DELETE /api/v1/memory handlers | 🔴 High |
| 4 | /search/chunks ?source filter + ChunkResult.source field | 🔴 High |
| 5 | Tests: test_memory.rs (full coverage) | 🔴 Must complete |
| 6 | Docs: API.md, ARCHITECTURE.md, STATE.md | 🟠 Medium |
