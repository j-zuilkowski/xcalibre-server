use crate::{
    db::queries::books as book_queries,
    search::SearchQuery,
    AppError, AppState,
};
use axum::{
    extract::{Query, State},
    middleware,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/search", get(search_books))
        .route("/api/v1/search/suggestions", get(search_suggestions))
        .route("/api/v1/system/search-status", get(search_status))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, Default)]
struct SearchQueryParams {
    q: Option<String>,
    author_id: Option<String>,
    tag: Option<String>,
    language: Option<String>,
    format: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct SuggestionsQueryParams {
    q: Option<String>,
    limit: Option<u8>,
}

#[derive(Debug, Serialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
    total: u64,
    page: u32,
    page_size: u32,
}

#[derive(Debug, Serialize)]
struct SearchResultItem {
    #[serde(flatten)]
    book: book_queries::BookSummary,
    score: f32,
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

async fn search_books(
    State(state): State<AppState>,
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

    let summaries = book_queries::list_book_summaries_by_ids(&state.db, &ordered_ids)
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

async fn search_status(State(state): State<AppState>) -> Result<Json<SearchStatusResponse>, AppError> {
    let backend = state.search.backend_name().to_string();
    let meilisearch = backend == "meilisearch" && state.search.is_available().await;

    Ok(Json(SearchStatusResponse {
        fts: true,
        meilisearch,
        semantic: state.llm.is_some(),
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
