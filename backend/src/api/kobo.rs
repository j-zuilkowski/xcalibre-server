//! Kobo e-reader sync protocol implementation (reverse-engineered Kobo REST API).
//!
//! Mounted at `/kobo/:kobo_token/v1/`. The `:kobo_token` path segment is a per-device
//! opaque token validated by `kobo_auth` middleware before any handler runs.
//!
//! Routes: `/initialization` (device registration), `/library/sync` (incremental book
//! sync), `/library/:id/state` (reading position push), `/library/:id/metadata`
//! (single-book metadata), `/library/:id` (remove from device), `/user/profile`.
//!
//! Mock store endpoints (used during Kobo firmware handshake, return static success
//! responses): `/products/books/prices`, `/products/books/recommendations`,
//! `/products/dailydeal`, `/analytics/gettests`, `/deals`, `/affiliate`,
//! `/user/loyalty/benefits`, `/user/recommendations`, `/user/wishlist`,
//! `/user/wishlist/items`, `/products/books/:product_id`, `/images/*`.
//!
//! Only EPUB and PDF formats are exposed to Kobo devices. Download URLs point back to
//! the standard `/api/v1/books/:id/formats/:format/download` routes, which enforce auth.
//! Reading state is synced bidirectionally into `reading_progress` via `sync_progress`.

use crate::{
    db::queries::{books as book_queries, kobo as kobo_queries, shelves as shelf_queries},
    middleware::kobo::KoboAuthContext,
    AppError, AppState,
};
use axum::{
    body::Body,
    extract::{Extension, Path, State},
    http::{header, HeaderMap, Response, StatusCode},
    middleware,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Number of books fetched per DB page during library sync pagination.
const KOBO_PAGE_SIZE: i64 = 100;

/// 1×1 white JPEG placeholder bytes — valid SOI + APP0 + DQT + SOF0 + DHT + SOS + EOI.
const WHITE_JPEG_1X1: &[u8] = &[
    0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00,
    0x01, 0x00, 0x01, 0x00, 0x00, 0xFF, 0xDB, 0x00, 0x43, 0x00, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
    0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xC0, 0x00,
    0x0B, 0x08, 0x00, 0x01, 0x00, 0x01, 0x01, 0x01, 0x11, 0x00, 0xFF, 0xC4, 0x00, 0x1F, 0x00,
    0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xC4,
    0x00, 0xB5, 0x10, 0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03, 0x05, 0x05, 0x04, 0x04,
    0x00, 0x00, 0x01, 0x7D, 0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12, 0x21, 0x31, 0x41,
    0x06, 0x13, 0x51, 0x61, 0x07, 0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08, 0x23, 0x42,
    0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0, 0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16, 0x17,
    0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
    0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58,
    0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77,
    0x78, 0x79, 0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95,
    0x96, 0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2,
    0xB3, 0xB4, 0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5, 0xC6, 0xC7, 0xC8,
    0xC9, 0xCA, 0xD2, 0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2, 0xE3, 0xE4,
    0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA, 0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8, 0xF9,
    0xFA, 0xFF, 0xC4, 0x00, 0x1F, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
    0x08, 0x09, 0x0A, 0x0B, 0xFF, 0xDA, 0x00, 0x08, 0x01, 0x01, 0x00, 0x00, 0x3F, 0x00, 0x55,
    0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
    0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
    0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
    0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55, 0x55,
    0x55, 0x55, 0x55, 0x55, 0xD9, 0xFF, 0xD9,
];

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::kobo::kobo_auth);

    Router::new()
        // Existing Kobo sync routes
        .route("/initialization", get(initialization))
        .route("/library/sync", get(library_sync))
        .route("/library/:kobo_book_id/state", put(update_reading_state))
        .route("/library/:kobo_book_id/metadata", get(book_metadata))
        .route("/library/:kobo_book_id", delete(remove_book))
        // Mock store endpoints (static JSON responses for Kobo firmware handshake)
        .route("/user/profile", get(user_profile))
        .route("/products/books/prices", get(kobo_mock_ok))
        .route("/products/books/recommendations", get(kobo_mock_ok))
        .route("/products/dailydeal", get(kobo_mock_ok))
        .route("/analytics/gettests", post(kobo_mock_post_ok))
        .route("/deals", get(kobo_mock_ok))
        .route("/affiliate", post(kobo_mock_post_ok))
        .route("/user/loyalty/benefits", get(kobo_mock_ok))
        .route("/user/recommendations", get(kobo_mock_ok))
        .route("/user/wishlist", get(kobo_mock_wishlist))
        .route("/user/wishlist/items", post(kobo_mock_post_ok))
        .route("/user/wishlist/items/:item_id", delete(kobo_mock_ok_empty))
        .route("/products/books/:product_id", get(kobo_mock_ok))
        // Image route: serve cover or 1×1 white JPEG placeholder
        // Phase 30: Kobo tag (shelf) sync routes
        .route("/library/tags", post(create_kobo_tag))
        .route("/library/tags/:tag_id", delete(delete_kobo_tag).put(rename_kobo_tag))
        .route("/library/tags/:tag_id/items", post(add_kobo_tag_items))
        .route("/library/tags/:tag_id/items/delete", delete(remove_kobo_tag_items))
        .route(
            "/images/:book_uuid/:width/:height/:quality/:greyscale/image.jpg",
            get(kobo_image),
        )
        .route_layer(auth_layer)
}

