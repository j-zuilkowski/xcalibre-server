//! Search endpoints: full-text, semantic, and hybrid chunk search.
//!
//! Routes under `/api/v1/search/`. All routes require a valid JWT.
//!
//! - `GET /search` — full-text search via Meilisearch (falls back to SQLite FTS5).
//! - `GET /search/semantic` — vector similarity search via sqlite-vec embeddings;
//!   returns 503 when LLM/semantic indexing is disabled.
//! - `GET /search/chunks` — hybrid BM25+vector passage search with Reciprocal Rank
//!   Fusion scoring and optional LLM reranking; the primary RAG retrieval surface.
//! - `GET /search/suggestions` — query autocomplete from the search backend.
//! - `GET /system/search-status` — reports which backends are active.
//!
//! `run_chunk_search` and `collection_book_ids_for_search` are exported for use
//! by `collections.rs` (collection-scoped chunk search).

use crate::{
    db::queries::{
        book_chunks as chunk_queries, books as book_queries, collections as collection_queries,
    },
    ingest::chunker::ChunkType,
    middleware::auth::AuthenticatedUser,
    search::SearchQuery,
    AppError, AppState,
};
use axum::{
    extract::{Extension, Query, State},
    middleware,
    routing::get,
    Json, Router,
};
use futures::future::join_all;
use serde::de::{self, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::time::{Duration, Instant};
use utoipa::{IntoParams, ToSchema};

const MAX_CHUNK_SEARCH_RESULTS: u32 = 100;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/search", get(search_books))
        .route("/api/v1/search/semantic", get(search_semantic))
        .route("/api/v1/search/chunks", get(search_chunks))
        .route("/api/v1/search/suggestions", get(search_suggestions))
        .route("/api/v1/system/search-status", get(search_status))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, Default, IntoParams)]
pub(crate) struct SearchQueryParams {
    q: Option<String>,
    author_id: Option<String>,
    tag: Option<String>,
    language: Option<String>,
    format: Option<String>,
    collection_id: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Deserialize, Default, IntoParams)]
struct SuggestionsQueryParams {
    q: Option<String>,
    limit: Option<u8>,
}

#[derive(Debug, Deserialize, Default, IntoParams)]
pub(crate) struct SemanticSearchQueryParams {
    q: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Deserialize, Default, IntoParams)]
