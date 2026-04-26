//! LLM feature modules for xcalibre-server.
//!
//! All LLM functionality is **disabled by default** (`ENABLE_LLM_FEATURES = false`).
//! The central enablement flag is `config.llm.enabled`; when it is `false`, callers
//! receive `None` for `Option<LlmClient>` (stored in `AppState`) and must fall back
//! gracefully without returning errors.
//!
//! # Module overview
//! - [`chat`] — HTTP client wrapper (10s timeout, OpenAI-compatible API)
//! - [`classify`] — Genre/tag classification for a single book
//! - [`classify_type`] — Domain classification (electronics, culinary, etc.)
//! - [`derive`] — Summary, related titles, discussion questions
//! - [`embeddings`] — Text embedding via embedding endpoint
//! - [`job_runner`] — Background job loop polling `llm_jobs` table
//! - [`quality`] — Prose quality scoring
//! - [`synthesize`] — Multi-source cross-document synthesis (14 formats)
//! - [`validate`] — Metadata validation (title/author/ISBN/description)
//! - [`vision`] — Vision LLM pass for image-heavy PDF/EPUB pages
//!
//! # `Option<LlmClient>` pattern
//! `AppState.chat_client: Option<LlmClient>` is `None` when LLM is disabled.
//! Every module that calls the LLM accepts `Option<&LlmClient>` or checks the flag
//! before proceeding. This makes the disabled-path a zero-cost early return.

pub mod chat;
pub mod classify;
pub mod classify_type;
pub mod derive;
pub mod embeddings;
pub mod job_runner;
pub mod quality;
pub mod synthesize;
pub mod validate;
pub mod vision;

/// Type alias for the configured chat completion client.
pub type LlmClient = chat::ChatClient;