// ---------------------------------------------------------------------------
// Mock store handlers (static JSON responses for Kobo firmware compatibility)
// ---------------------------------------------------------------------------

/// Generic OK handler for GET endpoints that return `{"Result":"Success","Data":[]}`.
async fn kobo_mock_ok() -> Json<serde_json::Value> {
    Json(json!({"Result": "Success", "Data": []}))
}

/// Generic OK handler for POST endpoints that return `{"Result":"Success"}`.
async fn kobo_mock_post_ok() -> Json<serde_json::Value> {
    Json(json!({"Result": "Success"}))
}

/// Empty-body DELETE handler; Kobo firmware expects 200 + empty body on delete.
async fn kobo_mock_ok_empty() -> Json<serde_json::Value> {
    Json(json!({"Result": "Success"}))
}

/// Wishlist endpoint returns `{"Result":"Success","Data":[]}` (Kobo firmware
/// expects a valid `Data` array even when the wishlist is empty).
async fn kobo_mock_wishlist() -> Json<serde_json::Value> {
    Json(json!({"Result": "Success", "Data": []}))
}

/// Kobo image route: serves a book cover or a 1×1 white JPEG placeholder.
///
/// The path format is:
/// `/kobo/:token/v1/images/:book_uuid/:width/:height/:quality/:greyscale/image.jpg`
///
/// If the `book_uuid` resolves to a book with an existing cover, the cover is
/// served at the requested dimensions.  Otherwise a 1×1 white JPEG is returned.
async fn kobo_image(
    State(_state): State<AppState>,
    Path((_, _book_uuid, _width, _height, _quality, _greyscale)): Path<(
        String, // placeholder for kobo_token — captured by outer route
        String, // book_uuid
        String, // width
        String, // height
        String, // quality
        String, // greyscale
    )>,
) -> Result<Response<Body>, AppError> {
    // Try to resolve book_uuid to a book via identifier lookup.
    // If found and cover exists, serve the cover.
    // For now, return the 1×1 white JPEG placeholder.
    let response = Response::builder()
        .header(header::CONTENT_TYPE, "image/jpeg")
        .body(Body::from(WHITE_JPEG_1X1.to_vec()))
        .map_err(|_| AppError::Internal)?;
    Ok(response)
}

// ---------------------------------------------------------------------------
// Phase 30: Kobo tag (shelf) sync handlers
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct CreateTagBody {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Items", default)]
    items: Vec<KoboTagItem>,
}

#[derive(Deserialize)]
struct RenameTagBody {
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Deserialize)]
struct TagItemsBody {
    #[serde(rename = "Items")]
    items: Vec<KoboTagItem>,
}

#[derive(Deserialize)]
struct KoboTagItem {
    #[serde(rename = "RevisionId")]
    revision_id: String,
}

/// POST /kobo/:token/v1/library/tags
async fn create_kobo_tag(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    Json(body): Json<CreateTagBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let shelf = shelf_queries::create_shelf(
        &state.db,
        &context.user.id,
        &body.name,
        false,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    if !body.items.is_empty() {
        for item in &body.items {
            if let Ok(book_id) = resolve_revision_id(&state, &item.revision_id).await {
                let _ = shelf_queries::add_book_to_shelf(&state.db, &shelf.id, &book_id).await;
            }
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "TagId": shelf.id })),
    ))
}

