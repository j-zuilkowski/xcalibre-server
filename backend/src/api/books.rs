//! Book library CRUD, file serving, reading progress, annotations, and text extraction.
//!
//! All routes under `/api/v1/books/`. All routes require a valid JWT.
//!
//! Role guards applied inside handlers (not at router level):
//! - `can_upload` — required for `POST /books` (upload) and `PATCH /books` (bulk edit).
//! - `can_edit` — required for `PATCH /books/:id`, `DELETE /books/:id`, merge, custom columns.
//! - `can_download` — required for `GET /books/:id/formats/:format/download` and stream.
//! - Admin — required for bulk-edit across users and certain tag confirmation flows.
//!
//! Path traversal prevention: book file paths are stored relative to the storage root
//! and validated by `validate_relative_path` before any file is opened.
//!
//! Cover images: stored as `{book_id}.jpg` relative to the storage root; served with
//! conditional GET (ETag/If-None-Match) and range support.
//!
//! RAG surface: `GET /books/:id/chunks` re-chunks the book on demand if needed and
//! returns structured text passages. `GET /books/:id/text` extracts plain text for
//! display or external consumption.

use crate::{
    db::queries::{
        annotations as annotation_queries, book_chunks as chunk_queries,
        book_user_state as book_state_queries, books as book_queries,
        download_history as download_history_queries, llm as llm_queries,
    },
    ingest::{
        chunker::{ChunkDomain, ChunkType},
        mobi_util, text as ingest_text,
    },
    llm::classify_type::{classify_document_type, DocumentType},
    metrics::ImportMetricsGuard,
    middleware::auth::AuthenticatedUser,
    webhooks as webhook_engine, AppError, AppState,
};
use axum::{
    body::Body,
    extract::{DefaultBodyLimit, Extension, Multipart, Path, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware,
    routing::{delete, get, patch, post},
    Json, Router,
};
use lettre::{
    message::{header::ContentType, Attachment, Mailbox, MultiPart, SinglePart},
    transport::smtp::authentication::Credentials,
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::time::Duration;
use std::{
    borrow::Cow,
    io::{Cursor, Read, Seek, Write},
    path::{Component, Path as FsPath, PathBuf},
    sync::Arc,
};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use zip::{write::FileOptions, CompressionMethod, ZipArchive, ZipWriter};

/// Assembles the books sub-router and attaches the JWT auth middleware to all routes.
pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    let upload_max_bytes = state.config.limits.upload_max_bytes;

    Router::new()
        .route(
            "/api/v1/books",
            get(list_books).patch(bulk_edit_books),
        )
        .route(
            "/api/v1/books",
            post(upload_book).layer(DefaultBodyLimit::max(upload_max_bytes as usize)),
        )
        .route(
            "/api/v1/books/in-progress",
            get(list_in_progress_books),
        )
        .route(
            "/api/v1/books/custom-columns",
            get(list_custom_columns).post(create_custom_column),
        )
        .route(
            "/api/v1/books/custom-columns/:id",
            delete(delete_custom_column),
        )
        .route("/api/v1/books/downloads", get(list_download_history))
        .route(
            "/api/v1/books/:id/custom-values",
            get(get_book_custom_values).patch(patch_book_custom_values),
        )
        .route("/api/v1/books/:id/merge", post(merge_book))
        .route(
            "/api/v1/books/:id",
            get(get_book).patch(patch_book).delete(delete_book),
        )
        .route(
            "/api/v1/books/:id/progress",
            get(get_reading_progress)
                .patch(upsert_reading_progress)
                .put(upsert_reading_progress),
        )
        .route(
            "/api/v1/books/:id/annotations",
            get(list_annotations).post(create_annotation),
        )
        .route(
            "/api/v1/books/:id/annotations/:ann_id",
            patch(patch_annotation).delete(delete_annotation),
        )
        .route(
            "/api/v1/reading-progress/:id",
            get(get_reading_progress)
                .patch(upsert_reading_progress)
                .put(upsert_reading_progress),
        )
        .route("/api/v1/books/:id/read", post(set_read))
        .route("/api/v1/books/:id/archive", post(set_archive))
        .route("/api/v1/books/:id/cover", get(get_cover))
        .route("/api/v1/books/:id/chapters", get(get_chapters))
        .route("/api/v1/books/:id/chunks", get(get_chunks))
        .route("/api/v1/books/:id/text", get(get_text))
        .route("/api/v1/books/:id/send", post(send_book))
        .route("/api/v1/books/:id/comic/pages", get(get_comic_pages))
        .route("/api/v1/books/:id/comic/page/:index", get(get_comic_page))
        .route("/api/v1/books/:id/metadata-lookup", get(metadata_lookup))
        .route("/api/v1/books/:id/metadata/search", get(search_book_metadata))
        .route(
            "/api/v1/books/:id/formats/:format/download",
            get(download_format),
        )
        .route(
            "/api/v1/books/:id/formats/:format/stream",
            get(stream_format),
        )
        .route(
            "/api/v1/books/:id/formats/:format/to-epub",
            get(mobi_to_epub),
        )
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, Default)]
pub(crate) struct ListBooksQuery {
    q: Option<String>,
    author_id: Option<String>,
    series_id: Option<String>,
    tag: Option<SingleOrMany>,
    language: Option<String>,
    format: Option<String>,
    document_type: Option<String>,
    sort: Option<String>,
    order: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
    since: Option<String>,
    show_archived: Option<bool>,
    only_read: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SingleOrMany {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PaginatedResponse<T> {
    items: Vec<T>,
    total: i64,
    page: i64,
    page_size: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct PatchBookRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    sort_title: Option<String>,
    #[serde(default)]
    description: Option<Option<String>>,
    #[serde(default)]
    pubdate: Option<Option<String>>,
    #[serde(default)]
    language: Option<Option<String>>,
    #[serde(default)]
    rating: Option<i64>,
    #[serde(default)]
    series_id: Option<Option<String>>,
    #[serde(default)]
    series_index: Option<Option<f64>>,
    #[serde(default)]
    authors: Option<Vec<String>>,
    #[serde(default)]
    identifiers: Option<Vec<IdentifierPatch>>,
}

#[derive(Debug, Deserialize)]
struct MergeBookRequest {
    duplicate_id: String,
}

#[derive(Debug, Deserialize)]
struct CreateCustomColumnRequest {
    name: String,
    label: String,
    column_type: String,
    #[serde(default)]
    is_multiple: bool,
}

#[derive(Debug, Deserialize)]
struct CustomValuePatchRequest {
    column_id: String,
    value: Value,
}

#[derive(Debug, Deserialize)]
struct SendBookRequest {
    to: String,
    format: String,
}

#[derive(Debug, Deserialize)]
struct MetadataLookupQuery {
    source: Option<String>,
}

#[derive(Debug, Deserialize, Default, IntoParams)]
pub(crate) struct MetadataSearchQuery {
    q: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct GetChunksQuery {
    #[serde(default)]
    size: Option<usize>,
    #[serde(default)]
    overlap: Option<usize>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(rename = "type", default)]
    chunk_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct ComicPageEntry {
    index: usize,
    url: String,
}

#[derive(Debug, Serialize)]
struct ComicPagesResponse {
    total_pages: usize,
    pages: Vec<ComicPageEntry>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ChunkResponse {
    pub id: String,
    pub chunk_index: usize,
    pub chapter_index: usize,
    pub heading_path: Option<String>,
    pub chunk_type: ChunkType,
    pub text: String,
    pub word_count: usize,
    pub has_image: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ChunksResponse {
    pub book_id: String,
    pub chunk_count: usize,
    pub chunks: Vec<ChunkResponse>,
}

#[derive(Debug, Deserialize)]
struct BulkEditRequest {
    book_ids: Vec<String>,
    fields: BulkEditFieldsRequest,
}

#[derive(Debug, Deserialize, Default)]
struct BulkEditFieldsRequest {
    tags: Option<BulkEditTagField>,
    series: Option<BulkEditStringField>,
    rating: Option<BulkEditNumberField>,
    language: Option<BulkEditStringField>,
    publisher: Option<BulkEditStringField>,
}

#[derive(Debug, Deserialize)]
struct BulkEditTagField {
    mode: String,
    values: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct BulkEditStringField {
    mode: String,
    value: String,
}

#[derive(Debug, Deserialize)]
struct BulkEditNumberField {
    mode: String,
    value: i64,
}

#[derive(Debug, Serialize)]
struct BulkEditResponse {
    updated: i64,
    errors: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ReadStateRequest {
    is_read: bool,
}

#[derive(Debug, Deserialize, Default, ToSchema)]
pub(crate) struct ReadingProgressRequest {
    #[serde(default)]
    format_id: Option<String>,
    #[serde(default)]
    format: Option<String>,
    percentage: f64,
    #[serde(default)]
    cfi: Option<String>,
    #[serde(default)]
    page: Option<i64>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ReadingProgressResponse {
    id: String,
    book_id: String,
    format_id: String,
    cfi: Option<String>,
    page: Option<i64>,
    percentage: f64,
    updated_at: String,
    last_modified: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateAnnotationRequest {
    #[serde(rename = "type")]
    annotation_type: String,
    cfi_range: String,
    #[serde(default)]
    highlighted_text: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default)]
    color: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct PatchAnnotationRequest {
    #[serde(default)]
    note: Option<Option<String>>,
    #[serde(default)]
    color: Option<String>,
}

#[derive(Debug, ToSchema)]
#[allow(dead_code)]
struct UploadBookRequestDoc {
    #[schema(value_type = String, format = Binary)]
    file: String,
    #[schema(value_type = Option<String>)]
    metadata: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
struct DeleteBookResponse {
    success: bool,
}

#[derive(Debug, Deserialize)]
struct ArchiveStateRequest {
    is_archived: bool,
}

#[derive(Debug, Deserialize, Default)]
struct DownloadHistoryQuery {
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Serialize)]
struct DownloadHistoryResponseItem {
    book_id: String,
    title: String,
    format: String,
    downloaded_at: String,
}

#[derive(Debug, Serialize)]
struct MetadataLookupResponse {
    source: String,
    title: String,
    authors: Vec<String>,
    description: Option<String>,
    publisher: Option<String>,
    published_date: Option<String>,
    cover_url: Option<String>,
    isbn_13: Option<String>,
    categories: Vec<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/metadata/search",
    tag = "books",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id"),
        MetadataSearchQuery
    ),
    responses(
        (status = 200, description = "Metadata candidates", body = [crate::metadata::MetadataCandidate]),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Returns metadata candidates from Google Books and Open Library for a book.
/// Uses the provided query when present, otherwise falls back to `title + first author`.
pub(crate) async fn search_book_metadata(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Query(query): Query<MetadataSearchQuery>,
) -> Result<Json<Vec<crate::metadata::MetadataCandidate>>, AppError> {
    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let query = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            let author = book
                .authors
                .first()
                .map(|author| author.name.trim())
                .filter(|value| !value.is_empty());
            match author {
                Some(author) => format!("{} {}", book.title.trim(), author),
                None => book.title.trim().to_string(),
            }
        });

    let (google_books, open_library) = tokio::join!(
        crate::metadata::google_books::search(&query),
        crate::metadata::open_library::search(&query),
    );

    Ok(Json(interleave_metadata_candidates(
        google_books.unwrap_or_default(),
        open_library.unwrap_or_default(),
    )))
}

#[derive(Debug, Deserialize, Default)]
struct GetBookTextQuery {
    chapter: Option<u32>,
}

#[derive(Debug, Serialize)]
struct ChaptersResponse {
    book_id: String,
    format: String,
    chapters: Vec<ingest_text::Chapter>,
}

#[derive(Debug, Serialize)]
struct BookTextResponse {
    book_id: String,
    format: String,
    chapter: Option<u32>,
    text: String,
    word_count: usize,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
struct IdentifierPatch {
    id_type: String,
    value: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
struct UploadMetadata {
    title: Option<String>,
    sort_title: Option<String>,
    author: Option<String>,
    authors: Option<Vec<String>>,
    description: Option<String>,
    pubdate: Option<String>,
    language: Option<String>,
    rating: Option<i64>,
    series_id: Option<String>,
    series_index: Option<f64>,
    identifiers: Option<Vec<IdentifierPatch>>,
}

#[derive(Debug, Clone)]
struct ParsedUpload {
    file_name: String,
    bytes: Vec<u8>,
    metadata: UploadMetadata,
}

#[derive(Debug, Default)]
struct IngestMetadata {
    title: Option<String>,
    sort_title: Option<String>,
    authors: Vec<String>,
    description: Option<String>,
    pubdate: Option<String>,
    language: Option<String>,
    rating: Option<i64>,
    series_id: Option<String>,
    series_index: Option<f64>,
    identifiers: Vec<book_queries::IdentifierInput>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum UploadFormat {
    Epub,
    Pdf,
    Mobi,
}

impl UploadFormat {
    fn as_db_format(self) -> &'static str {
        match self {
            UploadFormat::Epub => "EPUB",
            UploadFormat::Pdf => "PDF",
            UploadFormat::Mobi => "MOBI",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            UploadFormat::Epub => "epub",
            UploadFormat::Pdf => "pdf",
            UploadFormat::Mobi => "mobi",
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/v1/books",
    tag = "books",
    security(("bearer_auth" = [])),
    params(
        ("q" = Option<String>, Query, description = "Search query"),
        ("author_id" = Option<String>, Query, description = "Filter by author id"),
        ("series_id" = Option<String>, Query, description = "Filter by series id"),
        ("tag" = Option<String>, Query, description = "Filter by tag"),
        ("language" = Option<String>, Query, description = "Filter by language"),
        ("format" = Option<String>, Query, description = "Filter by format"),
        ("document_type" = Option<String>, Query, description = "Filter by document type"),
        ("sort" = Option<String>, Query, description = "Sort field"),
        ("order" = Option<String>, Query, description = "Sort order"),
        ("page" = Option<i64>, Query, description = "Page number"),
        ("page_size" = Option<i64>, Query, description = "Page size")
    ),
    responses(
        (status = 200, description = "Paginated books", body = PaginatedResponse<book_queries::BookSummary>),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Returns a paginated list of books, scoped to the caller's library (admins see all libraries).
/// Supports full-text search, multi-field filtering, and configurable sort order.
pub(crate) async fn list_books(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Query(query): Query<ListBooksQuery>,
) -> Result<Json<PaginatedResponse<book_queries::BookSummary>>, AppError> {
    let library_id = accessible_library_id(&auth_user.user);
    let params = book_queries::ListBooksParams {
        q: query.q,
        library_id: library_id.map(str::to_string),
        author_id: query.author_id,
        series_id: query.series_id,
        tags: parse_tag_query(query.tag),
        language: query.language,
        publisher: None,
        format: query.format,
        document_type: query.document_type,
        rating_bucket: None,
        sort: query.sort,
        order: query.order,
        page: query.page.unwrap_or(1),
        page_size: query.page_size.unwrap_or(30),
        since: query.since,
        user_id: Some(auth_user.user.id.clone()),
        show_archived: query.show_archived,
        only_read: query.only_read,
    };

    let page = book_queries::list_books(&state.db, &params)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(Json(PaginatedResponse {
        items: page.items,
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/books/in-progress",
    tag = "books",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Books with in-progress reading state", body = [book_queries::BookSummary]),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Returns up to 20 books that the authenticated user has started but not finished.
/// Books are ordered by most recently updated reading progress.
pub(crate) async fn list_in_progress_books(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<book_queries::BookSummary>>, AppError> {
    let library_id = accessible_library_id(&auth_user.user);
    let books = book_queries::list_in_progress_books(
        &state.db,
        &auth_user.user.id,
        library_id,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(books))
}

#[utoipa::path(
    get,
    path = "/api/v1/books/{id}",
    tag = "books",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id")
    ),
    responses(
        (status = 200, description = "Book", body = crate::db::models::Book),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Fetches a single book by id with full detail (formats, authors, tags, identifiers);
/// returns 404 if the book does not exist or belongs to a different library.
pub(crate) async fn get_book(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<crate::db::models::Book>, AppError> {
    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    Ok(Json(book))
}

#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/progress",
    tag = "reader",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id")
    ),
    responses(
        (status = 200, description = "Reading progress", body = ReadingProgressResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Returns the most-recent reading progress record for the authenticated user and book;
/// returns 404 if the book doesn't exist or no progress has been saved yet.
pub(crate) async fn get_reading_progress(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<ReadingProgressResponse>, AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let row = sqlx::query(
        r#"
        SELECT id, book_id, format_id, cfi, page, percentage, updated_at, last_modified
        FROM reading_progress
        WHERE user_id = ? AND book_id = ?
        LIMIT 1
        "#,
    )
    .bind(&auth_user.user.id)
    .bind(&book_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let Some(row) = row else {
        return Err(AppError::NotFound);
    };

    Ok(Json(ReadingProgressResponse {
        id: row.get("id"),
        book_id: row.get("book_id"),
        format_id: row.get("format_id"),
        cfi: row.get("cfi"),
        page: row.get("page"),
        percentage: row.get("percentage"),
        updated_at: row.get("updated_at"),
        last_modified: row.get("last_modified"),
    }))
}

#[utoipa::path(
    put,
    path = "/api/v1/books/{id}/progress",
    tag = "reader",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id")
    ),
    request_body = ReadingProgressRequest,
    responses(
        (status = 200, description = "Reading progress", body = ReadingProgressResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Creates or updates reading progress for the caller on a book using INSERT … ON CONFLICT;
/// percentage is clamped to [0, 100] and `format_id` is resolved from either a direct id or
/// a format name string — exactly one must be supplied.
pub(crate) async fn upsert_reading_progress(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Json(payload): Json<ReadingProgressRequest>,
) -> Result<Json<ReadingProgressResponse>, AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let format_id = resolve_progress_format_id(&state.db, &book_id, &payload).await?;
    let percentage = payload.percentage.clamp(0.0, 100.0);
    let now = chrono::Utc::now().to_rfc3339();
    let id = uuid::Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO reading_progress (
            id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(user_id, book_id) DO UPDATE SET
            format_id = excluded.format_id,
            cfi = excluded.cfi,
            page = excluded.page,
            percentage = excluded.percentage,
            updated_at = excluded.updated_at,
            last_modified = excluded.last_modified
        "#,
    )
    .bind(&id)
    .bind(&auth_user.user.id)
    .bind(&book_id)
    .bind(&format_id)
    .bind(&payload.cfi)
    .bind(payload.page)
    .bind(percentage)
    .bind(&now)
    .bind(&now)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let row = sqlx::query(
        r#"
        SELECT id, book_id, format_id, cfi, page, percentage, updated_at, last_modified
        FROM reading_progress
        WHERE user_id = ? AND book_id = ?
        LIMIT 1
        "#,
    )
    .bind(&auth_user.user.id)
    .bind(&book_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(ReadingProgressResponse {
        id: row.get("id"),
        book_id: row.get("book_id"),
        format_id: row.get("format_id"),
        cfi: row.get("cfi"),
        page: row.get("page"),
        percentage: row.get("percentage"),
        updated_at: row.get("updated_at"),
        last_modified: row.get("last_modified"),
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/annotations",
    tag = "reader",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id")
    ),
    responses(
        (status = 200, description = "Book annotations", body = [annotation_queries::Annotation]),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Returns all annotations created by the authenticated user for the given book.
pub(crate) async fn list_annotations(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<Vec<annotation_queries::Annotation>>, AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let annotations = annotation_queries::list_annotations(&state.db, &auth_user.user.id, &book_id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(annotations))
}

#[utoipa::path(
    post,
    path = "/api/v1/books/{id}/annotations",
    tag = "reader",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id")
    ),
    request_body = CreateAnnotationRequest,
    responses(
        (status = 201, description = "Created annotation", body = annotation_queries::Annotation),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Creates a new annotation for the authenticated user on a book; validates type ("highlight",
/// "note", "bookmark") and color, and enforces field presence rules per annotation type.
pub(crate) async fn create_annotation(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Json(payload): Json<CreateAnnotationRequest>,
) -> Result<(StatusCode, Json<annotation_queries::Annotation>), AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let annotation_type =
        normalize_annotation_type(&payload.annotation_type).ok_or(AppError::BadRequest)?;
    let color = normalize_annotation_color(payload.color.as_deref().unwrap_or("yellow"))
        .ok_or(AppError::BadRequest)?;
    let cfi_range = payload.cfi_range.trim();
    if cfi_range.is_empty() {
        return Err(AppError::BadRequest);
    }

    let highlighted_text = normalize_optional_text(payload.highlighted_text);
    let note = normalize_optional_text(payload.note);
    validate_annotation_create_fields(
        annotation_type,
        highlighted_text.as_deref(),
        note.as_deref(),
    )?;

    let created = annotation_queries::create_annotation(
        &state.db,
        annotation_queries::NewAnnotation {
            user_id: auth_user.user.id.clone(),
            book_id,
            annotation_type: annotation_type.to_string(),
            cfi_range: cfi_range.to_string(),
            highlighted_text,
            note,
            color: color.to_string(),
        },
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok((StatusCode::CREATED, Json(created)))
}

/// Updates the note or color on an existing annotation; returns 403 if the annotation belongs
/// to a different user, 404 if it does not exist or belongs to a different book.
async fn patch_annotation(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((book_id, annotation_id)): Path<(String, String)>,
    Json(payload): Json<PatchAnnotationRequest>,
) -> Result<Json<annotation_queries::Annotation>, AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let Some(existing) = annotation_queries::get_annotation_by_id(&state.db, &annotation_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    if existing.book_id != book_id {
        return Err(AppError::NotFound);
    }

    if existing.user_id != auth_user.user.id {
        return Err(AppError::Forbidden("forbidden".into()));
    }

    let color = match payload.color {
        Some(value) => {
            let normalized = normalize_annotation_color(&value).ok_or(AppError::BadRequest)?;
            Some(normalized.to_string())
        }
        None => None,
    };

    let note_patch = payload
        .note
        .map(|value| value.and_then(|text| normalize_optional_text(Some(text))));

    let updated = annotation_queries::update_annotation(
        &state.db,
        &annotation_id,
        &auth_user.user.id,
        annotation_queries::AnnotationPatch {
            note: note_patch,
            color,
        },
    )
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::NotFound)?;

    Ok(Json(updated))
}

/// Deletes an annotation owned by the authenticated user; returns 403 if the user doesn't own it.
async fn delete_annotation(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((book_id, annotation_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let Some(existing) = annotation_queries::get_annotation_by_id(&state.db, &annotation_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    if existing.book_id != book_id {
        return Err(AppError::NotFound);
    }

    if existing.user_id != auth_user.user.id {
        return Err(AppError::Forbidden("forbidden".into()));
    }

    let deleted =
        annotation_queries::delete_annotation(&state.db, &annotation_id, &auth_user.user.id)
            .await
            .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

/// Returns all admin-defined custom column definitions for the library.
async fn list_custom_columns(
    State(state): State<AppState>,
) -> Result<Json<Vec<book_queries::CustomColumn>>, AppError> {
    let columns = book_queries::list_custom_columns(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(columns))
}

/// Creates a new custom column definition; requires admin role.
/// Returns 409 Conflict if a column with the same name or label already exists.
async fn create_custom_column(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateCustomColumnRequest>,
) -> Result<(StatusCode, Json<book_queries::CustomColumn>), AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let name = payload.name.trim();
    let label = payload.label.trim();
    let Some(column_type) = normalize_custom_column_type(&payload.column_type) else {
        return Err(AppError::BadRequest);
    };
    if name.is_empty() || label.is_empty() {
        return Err(AppError::BadRequest);
    }

    let created = book_queries::create_custom_column(
        &state.db,
        name,
        label,
        column_type,
        payload.is_multiple,
    )
    .await
    .map_err(|err| {
        if err.to_string().to_lowercase().contains("unique") {
            AppError::Conflict
        } else {
            AppError::Internal
        }
    })?;

    Ok((StatusCode::CREATED, Json(created)))
}

/// Deletes a custom column and all associated book values; requires admin role.
async fn delete_custom_column(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(column_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    if column_id.trim().is_empty() {
        return Err(AppError::BadRequest);
    }

    let deleted = book_queries::delete_custom_column(&state.db, &column_id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Returns all custom column values set for a specific book.
async fn get_book_custom_values(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<Vec<book_queries::BookCustomValue>>, AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let values = book_queries::get_book_custom_values(&state.db, &book_id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(values))
}

/// Upserts a batch of custom column values for a book; requires `can_edit` permission.
/// Returns 400 for type mismatches and 404 if a referenced column does not exist.
async fn patch_book_custom_values(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Json(payload): Json<Vec<CustomValuePatchRequest>>,
) -> Result<StatusCode, AppError> {
    ensure_can_edit(&state, &auth_user.user.id).await?;
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let mut values = Vec::with_capacity(payload.len());
    for entry in payload {
        let column_id = entry.column_id.trim().to_string();
        if column_id.is_empty() {
            return Err(AppError::BadRequest);
        }
        values.push(book_queries::BookCustomValueInput {
            column_id,
            value: entry.value,
        });
    }

    let result = book_queries::upsert_book_custom_values(&state.db, &book_id, &values).await;
    match result {
        Ok(()) => Ok(StatusCode::NO_CONTENT),
        Err(err) => {
            let message = err.to_string().to_lowercase();
            if message.contains("invalid_") {
                Err(AppError::BadRequest)
            } else if message.contains("column_not_found") {
                Err(AppError::NotFound)
            } else {
                Err(AppError::Internal)
            }
        }
    }
}

/// Merges a duplicate book into a primary book, re-pointing all relations; requires admin role.
/// The duplicate is deleted after the merge and its search index entry is removed.
async fn merge_book(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(primary_id): Path<String>,
    Json(payload): Json<MergeBookRequest>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let primary_id = primary_id.trim().to_string();
    let duplicate_id = payload.duplicate_id.trim().to_string();
    if primary_id.is_empty() || duplicate_id.is_empty() {
        return Err(AppError::BadRequest);
    }
    if primary_id == duplicate_id {
        return Err(AppError::BadRequest);
    }

    let primary_exists = book_queries::get_book_by_id(
        &state.db,
        &primary_id,
        None,
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?;
    if primary_exists.is_none() {
        return Err(AppError::NotFound);
    }

    let duplicate_exists = book_queries::get_book_by_id(
        &state.db,
        &duplicate_id,
        None,
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?;
    if duplicate_exists.is_none() {
        return Err(AppError::NotFound);
    }

    book_queries::merge_books(&state.db, &primary_id, &duplicate_id)
        .await
        .map_err(|_| AppError::Internal)?;

    if let Some(merged_book) = book_queries::get_book_by_id(
        &state.db,
        &primary_id,
        None,
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?
    {
        enqueue_semantic_index_if_enabled(&state, &merged_book.id).await;
        queue_book_index(state.search.clone(), merged_book);
    }
    queue_book_removal(state.search.clone(), duplicate_id);

    Ok(StatusCode::NO_CONTENT)
}

/// Toggles the read/unread flag for the authenticated user on a specific book.
async fn set_read(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Json(payload): Json<ReadStateRequest>,
) -> Result<StatusCode, AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    book_state_queries::set_read(&state.db, &auth_user.user.id, &book_id, payload.is_read)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Toggles the archived flag for the authenticated user on a specific book.
async fn set_archive(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Json(payload): Json<ArchiveStateRequest>,
) -> Result<StatusCode, AppError> {
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    book_state_queries::set_archived(&state.db, &auth_user.user.id, &book_id, payload.is_archived)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}

/// Maps an annotation type string to its canonical static value; returns None for unknown types.
fn normalize_annotation_type(value: &str) -> Option<&'static str> {
    match value.trim() {
        "highlight" => Some("highlight"),
        "note" => Some("note"),
        "bookmark" => Some("bookmark"),
        _ => None,
    }
}

/// Maps a color string to its canonical static value (yellow, green, blue, pink); returns None otherwise.
fn normalize_annotation_color(value: &str) -> Option<&'static str> {
    match value.trim() {
        "yellow" => Some("yellow"),
        "green" => Some("green"),
        "blue" => Some("blue"),
        "pink" => Some("pink"),
        _ => None,
    }
}

/// Trims an optional string value and returns None if the trimmed result is empty.
fn normalize_optional_text(value: Option<String>) -> Option<String> {
    value.and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

/// Enforces per-type field presence rules: highlights require text but not a note;
/// notes require both text and a note field; bookmarks must have neither.
fn validate_annotation_create_fields(
    annotation_type: &str,
    highlighted_text: Option<&str>,
    note: Option<&str>,
) -> Result<(), AppError> {
    match annotation_type {
        "highlight" => {
            if highlighted_text.is_none() || note.is_some() {
                return Err(AppError::BadRequest);
            }
        }
        "note" => {
            if highlighted_text.is_none() || note.is_none() {
                return Err(AppError::BadRequest);
            }
        }
        "bookmark" => {
            if note.is_some() {
                return Err(AppError::BadRequest);
            }
        }
        _ => return Err(AppError::BadRequest),
    }

    Ok(())
}

/// Resolves a format id from a progress request, accepting either a direct `format_id` UUID
/// or a format name string; returns 400 if neither is supplied or if the referenced format
/// does not belong to the given book.
async fn resolve_progress_format_id(
    db: &sqlx::SqlitePool,
    book_id: &str,
    payload: &ReadingProgressRequest,
) -> Result<String, AppError> {
    if let Some(format_id) = payload.format_id.as_deref() {
        let row = sqlx::query(
            r#"
            SELECT id
            FROM formats
            WHERE id = ? AND book_id = ?
            LIMIT 1
            "#,
        )
        .bind(format_id)
        .bind(book_id)
        .fetch_optional(db)
        .await
        .map_err(|_| AppError::Internal)?;

        return row.map(|row| row.get("id")).ok_or(AppError::BadRequest);
    }

    if let Some(format) = payload.format.as_deref() {
        let Some(format_file) = book_queries::find_format_file(db, book_id, format)
            .await
            .map_err(|_| AppError::Internal)?
        else {
            return Err(AppError::NotFound);
        };
        return Ok(format_file.id);
    }

    Err(AppError::BadRequest)
}

/// Loads a book by id within the given library scope, returning 404 (not 403) if
/// the book is missing or inaccessible — preventing library existence disclosure.
pub(crate) async fn load_book_or_not_found(
    db: &sqlx::SqlitePool,
    book_id: &str,
    library_id: Option<&str>,
    user_id: Option<&str>,
) -> Result<crate::db::models::Book, AppError> {
    let Some(book) = book_queries::get_book_by_id(db, book_id, library_id, user_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };
    Ok(book)
}

/// Returns a paginated history of the authenticated user's format downloads.
async fn list_download_history(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Query(query): Query<DownloadHistoryQuery>,
) -> Result<Json<PaginatedResponse<DownloadHistoryResponseItem>>, AppError> {
    let page = download_history_queries::list_download_history(
        &state.db,
        &auth_user.user.id,
        query.page.unwrap_or(1),
        query.page_size.unwrap_or(50),
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(PaginatedResponse {
        items: page
            .items
            .into_iter()
            .map(|item| DownloadHistoryResponseItem {
                book_id: item.book_id,
                title: item.title,
                format: item.format,
                downloaded_at: item.downloaded_at,
            })
            .collect(),
        total: page.total,
        page: page.page,
        page_size: page.page_size,
    }))
}

/// Returns the library id that should be used as a DB filter for the given user.
/// Admins receive `None` (no library filter — sees all), regular users receive their default library id.
pub(crate) fn accessible_library_id(user: &crate::db::models::User) -> Option<&str> {
    if user.role.name.eq_ignore_ascii_case("admin") {
        None
    } else {
        Some(user.default_library_id.as_str())
    }
}

#[utoipa::path(
    post,
    path = "/api/v1/books",
    tag = "books",
    security(("bearer_auth" = [])),
    request_body(
        content = UploadBookRequestDoc,
        content_type = "multipart/form-data"
    ),
    responses(
        (status = 201, description = "Book created", body = crate::db::models::Book),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Accepts a multipart upload (file + optional JSON metadata), detects format by magic bytes,
/// extracts embedded metadata, optionally classifies the document type via LLM, stores the file
/// under a 2-char bucket path, and emits a `book.added` webhook event on success.
/// Requires `can_upload` permission; returns 409 on duplicate ISBN.
pub(crate) async fn upload_book(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<crate::db::models::Book>), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_upload {
        return Err(AppError::Forbidden("forbidden".into()));
    }

    let _import_metrics = ImportMetricsGuard::new();

    let parsed_upload =
        parse_upload_multipart(multipart, state.config.limits.upload_max_bytes).await?;
    let detected_format =
        detect_upload_format(&parsed_upload.bytes).ok_or(AppError::Unprocessable)?;
    validate_extension_matches(&parsed_upload.file_name, detected_format)?;
    let extracted_cover_source = if detected_format == UploadFormat::Epub {
        extract_epub_cover_source(&parsed_upload.bytes)
    } else {
        None
    };

    let mut ingest = extract_metadata(
        detected_format,
        &parsed_upload.file_name,
        &parsed_upload.bytes,
    )?;
    ingest = apply_metadata_override(ingest, parsed_upload.metadata)?;

    let title = ingest
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .ok_or(AppError::Unprocessable)?;
    let author_names = if ingest.authors.is_empty() {
        vec!["Unknown Author".to_string()]
    } else {
        ingest.authors.clone()
    };
    let authors_csv = author_names.join(", ");
    let description_for_type = ingest.description.clone().unwrap_or_default();
    let document_type = if let Some(client) = state.chat_client.as_ref() {
        classify_document_type(client, &title, &authors_csv, &description_for_type)
            .await
            .as_str()
            .to_string()
    } else {
        DocumentType::Unknown.as_str().to_string()
    };

    if let Some(rating) = ingest.rating {
        if !(0..=10).contains(&rating) {
            return Err(AppError::Unprocessable);
        }
    }

    if book_queries::has_duplicate_isbn(&state.db, &ingest.identifiers, None::<&str>)
        .await
        .map_err(|_| AppError::Internal)?
    {
        return Err(AppError::Conflict);
    }

    let file_id = Uuid::new_v4().to_string();
    let bucket = &file_id[..2];
    let relative_path = format!("books/{bucket}/{file_id}.{}", detected_format.extension());

    state
        .storage
        .put(
            &relative_path,
            bytes::Bytes::from(parsed_upload.bytes.clone()),
        )
        .await
        .map_err(|_| AppError::Internal)?;

    let insert_result = book_queries::insert_uploaded_book(
        &state.db,
        book_queries::UploadBookInput {
            library_id: auth_user.user.default_library_id.clone(),
            title: title.clone(),
            sort_title: ingest.sort_title.clone().unwrap_or_else(|| title.clone()),
            description: ingest.description,
            pubdate: ingest.pubdate,
            language: ingest.language,
            rating: ingest.rating,
            document_type,
            series_id: ingest.series_id,
            series_index: ingest.series_index,
            author_names,
            identifiers: ingest.identifiers,
            format: detected_format.as_db_format().to_string(),
            format_path: relative_path.clone(),
            format_size_bytes: parsed_upload.bytes.len() as i64,
        },
    )
    .await;

    let mut book = match insert_result {
        Ok(book) => book,
        Err(_) => {
            let _ = state.storage.delete(&relative_path).await;
            return Err(AppError::Internal);
        }
    };

    if let Some(raw_cover_bytes) = extracted_cover_source {
        if let Some(cover_variants) = render_cover_variants(&raw_cover_bytes) {
            let bucket = &book.id[..2];
            let cover_relative_path = format!("covers/{bucket}/{}.jpg", book.id);
            let thumb_relative_path = format!("covers/{bucket}/{}.thumb.jpg", book.id);
            let cover_webp_relative_path = format!("covers/{bucket}/{}.webp", book.id);
            let thumb_webp_relative_path = format!("covers/{bucket}/{}.thumb.webp", book.id);
            let generated_cover_paths = [
                cover_relative_path.as_str(),
                thumb_relative_path.as_str(),
                cover_webp_relative_path.as_str(),
                thumb_webp_relative_path.as_str(),
            ];

            state
                .storage
                .put(
                    &cover_relative_path,
                    bytes::Bytes::from(cover_variants.cover_jpg),
                )
                .await
                .map_err(|_| AppError::Internal)?;
            state
                .storage
                .put(
                    &thumb_relative_path,
                    bytes::Bytes::from(cover_variants.thumb_jpg),
                )
                .await
                .map_err(|_| AppError::Internal)?;
            state
                .storage
                .put(
                    &cover_webp_relative_path,
                    bytes::Bytes::from(cover_variants.cover_webp),
                )
                .await
                .map_err(|_| AppError::Internal)?;
            state
                .storage
                .put(
                    &thumb_webp_relative_path,
                    bytes::Bytes::from(cover_variants.thumb_webp),
                )
                .await
                .map_err(|_| AppError::Internal)?;

            if let Err(err) =
                book_queries::set_book_cover_path(&state.db, &book.id, &cover_relative_path).await
            {
                delete_storage_paths(&state, &generated_cover_paths).await;
                tracing::error!("failed to persist cover path for book {}: {err:#}", book.id);
                return Err(AppError::Internal);
            }

            if let Some(updated_book) = book_queries::get_book_by_id(
                &state.db,
                &book.id,
                accessible_library_id(&auth_user.user),
                Some(auth_user.user.id.as_str()),
            )
            .await
            .map_err(|_| AppError::Internal)?
            {
                book = updated_book;
            }
        }
    }

    enqueue_semantic_index_if_enabled(&state, &book.id).await;
    queue_book_index(state.search.clone(), book.clone());
    queue_book_chunk_generation(state.clone(), book.clone());
    let _ = webhook_engine::enqueue_event(
        &state.db,
        "book.added",
        serde_json::json!({
            "event": "book.added",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "library_name": state.config.app.library_name.clone(),
            "data": {
                "id": book.id.clone(),
                "title": book.title.clone(),
                "authors": book
                    .authors
                    .iter()
                    .map(|author| author.name.clone())
                    .collect::<Vec<_>>(),
                "formats": book
                    .formats
                    .iter()
                    .map(|format| format.format.clone())
                    .collect::<Vec<_>>(),
                "cover_url": book.cover_url.clone(),
            }
        }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(book)))
}

#[utoipa::path(
    patch,
    path = "/api/v1/books/{id}",
    tag = "books",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id")
    ),
    request_body = PatchBookRequest,
    responses(
        (status = 200, description = "Updated book", body = crate::db::models::Book),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Updates book metadata fields; requires `can_edit` permission.
/// Returns 409 on duplicate ISBN conflict and 422 if rating is out of [0, 10].
pub(crate) async fn patch_book(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Json(payload): Json<PatchBookRequest>,
) -> Result<Json<crate::db::models::Book>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_edit {
        return Err(AppError::Forbidden("forbidden".into()));
    }

    let patch = book_queries::PatchBookInput {
        title: payload.title.map(trim_owned),
        sort_title: payload.sort_title.map(trim_owned),
        description: payload.description.map(|opt| opt.map(trim_owned)),
        pubdate: payload.pubdate.map(|opt| opt.map(trim_owned)),
        language: payload.language.map(|opt| opt.map(trim_owned)),
        rating: payload.rating,
        series_id: payload.series_id.map(|opt| opt.map(trim_owned)),
        series_index: payload.series_index,
        authors: payload
            .authors
            .map(|authors| authors.into_iter().map(trim_owned).collect()),
        identifiers: payload.identifiers.map(|ids| {
            ids.into_iter()
                .map(|id| book_queries::IdentifierInput {
                    id_type: trim_owned(id.id_type),
                    value: trim_owned(id.value),
                })
                .collect()
        }),
    };

    let result = book_queries::patch_book_with_audit(
        &state.db,
        &book_id,
        &auth_user.user.id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
        patch,
    )
    .await;
    match result {
        Ok(Some(book)) => {
            enqueue_semantic_index_if_enabled(&state, &book.id).await;
            queue_book_index(state.search.clone(), book.clone());
            Ok(Json(book))
        }
        Ok(None) => Err(AppError::NotFound),
        Err(err) => {
            if format!("{err:#}").contains("duplicate_isbn") {
                Err(AppError::Conflict)
            } else if format!("{err:#}").contains("rating") {
                Err(AppError::Unprocessable)
            } else {
                Err(AppError::Internal)
            }
        }
    }
}

#[utoipa::path(
    delete,
    path = "/api/v1/books/{id}",
    tag = "books",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id")
    ),
    responses(
        (status = 200, description = "Delete result", body = DeleteBookResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Deletes a book and all its storage files (formats + cover variants); requires admin role.
/// Emits a `book.deleted` webhook event. File deletion failures are surfaced as 500 rather
/// than silently ignored so operators can investigate orphaned storage objects.
pub(crate) async fn delete_book(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.is_admin() {
        return Err(AppError::Forbidden("forbidden".into()));
    }

    let book_snapshot = book_queries::get_book_by_id(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::NotFound)?;

    let Some(paths) = book_queries::delete_book_and_collect_paths(
        &state.db,
        &book_id,
        &auth_user.user.id,
        accessible_library_id(&auth_user.user),
    )
    .await
    .map_err(|err| {
        tracing::error!(book_id = %book_id, error = %err, "failed to delete book from database");
        AppError::Internal
    })?
    else {
        return Err(AppError::NotFound);
    };

    queue_book_removal(state.search.clone(), book_id.clone());

    for path in paths {
        if let Err(err) = state.storage.delete(&path).await {
            tracing::error!(book_id = %book_id, path = %path, error = %err, "failed to delete book file");
            return Err(AppError::Internal);
        }
    }

    let _ = webhook_engine::enqueue_event(
        &state.db,
        "book.deleted",
        serde_json::json!({
            "event": "book.deleted",
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "library_name": state.config.app.library_name.clone(),
            "data": {
                "id": book_snapshot.id,
                "title": book_snapshot.title,
            }
        }),
    )
    .await;

    Ok(Json(
        serde_json::to_value(DeleteBookResponse { success: true })
            .map_err(|_| AppError::Internal)?,
    ))
}

#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/formats/{format}/download",
    tag = "books",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id"),
        ("format" = String, Path, description = "Format name")
    ),
    responses(
        (status = 200, description = "File download", content_type = "application/octet-stream", body = String),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Serves a book format file as an attachment download; requires `can_download` permission.
/// Supports HTTP range requests and records the download in the history table asynchronously.
pub(crate) async fn download_format(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((book_id, format)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<axum::response::Response, AppError> {
    ensure_download_permission(&state, &auth_user.user.id).await?;
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let format_file = book_queries::find_format_file(&state.db, &book_id, &format)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let file_extension = validated_download_format_extension(&format_file.format)?;
    let download_content_type = mime_guess::from_ext(&file_extension)
        .first_or_octet_stream()
        .essence_str()
        .to_string();

    let file_name = format!("{}.{}", book_id, file_extension);
    let disposition = format!("attachment; filename=\"{file_name}\"");
    let response = serve_storage_file(
        &state,
        request,
        &format_file.path,
        Some(download_content_type.as_str()),
        Some(disposition.as_str()),
    )
    .await?;

    let db = state.db.clone();
    let history_user_id = auth_user.user.id.clone();
    let history_book_id = book_id.clone();
    let history_format = format_file.format.clone();
    tokio::spawn(async move {
        if let Err(err) = download_history_queries::insert_download_history(
            &db,
            &history_user_id,
            &history_book_id,
            &history_format,
        )
        .await
        {
            tracing::warn!(
                book_id = %history_book_id,
                user_id = %history_user_id,
                format = %history_format,
                error = %err,
                "failed to persist download history"
            );
        }
    });

    Ok(response)
}

/// Serves a book format for inline streaming (e.g. audio/ebook reader); requires `can_download`.
/// Selects an appropriate MIME type for audio formats and supports HTTP range requests.
async fn stream_format(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((book_id, format)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<axum::response::Response, AppError> {
    ensure_download_permission(&state, &auth_user.user.id).await?;
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let format_file = book_queries::find_format_file(&state.db, &book_id, &format)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let file_extension = validated_download_format_extension(&format_file.format)?;
    let content_type: Cow<'static, str> = match file_extension.as_str() {
        "mp3" => Cow::Borrowed("audio/mpeg"),
        "m4b" | "m4a" => Cow::Borrowed("audio/mp4"),
        "ogg" | "opus" => Cow::Borrowed("audio/ogg"),
        "flac" => Cow::Borrowed("audio/flac"),
        _ => {
            let guessed_mime = mime_guess::from_ext(&file_extension).first_or_octet_stream();
            Cow::Owned(guessed_mime.essence_str().to_string())
        }
    };

    serve_storage_file(
        &state,
        request,
        &format_file.path,
        Some(content_type.as_ref()),
        None,
    )
    .await
}

/// Converts a MOBI/AZW3 format to a minimal EPUB on the fly and returns it inline;
/// requires `can_download` permission. Only accepts "mobi" or "azw3" as the format parameter.
async fn mobi_to_epub(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((book_id, format)): Path<(String, String)>,
) -> Result<axum::response::Response, AppError> {
    let normalized_format = format.trim().to_ascii_lowercase();
    if !matches!(normalized_format.as_str(), "mobi" | "azw3") {
        return Err(AppError::BadRequest);
    }

    ensure_download_permission(&state, &auth_user.user.id).await?;
    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let format_file = book_queries::find_format_file(&state.db, &book_id, &normalized_format)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let bytes = state
        .storage
        .get_bytes(&format_file.path)
        .await
        .map_err(map_storage_read_error)?;
    let mobi_book = mobi::Mobi::new(bytes.to_vec()).map_err(|_| AppError::Internal)?;
    let epub_bytes = build_epub_from_mobi(&mobi_book, &book_id)?;

    let source_title = mobi_book.title();
    let title_for_filename = if source_title.trim().is_empty() {
        "book".to_string()
    } else {
        source_title
    };
    let safe_filename = sanitize_file_name_for_header(&title_for_filename);
    let disposition = HeaderValue::from_str(&format!("inline; filename=\"{safe_filename}.epub\""))
        .map_err(|_| AppError::Internal)?;

    let mut response = axum::response::Response::new(Body::from(epub_bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/epub+zip"),
    );
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, disposition);
    Ok(response)
}

#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/cover",
    tag = "books",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id")
    ),
    responses(
        (status = 200, description = "Cover image", content_type = "image/*", body = String),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Serves the cover image for a book; requires `can_download` permission.
/// Automatically serves the WebP variant if the client sends `Accept: image/webp`
/// and the WebP file exists, falling back to JPEG otherwise.
pub(crate) async fn get_cover(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    request: Request<Body>,
) -> Result<axum::response::Response, AppError> {
    ensure_download_permission(&state, &auth_user.user.id).await?;

    let _ = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let cover_path = book_queries::find_book_cover_path(&state.db, &book_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let wants_webp = request
        .headers()
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("image/webp"))
        .unwrap_or(false);

    let mut selected_cover_path = cover_path.clone();
    if wants_webp {
        if let Some(webp_cover_path) = cover_path
            .strip_suffix(".jpg")
            .map(|prefix| format!("{prefix}.webp"))
        {
            if storage_path_exists(&state, &webp_cover_path).await {
                selected_cover_path = webp_cover_path;
            }
        }
    }

    let cover_content_type = mime_guess::from_path(&selected_cover_path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();

    serve_storage_file(
        &state,
        request,
        &selected_cover_path,
        Some(cover_content_type.as_str()),
        None,
    )
    .await
}

/// Returns the chapter table of contents for the preferred extractable format (EPUB > PDF > MOBI);
/// requires `can_download` permission.
async fn get_chapters(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<ChaptersResponse>, AppError> {
    ensure_download_permission(&state, &auth_user.user.id).await?;

    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let format = preferred_extractable_format(&book).ok_or(AppError::NoExtractableFormat)?;

    let format_file = book_queries::find_format_file(&state.db, &book.id, format)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NoExtractableFormat)?;
    let extractable_path =
        ingest_text::resolve_or_download_path(&*state.storage, &format_file.path)
            .await
            .map_err(map_storage_read_error)?;

    let chapters = list_extractable_chapters(extractable_path.path(), format);
    Ok(Json(ChaptersResponse {
        book_id,
        format: format.to_string(),
        chapters,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/books/{id}/chunks",
    tag = "books",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Book id"),
        ("size" = Option<usize>, Query, description = "Target chunk size in tokens"),
        ("overlap" = Option<usize>, Query, description = "Token overlap between chunks"),
        ("domain" = Option<String>, Query, description = "Chunking domain"),
        ("type" = Option<String>, Query, description = "Filter chunk type")
    ),
    responses(
        (status = 200, description = "Book chunks", body = ChunksResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 500, description = "Internal error", body = crate::error::AppErrorResponse)
    )
)]
/// Returns structured text chunks for the RAG surface; requires `can_download` permission.
/// If no chunks exist yet, generates and stores them on demand using the supplied (or default)
/// chunk configuration. Supports filtering by chunk type.
pub(crate) async fn get_chunks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Query(query): Query<GetChunksQuery>,
) -> Result<Json<ChunksResponse>, AppError> {
    ensure_download_permission(&state, &auth_user.user.id).await?;

    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    let target_size = query.size.unwrap_or(600).clamp(1, 2_000);
    let overlap = query
        .overlap
        .unwrap_or(100)
        .min(target_size.saturating_sub(1));
    let domain = parse_chunk_domain(query.domain.as_deref())?;
    let chunk_type = match query.chunk_type.as_deref() {
        Some(value) if !value.trim().is_empty() => Some(
            value
                .parse::<ChunkType>()
                .map_err(|_| AppError::BadRequest)?,
        ),
        _ => None,
    };

    let existing_count = chunk_queries::count_book_chunks(&state.db, &book.id)
        .await
        .map_err(|_| AppError::Internal)?;
    if existing_count == 0 {
        let Some(_) = preferred_extractable_format(&book) else {
            return Err(AppError::NoExtractableFormat);
        };

        let config = crate::ingest::chunker::ChunkConfig {
            target_size,
            overlap,
            domain,
        };
        ingest_text::generate_and_store_book_chunks(&state, &book, &config)
            .await
            .map_err(|_| AppError::Internal)?;
    }

    let chunks = chunk_queries::list_book_chunks(&state.db, &book.id, chunk_type)
        .await
        .map_err(|_| AppError::Internal)?;
    let payload = ChunksResponse {
        book_id: book.id,
        chunk_count: chunks.len(),
        chunks: chunks
            .into_iter()
            .map(|chunk| ChunkResponse {
                id: chunk.id,
                chunk_index: chunk.chunk_index as usize,
                chapter_index: chunk.chapter_index as usize,
                heading_path: chunk.heading_path,
                chunk_type: chunk.chunk_type,
                text: chunk.text,
                word_count: chunk.word_count as usize,
                has_image: chunk.has_image,
            })
            .collect(),
    };

    Ok(Json(payload))
}

/// Extracts and returns plain text from the book; requires `can_download` permission.
/// Supports optional chapter-index filtering; returns 400 if the chapter index is out of range.
async fn get_text(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Query(query): Query<GetBookTextQuery>,
) -> Result<Json<BookTextResponse>, AppError> {
    ensure_download_permission(&state, &auth_user.user.id).await?;

    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let format = preferred_extractable_format(&book).ok_or(AppError::NoExtractableFormat)?;

    let format_file = book_queries::find_format_file(&state.db, &book.id, format)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NoExtractableFormat)?;
    let extractable_path =
        ingest_text::resolve_or_download_path(&*state.storage, &format_file.path)
            .await
            .map_err(map_storage_read_error)?;

    let chapters = list_extractable_chapters(extractable_path.path(), format);
    if let Some(chapter) = query.chapter {
        if chapter as usize >= chapters.len() {
            tracing::warn!(
                book_id = %book.id,
                chapter = chapter,
                chapter_count = chapters.len(),
                "chapter index out of range"
            );
            return Err(AppError::BadRequest);
        }
    }

    let text = ingest_text::extract_text(extractable_path.path(), format, query.chapter)
        .unwrap_or_default();
    let word_count = text.split_whitespace().count();

    Ok(Json(BookTextResponse {
        book_id,
        format: format.to_string(),
        chapter: query.chapter,
        text,
        word_count,
    }))
}

/// Sends a book format to an email address via SMTP; returns 503 if SMTP is not configured.
async fn send_book(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Json(payload): Json<SendBookRequest>,
) -> Result<StatusCode, AppError> {
    let format_name = payload.format.trim();
    if format_name.is_empty() || payload.to.trim().is_empty() {
        return Err(AppError::BadRequest);
    }

    let Some(book) = book_queries::get_book_by_id(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let Some(format_file) = book_queries::find_format_file(&state.db, &book.id, format_name)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let bytes = state
        .storage
        .get_bytes(&format_file.path)
        .await
        .map_err(map_storage_read_error)?;
    let email_settings = crate::db::queries::email_settings::get_email_settings(&state.db)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::ServiceUnavailable)?;
    if email_settings.smtp_host.trim().is_empty() || email_settings.from_address.trim().is_empty() {
        return Err(AppError::ServiceUnavailable);
    }

    send_book_email(
        &email_settings,
        &book,
        &payload.to,
        &format_file.format,
        &bytes,
    )
    .await?;

    Ok(StatusCode::NO_CONTENT)
}

/// Fetches enriched metadata from Open Library or Google Books using the book's ISBN or title/authors.
/// Defaults to Open Library and falls back to Google Books if Open Library is unavailable.
async fn metadata_lookup(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Query(query): Query<MetadataLookupQuery>,
) -> Result<Json<MetadataLookupResponse>, AppError> {
    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let identifiers = book_queries::get_book_identifiers(&state.db, &book_id)
        .await
        .map_err(|_| AppError::Internal)?;
    let title = book.title.clone();
    let authors = book
        .authors
        .iter()
        .map(|author| author.name.clone())
        .collect::<Vec<_>>();

    let requested_source = query
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    let result = match requested_source.as_deref().unwrap_or("openlibrary") {
        "googlebooks" => {
            lookup_google_books(&state, identifiers.as_slice(), &title, &authors).await?
        }
        "openlibrary" => {
            match lookup_openlibrary(&state, identifiers.as_slice(), &title, &authors).await {
                Ok(result) => result,
                Err(AppError::ServiceUnavailable) => {
                    lookup_google_books(&state, identifiers.as_slice(), &title, &authors).await?
                }
                Err(err) => return Err(err),
            }
        }
        _ => return Err(AppError::BadRequest),
    };

    Ok(Json(result))
}

fn interleave_metadata_candidates(
    google_books: Vec<crate::metadata::MetadataCandidate>,
    open_library: Vec<crate::metadata::MetadataCandidate>,
) -> Vec<crate::metadata::MetadataCandidate> {
    let mut merged = Vec::with_capacity((google_books.len() + open_library.len()).min(20));
    let mut google_iter = google_books.into_iter();
    let mut open_library_iter = open_library.into_iter();

    loop {
        let mut advanced = false;

        if merged.len() >= 20 {
            break;
        }

        if let Some(candidate) = google_iter.next() {
            merged.push(candidate);
            advanced = true;
            if merged.len() >= 20 {
                break;
            }
        }

        if let Some(candidate) = open_library_iter.next() {
            merged.push(candidate);
            advanced = true;
            if merged.len() >= 20 {
                break;
            }
        }

        if !advanced {
            break;
        }
    }

    merged
}

/// Returns the page index for a CBZ/CBR comic archive; requires `can_download` permission.
async fn get_comic_pages(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<ComicPagesResponse>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_download {
        return Err(AppError::Forbidden("forbidden".into()));
    }

    let comic =
        load_comic_archive(&state, &book_id, accessible_library_id(&auth_user.user)).await?;
    let pages = comic
        .entries
        .iter()
        .enumerate()
        .map(|(index, _)| ComicPageEntry {
            index,
            url: format!("/api/v1/books/{book_id}/comic/page/{index}"),
        })
        .collect::<Vec<_>>();

    Ok(Json(ComicPagesResponse {
        total_pages: pages.len(),
        pages,
    }))
}

/// Serves a single page image from a comic archive by zero-based index;
/// requires `can_download` permission and returns 404 if the index is out of bounds.
async fn get_comic_page(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((book_id, index)): Path<(String, usize)>,
) -> Result<axum::response::Response, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_download {
        return Err(AppError::Forbidden("forbidden".into()));
    }

    let comic =
        load_comic_archive(&state, &book_id, accessible_library_id(&auth_user.user)).await?;
    let Some(entry) = comic.entries.get(index) else {
        return Err(AppError::NotFound);
    };

    let body = Body::from(entry.bytes.clone());
    let mut response = axum::response::Response::new(body);
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(entry.content_type),
    );
    Ok(response)
}

/// Applies batch metadata edits (tags, series, rating, language, publisher) to a list of books;
/// requires admin role. Tag edits support append/overwrite/remove modes; other fields are overwrite-only.
async fn bulk_edit_books(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<BulkEditRequest>,
) -> Result<Json<BulkEditResponse>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.is_admin() {
        return Err(AppError::Forbidden("forbidden".into()));
    }
    if payload.book_ids.is_empty() {
        return Err(AppError::BadRequest);
    }

    let mut input = book_queries::BulkUpdateBooksInput {
        book_ids: payload.book_ids,
        ..Default::default()
    };

    if let Some(tags) = payload.fields.tags {
        let mode = match tags.mode.to_lowercase().as_str() {
            "append" => book_queries::BulkTagMode::Append,
            "overwrite" => book_queries::BulkTagMode::Overwrite,
            "remove" => book_queries::BulkTagMode::Remove,
            _ => return Err(AppError::BadRequest),
        };
        input.tags = Some(book_queries::BulkTagUpdateInput {
            mode,
            values: tags.values,
        });
    }

    if let Some(series) = payload.fields.series {
        if !series.mode.eq_ignore_ascii_case("overwrite") {
            return Err(AppError::BadRequest);
        }
        input.series = Some(series.value);
    }

    if let Some(rating) = payload.fields.rating {
        if !rating.mode.eq_ignore_ascii_case("overwrite") {
            return Err(AppError::BadRequest);
        }
        input.rating = Some(rating.value);
    }

    if let Some(language) = payload.fields.language {
        if !language.mode.eq_ignore_ascii_case("overwrite") {
            return Err(AppError::BadRequest);
        }
        input.language = Some(language.value);
    }

    if let Some(publisher) = payload.fields.publisher {
        if !publisher.mode.eq_ignore_ascii_case("overwrite") {
            return Err(AppError::BadRequest);
        }
        input.publisher = Some(publisher.value);
    }

    let result = book_queries::bulk_update_books(&state.db, input)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(Json(BulkEditResponse {
        updated: result.updated,
        errors: result.errors,
    }))
}

#[derive(Debug, Deserialize)]
struct OpenLibraryBooksResponse {
    #[serde(flatten)]
    books: std::collections::HashMap<String, OpenLibraryBookRecord>,
}

#[derive(Debug, Deserialize)]
struct OpenLibraryBookRecord {
    title: Option<String>,
    authors: Option<Vec<OpenLibraryAuthorRecord>>,
    publishers: Option<Vec<OpenLibraryPublisherRecord>>,
    publish_date: Option<String>,
    cover: Option<OpenLibraryCoverRecord>,
    identifiers: Option<OpenLibraryIdentifiers>,
    subjects: Option<Vec<OpenLibrarySubjectRecord>>,
    description: Option<OpenLibraryDescription>,
}

#[derive(Debug, Deserialize)]
struct OpenLibraryAuthorRecord {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OpenLibraryPublisherRecord {
    name: String,
}

#[derive(Debug, Deserialize)]
struct OpenLibraryCoverRecord {
    large: Option<String>,
    medium: Option<String>,
    small: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenLibraryIdentifiers {
    isbn_10: Option<Vec<String>>,
    isbn_13: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct OpenLibrarySubjectRecord {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenLibraryDescription {
    Text(String),
    Object { value: String },
}

#[derive(Debug, Deserialize)]
struct OpenLibrarySearchResponse {
    docs: Vec<OpenLibrarySearchDoc>,
}

#[derive(Debug, Deserialize)]
struct OpenLibrarySearchDoc {
    title: Option<String>,
    author_name: Option<Vec<String>>,
    publisher: Option<Vec<String>>,
    publish_date: Option<Vec<String>>,
    cover_i: Option<i64>,
    isbn: Option<Vec<String>>,
    subject: Option<Vec<String>>,
    first_sentence: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct GoogleBooksResponse {
    items: Option<Vec<GoogleBooksItem>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleBooksItem {
    volume_info: GoogleVolumeInfo,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleVolumeInfo {
    title: Option<String>,
    authors: Option<Vec<String>>,
    description: Option<String>,
    publisher: Option<String>,
    published_date: Option<String>,
    categories: Option<Vec<String>>,
    image_links: Option<GoogleImageLinks>,
    industry_identifiers: Option<Vec<GoogleIdentifier>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GoogleImageLinks {
    thumbnail: Option<String>,
    small_thumbnail: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct GoogleIdentifier {
    #[serde(rename = "type")]
    kind: String,
    identifier: String,
}

/// Fetches metadata from the Open Library Books API using ISBN (preferred) or title/author search;
/// uses a 5-second HTTP timeout and maps non-200 responses to `ServiceUnavailable`.
async fn lookup_openlibrary(
    state: &AppState,
    identifiers: &[crate::db::models::Identifier],
    title: &str,
    authors: &[String],
) -> Result<MetadataLookupResponse, AppError> {
    let base_url = state
        .config
        .metadata
        .openlibrary_base_url
        .trim()
        .trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| AppError::Internal)?;

    if let Some(isbn) = extract_isbn(identifiers) {
        let url = format!("{base_url}/api/books?bibkeys=ISBN:{isbn}&format=json&jscmd=data");
        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|_| AppError::ServiceUnavailable)?;
        if response.status() == StatusCode::NOT_FOUND {
            return Err(AppError::NotFound);
        }
        let response = response
            .error_for_status()
            .map_err(|_| AppError::ServiceUnavailable)?;
        let data: OpenLibraryBooksResponse = response
            .json()
            .await
            .map_err(|_| AppError::ServiceUnavailable)?;
        let Some((_, record)) = data.books.into_iter().next() else {
            return Err(AppError::NotFound);
        };
        return Ok(render_openlibrary_record(record));
    }

    let search = build_openlibrary_search_query(title, authors);
    let url = format!("{base_url}/search.json?{}", search);
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|_| AppError::ServiceUnavailable)?
        .error_for_status()
        .map_err(|_| AppError::ServiceUnavailable)?;
    let data: OpenLibrarySearchResponse = response
        .json()
        .await
        .map_err(|_| AppError::ServiceUnavailable)?;
    let Some(doc) = data.docs.into_iter().next() else {
        return Err(AppError::NotFound);
    };
    Ok(render_openlibrary_search_doc(doc))
}

/// Fetches metadata from the Google Books Volumes API using ISBN (preferred) or title/author query.
async fn lookup_google_books(
    state: &AppState,
    identifiers: &[crate::db::models::Identifier],
    title: &str,
    authors: &[String],
) -> Result<MetadataLookupResponse, AppError> {
    let base_url = state
        .config
        .metadata
        .googlebooks_base_url
        .trim()
        .trim_end_matches('/');
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| AppError::Internal)?;

    let query = if let Some(isbn) = extract_isbn(identifiers) {
        format!("isbn:{isbn}")
    } else {
        build_google_books_query(title, authors)
    };

    let url = format!(
        "{base_url}/books/v1/volumes?q={}",
        urlencoding::encode(&query)
    );
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|_| AppError::ServiceUnavailable)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound);
    }
    let response = response
        .error_for_status()
        .map_err(|_| AppError::ServiceUnavailable)?;
    let data: GoogleBooksResponse = response
        .json()
        .await
        .map_err(|_| AppError::ServiceUnavailable)?;
    let Some(item) = data.items.and_then(|mut items| items.pop()) else {
        return Err(AppError::NotFound);
    };
    Ok(render_google_books_item(item))
}

fn render_openlibrary_record(record: OpenLibraryBookRecord) -> MetadataLookupResponse {
    let authors = record
        .authors
        .unwrap_or_default()
        .into_iter()
        .map(|author| author.name)
        .collect::<Vec<_>>();
    let publisher = record
        .publishers
        .and_then(|values| values.into_iter().next())
        .map(|publisher| publisher.name);
    let cover_url = record
        .cover
        .and_then(|cover| cover.large.or(cover.medium).or(cover.small));
    let isbn_13 = record.identifiers.as_ref().and_then(|ids| {
        ids.isbn_13
            .as_ref()
            .and_then(|values| values.last())
            .cloned()
            .or_else(|| {
                ids.isbn_10
                    .as_ref()
                    .and_then(|values| values.last())
                    .cloned()
            })
    });
    let categories = record
        .subjects
        .unwrap_or_default()
        .into_iter()
        .map(|subject| subject.name)
        .collect::<Vec<_>>();
    let description = record.description.map(|description| match description {
        OpenLibraryDescription::Text(text) => text,
        OpenLibraryDescription::Object { value } => value,
    });

    MetadataLookupResponse {
        source: "openlibrary".to_string(),
        title: record.title.unwrap_or_default(),
        authors,
        description,
        publisher,
        published_date: record.publish_date,
        cover_url,
        isbn_13,
        categories,
    }
}

fn render_openlibrary_search_doc(doc: OpenLibrarySearchDoc) -> MetadataLookupResponse {
    let cover_url = doc
        .cover_i
        .map(|cover_id| format!("https://covers.openlibrary.org/b/id/{cover_id}-L.jpg"));
    let description = doc.first_sentence.and_then(|mut values| values.pop());
    MetadataLookupResponse {
        source: "openlibrary".to_string(),
        title: doc.title.unwrap_or_default(),
        authors: doc.author_name.unwrap_or_default(),
        description,
        publisher: doc.publisher.and_then(|mut values| values.pop()),
        published_date: doc.publish_date.and_then(|mut values| values.pop()),
        cover_url,
        isbn_13: doc.isbn.and_then(|mut values| values.pop()),
        categories: doc.subject.unwrap_or_default(),
    }
}

fn render_google_books_item(item: GoogleBooksItem) -> MetadataLookupResponse {
    let volume = item.volume_info;
    let identifiers = volume.industry_identifiers.as_ref();
    let isbn_13 = identifiers
        .and_then(|values| {
            values
                .iter()
                .find(|identifier| identifier.kind == "ISBN_13")
                .map(|identifier| identifier.identifier.clone())
        })
        .or_else(|| {
            identifiers.and_then(|values| {
                values
                    .iter()
                    .find(|identifier| identifier.kind == "ISBN_10")
                    .map(|identifier| identifier.identifier.clone())
            })
        });
    MetadataLookupResponse {
        source: "googlebooks".to_string(),
        title: volume.title.unwrap_or_default(),
        authors: volume.authors.unwrap_or_default(),
        description: volume.description,
        publisher: volume.publisher,
        published_date: volume.published_date,
        cover_url: volume
            .image_links
            .and_then(|links| links.thumbnail.or(links.small_thumbnail)),
        isbn_13,
        categories: volume.categories.unwrap_or_default(),
    }
}

/// Finds the first ISBN identifier among a book's identifiers and normalizes it to alphanumeric.
fn extract_isbn(identifiers: &[crate::db::models::Identifier]) -> Option<String> {
    identifiers.iter().find_map(|identifier| {
        let id_type = identifier.id_type.trim().to_lowercase();
        if !id_type.contains("isbn") {
            return None;
        }
        normalize_isbn_candidate(&identifier.value)
    })
}

fn normalize_isbn_candidate(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect::<String>();
    if normalized.len() == 10 || normalized.len() == 13 {
        Some(normalized)
    } else {
        None
    }
}

fn build_openlibrary_search_query(title: &str, authors: &[String]) -> String {
    let mut query = format!("title={}", urlencoding::encode(title));
    if let Some(author) = authors.iter().find(|author| !author.trim().is_empty()) {
        query.push_str("&author=");
        query.push_str(&urlencoding::encode(author));
    }
    query.push_str("&limit=1");
    query
}

fn build_google_books_query(title: &str, authors: &[String]) -> String {
    let mut query = format!("intitle:{}", title);
    if let Some(author) = authors.iter().find(|author| !author.trim().is_empty()) {
        query.push_str("+inauthor:");
        query.push_str(author);
    }
    query
}

/// Builds and sends an email with the book file attached using the configured SMTP transport.
async fn send_book_email(
    settings: &crate::db::queries::email_settings::EmailSettings,
    book: &crate::db::models::Book,
    to: &str,
    format: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    let message = build_book_email_message(settings, book, to, format, bytes)?;
    let transport = build_smtp_transport(settings)?;
    send_message_via_transport(&transport, message).await
}

/// Constructs a `lettre` multipart email message with the book file attached;
/// exposed as `pub` so integration tests can inspect the message without an SMTP server.
pub fn build_book_email_message(
    settings: &crate::db::queries::email_settings::EmailSettings,
    book: &crate::db::models::Book,
    to: &str,
    format: &str,
    bytes: &[u8],
) -> Result<Message, AppError> {
    let from: Mailbox = settings
        .from_address
        .parse()
        .map_err(|_| AppError::BadRequest)?;
    let to: Mailbox = to.parse().map_err(|_| AppError::BadRequest)?;
    let attachment_name = format!("{}.{}", book.id, format.trim().to_lowercase());
    let mime = mime_guess::from_path(&attachment_name).first_or_octet_stream();
    let attachment = Attachment::new(attachment_name).body(
        bytes.to_vec(),
        ContentType::parse(mime.as_ref()).map_err(|_| AppError::Internal)?,
    );

    Message::builder()
        .from(from)
        .to(to)
        .subject(format!("{} ({})", book.title, format.trim().to_uppercase()))
        .multipart(
            MultiPart::mixed()
                .singlepart(SinglePart::plain(format!(
                    "Attached is {} in {} format.",
                    book.title,
                    format.trim().to_uppercase()
                )))
                .singlepart(attachment),
        )
        .map_err(|_| AppError::Internal)
}

/// Builds an async SMTP transport from email settings; uses STARTTLS when `use_tls` is set,
/// and adds SMTP credentials only when a non-empty username is configured.
fn build_smtp_transport(
    settings: &crate::db::queries::email_settings::EmailSettings,
) -> Result<AsyncSmtpTransport<Tokio1Executor>, AppError> {
    let port = u16::try_from(settings.smtp_port).map_err(|_| AppError::ServiceUnavailable)?;
    let mut builder = if settings.use_tls {
        AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&settings.smtp_host)
            .map_err(|_| AppError::ServiceUnavailable)?
    } else {
        AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&settings.smtp_host)
    };
    builder = builder.port(port);

    if !settings.smtp_user.trim().is_empty() {
        builder = builder.credentials(Credentials::new(
            settings.smtp_user.clone(),
            settings.smtp_password.clone(),
        ));
    }

    Ok(builder.build())
}

/// Sends a pre-built `lettre` message via any async transport; exposed as `pub` to allow
/// injection of a test transport in integration tests.
pub async fn send_message_via_transport<T>(transport: &T, message: Message) -> Result<(), AppError>
where
    T: AsyncTransport + Sync,
    T::Error: std::fmt::Debug,
{
    transport
        .send(message)
        .await
        .map_err(|_| AppError::ServiceUnavailable)?;
    Ok(())
}

struct ComicArchive {
    entries: Vec<ComicArchiveEntry>,
}

struct ComicArchiveEntry {
    filename: String,
    bytes: Vec<u8>,
    content_type: &'static str,
}

/// Loads a CBZ or CBR comic archive for a book, extracting sorted image pages into memory.
async fn load_comic_archive(
    state: &AppState,
    book_id: &str,
    library_id: Option<&str>,
) -> Result<ComicArchive, AppError> {
    let Some(book) = book_queries::get_book_by_id(&state.db, book_id, library_id, None)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let format = book
        .formats
        .iter()
        .find(|format| format.format.eq_ignore_ascii_case("CBZ"))
        .or_else(|| {
            book.formats
                .iter()
                .find(|format| format.format.eq_ignore_ascii_case("CBR"))
        })
        .cloned()
        .ok_or(AppError::NoExtractableFormat)?;

    let format_file = book_queries::find_format_file(&state.db, book_id, &format.format)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NoExtractableFormat)?;

    let extractable_path =
        ingest_text::resolve_or_download_path(&*state.storage, &format_file.path)
            .await
            .map_err(map_storage_read_error)?;
    let extension = FsPath::new(&format_file.path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    let entries = if extension == "cbz" {
        load_cbz_pages(extractable_path.path()).await?
    } else if extension == "cbr" {
        load_cbr_pages(extractable_path.path()).await?
    } else if format.format.eq_ignore_ascii_case("CBZ") {
        load_cbz_pages(extractable_path.path()).await?
    } else if format.format.eq_ignore_ascii_case("CBR") {
        load_cbr_pages(extractable_path.path()).await?
    } else {
        return Err(AppError::NoExtractableFormat);
    };

    Ok(ComicArchive { entries })
}

/// Reads image pages from a ZIP-based CBZ file; non-image entries are silently skipped.
async fn load_cbz_pages(path: &FsPath) -> Result<Vec<ComicArchiveEntry>, AppError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<Vec<ComicArchiveEntry>, AppError> {
        let file = std::fs::File::open(path).map_err(|_| AppError::NotFound)?;
        let mut archive = zip::ZipArchive::new(file).map_err(|_| AppError::Internal)?;
        let mut entries = Vec::new();

        for index in 0..archive.len() {
            let mut file = archive.by_index(index).map_err(|_| AppError::Internal)?;
            if file.is_dir() {
                continue;
            }

            let name = file.name().to_string();
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|_| AppError::Internal)?;
            if let Some(content_type) = detect_image_content_type(&name, &bytes) {
                entries.push(ComicArchiveEntry {
                    filename: name,
                    bytes,
                    content_type,
                });
            }
        }

        entries.sort_by(|left, right| left.filename.cmp(&right.filename));
        Ok(entries)
    })
    .await
    .map_err(|_| AppError::Internal)?
}

/// Reads image pages from a RAR-based CBR file using the `unrar` crate; non-image entries are skipped.
async fn load_cbr_pages(path: &FsPath) -> Result<Vec<ComicArchiveEntry>, AppError> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || -> Result<Vec<ComicArchiveEntry>, AppError> {
        let archive = unrar::Archive::new(&path)
            .as_first_part()
            .open_for_processing();
        let mut archive = archive.map_err(|_| AppError::NotFound)?;
        let mut entries = Vec::new();

        loop {
            let Some(next) = archive.read_header().map_err(|_| AppError::Internal)? else {
                break;
            };
            let filename = next.entry().filename.to_string_lossy().to_string();
            let (bytes, next_archive) = next.read().map_err(|_| AppError::Internal)?;
            archive = next_archive;
            if let Some(content_type) = detect_image_content_type(&filename, &bytes) {
                entries.push(ComicArchiveEntry {
                    filename,
                    bytes,
                    content_type,
                });
            }
        }

        entries.sort_by(|left, right| left.filename.cmp(&right.filename));
        Ok(entries)
    })
    .await
    .map_err(|_| AppError::Internal)?
}

/// Detects whether a file entry is a supported comic page image (PNG or JPEG) by magic bytes
/// with extension as a secondary hint; returns None for non-image entries.
fn detect_image_content_type(name: &str, bytes: &[u8]) -> Option<&'static str> {
    let extension = FsPath::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) || extension == "png" {
        return Some("image/png");
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF])
        || matches!(extension.as_str(), "jpg" | "jpeg" | "jfif")
    {
        return Some("image/jpeg");
    }

    None
}

/// Checks that the user has `can_edit` permission; returns 403 Forbidden otherwise.
async fn ensure_can_edit(state: &AppState, user_id: &str) -> Result<(), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_edit {
        return Err(AppError::Forbidden("forbidden".into()));
    }
    Ok(())
}

/// Checks that the user has admin role; returns 403 Forbidden otherwise.
async fn ensure_admin(state: &AppState, user_id: &str) -> Result<(), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.is_admin() {
        return Err(AppError::Forbidden("forbidden".into()));
    }
    Ok(())
}

/// Checks that the user has `can_download` permission; returns 403 Forbidden otherwise.
async fn ensure_download_permission(state: &AppState, user_id: &str) -> Result<(), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_download {
        return Err(AppError::Forbidden("forbidden".into()));
    }
    Ok(())
}

/// Normalizes a column type string to its canonical DB value; returns None for unknown types.
fn normalize_custom_column_type(raw: &str) -> Option<&'static str> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "text" => Some("text"),
        "int" | "integer" => Some("integer"),
        "float" => Some("float"),
        "bool" | "boolean" => Some("bool"),
        "datetime" => Some("datetime"),
        _ => None,
    }
}

/// Returns the first extractable format present on a book in priority order: EPUB > PDF > MOBI > AZW3 > TXT.
fn preferred_extractable_format(book: &crate::db::models::Book) -> Option<&str> {
    ["EPUB", "PDF", "MOBI", "AZW3", "TXT"]
        .into_iter()
        .find(|candidate| {
            book.formats
                .iter()
                .any(|format| format.format.eq_ignore_ascii_case(candidate))
        })
}

#[derive(Debug)]
struct MobiChapterForEpub {
    title: String,
    text: String,
}

/// Converts a `mobi::Mobi` book into a minimal valid EPUB 2.0 ZIP archive in memory,
/// splitting the source HTML on MOBI page-break tags or heading tags for chapter structure.
fn build_epub_from_mobi(book: &mobi::Mobi, book_id: &str) -> Result<Vec<u8>, AppError> {
    let title = {
        let raw = book.title();
        if raw.trim().is_empty() {
            "Converted Book".to_string()
        } else {
            raw.trim().to_string()
        }
    };
    let author = book
        .author()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Unknown Author".to_string());
    let source_html = mobi_util::safe_mobi_content(book);
    let chapters = mobi_chapters_for_epub(&source_html);

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let mimetype_options = FileOptions::default().compression_method(CompressionMethod::Stored);
    let compressed_options = FileOptions::default().compression_method(CompressionMethod::Deflated);

    zip.start_file("mimetype", mimetype_options)
        .map_err(|_| AppError::Internal)?;
    zip.write_all(b"application/epub+zip")
        .map_err(|_| AppError::Internal)?;

    zip.start_file("META-INF/container.xml", compressed_options)
        .map_err(|_| AppError::Internal)?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
    )
    .map_err(|_| AppError::Internal)?;

    let manifest_items = chapters
        .iter()
        .enumerate()
        .map(|(index, _)| {
            format!(
                r#"<item id="chap{index}" href="chapter{}.xhtml" media-type="application/xhtml+xml"/>"#,
                index + 1
            )
        })
        .collect::<Vec<_>>()
        .join("\n    ");
    let spine_items = chapters
        .iter()
        .enumerate()
        .map(|(index, _)| format!(r#"<itemref idref="chap{index}"/>"#))
        .collect::<Vec<_>>()
        .join("\n    ");
    let escaped_title = mobi_util::xml_escape(&title);
    let escaped_author = mobi_util::xml_escape(&author);
    let escaped_book_id = mobi_util::xml_escape(book_id);
    let content_opf = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<package version="2.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{escaped_title}</dc:title>
    <dc:creator>{escaped_author}</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="bookid">urn:xcalibre-server:{escaped_book_id}</dc:identifier>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    {manifest_items}
  </manifest>
  <spine toc="ncx">
    {spine_items}
  </spine>
</package>"#,
    );
    zip.start_file("OEBPS/content.opf", compressed_options)
        .map_err(|_| AppError::Internal)?;
    zip.write_all(content_opf.as_bytes())
        .map_err(|_| AppError::Internal)?;

    let nav_points = chapters
        .iter()
        .enumerate()
        .map(|(index, chapter)| {
            let play_order = index + 1;
            let chapter_title = mobi_util::xml_escape(&chapter.title);
            format!(
                r#"<navPoint id="navPoint-{play_order}" playOrder="{play_order}">
      <navLabel><text>{chapter_title}</text></navLabel>
      <content src="chapter{play_order}.xhtml"/>
    </navPoint>"#
            )
        })
        .collect::<Vec<_>>()
        .join("\n    ");
    let toc_ncx = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head>
    <meta name="dtb:uid" content="urn:xcalibre-server:{escaped_book_id}"/>
  </head>
  <docTitle><text>{}</text></docTitle>
  <navMap>
    {nav_points}
  </navMap>
</ncx>"#,
        escaped_title
    );
    zip.start_file("OEBPS/toc.ncx", compressed_options)
        .map_err(|_| AppError::Internal)?;
    zip.write_all(toc_ncx.as_bytes())
        .map_err(|_| AppError::Internal)?;

    for (index, chapter) in chapters.iter().enumerate() {
        let chapter_title = mobi_util::xml_escape(&chapter.title);
        let paragraphs = chapter
            .text
            .split("\n\n")
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| format!("<p>{}</p>", mobi_util::xml_escape(s)))
            .collect::<Vec<_>>()
            .join("\n    ");
        let chapter_xhtml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
  <head>
    <title>{chapter_title}</title>
  </head>
  <body>
    <h1>{chapter_title}</h1>
    {paragraphs}
  </body>
</html>"#
        );
        zip.start_file(
            format!("OEBPS/chapter{}.xhtml", index + 1),
            compressed_options,
        )
        .map_err(|_| AppError::Internal)?;
        zip.write_all(chapter_xhtml.as_bytes())
            .map_err(|_| AppError::Internal)?;
    }

    let cursor = zip.finish().map_err(|_| AppError::Internal)?;
    Ok(cursor.into_inner())
}

/// Splits MOBI HTML into chapter segments, preferring MOBI page-break markers over heading tags;
/// always returns at least one chapter with "No content available." if the book has no extractable text.
fn mobi_chapters_for_epub(raw_html: &str) -> Vec<MobiChapterForEpub> {
    let mut segments = mobi_util::split_on_mobi_pagebreak(raw_html);
    if segments.len() <= 1 {
        segments = mobi_util::split_on_heading_tags(raw_html);
    }
    if segments.is_empty() {
        segments.push(raw_html.to_string());
    }

    let mut chapters = Vec::new();
    for (index, segment) in segments.into_iter().enumerate() {
        let text = mobi_util::strip_html_to_text(&segment);
        if text.is_empty() {
            continue;
        }
        let title = mobi_util::extract_heading_title(&segment)
            .unwrap_or_else(|| format!("Chapter {}", index + 1));
        chapters.push(MobiChapterForEpub { title, text });
    }

    if chapters.is_empty() {
        let text = mobi_util::strip_html_to_text(raw_html);
        if text.is_empty() {
            vec![MobiChapterForEpub {
                title: "Chapter 1".to_string(),
                text: "No content available.".to_string(),
            }]
        } else {
            vec![MobiChapterForEpub {
                title: "Chapter 1".to_string(),
                text,
            }]
        }
    } else {
        chapters
    }
}

/// Strips non-ASCII-safe characters from a filename for use in a Content-Disposition header;
/// non-allowed characters are replaced with underscores to avoid HTTP header injection.
fn sanitize_file_name_for_header(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        let keep = ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '.' | '_' | '-');
        if keep {
            output.push(ch);
        } else {
            output.push('_');
        }
    }

    let trimmed = output.trim();
    if trimmed.is_empty() {
        "book".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Validates and normalizes a relative storage path, rejecting absolute paths, Windows drive
/// paths, and any `..` components to prevent path traversal attacks.
fn sanitize_relative_path(relative_path: &str) -> Result<PathBuf, AppError> {
    if looks_like_windows_absolute_path(relative_path) {
        return Err(AppError::BadRequest);
    }

    let path = FsPath::new(relative_path);
    if path.is_absolute() {
        return Err(AppError::BadRequest);
    }

    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(AppError::BadRequest);
            }
        }
    }

    if clean.as_os_str().is_empty() {
        return Err(AppError::BadRequest);
    }

    Ok(clean)
}

fn looks_like_windows_absolute_path(relative_path: &str) -> bool {
    let bytes = relative_path.as_bytes();
    bytes.len() >= 3
        && bytes[1] == b':'
        && bytes[0].is_ascii_alphabetic()
        && matches!(bytes[2], b'/' | b'\\')
}

/// Resolves a storage-relative path to a canonical absolute path and verifies it is contained
/// within the storage root; returns 400 if the canonical path escapes the root directory.
async fn canonicalize_storage_file_path(
    state: &AppState,
    relative_path: &str,
) -> Result<PathBuf, AppError> {
    let clean = sanitize_relative_path(relative_path)?;
    let storage_root = PathBuf::from(&state.config.app.storage_path);
    tokio::fs::create_dir_all(&storage_root)
        .await
        .map_err(|_| AppError::Internal)?;
    let canonical_root = tokio::fs::canonicalize(&storage_root)
        .await
        .map_err(|_| AppError::Internal)?;

    let joined = canonical_root.join(clean);
    let canonical_target = tokio::fs::canonicalize(&joined)
        .await
        .map_err(|_| AppError::NotFound)?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(AppError::BadRequest);
    }

    Ok(canonical_target)
}

/// Converts storage backend errors to `AppError::NotFound` or `AppError::Internal`
/// by inspecting the error message for "not found" or "NoSuchKey" patterns.
fn map_storage_read_error(err: anyhow::Error) -> AppError {
    if let Some(io_err) = err.downcast_ref::<std::io::Error>() {
        if io_err.kind() == std::io::ErrorKind::NotFound {
            return AppError::NotFound;
        }
    }

    let message = format!("{err:#}");
    if message.contains("not found") || message.contains("NoSuchKey") {
        AppError::NotFound
    } else {
        AppError::Internal
    }
}

/// Parse an HTTP Range header value against the known file size.
/// Returns None for malformed or unsatisfiable ranges.
fn parse_range(range_str: &str, total: u64) -> Option<(u64, u64)> {
    if total == 0 {
        return None;
    }

    let bytes = range_str.strip_prefix("bytes=")?.trim();
    if bytes.contains(',') {
        return None;
    }

    let (start_str, end_str) = bytes.split_once('-')?;
    let start: u64 = start_str.trim().parse().ok()?;
    if start >= total {
        return None;
    }

    let end: u64 = if end_str.trim().is_empty() {
        total - 1
    } else {
        let parsed_end: u64 = end_str.trim().parse().ok()?;
        if parsed_end >= total {
            return None;
        }
        parsed_end
    };
    if end < start {
        return None;
    }

    Some((start, end))
}

/// Builds a 416 Range Not Satisfiable response with the required `Content-Range: bytes */N` header.
fn range_not_satisfiable_response(total_length: u64) -> Result<axum::response::Response, AppError> {
    axum::response::Response::builder()
        .status(StatusCode::RANGE_NOT_SATISFIABLE)
        .header(header::CONTENT_RANGE, format!("bytes */{total_length}"))
        .body(Body::empty())
        .map_err(|_| AppError::Internal)
}

/// Serves a file from the storage backend with full HTTP range-request support.
/// For the local backend, delegates non-range requests to `tower_http::ServeFile` (which adds
/// ETag/If-None-Match/conditional-GET handling); for remote backends, streams bytes directly.
async fn serve_storage_file(
    state: &AppState,
    request: Request<Body>,
    relative_path: &str,
    content_type: Option<&str>,
    content_disposition: Option<&str>,
) -> Result<axum::response::Response, AppError> {
    let range_header = request
        .headers()
        .get(header::RANGE)
        .and_then(|value| value.to_str().ok())
        .map(ToOwned::to_owned);
    let using_local_backend = state
        .config
        .storage
        .backend
        .trim()
        .eq_ignore_ascii_case("local");

    if using_local_backend {
        let full_path = canonicalize_storage_file_path(state, relative_path).await?;
        if let Some(range_str) = range_header.as_deref() {
            // Single-syscall intent: fetch file size once here and pass it through.
            let file_size = match tokio::fs::metadata(&full_path).await {
                Ok(metadata) => metadata.len(),
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    return Err(AppError::NotFound);
                }
                Err(_) => return Err(AppError::Internal),
            };
            let Some(range) = parse_range(range_str, file_size) else {
                return range_not_satisfiable_response(file_size);
            };

            let result = state
                .storage
                .get_range(relative_path, Some(range), Some(file_size))
                .await
                .map_err(map_storage_read_error)?;

            let mut response_builder = axum::response::Response::builder()
                .status(StatusCode::PARTIAL_CONTENT)
                .header(header::ACCEPT_RANGES, "bytes")
                .header(header::CONTENT_LENGTH, result.bytes.len().to_string());
            if let Some(value) = content_type {
                response_builder = response_builder.header(header::CONTENT_TYPE, value);
            }
            if let Some(value) = content_disposition {
                response_builder = response_builder.header(header::CONTENT_DISPOSITION, value);
            }
            if let Some(content_range) = result.content_range.as_deref() {
                response_builder = response_builder.header(header::CONTENT_RANGE, content_range);
            }

            return response_builder
                .body(Body::from(result.bytes))
                .map_err(|_| AppError::Internal);
        }

        let mut response = serve_file(request, full_path).await?;
        if let Some(value) = content_type {
            let content_type_header =
                HeaderValue::from_str(value).map_err(|_| AppError::Internal)?;
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, content_type_header);
        }
        if let Some(value) = content_disposition {
            let disposition_header =
                HeaderValue::from_str(value).map_err(|_| AppError::Internal)?;
            response
                .headers_mut()
                .insert(header::CONTENT_DISPOSITION, disposition_header);
        }
        Ok(response)
    } else if let Some(range_str) = range_header.as_deref() {
        let file_size = state
            .storage
            .file_size(relative_path)
            .await
            .map_err(map_storage_read_error)?;
        let Some(range) = parse_range(range_str, file_size) else {
            return range_not_satisfiable_response(file_size);
        };

        let result = state
            .storage
            .get_range(relative_path, Some(range), Some(file_size))
            .await
            .map_err(map_storage_read_error)?;

        let mut response_builder = axum::response::Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, result.bytes.len().to_string());
        if let Some(value) = content_type {
            response_builder = response_builder.header(header::CONTENT_TYPE, value);
        }
        if let Some(value) = content_disposition {
            response_builder = response_builder.header(header::CONTENT_DISPOSITION, value);
        }
        if let Some(content_range) = result.content_range.as_deref() {
            response_builder = response_builder.header(header::CONTENT_RANGE, content_range);
        }

        response_builder
            .body(Body::from(result.bytes))
            .map_err(|_| AppError::Internal)
    } else {
        let result = state
            .storage
            .get_range(relative_path, None, None)
            .await
            .map_err(map_storage_read_error)?;

        let mut response_builder = axum::response::Response::builder()
            .status(StatusCode::OK)
            .header(header::ACCEPT_RANGES, "bytes")
            .header(header::CONTENT_LENGTH, result.bytes.len().to_string());
        if let Some(value) = content_type {
            response_builder = response_builder.header(header::CONTENT_TYPE, value);
        }
        if let Some(value) = content_disposition {
            response_builder = response_builder.header(header::CONTENT_DISPOSITION, value);
        }

        response_builder
            .body(Body::from(result.bytes))
            .map_err(|_| AppError::Internal)
    }
}

/// Delegates a request to `tower_http::ServeFile` for local storage, returning 404 if the file
/// is missing rather than letting tower return its default response body.
async fn serve_file(
    request: Request<Body>,
    full_path: PathBuf,
) -> Result<axum::response::Response, AppError> {
    let response = ServeFile::new(full_path)
        .oneshot(request)
        .await
        .map_err(|_| AppError::Internal)?
        .map(Body::new);

    if response.status() == StatusCode::NOT_FOUND {
        return Err(AppError::NotFound);
    }
    Ok(response)
}

/// Reads the multipart upload stream, extracting the required "file" field and optional
/// "metadata" JSON field; enforces the `max_bytes` size limit at the field level.
async fn parse_upload_multipart(
    mut multipart: Multipart,
    max_bytes: u64,
) -> Result<ParsedUpload, AppError> {
    let mut file_name: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;
    let mut metadata = UploadMetadata::default();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest)?
    {
        let Some(name) = field.name().map(ToOwned::to_owned) else {
            continue;
        };

        match name.as_str() {
            "file" => {
                let field_file_name = field
                    .file_name()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| "upload.bin".to_string());
                let field_bytes = field.bytes().await.map_err(|_| AppError::BadRequest)?;
                if field_bytes.len() as u64 > max_bytes {
                    return Err(AppError::PayloadTooLarge);
                }
                file_name = Some(sanitize_upload_file_name(&field_file_name));
                bytes = Some(field_bytes.to_vec());
            }
            "metadata" => {
                let value = field.text().await.map_err(|_| AppError::BadRequest)?;
                metadata = serde_json::from_str::<UploadMetadata>(&value)
                    .map_err(|_| AppError::Unprocessable)?;
            }
            _ => {}
        }
    }

    let file_name = file_name.ok_or(AppError::BadRequest)?;
    let bytes = bytes.ok_or(AppError::BadRequest)?;
    if bytes.is_empty() {
        return Err(AppError::Unprocessable);
    }

    Ok(ParsedUpload {
        file_name,
        bytes,
        metadata,
    })
}

/// Identifies the upload format from magic bytes: `%PDF` → PDF, `PK\x03\x04` → EPUB (ZIP),
/// `BOOKMOBI` or `PalmDOC` anywhere in the first bytes → MOBI.
fn detect_upload_format(bytes: &[u8]) -> Option<UploadFormat> {
    if bytes.starts_with(b"%PDF") {
        return Some(UploadFormat::Pdf);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some(UploadFormat::Epub);
    }
    if bytes
        .windows(b"BOOKMOBI".len())
        .any(|window| window == b"BOOKMOBI")
        || bytes
            .windows(b"PalmDOC".len())
            .any(|window| window == b"PalmDOC")
    {
        return Some(UploadFormat::Mobi);
    }
    None
}

fn extension_format(file_name: &str) -> Option<UploadFormat> {
    let ext = FsPath::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_lowercase())?;

    match ext.as_str() {
        "epub" => Some(UploadFormat::Epub),
        "pdf" => Some(UploadFormat::Pdf),
        "mobi" => Some(UploadFormat::Mobi),
        _ => None,
    }
}

/// Rejects uploads where the file extension contradicts the magic-byte detected format.
fn validate_extension_matches(file_name: &str, detected: UploadFormat) -> Result<(), AppError> {
    if let Some(by_extension) = extension_format(file_name) {
        if by_extension != detected {
            return Err(AppError::Unprocessable);
        }
    }
    Ok(())
}

/// Validates that a format string is in the allowed download extension allowlist; returns 400
/// for unknown formats, preventing arbitrary file extension serving.
fn validated_download_format_extension(format: &str) -> Result<String, AppError> {
    let normalized = format.trim().to_ascii_lowercase();
    if matches!(
        normalized.as_str(),
        "epub"
            | "pdf"
            | "mobi"
            | "azw3"
            | "cbz"
            | "txt"
            | "djvu"
            | "mp3"
            | "m4b"
            | "m4a"
            | "ogg"
            | "opus"
            | "flac"
            | "wav"
            | "aac"
    ) {
        Ok(normalized)
    } else {
        Err(AppError::BadRequest)
    }
}

/// Strips path separators, null bytes, and double-dot sequences from the uploaded filename
/// to prevent path traversal when the name is later used in storage paths.
fn sanitize_upload_file_name(file_name: &str) -> String {
    let without_nulls = file_name.replace('\0', "");
    let final_component = without_nulls
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("upload.bin");
    let stripped = final_component.replace("..", "");
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        "upload.bin".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Normalizes the `tag` query parameter: accepts a single string (possibly comma-separated)
/// or a JSON array, returning a deduplicated list of trimmed non-empty tag strings.
fn parse_tag_query(raw_tags: Option<SingleOrMany>) -> Vec<String> {
    let tags = match raw_tags {
        Some(SingleOrMany::One(tag)) => vec![tag],
        Some(SingleOrMany::Many(tags)) => tags,
        None => Vec::new(),
    };

    tags.into_iter()
        .flat_map(|tag| tag.split(',').map(ToOwned::to_owned).collect::<Vec<_>>())
        .map(trim_owned)
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn trim_owned(value: String) -> String {
    value.trim().to_string()
}

/// Wraps `ingest_text::list_chapters`, returning an empty list instead of propagating errors.
fn list_extractable_chapters(full_path: &FsPath, format: &str) -> Vec<ingest_text::Chapter> {
    ingest_text::list_chapters(full_path, format).unwrap_or_default()
}

/// Parses an optional chunk domain query param; defaults to `Technical` when absent or empty.
fn parse_chunk_domain(value: Option<&str>) -> Result<ChunkDomain, AppError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(domain) => domain
            .parse::<ChunkDomain>()
            .map_err(|_| AppError::BadRequest),
        None => Ok(ChunkDomain::Technical),
    }
}

/// Spawns a background task to index the book in the search backend; logs a warning on failure.
fn queue_book_index(search: Arc<dyn crate::search::SearchBackend>, book: crate::db::models::Book) {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(async move {
                if let Err(err) = search.index_book(&book).await {
                    tracing::warn!(
                        book_id = %book.id,
                        error = %err,
                        "failed to index book in search backend"
                    );
                }
            });
        }
        Err(_) => {
            tracing::warn!(book_id = %book.id, "no active runtime available for search indexing");
        }
    }
}

/// Spawns a background task to generate and store text chunks for the book using default
/// chunk settings (size=600, overlap=100, domain=Technical); logs a warning on failure.
fn queue_book_chunk_generation(state: AppState, book: crate::db::models::Book) {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(async move {
                let config = crate::ingest::chunker::ChunkConfig {
                    target_size: 600,
                    overlap: 100,
                    domain: ChunkDomain::Technical,
                };
                if let Err(err) =
                    ingest_text::generate_and_store_book_chunks(&state, &book, &config).await
                {
                    tracing::warn!(
                        book_id = %book.id,
                        error = %err,
                        "failed to build book chunks"
                    );
                }
            });
        }
        Err(_) => {
            tracing::warn!(book_id = %book.id, "no active runtime available for chunk generation");
        }
    }
}

/// Spawns a background task to remove the book from the search index; logs a warning on failure.
fn queue_book_removal(search: Arc<dyn crate::search::SearchBackend>, book_id: String) {
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn(async move {
                if let Err(err) = search.remove_book(&book_id).await {
                    tracing::warn!(
                        book_id = %book_id,
                        error = %err,
                        "failed to remove book from search backend"
                    );
                }
            });
        }
        Err(_) => {
            tracing::warn!(book_id = %book_id, "no active runtime available for search deindexing");
        }
    }
}

/// Enqueues a `semantic_index` LLM job for the book if LLM features are enabled;
/// silently logs and continues if the enqueue fails.
async fn enqueue_semantic_index_if_enabled(state: &AppState, book_id: &str) {
    if !state.config.llm.enabled {
        return;
    }

    if let Err(err) = llm_queries::enqueue_semantic_index_job(&state.db, book_id).await {
        tracing::warn!(
            book_id = %book_id,
            error = %err,
            "failed to enqueue semantic_index job"
        );
    }
}

/// Extracts embedded metadata from an uploaded file: parses OPF for EPUB, derives title/author
/// from the filename stem for PDF and MOBI (format `"Title - Author.ext"` is recognized).
fn extract_metadata(
    format: UploadFormat,
    file_name: &str,
    bytes: &[u8],
) -> Result<IngestMetadata, AppError> {
    let (fallback_title, fallback_author) = parse_title_author_from_filename(file_name);
    match format {
        UploadFormat::Epub => {
            let mut meta = parse_epub_metadata(bytes).unwrap_or_default();
            if meta.title.as_deref().unwrap_or_default().trim().is_empty() {
                meta.title = Some(fallback_title.clone());
            }
            if meta.authors.is_empty() {
                meta.authors = vec![fallback_author];
            }
            Ok(meta)
        }
        UploadFormat::Pdf | UploadFormat::Mobi => Ok(IngestMetadata {
            title: Some(fallback_title),
            authors: vec![fallback_author],
            ..IngestMetadata::default()
        }),
    }
}

/// Merges caller-supplied upload metadata over the auto-extracted values;
/// empty or whitespace-only strings are treated as absent (not used to clear extracted data).
fn apply_metadata_override(
    mut extracted: IngestMetadata,
    metadata: UploadMetadata,
) -> Result<IngestMetadata, AppError> {
    if let Some(title) = metadata.title {
        let title = trim_owned(title);
        if !title.is_empty() {
            extracted.title = Some(title);
        }
    }
    if let Some(sort_title) = metadata.sort_title {
        let sort_title = trim_owned(sort_title);
        if !sort_title.is_empty() {
            extracted.sort_title = Some(sort_title);
        }
    }

    if let Some(authors) = metadata.authors {
        let parsed = authors
            .into_iter()
            .map(trim_owned)
            .filter(|a| !a.is_empty())
            .collect::<Vec<_>>();
        if !parsed.is_empty() {
            extracted.authors = parsed;
        }
    } else if let Some(author) = metadata.author {
        let author = trim_owned(author);
        if !author.is_empty() {
            extracted.authors = vec![author];
        }
    }

    if let Some(description) = metadata.description {
        extracted.description = Some(trim_owned(description));
    }
    if let Some(pubdate) = metadata.pubdate {
        extracted.pubdate = Some(trim_owned(pubdate));
    }
    if let Some(language) = metadata.language {
        extracted.language = Some(trim_owned(language));
    }
    if let Some(rating) = metadata.rating {
        if !(0..=10).contains(&rating) {
            return Err(AppError::Unprocessable);
        }
        extracted.rating = Some(rating);
    }
    if let Some(series_id) = metadata.series_id {
        let series_id = trim_owned(series_id);
        extracted.series_id = if series_id.is_empty() {
            None
        } else {
            Some(series_id)
        };
    }
    if let Some(series_index) = metadata.series_index {
        extracted.series_index = Some(series_index);
    }
    if let Some(identifiers) = metadata.identifiers {
        extracted.identifiers = identifiers
            .into_iter()
            .map(|id| book_queries::IdentifierInput {
                id_type: trim_owned(id.id_type).to_lowercase(),
                value: trim_owned(id.value),
            })
            .filter(|id| !id.id_type.is_empty() && !id.value.is_empty())
            .collect();
    }

    Ok(extracted)
}

/// Attempts to extract title and author from a filename stem using the "Title - Author" convention;
/// falls back to the full stem as title with "Unknown Author" if the separator is absent.
fn parse_title_author_from_filename(file_name: &str) -> (String, String) {
    let stem = FsPath::new(file_name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("Untitled")
        .trim()
        .to_string();

    if let Some((title, author)) = stem.split_once(" - ") {
        let title = title.trim().to_string();
        let author = author.trim().to_string();
        if !title.is_empty() && !author.is_empty() {
            return (title, author);
        }
    }

    if stem.is_empty() {
        ("Untitled".to_string(), "Unknown Author".to_string())
    } else {
        (stem, "Unknown Author".to_string())
    }
}

/// Parses EPUB metadata by opening the ZIP archive, reading `META-INF/container.xml` to
/// locate the OPF package document, then extracting Dublin Core fields.
fn parse_epub_metadata(bytes: &[u8]) -> Option<IngestMetadata> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).ok()?;

