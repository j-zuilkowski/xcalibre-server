//! Ingest pipeline modules for xcalibre-server.
//!
//! The ingest pipeline transforms uploaded book files into indexed, searchable content.
//! Pipeline order (orchestrated by the API handlers in `api/books.rs`):
//!
//! 1. **Format detection** — file extension + magic bytes → format string.
//! 2. **Metadata extraction** — title/author/ISBN from EPUB OPF, PDF metadata, etc.
//! 3. **Cover extraction** — first EPUB image or PDF page render.
//! 4. **Duplicate check** — ISBN or title+author collision detection.
//! 5. **LLM classify** — enqueue a `classify` job (runs asynchronously).
//! 6. **Write** — persist book record, format file, cover to storage + DB.
//! 7. **Index** — [`text::generate_and_store_book_chunks`] produces RAG chunks,
//!    embeddings, and updates `indexed_at` on the book row.
//!
//! # Sub-modules
//! - [`chunker`] — domain-aware text chunking algorithm
//! - [`goodreads`] — CSV parsers for Goodreads and StoryGraph exports
//! - [`mobi_util`] — MOBI/AZW3 HTML decode and chapter splitting utilities
//! - [`text`] — plain-text extraction for EPUB/PDF/MOBI/TXT
//! - [`vision`] — re-exports [`llm::vision::describe_image_page`] so ingest code
//!   doesn't need to reach into the `llm` module directly

pub mod chunker;
pub mod goodreads;
pub mod mobi_util;
pub mod text;
pub mod vision;