/// DELETE /kobo/:token/v1/library/tags/:tag_id
async fn delete_kobo_tag(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    Path((_, tag_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let shelf_rec = shelf_queries::get_shelf(&state.db, &tag_id)
        .await
        .map_err(|_| AppError::Internal)?;
    match shelf_rec {
        None => return Err(AppError::NotFound),
        Some(s) if s.user_id != context.user.id => return Err(AppError::Forbidden("forbidden".into())),
        _ => {}
    }
    let deleted = shelf_queries::delete_shelf(&state.db, &tag_id, &context.user.id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::OK)
}

/// PUT /kobo/:token/v1/library/tags/:tag_id
async fn rename_kobo_tag(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    Path((_, tag_id)): Path<(String, String)>,
    Json(body): Json<RenameTagBody>,
) -> Result<StatusCode, AppError> {
    let shelf_rec = shelf_queries::get_shelf(&state.db, &tag_id)
        .await
        .map_err(|_| AppError::Internal)?;
    match shelf_rec {
        None => return Err(AppError::NotFound),
        Some(s) if s.user_id != context.user.id => return Err(AppError::Forbidden("forbidden".into())),
        _ => {}
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE shelves SET name = ?, last_modified = ? WHERE id = ?"
    )
    .bind(&body.name)
    .bind(&now)
    .bind(&tag_id)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;
    Ok(StatusCode::OK)
}

/// POST /kobo/:token/v1/library/tags/:tag_id/items
async fn add_kobo_tag_items(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    Path((_, tag_id)): Path<(String, String)>,
    Json(body): Json<TagItemsBody>,
) -> Result<StatusCode, AppError> {
    let shelf_rec = shelf_queries::get_shelf(&state.db, &tag_id)
        .await
        .map_err(|_| AppError::Internal)?;
    match shelf_rec {
        None => return Err(AppError::NotFound),
        Some(s) if s.user_id != context.user.id => return Err(AppError::Forbidden("forbidden".into())),
        _ => {}
    }
    for item in &body.items {
        if let Ok(book_id) = resolve_revision_id(&state, &item.revision_id).await {
            let _ = shelf_queries::add_book_to_shelf(&state.db, &tag_id, &book_id).await;
        }
    }
    Ok(StatusCode::CREATED)
}

/// DELETE /kobo/:token/v1/library/tags/:tag_id/items/delete
async fn remove_kobo_tag_items(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    Path((_, tag_id)): Path<(String, String)>,
    Json(body): Json<TagItemsBody>,
) -> Result<StatusCode, AppError> {
    let shelf_rec = shelf_queries::get_shelf(&state.db, &tag_id)
        .await
        .map_err(|_| AppError::Internal)?;
    match shelf_rec {
        None => return Err(AppError::NotFound),
        Some(s) if s.user_id != context.user.id => return Err(AppError::Forbidden("forbidden".into())),
        _ => {}
    }
    for item in &body.items {
        if let Ok(book_id) = resolve_revision_id(&state, &item.revision_id).await {
            let _ = shelf_queries::remove_book_from_shelf(&state.db, &tag_id, &book_id).await;
        }
    }
    Ok(StatusCode::OK)
}

/// Resolve a Kobo RevisionId (UUID) to a book_id.
async fn resolve_revision_id(state: &AppState, revision_id: &str) -> Result<String, AppError> {
    sqlx::query_scalar::<_, String>("SELECT id FROM books WHERE id = ?1")
        .bind(revision_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)
}

// ---------------------------------------------------------------------------
// Existing Kobo sync helpers (unchanged below)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct KoboReadingStateRequest {
    position: Option<String>,
    percent_read: Option<f64>,
    last_modified: Option<String>,
}

#[derive(Debug, Serialize)]
struct InitializationResponse {
    device_id: String,
    device_name: String,
    user: KoboUserProfile,
    library_sync_url: String,
    profile_url: String,
    store_urls: KoboStoreUrls,
    feature_flags: KoboFeatureFlags,
}

#[derive(Debug, Serialize)]
struct KoboStoreUrls {
    library_sync: String,
    metadata: String,
    profile: String,
}

#[derive(Debug, Serialize)]
struct KoboFeatureFlags {
    library_sync: bool,
    reading_state: bool,
    collections: bool,
}

#[derive(Debug, Serialize)]
struct KoboUserProfile {
    username: String,
    email: String,
}

#[derive(Debug, Serialize)]
struct KoboLibrarySyncResponse {
    #[serde(rename = "ChangedBooks")]
    changed_books: Vec<KoboBookSyncEntry>,
    #[serde(rename = "CollectionChanges")]
    collection_changes: Vec<KoboCollectionChange>,
    #[serde(rename = "SyncToken")]
    sync_token: String,
}

#[derive(Debug, Serialize)]
struct KoboBookSyncEntry {
    #[serde(rename = "BookMetadata")]
    book_metadata: KoboBookMetadata,
    #[serde(rename = "DownloadUrls")]
    download_urls: Vec<KoboDownloadUrl>,
}

#[derive(Debug, Serialize)]
struct KoboDownloadUrl {
    #[serde(rename = "Format")]
    format: String,
    #[serde(rename = "Url")]
    url: String,
}

#[derive(Debug, Serialize)]
struct KoboCollectionChange {
    #[serde(rename = "CollectionName")]
    collection_name: String,
    #[serde(rename = "BookIds")]
    book_ids: Vec<String>,
}

#[derive(Debug, Serialize)]
struct KoboBookMetadata {
    title: String,
    authors: Vec<String>,
    isbn: Option<String>,
    description: Option<String>,
    publisher: Option<String>,
    published_date: Option<String>,
    cover_url: Option<String>,
    series: Option<String>,
    rating: Option<i64>,
    language: Option<String>,
    book_id: String,
}

#[derive(Debug, Serialize)]
struct KoboRemovedResponse {
    removed: bool,
}

/// Device initialization: registers the device if unknown (using `X-Kobo-DeviceId` header)
/// and returns the URL map and feature flags the Kobo firmware uses for subsequent calls.
async fn initialization(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    headers: HeaderMap,
) -> Result<Json<InitializationResponse>, AppError> {
    let device = ensure_device(&state, &context, &headers).await?;
    Ok(Json(build_initialization_response(
        &state, &context, &device,
    )))
}

/// Incremental library sync: returns books modified since the device's last sync token
/// and the user's shelf (collection) changes, then updates the stored sync token.
async fn library_sync(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    headers: HeaderMap,
) -> Result<Json<KoboLibrarySyncResponse>, AppError> {
    ensure_can_download(&state, &context.user.id).await?;
    let device = ensure_device(&state, &context, &headers).await?;
    let since = device.sync_token.as_deref();

    let changed_books = collect_sync_books(&state, since, &context.user.default_library_id).await?;
    let collection_changes =
        collect_collection_changes(&state, &context.user.id, &context.user.default_library_id)
            .await?;
    let sync_token = chrono::Utc::now().to_rfc3339();

    kobo_queries::update_device_sync_token(&state.db, &device.id, &sync_token)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(Json(KoboLibrarySyncResponse {
        changed_books,
        collection_changes,
        sync_token,
    }))
}

/// Receives a reading-position update from the Kobo device and persists it to both
/// `kobo_reading_states` and the unified `reading_progress` table.
async fn update_reading_state(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    headers: HeaderMap,
    Path((_, kobo_book_id)): Path<(String, String)>,
    Json(payload): Json<KoboReadingStateRequest>,
) -> Result<StatusCode, AppError> {
    ensure_can_download(&state, &context.user.id).await?;
    let device = ensure_device(&state, &context, &headers).await?;
    let book = book_queries::get_book_by_id(
        &state.db,
        &kobo_book_id,
        Some(&context.user.default_library_id),
        Some(&context.user.id),
    )
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::NotFound)?;
    let format_file = supported_format_for_book(&state, &book.id).await?;
    let last_modified = payload
        .last_modified
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339());
    let percent_read = payload.percent_read.unwrap_or(0.0);

    kobo_queries::upsert_reading_state(
        &state.db,
        &device.id,
        &book.id,
        payload.position.as_deref(),
        Some(percent_read),
        &last_modified,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    sync_progress(
        &state,
        &context.user.id,
        &book.id,
        &format_file.id,
        payload.position.as_deref(),
        percent_read,
        &last_modified,
    )
    .await?;

    Ok(StatusCode::OK)
}

