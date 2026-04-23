# Codex Desktop App — autolibre Phase 15: Cross-Document Synthesis Engine

## What Phase 15 Builds

The retrieval and synthesis infrastructure that makes autolibre a grounded knowledge base for any agent — engineering documentation sets, electronics datasheets, culinary libraries, legal corpora, academic collections, or any domain:

- **Stage 1** — Sub-chapter chunking + vision LLM pass (image-heavy pages: schematics, diagrams, charts read natively)
- **Stage 2** — Hybrid BM25 + semantic retrieval + cross-encoder reranking
- **Stage 3** — Collections + `synthesize` MCP tool (machine-readable output formats: runsheet, SPICE netlist, KiCad schematic, BOM, and more)

## Key Design Decisions

**Why chunk-level retrieval replaces chapter-level:**
Chapter-level retrieval (`GET /books/:id/text?chapter=N`) returns 5,000–25,000 tokens per call. An Oracle admin guide chapter on "Backup Configuration" contains 40+ distinct procedures — the agent gets all 40 when it needs 1. With 600-token chunks at section/procedure boundaries, retrieval precision is exact. The agent receives only the relevant procedure, with full heading-path provenance for citation.

**Vision LLM pass — no special cases:**
Vision-capable LLMs read schematics, assembly diagrams, wiring drawings, data flow diagrams, and charts the same way they read text. The ingest pipeline sends image-heavy pages to the LLM and stores the response as chunk text. Circuit topology, component connections, design intent — all become indexed, searchable, retrievable. No domain-specific logic. OCR always runs first; vision is appended. If the vision call fails or LLM is disabled, the OCR-only chunk is stored.

**Hybrid retrieval is mandatory for technical corpora:**
Semantic search alone fails on exact technical tokens: error codes (`ORA-01555`), parameter names (`UNDO_RETENTION`), CLI commands (`srvctl add database`), part numbers (`LMR33630`), standard clause references (`IPC-A-610 §3.4.2`). BM25 (existing FTS5 index) catches these exactly. Combined via Reciprocal Rank Fusion with cosine similarity. Cross-encoder reranking (optional LLM call) promotes the most relevant chunks from the fused top-50.

**`synthesize` output formats are not limited to prose:**
The synthesis tool accepts a `format` parameter that drives the output shape. Machine-readable formats (SPICE netlist, KiCad schematic, BOM, SVG, structured JSON) are first-class output types alongside prose formats. The retrieval pipeline is identical regardless of format — only the synthesis prompt changes. LLMs with image output capability can generate schematic images directly; the architecture does not constrain output modality.

**Collections are the unit of synthesis:**
A collection groups related books (e.g., "Oracle Database 19c", "TI Power Management Library", "Julia Child Complete Works") and is searched as a single corpus. Cross-collection search is also supported. The agent specifies `collection_id` or a list of `book_ids`; retrieval spans all specified content simultaneously with per-book provenance preserved.

## Key Schema Facts (new tables this phase)

```sql
-- Stage 1 — Chunk storage (migration 0019)
CREATE TABLE book_chunks (
    id           TEXT PRIMARY KEY,
    book_id      TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    chunk_index  INTEGER NOT NULL,
    chapter_index INTEGER NOT NULL,
    heading_path TEXT,               -- "Admin Guide > Part III > §12.3"
    chunk_type   TEXT NOT NULL DEFAULT 'text'
                   CHECK(chunk_type IN ('text','procedure','reference',
                                        'concept','example','image')),
    text         TEXT NOT NULL,      -- OCR text + vision LLM description (if applicable)
    word_count   INTEGER NOT NULL,
    has_image    INTEGER NOT NULL DEFAULT 0,  -- 1 = vision pass was run on this chunk
    embedding    BLOB,               -- sqlite-vec embedding (replaces book_embeddings)
    created_at   TEXT NOT NULL
);
CREATE INDEX idx_book_chunks_book    ON book_chunks(book_id, chunk_index);
CREATE INDEX idx_book_chunks_type    ON book_chunks(book_id, chunk_type);

-- Stage 3 — Collections (migration 0020)
CREATE TABLE collections (
    id           TEXT PRIMARY KEY,
    name         TEXT NOT NULL,
    description  TEXT,
    domain       TEXT NOT NULL DEFAULT 'technical'
                   CHECK(domain IN ('technical','electronics','culinary',
                                    'legal','academic','narrative')),
    owner_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    is_public    INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE TABLE collection_books (
    collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
    book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    added_at      TEXT NOT NULL,
    PRIMARY KEY (collection_id, book_id)
);
CREATE INDEX idx_collection_books_collection ON collection_books(collection_id);
```