    let container_xml = read_zip_text(&mut archive, "META-INF/container.xml")?;
    let opf_path = find_opf_path(&container_xml).unwrap_or_else(|| "content.opf".to_string());
    let opf_xml = read_zip_text(&mut archive, &opf_path)?;
    parse_opf_xml(&opf_xml)
}

/// Extracts the raw cover image bytes from an EPUB by following the OPF manifest cover item;
/// supports both EPUB 3 `properties="cover-image"` and EPUB 2 `<meta name="cover">` conventions.
fn extract_epub_cover_source(bytes: &[u8]) -> Option<Vec<u8>> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).ok()?;

    let container_xml = read_zip_text(&mut archive, "META-INF/container.xml")?;
    let opf_path = find_opf_path(&container_xml).unwrap_or_else(|| "content.opf".to_string());
    let opf_xml = read_zip_text(&mut archive, &opf_path)?;
    let cover_path = find_cover_image_path(&opf_path, &opf_xml)?;
    read_zip_bytes(&mut archive, &cover_path)
}

struct CoverVariants {
    cover_jpg: Vec<u8>,
    thumb_jpg: Vec<u8>,
    cover_webp: Vec<u8>,
    thumb_webp: Vec<u8>,
}

/// Decodes a raw cover image and renders four variants: 400×600 JPEG, 100×150 JPEG thumbnail,
/// 400×600 lossless WebP, and 100×150 lossless WebP thumbnail.
fn render_cover_variants(raw_cover: &[u8]) -> Option<CoverVariants> {
    let image = image::load_from_memory(raw_cover).ok()?;
    if image.width() == 0 || image.height() == 0 {
        return None;
    }

    let cover = image.thumbnail(400, 600);
    let thumb = image.thumbnail(100, 150);

    let mut cover_writer = std::io::Cursor::new(Vec::new());
    cover
        .write_to(&mut cover_writer, image::ImageFormat::Jpeg)
        .ok()?;

    let mut thumb_writer = std::io::Cursor::new(Vec::new());
    thumb
        .write_to(&mut thumb_writer, image::ImageFormat::Jpeg)
        .ok()?;

    let mut cover_webp_writer = std::io::Cursor::new(Vec::new());
    cover
        .write_with_encoder(image::codecs::webp::WebPEncoder::new_lossless(
            &mut cover_webp_writer,
        ))
        .ok()?;

    let mut thumb_webp_writer = std::io::Cursor::new(Vec::new());
    thumb
        .write_with_encoder(image::codecs::webp::WebPEncoder::new_lossless(
            &mut thumb_webp_writer,
        ))
        .ok()?;

    Some(CoverVariants {
        cover_jpg: cover_writer.into_inner(),
        thumb_jpg: thumb_writer.into_inner(),
        cover_webp: cover_webp_writer.into_inner(),
        thumb_webp: thumb_webp_writer.into_inner(),
    })
}