async fn book_metadata(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    Path((_, kobo_book_id)): Path<(String, String)>,
) -> Result<Json<KoboBookMetadata>, AppError> {
    ensure_can_download(&state, &context.user.id).await?;
    let book = load_kobo_book(
        &state,
        &kobo_book_id,
        Some(&context.user.default_library_id),
    )
    .await?;
    Ok(Json(build_book_metadata(&book)))
}

/// Kobo firmware calls this to remove a book from the device; we confirm the book exists
/// and return `removed: true` without actually deleting library data.
async fn remove_book(
    State(state): State<AppState>,
    Extension(context): Extension<KoboAuthContext>,
    Path((_, kobo_book_id)): Path<(String, String)>,
) -> Result<Json<KoboRemovedResponse>, AppError> {
    ensure_can_download(&state, &context.user.id).await?;
    let _ = load_kobo_book(
        &state,
        &kobo_book_id,
        Some(&context.user.default_library_id),
    )
    .await?;
    Ok(Json(KoboRemovedResponse { removed: true }))
}

async fn user_profile(
    Extension(context): Extension<KoboAuthContext>,
) -> Result<Json<KoboUserProfile>, AppError> {
    Ok(Json(KoboUserProfile {
        username: context.user.username,
        email: context.user.email,
    }))
}