## Reference Files

Read before starting each stage:
- `docs/ARCHITECTURE.md` — Cross-Document Synthesis and Derivative Works section (the full design)
- `backend/src/ingest/text.rs` — existing text extraction pipeline to extend (Stage 1)
- `backend/src/storage.rs` — StorageBackend trait (needed for image page extraction in Stage 1)
- `backend/src/api/search.rs` — existing search handlers to extend (Stage 2)
- `backend/src/db/queries/books.rs` — query patterns (Stage 2)
- `backend/src/api/books.rs` — MCP tool surface (Stage 3)
- `autolibre-mcp/src/tools/mod.rs` — existing MCP tool definitions (Stage 3)
- `backend/migrations/sqlite/0019_chunks.sql` — (Stage 1, created by this phase)

---

## STAGE 1 — Sub-Chapter Chunking + Vision LLM Pass

**Priority: Critical — all of Phase 15 depends on this.**
**Blocks: Stages 2 and 3. Must complete first.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/ingest/text.rs, backend/src/api/books.rs,
backend/src/db/queries/books.rs, backend/src/lib.rs (AppState),
backend/Cargo.toml, and backend/migrations/sqlite/0013_scheduled_tasks.sql
(for migration format reference).
Now implement sub-chapter chunking with a vision LLM pass for image-heavy pages.

─────────────────────────────────────────
SCHEMA — migration 0019
─────────────────────────────────────────

backend/migrations/sqlite/0019_chunks.sql:

  CREATE TABLE book_chunks (
      id            TEXT PRIMARY KEY,
      book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
      chunk_index   INTEGER NOT NULL,
      chapter_index INTEGER NOT NULL,
      heading_path  TEXT,
      chunk_type    TEXT NOT NULL DEFAULT 'text'
                      CHECK(chunk_type IN ('text','procedure','reference',
                                           'concept','example','image')),
      text          TEXT NOT NULL,
      word_count    INTEGER NOT NULL,
      has_image     INTEGER NOT NULL DEFAULT 0,
      embedding     BLOB,
      created_at    TEXT NOT NULL
  );
  CREATE INDEX idx_book_chunks_book ON book_chunks(book_id, chunk_index);
  CREATE INDEX idx_book_chunks_type ON book_chunks(book_id, chunk_type);

backend/migrations/mariadb/0018_chunks.sql — equivalent MariaDB DDL.

─────────────────────────────────────────
CHUNKER DESIGN
─────────────────────────────────────────