/// Checks whether a storage path exists; uses filesystem canonicalization for the local backend
/// and a zero-byte range GET probe for remote backends.
async fn storage_path_exists(state: &AppState, relative_path: &str) -> bool {
    let using_local_backend = state
        .config
        .storage
        .backend
        .trim()
        .eq_ignore_ascii_case("local");

    if using_local_backend {
        return canonicalize_storage_file_path(state, relative_path).await.is_ok();
    }

    state
        .storage
        .get_range(relative_path, Some((0, 0)), None)
        .await
        .is_ok()
}

/// Deletes multiple storage paths, ignoring individual failures (best-effort cleanup).
async fn delete_storage_paths(state: &AppState, paths: &[&str]) {
    for path in paths {
        let _ = state.storage.delete(path).await;
    }
}

fn read_zip_text<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str) -> Option<String> {
    let mut file = archive.by_name(path).ok()?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    String::from_utf8(buffer).ok()
}

fn read_zip_bytes<R: Read + Seek>(archive: &mut ZipArchive<R>, path: &str) -> Option<Vec<u8>> {
    let mut file = archive.by_name(path).ok()?;
    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).ok()?;
    Some(buffer)
}

/// Parses `META-INF/container.xml` to find the `full-path` attribute of the first `<rootfile>` element.
fn find_opf_path(container_xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(container_xml).ok()?;
    doc.descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "rootfile")
        .and_then(|node| node.attribute("full-path"))
        .map(ToOwned::to_owned)
}