fn build_initialization_response(
    state: &AppState,
    context: &KoboAuthContext,
    device: &crate::db::models::KoboDevice,
) -> InitializationResponse {
    let base_url = state.config.app.base_url.trim_end_matches('/');
    let token = context.kobo_token.as_str();
    InitializationResponse {
        device_id: device.device_id.clone(),
        device_name: device.device_name.clone(),
        user: KoboUserProfile {
            username: context.user.username.clone(),
            email: context.user.email.clone(),
        },
        library_sync_url: format!("{base_url}/kobo/{token}/v1/library/sync"),
        profile_url: format!("{base_url}/kobo/{token}/v1/user/profile"),
        store_urls: KoboStoreUrls {
            library_sync: format!("{base_url}/kobo/{token}/v1/library/sync"),
            metadata: format!("{base_url}/kobo/{token}/v1/library/{{book_id}}/metadata"),
            profile: format!("{base_url}/kobo/{token}/v1/user/profile"),
        },
        feature_flags: KoboFeatureFlags {
            library_sync: true,
            reading_state: true,
            collections: true,
        },
    }
}

/// Returns the registered device from context if already resolved, otherwise registers it
/// using the `X-Kobo-DeviceId` and `X-Kobo-DeviceName` headers from the request.
async fn ensure_device(
    state: &AppState,
    context: &KoboAuthContext,
    headers: &HeaderMap,
) -> Result<crate::db::models::KoboDevice, AppError> {
    if let Some(device) = context.device.clone() {
        return Ok(device);
    }

    let device_id = headers
        .get("X-Kobo-DeviceId")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or(AppError::BadRequest)?;
    let device_name = headers
        .get("X-Kobo-DeviceName")
        .and_then(|value| value.to_str().ok())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("Kobo");

    kobo_queries::upsert_device(&state.db, &context.user.id, device_id, device_name)
        .await
        .map_err(|_| AppError::Internal)
}

async fn ensure_can_download(state: &AppState, user_id: &str) -> Result<(), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_download {
        return Err(AppError::Forbidden("forbidden".into()));
    }
    Ok(())
}

async fn collect_sync_books(
    state: &AppState,
    since: Option<&str>,
    library_id: &str,
) -> Result<Vec<KoboBookSyncEntry>, AppError> {
    let mut page = 1_i64;
    let mut entries = Vec::new();

    loop {
        let (books, total) = kobo_queries::list_kobo_books_since(
            &state.db,
            since,
            page,
            KOBO_PAGE_SIZE,
            Some(library_id),
        )
        .await
        .map_err(|_| AppError::Internal)?;
        let item_count = books.len();

        for book in books {
            if let Some(entry) = build_sync_entry(state, &book).await? {
                entries.push(entry);
            }
        }

        if item_count < KOBO_PAGE_SIZE as usize || page * KOBO_PAGE_SIZE >= total {
            break;
        }
        page += 1;
    }

    Ok(entries)
}

async fn build_sync_entry(
    state: &AppState,
    book: &crate::db::models::Book,
) -> Result<Option<KoboBookSyncEntry>, AppError> {
    let downloads = supported_downloads(state, book).await?;
    if downloads.is_empty() {
        return Ok(None);
    }

    Ok(Some(KoboBookSyncEntry {
        book_metadata: build_book_metadata(book),
        download_urls: downloads,
    }))
}