backend/src/ingest/chunker.rs — new file:

  pub struct ChunkConfig {
    pub target_size: usize,     // target tokens per chunk (default 600)
    pub overlap: usize,         // overlap tokens between adjacent chunks (default 100)
    pub domain: ChunkDomain,
  }

  pub enum ChunkDomain {
    Technical,    // detect numbered lists, Prerequisites/Steps/Verification headings
    Electronics,  // spec table blocks, application circuit paragraphs, pin tables
    Culinary,     // recipe title + ingredients + method as one unit
    Legal,        // article/clause/sub-clause structure
    Academic,     // abstract, section headings, theorem/proof blocks
    Narrative,    // paragraph groups with overlap, no structure detection
  }

  pub struct Chunk {
    pub chunk_index: usize,
    pub chapter_index: usize,
    pub heading_path: Option<String>,   // e.g. "Admin Guide > Part III > §12.3"
    pub chunk_type: ChunkType,
    pub text: String,
    pub word_count: usize,
    pub is_image_heavy: bool,           // true = vision pass should run
  }

  pub enum ChunkType { Text, Procedure, Reference, Concept, Example, Image }

  pub fn chunk_chapters(
    chapters: &[ChapterText],   // output of existing text extraction
    config: &ChunkConfig,
  ) -> Vec<Chunk>

  Implementation rules (apply in order for all domains except Narrative):

  1. HEADING DETECTION:
     Lines matching /^#{1,4}\s/ (Markdown headings from EPUB) or
     /^\d+(\.\d+)*\s+[A-Z]/ (numbered section headings from PDFs)
     start a new chunk boundary regardless of current chunk size.
     Store the heading hierarchy as heading_path.

  2. PROCEDURE DETECTION (Technical, Electronics):
     A sequence of 3+ lines matching /^\s*(Step\s+)?\d+[\.\)]\s+\S/ is a
     numbered list. Treat as a single atomic Procedure chunk — never split it.
     Mark chunk_type = Procedure.

  3. IMAGE DENSITY CHECK:
     If a chapter section contains < 80 words and was extracted from a page
     flagged as image-heavy by the PDF extractor, set is_image_heavy = true
     and chunk_type = Image. The vision pass runs on these chunks.

  4. SIZE CONTROL:
     If a text segment exceeds target_size tokens, split at the nearest
     paragraph boundary (blank line). Add overlap tokens from the end of
     the previous chunk to the start of the next.

  5. CULINARY:
     Detect recipe boundaries: a line that is a recipe title (Title Case,
     no punctuation, < 8 words) followed by an "Ingredients" or "Serves N"
     line starts a new atomic Culinary chunk. Keep the title + ingredients
     + method as one unit up to 1200 tokens (recipes are longer than
     procedure chunks).

─────────────────────────────────────────
VISION LLM PASS
─────────────────────────────────────────

backend/src/ingest/vision.rs — new file:

  pub async fn describe_image_page(
    llm: &LlmClient,
    page_image_bytes: &[u8],   // PNG or JPEG bytes of the page
    domain: &ChunkDomain,
  ) -> anyhow::Result<String>

  Build the prompt based on domain:

  Technical / Electronics:
    "You are analyzing a page from a technical document. Describe everything
     you see: all text (component labels, values, net names, annotations),
     the circuit or diagram topology (what connects to what), the function
     of the circuit or diagram, and any design notes. Be precise and complete.
     Include all component reference designators, values, and units."

  All other domains:
    "Describe the content of this image completely. Include all text visible
     in the image, the structure or layout of any diagram or chart, and the
     meaning or function it communicates."

  Append the LLM response to the existing OCR text for the chunk:
    format!("{ocr_text}\n\n[Visual content description:]\n{vision_response}")

  Gate: only called if llm.enabled AND the configured LLM reports vision
  capability (check `image_input` or equivalent in /v1/models response).
  On any error (timeout, model refusal, capability absent): log at WARN level,
  return the OCR-only text. Never fail the ingest on a vision call failure.

─────────────────────────────────────────
DELIVERABLE 1 — Chunk API endpoint
─────────────────────────────────────────

