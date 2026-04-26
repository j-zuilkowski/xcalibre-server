//! Search dispatcher and backend abstraction.
//!
//! Implements a **graceful degradation chain**:
//! 1. If `meilisearch.enabled = true` and the Meilisearch server is reachable at
//!    startup, use [`meili::MeilisearchBackend`] wrapped in
//!    [`MeiliWithFallbackBackend`].
//! 2. If Meilisearch is unavailable (health check fails at startup), fall through to
//!    [`fts5::Fts5Backend`] permanently for this process lifetime.
//! 3. At runtime, if a Meilisearch request fails, [`MeiliWithFallbackBackend`]
//!    automatically retries the same query against FTS5.
//!
//! [`build_search_backend`] is called once during app startup and returns an
//! `Arc<dyn SearchBackend>` stored in `AppState`.
//!
//! # Feature flag
//! Meilisearch support is compiled in only when the `meilisearch` Cargo feature is
//! enabled (default in production builds). Without it, only FTS5 is available.

use crate::{config::AppConfig, db::models::Book};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::SqlitePool;
use std::sync::Arc;

pub mod fts5;
#[cfg(feature = "meilisearch")]
pub mod meili;
pub mod semantic;

/// Parameters for a full-text search request.
///
/// Filters are additive (AND semantics). `book_ids` is an allowlist; pass an
/// empty vec to guarantee zero results (used by scoped search in reading lists).
/// `page` is 1-indexed; `page_size` is clamped to a backend-defined maximum.
#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub q: String,
    pub author_id: Option<String>,
    pub tag: Option<String>,
    pub language: Option<String>,
    pub format: Option<String>,
    pub book_ids: Option<Vec<String>>,
    pub page: u32,
    pub page_size: u32,
}

/// A single search result with a normalized relevance score.
///
/// `score` is in `[0.0, 1.0]` where 1.0 is most relevant. Backend scoring
/// differs: Meilisearch uses `_rankingScore`; FTS5 uses normalized BM25 rank.
#[derive(Clone, Debug)]
pub struct SearchHit {
    pub book_id: String,
    pub score: f32,
}

/// A paginated search result page.
#[derive(Clone, Debug)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

/// Trait implemented by all search backends.
///
/// Default no-op implementations of `index_book` and `remove_book` allow FTS5
/// (which is maintained by DB triggers) to satisfy the trait without overrides.
#[async_trait]
pub trait SearchBackend: Send + Sync {
    async fn search(&self, query: &SearchQuery) -> Result<SearchPage>;
    async fn suggest(&self, q: &str, limit: u8) -> Result<Vec<String>>;
    async fn index_book(&self, _book: &Book) -> Result<()> {
        Ok(())
    }
    async fn remove_book(&self, _book_id: &str) -> Result<()> {
        Ok(())
    }
    async fn is_available(&self) -> bool;
    fn backend_name(&self) -> &'static str;
}

/// Build and return the appropriate search backend based on config.
///
/// Always constructs an FTS5 backend as the baseline. If the `meilisearch` feature
/// is compiled in and `meilisearch.enabled = true`, attempts to connect to Meilisearch.
/// On success, returns a [`MeiliWithFallbackBackend`] that wraps both.
/// On failure (health check timeout, refused connection), logs a warning and returns
/// the plain FTS5 backend.
pub async fn build_search_backend(config: &AppConfig, db: SqlitePool) -> Arc<dyn SearchBackend> {
    let fts = Arc::new(fts5::Fts5Backend::new(db.clone()));

    #[cfg(feature = "meilisearch")]
    {
        if config.meilisearch.enabled {
            let api_key = (!config.meilisearch.api_key.trim().is_empty())
                .then(|| config.meilisearch.api_key.clone());

            if let Some(meili_backend) =
                meili::MeilisearchBackend::new(config.meilisearch.url.clone(), api_key).await
            {
                return Arc::new(MeiliWithFallbackBackend::new(meili_backend, fts.clone()));
            }

            tracing::warn!(
                url = %config.meilisearch.url,
                "meilisearch is enabled but unavailable; falling back to fts5"
            );
        }
    }

    #[cfg(not(feature = "meilisearch"))]
    {
        if config.meilisearch.enabled {
            tracing::warn!(
                "meilisearch is enabled in config, but backend was built without the meilisearch feature; falling back to fts5"
            );
        }
    }

    fts
}

#[cfg(feature = "meilisearch")]
#[derive(Clone)]
struct MeiliWithFallbackBackend {
    meili: Arc<meili::MeilisearchBackend>,
    fallback: Arc<fts5::Fts5Backend>,
}

#[cfg(feature = "meilisearch")]
impl MeiliWithFallbackBackend {
    fn new(meili: meili::MeilisearchBackend, fallback: Arc<fts5::Fts5Backend>) -> Self {
        Self {
            meili: Arc::new(meili),
            fallback,
        }
    }
}

#[cfg(feature = "meilisearch")]
#[async_trait]
impl SearchBackend for MeiliWithFallbackBackend {
    async fn search(&self, query: &SearchQuery) -> Result<SearchPage> {
        match self.meili.search(query).await {
            Ok(page) => Ok(page),
            Err(err) => {
                tracing::warn!(error = %err, "meilisearch search failed; falling back to fts5");
                self.fallback.search(query).await
            }
        }
    }

    async fn suggest(&self, q: &str, limit: u8) -> Result<Vec<String>> {
        match self.meili.suggest(q, limit).await {
            Ok(suggestions) => Ok(suggestions),
            Err(err) => {
                tracing::warn!(error = %err, "meilisearch suggest failed; falling back to fts5");
                self.fallback.suggest(q, limit).await
            }
        }
    }

    async fn index_book(&self, book: &Book) -> Result<()> {
        self.meili.index_book(book).await
    }

    async fn remove_book(&self, book_id: &str) -> Result<()> {
        self.meili.remove_book(book_id).await
    }

    async fn is_available(&self) -> bool {
        self.meili.is_available().await
    }

    fn backend_name(&self) -> &'static str {
        "meilisearch"
    }
}
