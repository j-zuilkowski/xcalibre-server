use crate::{
    db::queries::{books as book_queries, kobo as kobo_queries, shelves as shelf_queries},
    middleware::kobo::KoboAuthContext,
    AppError, AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::HeaderMap,
    http::StatusCode,
    middleware,
    routing::{delete, get, put},
    Json, Router,
};
use serde::{Deserialize, Serialize};

const KOBO_PAGE_SIZE: i64 = 100;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::kobo::kobo_auth);

    Router::new()
        .route("/initialization", get(initialization))
        .route("/library/sync", get(library_sync))
        .route("/library/:kobo_book_id/state", put(update_reading_state))
        .route("/library/:kobo_book_id/metadata", get(book_metadata))
        .route("/library/:kobo_book_id", delete(remove_book))
        .route("/user/profile", get(user_profile))
        .route_layer(auth_layer)
}

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