pub(crate) struct ChunkSearchQueryParams {
    q: Option<String>,
    #[serde(
        default,
        alias = "book_ids[]",
        deserialize_with = "deserialize_string_or_many"
    )]
    book_ids: Vec<String>,
    collection_id: Option<String>,
    #[serde(default, rename = "type")]
    chunk_type: Option<String>,
    limit: Option<u32>,
    rerank: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PaginatedResponse<T> {
    items: Vec<T>,
    total: u64,
    page: u32,
    page_size: u32,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SearchResultItem {
    #[serde(flatten)]
    book: book_queries::BookSummary,
    score: f32,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ChunkSearchResponse {
    query: String,
    chunks: Vec<ChunkSearchItem>,
    total_searched: u64,
    retrieval_ms: u64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ChunkSearchItem {
    chunk_id: String,
    book_id: String,
    book_title: String,
    heading_path: Option<String>,
    chunk_type: ChunkType,
    text: String,
    word_count: i64,
    bm25_score: Option<f32>,
    cosine_score: Option<f32>,
    rrf_score: f32,
    rerank_score: Option<f32>,
}

#[derive(Debug, Serialize)]
struct SuggestionsResponse {
    suggestions: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SearchStatusResponse {
    fts: bool,
    meilisearch: bool,
    semantic: bool,
    backend: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/search",
    tag = "search",
    security(("bearer_auth" = [])),
    params(SearchQueryParams),
    responses(
        (status = 200, description = "Paginated search results", body = PaginatedResponse<SearchResultItem>),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn search_books(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Query(query): Query<SearchQueryParams>,
) -> Result<Json<PaginatedResponse<SearchResultItem>>, AppError> {
    let q = query.q.unwrap_or_default();
    if q.trim().is_empty() {
        return Err(AppError::BadRequest);
    }

    let page = query.page.unwrap_or(1).max(1);
    let page_size = clamp_page_size(query.page_size.unwrap_or(24));

    let search_page = state
        .search
        .search(&SearchQuery {
            q,
            author_id: query.author_id,
            tag: query.tag,
            language: query.language,
            format: query.format,
            book_ids: collection_book_ids_for_search(
                &state,
                &auth_user.user.id,
                query.collection_id.as_deref(),
            )
            .await?,
            page,
            page_size,
        })
        .await
        .map_err(|_| AppError::Internal)?;

    let ordered_ids = search_page
        .hits
        .iter()
        .map(|hit| hit.book_id.clone())
        .collect::<Vec<_>>();

    let summaries = book_queries::list_book_summaries_by_ids(
        &state.db,
        &ordered_ids,
        Some(auth_user.user.default_library_id.as_str()),
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?;

    let mut summary_by_id = HashMap::new();
    for summary in summaries {
        summary_by_id.insert(summary.id.clone(), summary);
    }

    let mut items = Vec::with_capacity(search_page.hits.len());
    for hit in search_page.hits {
        if let Some(book) = summary_by_id.remove(&hit.book_id) {
            items.push(SearchResultItem {
                book,
                score: hit.score,
            });
        }
    }

    Ok(Json(PaginatedResponse {
        items,
        total: search_page.total,
        page: search_page.page,
        page_size: search_page.page_size,
    }))
}

async fn search_suggestions(
    State(state): State<AppState>,
    Query(query): Query<SuggestionsQueryParams>,
) -> Result<Json<SuggestionsResponse>, AppError> {
    let q = query.q.unwrap_or_default();
    if q.trim().is_empty() {
        return Err(AppError::BadRequest);
    }

    let limit = query.limit.unwrap_or(5).clamp(1, 10);
    let suggestions = state
        .search
        .suggest(&q, limit)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(Json(SuggestionsResponse { suggestions }))
}

#[utoipa::path(
    get,
    path = "/api/v1/search/semantic",
    tag = "search",
    security(("bearer_auth" = [])),
    params(SemanticSearchQueryParams),
    responses(
        (status = 200, description = "Paginated semantic search results", body = PaginatedResponse<SearchResultItem>),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn search_semantic(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Query(query): Query<SemanticSearchQueryParams>,
) -> Result<Json<PaginatedResponse<SearchResultItem>>, AppError> {
    let q = query.q.unwrap_or_default();
    if q.trim().is_empty() {
        return Err(AppError::BadRequest);
    }

    let semantic = semantic_search_or_unavailable(&state)?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = clamp_semantic_page_size(query.page_size.unwrap_or(24));

    let search_page = semantic
        .search_semantic(&q, page, page_size)
        .await
        .map_err(|err| {
            tracing::warn!(error = %err, "semantic search failed");
            AppError::ServiceUnavailable
        })?;

    let ordered_ids = search_page
        .hits
        .iter()
        .map(|hit| hit.book_id.clone())
        .collect::<Vec<_>>();

    let summaries = book_queries::list_book_summaries_by_ids(
        &state.db,
        &ordered_ids,
        Some(auth_user.user.default_library_id.as_str()),
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?;

    let mut summary_by_id = HashMap::new();
    for summary in summaries {
        summary_by_id.insert(summary.id.clone(), summary);
    }

    let mut items = Vec::with_capacity(search_page.hits.len());
    for hit in search_page.hits {
        if let Some(book) = summary_by_id.remove(&hit.book_id) {
            items.push(SearchResultItem {
                book,
                score: hit.score,
            });
        }
    }

    Ok(Json(PaginatedResponse {
        items,
        total: search_page.total,
        page: search_page.page,
        page_size: search_page.page_size,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/search/chunks",
    tag = "search",
    security(("bearer_auth" = [])),
    params(ChunkSearchQueryParams),
    responses(
        (status = 200, description = "Hybrid chunk search results", body = ChunkSearchResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn search_chunks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Query(query): Query<ChunkSearchQueryParams>,
) -> Result<Json<ChunkSearchResponse>, AppError> {
    let raw_query = query.q.unwrap_or_default();
    let query_text = raw_query.trim().to_string();
    if query_text.is_empty() {
        return Err(AppError::BadRequest);
    }
    let limit = clamp_chunk_limit(query.limit.unwrap_or(10));
    let rerank = parse_truthy_bool(query.rerank.as_deref());
    let book_ids = normalize_ids(query.book_ids);
    let collection_id = query
        .collection_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let chunk_type = query
        .chunk_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let collection_book_ids =
        collection_book_ids_for_search(&state, &auth_user.user.id, collection_id.as_deref())
            .await?;
    let scoped_book_ids = match (collection_book_ids, book_ids.is_empty()) {
        (None, true) => None,
        (None, false) => Some(book_ids),
        (Some(allowed), true) => Some(allowed),
        (Some(allowed), false) => {
            let mut scoped = book_ids;
            scoped.retain(|book_id| allowed.iter().any(|allowed| allowed == book_id));
            Some(scoped)
        }
    };

    let response = run_chunk_search(
        &state,
        &auth_user,
        query_text,
        scoped_book_ids,
        chunk_type,
        limit,
        rerank,
    )
    .await?;

    Ok(Json(response))
}

async fn search_status(
    State(state): State<AppState>,
) -> Result<Json<SearchStatusResponse>, AppError> {
    let backend = state.search.backend_name().to_string();
    let meilisearch = backend == "meilisearch" && state.search.is_available().await;
    let semantic = state
        .semantic_search
        .as_ref()
        .map(|semantic| semantic.is_configured())
        .unwrap_or(false);

    Ok(Json(SearchStatusResponse {
        fts: true,
        meilisearch,
        semantic,
        backend,
    }))
}

fn clamp_page_size(page_size: u32) -> u32 {
    match page_size {
        0 => 24,
        n if n > 100 => 100,
        n => n,
    }
}

fn clamp_semantic_page_size(page_size: u32) -> u32 {
    match page_size {
        0 => 24,
        n if n > 50 => 50,
        n => n,
    }
}

fn clamp_chunk_limit(limit: u32) -> usize {
    match limit {
        0 => 10,
        n if n > MAX_CHUNK_SEARCH_RESULTS => MAX_CHUNK_SEARCH_RESULTS as usize,
        n => n as usize,
    }
}

fn normalize_ids(mut ids: Vec<String>) -> Vec<String> {
    ids.retain(|id| !id.trim().is_empty());
    for id in &mut ids {
        *id = id.trim().to_string();
    }

    let mut seen = std::collections::HashSet::new();
    ids.retain(|id| seen.insert(id.clone()));
    ids
}

/// Sanitizes a raw query string for SQLite FTS5: strips non-alphanumeric characters
/// (to prevent FTS5 syntax injection) and appends `*` prefix-matching to each token.
fn normalize_fts_query(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_alphanumeric() || ch.is_whitespace() || ch == '*' {
            sanitized.push(ch);
        } else {
            sanitized.push(' ');
        }
    }

    let terms = sanitized
        .split_whitespace()
        .map(|term| term.trim_matches('*'))
        .filter(|term| !term.is_empty())
        .map(|term| format!("{term}*"))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// Core chunk retrieval pipeline: runs BM25 and (if available) semantic search in parallel,
/// fuses results with RRF, optionally reranks with an LLM, then resolves book titles.
/// `book_ids` is `None` to search all accessible books or `Some(ids)` to scope to a subset.
pub(crate) async fn run_chunk_search(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    query_text: String,
    book_ids: Option<Vec<String>>,
    chunk_type: Option<String>,
    limit: usize,
    rerank: bool,
) -> Result<ChunkSearchResponse, AppError> {
    let normalized_query =
        normalize_fts_query(Some(query_text.as_str())).ok_or(AppError::BadRequest)?;
    let filters = chunk_queries::ChunkSearchFilters {
        book_ids: book_ids.as_deref().unwrap_or(&[]),
        collection_id: None,
        chunk_type: chunk_type.as_deref(),
    };

    let started_at = Instant::now();
    let total_searched = chunk_queries::count_searchable_book_chunks(&state.db, &filters)
        .await
        .map_err(|_| AppError::Internal)? as u64;

    let bm25_hits = chunk_queries::search_chunks_bm25(&state.db, &normalized_query, &filters, 100)
        .await
        .map_err(|_| AppError::Internal)?;

    let semantic_hits = if let Some(semantic) = state.semantic_search.as_ref() {
        if semantic.is_configured() {
            match semantic.embed_text(query_text.as_str()).await {
                Ok(vector) => {
                    match chunk_queries::search_chunks_semantic(&state.db, &vector, &filters, 100)
                        .await
                    {
                        Ok(hits) => hits,
                        Err(err) => {
                            tracing::warn!(error = %err, "chunk semantic search failed");
                            Vec::new()
                        }
                    }
                }
                Err(err) => {
                    tracing::warn!(error = %err, "chunk query embedding failed");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let mut fused = fuse_chunk_results(bm25_hits, semantic_hits);
    let fused_limit = limit.saturating_mul(5).max(1);
    fused.truncate(fused_limit);

    if rerank {
        if let Some(chat_client) = state.chat_client.as_ref() {
            if let Some(reranked) =
                rerank_chunk_results(chat_client, query_text.as_str(), fused.clone()).await
            {
                fused = reranked;
            }
        }
    }

    let ordered_ids = fused
        .iter()
        .map(|chunk| chunk.book_id.clone())
        .collect::<Vec<_>>();
    let summaries = book_queries::list_book_summaries_by_ids(
        &state.db,
        &ordered_ids,
        Some(auth_user.user.default_library_id.as_str()),
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?;

    let mut summary_by_id = HashMap::new();
    for summary in summaries {
        summary_by_id.insert(summary.id.clone(), summary);
    }

    let mut chunks = Vec::with_capacity(fused.len());
    for chunk in fused {
        if let Some(book) = summary_by_id.get(&chunk.book_id) {
            chunks.push(ChunkSearchItem {
                chunk_id: chunk.id,
                book_id: chunk.book_id,
                book_title: book.title.clone(),
                heading_path: chunk.heading_path,
                chunk_type: chunk.chunk_type,
                text: chunk.text,
                word_count: chunk.word_count,
                bm25_score: chunk.bm25_score,
                cosine_score: chunk.cosine_score,
                rrf_score: chunk.rrf_score as f32,
                rerank_score: chunk.rerank_score,
            });
        }

        if chunks.len() >= limit {
            break;
        }
    }

    Ok(ChunkSearchResponse {
        query: query_text,
        chunks,
        total_searched,
        retrieval_ms: started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

/// Resolves the book ID allow-list for a collection-scoped search, verifying the caller
/// can see the collection. Returns `None` when no `collection_id` is given (search all books).
pub(crate) async fn collection_book_ids_for_search(
    state: &AppState,
    user_id: &str,
    collection_id: Option<&str>,
) -> Result<Option<Vec<String>>, AppError> {
    let Some(collection_id) = collection_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };

    crate::api::collections::ensure_visible_collection(state, user_id, collection_id).await?;
    collection_queries::get_collection_book_ids(&state.db, collection_id)
        .await
        .map(Some)
        .map_err(|_| AppError::Internal)
}

#[derive(Clone)]
struct FusedChunk {
    id: String,
    book_id: String,
    chunk_index: i64,
    heading_path: Option<String>,
    chunk_type: ChunkType,
    text: String,
    word_count: i64,
    bm25_score: Option<f32>,
    cosine_score: Option<f32>,
    bm25_rank: Option<usize>,
    cosine_rank: Option<usize>,
    rrf_score: f64,
    rerank_score: Option<f32>,
}

/// Merges BM25 and semantic hit lists using Reciprocal Rank Fusion, deduplicating by chunk ID.
fn fuse_chunk_results(
    bm25_hits: Vec<chunk_queries::ChunkSearchRecord>,
    semantic_hits: Vec<chunk_queries::ChunkSearchRecord>,
) -> Vec<FusedChunk> {
    let mut chunks = HashMap::<String, FusedChunk>::new();

    for (rank, hit) in bm25_hits.into_iter().enumerate() {
        let entry = chunks.entry(hit.id.clone()).or_insert_with(|| FusedChunk {
            id: hit.id.clone(),
            book_id: hit.book_id.clone(),
            chunk_index: hit.chunk_index,
            heading_path: hit.heading_path.clone(),
            chunk_type: hit.chunk_type,
            text: hit.text.clone(),
            word_count: hit.word_count,
            bm25_score: hit.bm25_score,
            cosine_score: hit.cosine_score,
            bm25_rank: None,
            cosine_rank: None,
            rrf_score: 0.0,
            rerank_score: None,
        });
        entry.bm25_score = hit.bm25_score;
        entry.bm25_rank = Some(rank + 1);
    }

    for (rank, hit) in semantic_hits.into_iter().enumerate() {
        let entry = chunks.entry(hit.id.clone()).or_insert_with(|| FusedChunk {
            id: hit.id.clone(),
            book_id: hit.book_id.clone(),
            chunk_index: hit.chunk_index,
            heading_path: hit.heading_path.clone(),
            chunk_type: hit.chunk_type,
            text: hit.text.clone(),
            word_count: hit.word_count,
            bm25_score: hit.bm25_score,
            cosine_score: hit.cosine_score,
            bm25_rank: None,
            cosine_rank: None,
            rrf_score: 0.0,
            rerank_score: None,
        });
        entry.cosine_score = hit.cosine_score;
        entry.cosine_rank = Some(rank + 1);
    }

    for chunk in chunks.values_mut() {
        chunk.rrf_score = reciprocal_rank_fusion_score(chunk.bm25_rank, chunk.cosine_rank);
    }

    let mut ordered = chunks.into_values().collect::<Vec<_>>();
    ordered.sort_by(|left, right| {
        right
            .rrf_score
            .partial_cmp(&left.rrf_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.book_id.cmp(&right.book_id))
            .then_with(|| left.chunk_index.cmp(&right.chunk_index))
            .then_with(|| left.id.cmp(&right.id))
    });
    ordered
}

/// Computes the RRF score for a chunk given its optional rank in each result list.
/// Uses the standard k=60 constant: `score = Σ 1/(k + rank)`.
fn reciprocal_rank_fusion_score(bm25_rank: Option<usize>, cosine_rank: Option<usize>) -> f64 {
    const K: f64 = 60.0;
    let mut score = 0.0;
    if let Some(rank) = bm25_rank {
        score += 1.0 / (K + rank as f64);
    }
    if let Some(rank) = cosine_rank {
        score += 1.0 / (K + rank as f64);
    }
    score
}

/// Asks the LLM to score each chunk's relevance to the query (0.0–1.0) and re-sorts by that score.
/// Operates on at most the top 50 RRF-fused chunks; applies a 10-second timeout and returns
/// `None` on any failure so callers fall back to RRF ordering silently.
async fn rerank_chunk_results(
    chat_client: &crate::llm::chat::ChatClient,
    query: &str,
    chunks: Vec<FusedChunk>,
) -> Option<Vec<FusedChunk>> {
    let chunks = chunks.into_iter().take(50).collect::<Vec<_>>();
    let futures = chunks.iter().map(|chunk| {
        let prompt = format!(
            "Query: {query}\n\nPassage: {}\n\nScore the relevance of this passage to the query from 0.0 to 1.0.\nReply with only the number.",
            chunk.text
        );
        async move {
            let response = chat_client.complete(&prompt).await?;
            let score = parse_rerank_score(&response).ok_or_else(|| {
                anyhow::anyhow!("invalid rerank score response: {response}")
            })?;
            Ok::<f32, anyhow::Error>(score)
        }
    });

    let rerank_results =
        match tokio::time::timeout(Duration::from_secs(10), join_all(futures)).await {
            Ok(scores) => scores,
            Err(_) => return None,
        };

    let mut scores = Vec::with_capacity(rerank_results.len());
    for score in rerank_results {
        let score = match score {
            Ok(score) => score,
            Err(err) => {
                tracing::warn!(error = %err, "chunk rerank failed");
                return None;
            }
        };
        scores.push(score);
    }

    let mut reranked = chunks;
    for (chunk, score) in reranked.iter_mut().zip(scores.into_iter()) {
        chunk.rerank_score = Some(score);
    }

    reranked.sort_by(|left, right| {
        let left_score = left.rerank_score.unwrap_or(0.0);
        let right_score = right.rerank_score.unwrap_or(0.0);
        right_score
            .partial_cmp(&left_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| {
                right
                    .rrf_score
                    .partial_cmp(&left.rrf_score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| left.book_id.cmp(&right.book_id))
            .then_with(|| left.chunk_index.cmp(&right.chunk_index))
            .then_with(|| left.id.cmp(&right.id))
    });

    Some(reranked)
}

fn parse_rerank_score(response: &str) -> Option<f32> {
    let token = response.split_whitespace().next()?;
    let score = token.parse::<f32>().ok()?;
    Some(score.clamp(0.0, 1.0))
}

fn semantic_search_or_unavailable(
    state: &AppState,
) -> Result<std::sync::Arc<crate::search::semantic::SemanticSearch>, AppError> {
    if !state.config.llm.enabled {
        return Err(AppError::ServiceUnavailable);
    }

    let Some(semantic) = state.semantic_search.clone() else {
        return Err(AppError::ServiceUnavailable);
    };

    if !semantic.is_configured() {
        return Err(AppError::ServiceUnavailable);
    }

    Ok(semantic)
}

/// Serde helper that accepts either a single string or an array of strings for the `book_ids[]`
/// query parameter, because different HTTP clients serialize repeated params differently.
pub(crate) fn deserialize_string_or_many<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct StringOrMany;

    impl<'de> Visitor<'de> for StringOrMany {
        type Value = Vec<String>;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string or a sequence of strings")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![value.to_string()])
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            Ok(vec![value])
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut values = Vec::new();
            while let Some(value) = seq.next_element::<String>()? {
                values.push(value);
            }
            Ok(values)
        }
    }

    deserializer.deserialize_any(StringOrMany)
}

fn parse_truthy_bool(value: Option<&str>) -> bool {
    matches!(
        value.map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "true" | "1" | "yes" | "on")
    )
}
