//! Query modules, one per domain area.  Each module owns all SQL for its
//! tables and exposes async functions that take a `&SqlitePool` (or a
//! `&mut Transaction`) and return `anyhow::Result<T>`.
//!
//! Module responsibilities at a glance:
//! - `books` / `book_insert` — catalogue, list, patch, merge, delete, formats
//! - `auth` — user CRUD, lockout, refresh tokens, audit log
//! - `kobo` — device registration, delta sync cursor, reading state
//! - `collections` / `book_chunks` — LLM collections and semantic search
//! - `authors` / `tags` / `shelves` / `annotations` — editorial data
//! - `llm` / `webhooks` / `scheduled_tasks` — background job plumbing
//! - `totp` / `oauth` / `api_tokens` — authentication extensions
//! - `opds` / `stats` / `import_logs` / `libraries` — auxiliary features

pub mod annotations;
pub mod api_tokens;
pub mod auth;
pub mod authors;
pub mod book_chunks;
pub mod book_insert;
pub mod book_user_state;
pub mod books;
pub mod collections;
pub mod download_history;
pub mod email_settings;
pub mod import_logs;
pub mod kobo;
pub mod libraries;
pub mod llm;
pub mod oauth;
pub mod opds;
pub mod scheduled_tasks;
pub mod shelves;
pub mod stats;
pub mod tags;
pub mod totp;
pub mod user_tag_restrictions;
pub mod webhooks;