/// Locates the cover image href from the OPF manifest and resolves it relative to the OPF directory.
fn find_cover_image_path(opf_path: &str, opf_xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(opf_xml).ok()?;
    let cover_href = find_cover_href_in_manifest(&doc)?;
    resolve_zip_relative_path(opf_path, &cover_href)
}

/// Searches the OPF manifest for the cover item, trying EPUB 3 `properties="cover-image"` first
/// and falling back to the EPUB 2 `<meta name="cover" content="id">` pointer.
fn find_cover_href_in_manifest(doc: &roxmltree::Document<'_>) -> Option<String> {
    for node in doc.descendants().filter(|node| node.is_element()) {
        if node.tag_name().name() != "item" {
            continue;
        }
        let properties = node.attribute("properties").unwrap_or_default();
        let has_cover_property = properties
            .split_whitespace()
            .any(|property| property.eq_ignore_ascii_case("cover-image"));
        if has_cover_property {
            if let Some(href) = node.attribute("href") {
                return Some(href.to_string());
            }
        }
    }

    let mut cover_item_id: Option<String> = None;
    for node in doc.descendants().filter(|node| node.is_element()) {
        if node.tag_name().name() == "meta"
            && node
                .attribute("name")
                .is_some_and(|name| name.eq_ignore_ascii_case("cover"))
        {
            if let Some(content) = node.attribute("content") {
                cover_item_id = Some(content.to_string());
                break;
            }
        }
    }

    if let Some(cover_id) = cover_item_id {
        for node in doc.descendants().filter(|node| node.is_element()) {
            if node.tag_name().name() != "item" {
                continue;
            }
            if node
                .attribute("id")
                .is_some_and(|id| id == cover_id.as_str())
            {
                if let Some(href) = node.attribute("href") {
                    return Some(href.to_string());
                }
            }
        }
    }

    None
}