backend/src/api/books.rs — add:

  GET /books/:id/chunks

  Query params:
    size:    usize  (default 600, max 2000)
    overlap: usize  (default 100)
    domain:  string (default: from book's collection domain, or "technical")
    type:    string (filter by chunk_type — optional)

  Response:
    {
      "book_id": "...",
      "chunk_count": 847,
      "chunks": [
        {
          "id": "...",
          "chunk_index": 0,
          "chapter_index": 2,
          "heading_path": "Admin Guide > Part III > §12.3",
          "chunk_type": "procedure",
          "text": "Step 1: Connect to RMAN...",
          "word_count": 312,
          "has_image": false
        }
      ]
    }

  This endpoint triggers chunking on-demand if book_chunks for this book
  is empty. Otherwise returns the stored chunks.

─────────────────────────────────────────
DELIVERABLE 2 — Chunk ingest job
─────────────────────────────────────────

backend/src/ingest/text.rs — extend the post-ingest pipeline:

  After text extraction completes for a newly ingested book:
    1. Run chunk_chapters() with domain from the book's collection (or "technical")
    2. For each chunk where is_image_heavy = true and LLM vision is available:
       a. Extract the page image (via pdfium or EPUB image extraction)
       b. Call describe_image_page()
       c. Append the vision description to chunk.text
    3. Embed each chunk (same embedding model as current book_embeddings)
    4. INSERT INTO book_chunks (batch insert for efficiency)

  Background re-chunking job for existing books:
    The scheduler runs a one-time job ("rechunk_library") that processes
    books with no book_chunks rows. Run at low priority (1 book per 5 seconds
    to avoid overwhelming the LLM for vision passes).
    Add this job to the scheduler on first startup after migration 0019 applies.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_chunker.rs:
  test_chunk_respects_target_size
  test_chunk_never_splits_numbered_procedure
  test_chunk_detects_heading_boundary
  test_chunk_heading_path_is_hierarchical
  test_chunk_marks_image_heavy_pages
  test_culinary_domain_keeps_recipe_intact
  test_overlap_tokens_appear_in_adjacent_chunks

backend/tests/test_chunks_api.rs:
  test_get_chunks_returns_stored_chunks
  test_get_chunks_triggers_chunking_on_empty
  test_get_chunks_filter_by_type
  test_get_chunks_respects_size_param
  test_vision_pass_appends_to_ocr_text
  test_vision_pass_falls_back_gracefully_on_llm_disabled

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Manual: ingest a technical PDF; GET /books/:id/chunks; verify procedure chunks
#         are intact (not split mid-step); verify heading_path is populated
# Manual (LLM enabled): ingest a PDF with embedded diagrams; verify
#         has_image=true chunks contain vision description in text field
git add backend/
git commit -m "Phase 15 Stage 1: sub-chapter chunking + vision LLM pass for image-heavy pages"
```

---

## STAGE 2 — Hybrid BM25 + Semantic Retrieval + Reranking

**Priority: Critical**
**Blocks: Stage 3. Blocked by: Stage 1 must be complete.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read backend/src/api/search.rs, backend/src/db/queries/books.rs,
backend/src/lib.rs, backend/Cargo.toml,
and backend/migrations/sqlite/0002_fts.sql (FTS5 setup reference).
Now implement hybrid BM25 + semantic chunk retrieval with optional reranking.

─────────────────────────────────────────
BACKGROUND
─────────────────────────────────────────

The existing search pipeline:
  - FTS5 virtual table (books_fts) for keyword search on book-level metadata
  - sqlite-vec for semantic search on book_embeddings (chapter-level)

Phase 15 Stage 1 created book_chunks with per-chunk embeddings. This stage
replaces chapter-level semantic search with chunk-level search and adds BM25
hybrid scoring across both the FTS5 index and chunk embeddings.

─────────────────────────────────────────
DELIVERABLE 1 — FTS5 index for chunks
─────────────────────────────────────────

backend/migrations/sqlite/0019b_chunks_fts.sql (or append to 0019):

  CREATE VIRTUAL TABLE book_chunks_fts USING fts5(
      text,
      heading_path,
      content='book_chunks',
      content_rowid='rowid'
  );

  CREATE TRIGGER book_chunks_fts_insert AFTER INSERT ON book_chunks BEGIN
      INSERT INTO book_chunks_fts(rowid, text, heading_path)
      VALUES (new.rowid, new.text, new.heading_path);
  END;

  CREATE TRIGGER book_chunks_fts_delete AFTER DELETE ON book_chunks BEGIN
      INSERT INTO book_chunks_fts(book_chunks_fts, rowid, text, heading_path)
      VALUES ('delete', old.rowid, old.text, old.heading_path);
  END;

  CREATE TRIGGER book_chunks_fts_update AFTER UPDATE ON book_chunks BEGIN
      INSERT INTO book_chunks_fts(book_chunks_fts, rowid, text, heading_path)
      VALUES ('delete', old.rowid, old.text, old.heading_path);
      INSERT INTO book_chunks_fts(rowid, text, heading_path)
      VALUES (new.rowid, new.text, new.heading_path);
  END;

─────────────────────────────────────────
DELIVERABLE 2 — Hybrid search endpoint
─────────────────────────────────────────

backend/src/api/search.rs — add:

  GET /api/v1/search/chunks

  Query params:
    q:           string  (required)
    book_ids[]:  string  (optional, repeatable — filter to specific books)
    collection_id: string (optional — filter to a collection's books)
    type:        string  (optional — filter chunk_type)
    limit:       usize   (default 10, max 50)
    rerank:      bool    (default false — trigger cross-encoder rerank pass)

  Algorithm:

  Step 1 — BM25 retrieval (FTS5):
    SELECT bc.id, bc.book_id, bc.chunk_index, bc.heading_path,
           bc.chunk_type, bc.text, bc.word_count,
           bm25(book_chunks_fts) AS bm25_score
    FROM book_chunks_fts
    JOIN book_chunks bc ON bc.rowid = book_chunks_fts.rowid
    WHERE book_chunks_fts MATCH ?
      AND (bc.book_id IN (book_ids) OR book_ids is empty)
    ORDER BY bm25_score
    LIMIT 100

  Step 2 — Semantic retrieval (sqlite-vec):
    Embed the query using the same model as chunk embeddings.
    SELECT bc.id, bc.book_id, ...,
           vec_distance_cosine(bc.embedding, ?) AS cosine_score
    FROM book_chunks bc
    WHERE (bc.book_id IN (book_ids) OR book_ids is empty)
    ORDER BY cosine_score
    LIMIT 100

  Step 3 — Reciprocal Rank Fusion:
    For each unique chunk_id from both result sets:
      rrf_score = 1/(k + bm25_rank) + 1/(k + cosine_rank)
      where k = 60 (standard RRF constant)
      chunks that appear in only one list get the other rank = infinity (score = 0)
    Sort by rrf_score DESC. Take top `limit * 5` (e.g., 50 for limit=10).

  Step 4 — Cross-encoder reranking (optional, if rerank=true and llm.enabled):
    For each of the top-50 fused chunks, call the LLM:
      Prompt: "Query: {q}\n\nPassage: {chunk.text}\n\n
               Score the relevance of this passage to the query from 0.0 to 1.0.
               Reply with only the number."
    Sort by rerank score DESC. Take top `limit`.
    Max 10s total timeout across all rerank calls (run in parallel, cancel stragglers).
    Fallback: if rerank times out, return the RRF-sorted results.

  Response:
    {
      "query": "configure RMAN retention policy",
      "chunks": [
        {
          "chunk_id": "...",
          "book_id": "...",
          "book_title": "Backup and Recovery Guide 19c",
          "heading_path": "Chapter 8 > §8.3 Retention Policies",
          "chunk_type": "procedure",
          "text": "Step 1: Connect to RMAN...",
          "word_count": 298,
          "bm25_score": -4.21,
          "cosine_score": 0.87,
          "rrf_score": 0.031,
          "rerank_score": 0.94    // null if rerank=false
        }
      ],
      "total_searched": 12847,
      "retrieval_ms": 43
    }

─────────────────────────────────────────
DELIVERABLE 3 — Update MCP tool
─────────────────────────────────────────

autolibre-mcp/src/tools/mod.rs — update semantic_search tool:

  Replace the existing chapter-level semantic search with chunk-level hybrid search:

  Tool name: "search_chunks" (add new; keep "semantic_search" as deprecated alias)

  Input schema:
    {
      "query": string,
      "book_ids": string[],        // optional
      "collection_id": string,     // optional
      "chunk_type": string,        // optional: "procedure" | "reference" | etc.
      "limit": number,             // default 10
      "rerank": boolean            // default false
    }

  Calls GET /api/v1/search/chunks with the API token.
  Returns the chunks array with provenance fields.

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_hybrid_search.rs:
  test_bm25_finds_exact_technical_token         -- "ORA-01555" found by BM25
  test_semantic_finds_conceptual_match          -- "snapshot too old" finds ORA-01555 chunk
  test_hybrid_outranks_either_alone             -- synthetic corpus, verify RRF > pure BM25 or pure semantic
  test_book_ids_filter_limits_results
  test_collection_id_filter_spans_all_books_in_collection
  test_chunk_type_filter_returns_only_procedures
  test_rerank_reorders_results                  -- mock LLM reranker
  test_rerank_falls_back_on_timeout
  test_response_includes_provenance_fields

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
# Manual: ingest a technical PDF; search for an exact error code or parameter name;
#         verify BM25 finds it; verify semantic finds related concepts
# Manual: search with rerank=true (LLM enabled); verify ordering improves
git add backend/ autolibre-mcp/
git commit -m "Phase 15 Stage 2: hybrid BM25+semantic chunk retrieval, RRF fusion, cross-encoder reranking"
```

---

## STAGE 3 — Collections + `synthesize` MCP Tool

**Priority: Critical**
**Blocks: nothing. Blocked by: Stages 1 and 2 must be complete.**
**Model: GPT-5.3-Codex**

**Paste this into Codex:**

```
Read autolibre-mcp/src/tools/mod.rs, backend/src/api/search.rs,
backend/src/api/admin.rs, backend/Cargo.toml,
docs/ARCHITECTURE.md (the Cross-Document Synthesis section),
and backend/migrations/sqlite/0019_chunks.sql.
Now implement collections and the synthesize MCP tool.

─────────────────────────────────────────
SCHEMA — migration 0020
─────────────────────────────────────────

backend/migrations/sqlite/0020_collections.sql:

  CREATE TABLE collections (
      id          TEXT PRIMARY KEY,
      name        TEXT NOT NULL,
      description TEXT,
      domain      TEXT NOT NULL DEFAULT 'technical'
                    CHECK(domain IN ('technical','electronics','culinary',
                                     'legal','academic','narrative')),
      owner_id    TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      is_public   INTEGER NOT NULL DEFAULT 0,
      created_at  TEXT NOT NULL,
      updated_at  TEXT NOT NULL
  );

  CREATE TABLE collection_books (
      collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
      book_id       TEXT NOT NULL REFERENCES books(id) ON DELETE CASCADE,
      added_at      TEXT NOT NULL,
      PRIMARY KEY (collection_id, book_id)
  );
  CREATE INDEX idx_collection_books_collection ON collection_books(collection_id);
  CREATE INDEX idx_collection_books_book       ON collection_books(book_id);

backend/migrations/mariadb/0019_collections.sql — equivalent.

─────────────────────────────────────────
DELIVERABLE 1 — Collections API
─────────────────────────────────────────

backend/src/api/collections.rs — new file:

  GET    /collections                   — list accessible collections (own + public)
  POST   /collections                   — create collection
  GET    /collections/:id               — collection detail + book list
  PATCH  /collections/:id               — update name, description, domain, is_public
  DELETE /collections/:id               — delete collection (owner or admin)
  POST   /collections/:id/books         — add books to collection
  DELETE /collections/:id/books/:book_id — remove book from collection

  GET /collections/:id/search/chunks    — cross-book chunk search within collection
    Delegates to the hybrid search endpoint (Stage 2) with the collection's
    book_ids pre-loaded. Same query params as GET /api/v1/search/chunks.
    Adds collection-level domain hint to chunking if re-chunking is triggered.

  Response shape for GET /collections/:id:
    {
      "id": "...",
      "name": "Oracle Database 19c",
      "description": "Complete Oracle 19c documentation set",
      "domain": "technical",
      "is_public": false,
      "book_count": 54,
      "total_chunks": 183_421,
      "books": [BookSummary, ...]
    }

─────────────────────────────────────────
DELIVERABLE 2 — `synthesize` MCP tool
─────────────────────────────────────────

autolibre-mcp/src/tools/mod.rs — add synthesize tool:

  Tool name: "synthesize"
  Description: "Retrieve relevant passages from the library and synthesize
                a grounded derivative work in the specified format."

  Input schema:
    {
      "query": string,              // what to synthesize
      "format": string,             // see format table below
      "collection_id": string,      // optional — search a collection
      "book_ids": string[],         // optional — search specific books
      "chunk_type": string,         // optional — "procedure" | "reference" | etc.
      "rerank": boolean,            // default true — use cross-encoder if available
      "limit": number,              // chunks to retrieve before synthesis (default 15)
      "custom_prompt": string       // only used when format = "custom"
    }

  Supported format values and their synthesis prompts:

  ┌─────────────────────┬────────────────────────────────────────────────────────┐
  │ Format              │ Synthesis prompt instruction                           │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ runsheet            │ "Produce a runsheet with: Prerequisites, numbered      │
  │                     │  Steps (each with the exact command or action),        │
  │                     │  Verification steps, and Rollback procedure.           │
  │                     │  Cite the source chunk for each step."                 │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ design-spec         │ "Produce a design specification with: Requirements,    │
  │                     │  Proposed Design, Component/Material List with values, │
  │                     │  Calculations (show working), Constraints and          │
  │                     │  Trade-offs, and References."                          │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ spice-netlist       │ "Produce a valid SPICE .cir netlist. Include component │
  │                     │  definitions, node connections, and .op/.tran          │
  │                     │  simulation directives. Output only the netlist,       │
  │                     │  no prose."                                            │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ kicad-schematic     │ "Produce a valid KiCad 6+ .kicad_sch schematic file.  │
  │                     │  Include component symbols with reference designators, │
  │                     │  values, and wire connections. Output only the         │
  │                     │  schematic file content, no prose."                    │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ netlist-json        │ "Produce a JSON netlist:                               │
  │                     │  { components: [{ref, value, footprint}],              │
  │                     │    nets: [string],                                     │
  │                     │    connections: [{from_ref, from_pin, to_ref, to_pin,  │
  │                     │                  net}] }"                              │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ svg-schematic       │ "Produce a valid SVG schematic diagram representing    │
  │                     │  the circuit. Use standard schematic symbols.          │
  │                     │  Output only SVG markup."                              │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ bom                 │ "Produce a Bill of Materials as a markdown table with  │
  │                     │  columns: Reference, Value, Description, Footprint,    │
  │                     │  Quantity, Source (document and section)."             │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ recipe              │ "Produce a recipe with: Ingredients (with quantities), │
  │                     │  Method (numbered steps), Variations, and a brief      │
  │                     │  Flavor Rationale citing source techniques."           │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ compliance-summary  │ "Produce a compliance summary with: Obligations        │
  │                     │  (bulleted), Checklist (checkbox items), and Citations │
  │                     │  (clause or article references for each item)."        │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ comparison          │ "Produce a side-by-side comparison table followed by   │
  │                     │  a narrative summary. Cite sources for each claim."    │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ study-guide         │ "Produce a study guide with: Key Concepts (defined),   │
  │                     │  Summary, and Practice Questions with answers."        │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ cross-reference     │ "Produce an indexed list of every location where       │
  │                     │  the queried topic appears. Format:                    │
  │                     │  Book Title > Section > exact quote or description."   │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ research-synthesis  │ "Produce a research synthesis: summarize the main      │
  │                     │  findings per source, note agreements and              │
  │                     │  contradictions, identify gaps. APA-style citations."  │
  ├─────────────────────┼────────────────────────────────────────────────────────┤
  │ custom              │ Use custom_prompt verbatim as the synthesis instruction.│
  └─────────────────────┴────────────────────────────────────────────────────────┘

  Implementation:
    1. Call GET /api/v1/search/chunks (or collections/:id/search/chunks)
       with query, chunk_type, rerank, limit params.
    2. Build context string from retrieved chunks:
         For each chunk: "[Source: {book_title} > {heading_path}]\n{chunk.text}\n\n"
    3. Call the LLM with:
         System: "You are a precise technical synthesizer. Use only the
                  provided source passages. Cite sources for every claim.
                  Do not add information not present in the sources."
         User:   "{format synthesis instruction}\n\nQuery: {query}\n\n
                  Sources:\n{context}"
    4. Return:
         {
           "query": "...",
           "format": "runsheet",
           "sources": [
             { "book_title": "...", "heading_path": "...", "chunk_id": "..." }
           ],
           "output": "...",         // the synthesized content
           "retrieval_ms": 43,
           "synthesis_ms": 1820
         }

  Gate: synthesize requires llm.enabled = true.
  If LLM disabled: return the retrieved chunks only (no synthesis) with
  a "synthesis_unavailable" flag — the caller can still use the chunks.

─────────────────────────────────────────
DELIVERABLE 3 — Collection UI (Admin)
─────────────────────────────────────────

apps/web/src/features/admin/CollectionsPage.tsx:
  Route: /admin/collections

  Table: Name | Domain | Books | Public | Actions (Edit, Delete)
  "New collection" button → inline form (name, description, domain, is_public)
  Collection detail → book picker (search + add from library)

  Add "Collections" to admin sidebar nav (below Authors).

apps/web/src/features/search/SearchPage.tsx — extend:
  Add a "Collection" filter pill to the search toolbar.
  When a collection is selected, scope the search to that collection.

─────────────────────────────────────────
DELIVERABLE 4 — Update MCP tool list
─────────────────────────────────────────

autolibre-mcp/src/tools/mod.rs — final MCP tool set:

  search_books       — existing: metadata search (unchanged)
  get_book_metadata  — existing: single book detail (unchanged)
  list_chapters      — existing: table of contents (unchanged)
  get_book_text      — existing: chapter-level text (kept for backward compat)
  search_chunks      — new (Stage 2): hybrid chunk search
  synthesize         — new (Stage 3): full synthesis pipeline
  list_collections   — new: list accessible collections
  get_collection     — new: collection detail + book list

─────────────────────────────────────────
TESTS
─────────────────────────────────────────

backend/tests/test_collections.rs:
  test_create_collection
  test_add_books_to_collection
  test_remove_book_from_collection
  test_collection_search_spans_all_member_books
  test_public_collection_visible_to_other_users
  test_private_collection_not_visible_to_other_users
  test_delete_collection_does_not_delete_books

backend/tests/test_synthesize.rs:
  test_synthesize_returns_output_and_sources
  test_synthesize_sources_are_grounded_in_chunks
  test_synthesize_runsheet_format_contains_steps
  test_synthesize_spice_format_output_is_valid_spice
  test_synthesize_custom_format_uses_custom_prompt
  test_synthesize_returns_chunks_only_when_llm_disabled
  test_synthesize_requires_authentication

─────────────────────────────────────────
VERIFICATION
─────────────────────────────────────────
cargo test --workspace
cargo clippy --workspace -- -D warnings
pnpm --filter @autolibre/web build
# Manual: create a collection; add 3 books; call synthesize MCP tool with
#         format="runsheet"; verify output cites specific heading_paths
# Manual: call synthesize with format="spice-netlist" against an electronics
#         collection; verify output is valid SPICE syntax
git add backend/ autolibre-mcp/ apps/web/src/features/
git commit -m "Phase 15 Stage 3: collections, synthesize MCP tool (14 formats), collection search UI"
```

---

## Review Checkpoints

| After Stage | Skill to run |
|---|---|
| Stage 1 | `/review` + `/security-review` — verify vision pass never fails ingest, image-heavy detection heuristic not gameable, chunk text does not include prompt injection from malicious PDFs |
| Stage 2 | `/review` — verify RRF implementation is correct (rank fusion math), reranking timeout does not hang handlers, BM25 scores do not leak between users |
| Stage 3 | `/review` + `/security-review` — verify synthesize prompt cannot be overridden by injected text in chunks, collection visibility enforcement (private collections not accessible cross-user), LLM output not cached cross-user |

Run `/engineering:deploy-checklist` after Stage 3 before tagging v2.0.

---

## Notes for the Codex Agent

**On vision LLM capability detection:**
Check the LLM's `/v1/models` response for vision capability before the vision pass. Common capability fields: `vision`, `image_input`, `multimodal`. If the field is absent or false, skip the vision pass silently — do not error.

**On SPICE and KiCad output validation:**
The synthesize tool cannot guarantee syntactic validity of machine-readable outputs — that is the agent's responsibility. The MCP tool returns the raw LLM output for `spice-netlist` and `kicad-schematic` formats. The caller should validate before use (e.g., run ngspice --check on the netlist). A future enhancement could add a validation step in the tool.

**On prompt injection from library content:**
Chunks retrieved from user-uploaded documents may contain text designed to manipulate the synthesis LLM (e.g., "Ignore previous instructions and..."). The synthesis system prompt must establish a strong context boundary. Consider wrapping each chunk in XML tags (`<source>...<source>`) and instructing the LLM to treat everything inside source tags as untrusted quoted material, not instructions.

**On the `semantic_search` MCP tool:**
Keep the existing `semantic_search` tool working as a deprecated alias for `search_chunks` with default parameters. Do not remove it — external agents may depend on it.