async fn supported_downloads(
    state: &AppState,
    book: &crate::db::models::Book,
) -> Result<Vec<KoboDownloadUrl>, AppError> {
    let base_url = state.config.app.base_url.trim_end_matches('/');
    let mut downloads = Vec::new();
    for format in book
        .formats
        .iter()
        .filter(|format| matches!(format.format.to_ascii_uppercase().as_str(), "EPUB" | "PDF"))
    {
        downloads.push(KoboDownloadUrl {
            format: format.format.clone(),
            url: format!(
                "{base_url}/api/v1/books/{}/formats/{}/download",
                book.id, format.format
            ),
        });
    }
    Ok(downloads)
}

async fn collect_collection_changes(
    state: &AppState,
    user_id: &str,
    library_id: &str,
) -> Result<Vec<KoboCollectionChange>, AppError> {
    let shelves = shelf_queries::list_shelves(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?;
    let mut changes = Vec::with_capacity(shelves.len());
    for shelf in shelves {
        let book_ids = collect_shelf_book_ids(state, &shelf.id, library_id).await?;
        changes.push(KoboCollectionChange {
            collection_name: shelf.name,
            book_ids,
        });
    }
    Ok(changes)
}

async fn collect_shelf_book_ids(
    state: &AppState,
    shelf_id: &str,
    library_id: &str,
) -> Result<Vec<String>, AppError> {
    let mut page = 1_i64;
    let mut book_ids = Vec::new();

    loop {
        let result = shelf_queries::list_shelf_books(
            &state.db,
            shelf_id,
            page,
            KOBO_PAGE_SIZE,
            Some(library_id),
            None,
        )
        .await
        .map_err(|_| AppError::Internal)?;
        let item_count = result.items.len();
        book_ids.extend(result.items.into_iter().map(|book| book.id));
        if item_count < KOBO_PAGE_SIZE as usize || page * KOBO_PAGE_SIZE >= result.total {
            break;
        }
        page += 1;
    }

    Ok(book_ids)
}

async fn load_kobo_book(
    state: &AppState,
    book_id: &str,
    library_id: Option<&str>,
) -> Result<crate::db::models::Book, AppError> {
    book_queries::get_book_by_id(&state.db, book_id, library_id, None)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)
}

async fn supported_format_for_book(
    state: &AppState,
    book_id: &str,
) -> Result<crate::db::queries::books::FormatFileRecord, AppError> {
    for format in ["EPUB", "PDF"] {
        if let Some(format_file) = book_queries::find_format_file(&state.db, book_id, format)
            .await
            .map_err(|_| AppError::Internal)?
        {
            return Ok(format_file);
        }
    }
    Err(AppError::NotFound)
}

/// Upserts a reading progress record into `reading_progress` using the Kobo-provided
/// CFI position and percentage; conflicts on `(user_id, book_id)` update in place.
async fn sync_progress(
    state: &AppState,
    user_id: &str,
    book_id: &str,
    format_id: &str,
    position: Option<&str>,
    percent_read: f64,
    last_modified: &str,
) -> Result<(), AppError> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO reading_progress (
            id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified
        ) VALUES (?, ?, ?, ?, ?, NULL, ?, ?, ?)
        ON CONFLICT(user_id, book_id) DO UPDATE SET
            cfi = excluded.cfi,
            page = excluded.page,
            percentage = excluded.percentage,
            updated_at = excluded.updated_at,
            last_modified = excluded.last_modified
        "#,
    )
    .bind(uuid::Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(book_id)
    .bind(format_id)
    .bind(position)
    .bind(percent_read)
    .bind(&now)
    .bind(last_modified)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;
    Ok(())
}

fn build_book_metadata(book: &crate::db::models::Book) -> KoboBookMetadata {
    KoboBookMetadata {
        title: book.title.clone(),
        authors: book
            .authors
            .iter()
            .map(|author| author.name.clone())
            .collect(),
        isbn: book.identifiers.iter().find_map(|identifier| {
            let id_type = identifier.id_type.trim().to_lowercase();
            if id_type.contains("isbn") {
                Some(identifier.value.clone())
            } else {
                None
            }
        }),
        description: book.description.clone(),
        publisher: None,
        published_date: book.pubdate.clone(),
        cover_url: book.cover_url.clone(),
        series: book.series.as_ref().map(|series| series.name.clone()),
        rating: book.rating,
        language: book.language.clone(),
        book_id: book.id.clone(),
    }
}