/// Resolves a manifest href relative to the OPF document's directory, normalizing `.` components
/// and rejecting any `..` traversal that would escape the EPUB ZIP root.
fn resolve_zip_relative_path(opf_path: &str, candidate_href: &str) -> Option<String> {
    let opf_dir = FsPath::new(opf_path)
        .parent()
        .map(ToOwned::to_owned)
        .unwrap_or_default();
    let joined = opf_dir.join(candidate_href);

    let mut clean = PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    if clean.as_os_str().is_empty() {
        return None;
    }

    Some(
        clean
            .iter()
            .map(|part| part.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Parses Dublin Core metadata fields (title, creator, identifier) from the OPF package document.
fn parse_opf_xml(opf_xml: &str) -> Option<IngestMetadata> {
    let doc = roxmltree::Document::parse(opf_xml).ok()?;
    let mut metadata = IngestMetadata::default();

    for node in doc.descendants().filter(|node| node.is_element()) {
        match node.tag_name().name() {
            "title" => {
                if metadata.title.is_none() {
                    metadata.title = node
                        .text()
                        .map(str::trim)
                        .filter(|text| !text.is_empty())
                        .map(ToOwned::to_owned);
                }
            }
            "creator" => {
                if let Some(creator) = node.text().map(str::trim).filter(|text| !text.is_empty()) {
                    metadata.authors.push(creator.to_string());
                }
            }
            "identifier" => {
                if let Some(value) = node.text().map(str::trim).filter(|text| !text.is_empty()) {
                    let raw_type = node
                        .attribute("opf:scheme")
                        .or_else(|| node.attribute("scheme"))
                        .or_else(|| node.attribute("id"))
                        .unwrap_or("");

                    let normalized_value = value.trim().to_string();
                    let mut id_type = raw_type.to_lowercase();
                    if id_type.contains("isbn")
                        || (id_type.is_empty() && looks_like_isbn(&normalized_value))
                    {
                        id_type = isbn_type(&normalized_value);
                    }
                    if !id_type.is_empty() {
                        metadata.identifiers.push(book_queries::IdentifierInput {
                            id_type,
                            value: normalized_value,
                        });
                    }
                }
            }
            _ => {}
        }
    }

    Some(metadata)
}

/// Returns true if the compact (alphanumeric-only) form of the value is 10 or 13 characters — a heuristic for ISBN.
fn looks_like_isbn(value: &str) -> bool {
    let compact = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();
    compact.len() == 10 || compact.len() == 13
}

/// Returns "isbn13" for 13-character compact values and "isbn" for 10-character values.
fn isbn_type(value: &str) -> String {
    let compact = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();
    if compact.len() == 13 {
        "isbn13".to_string()
    } else {
        "isbn".to_string()
    }
}
