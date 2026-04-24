use crate::{
    api::{
        books::{accessible_library_id, load_book_or_not_found},
        search::{collection_book_ids_for_search, run_chunk_search},
    },
    db::queries::collections as collection_queries,
    middleware::auth::AuthenticatedUser,
    AppError, AppState,
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    middleware,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use utoipa::{IntoParams, ToSchema};

const MAX_CHUNK_SEARCH_RESULTS: u32 = 100;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route(
            "/api/v1/collections",
            get(list_collections).post(create_collection),
        )
        .route(
            "/api/v1/collections/:id",
            get(get_collection)
                .patch(update_collection)
                .delete(delete_collection),
        )
        .route(
            "/api/v1/collections/:id/books",
            post(add_books_to_collection),
        )
        .route(
            "/api/v1/collections/:id/books/:book_id",
            delete(remove_book_from_collection),
        )
        .route(
            "/api/v1/collections/:id/search/chunks",
            get(search_collection_chunks),
        )
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateCollectionRequest {
    name: String,
    description: Option<String>,
    #[serde(default)]
    domain: Option<String>,
    #[serde(default)]
    is_public: bool,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct UpdateCollectionRequest {
    name: Option<String>,
    description: Option<String>,
    domain: Option<String>,
    is_public: Option<bool>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct AddBooksRequest {
    #[serde(default)]
    book_ids: Vec<String>,
}

#[derive(Debug, Deserialize, Default, IntoParams)]
pub(crate) struct CollectionChunkSearchQueryParams {
    q: Option<String>,
    #[serde(
        default,
        alias = "book_ids[]",
        deserialize_with = "crate::api::search::deserialize_string_or_many"
    )]
    book_ids: Vec<String>,
    #[serde(default, rename = "type")]
    chunk_type: Option<String>,
    limit: Option<u32>,
    rerank: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/v1/collections",
    tag = "collections",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Visible collections", body = [crate::db::queries::collections::CollectionSummary]),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn list_collections(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<collection_queries::CollectionSummary>>, AppError> {
    let collections = collection_queries::list_collections(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(collections))
}

#[utoipa::path(
    post,
    path = "/api/v1/collections",
    tag = "collections",
    security(("bearer_auth" = [])),
    request_body = CreateCollectionRequest,
    responses(
        (status = 201, description = "Collection created", body = crate::db::queries::collections::CollectionSummary),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn create_collection(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateCollectionRequest>,
) -> Result<(StatusCode, Json<collection_queries::CollectionSummary>), AppError> {
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest);
    }

    let description = payload
        .description
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let domain = payload.domain.unwrap_or_else(|| "technical".to_string());
    let created = collection_queries::create_collection(
        &state.db,
        &auth_user.user.id,
        collection_queries::CollectionInput {
            name: name.to_string(),
            description,
            domain,
            is_public: payload.is_public,
        },
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok((StatusCode::CREATED, Json(created)))
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Collection id")
    ),
    responses(
        (status = 200, description = "Collection details", body = crate::db::queries::collections::CollectionDetail),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn get_collection(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(collection_id): Path<String>,
) -> Result<Json<collection_queries::CollectionDetail>, AppError> {
    ensure_visible_collection(&state, &auth_user.user.id, &collection_id).await?;
    let collection = collection_queries::get_collection_detail(
        &state.db,
        &collection_id,
        Some(auth_user.user.default_library_id.as_str()),
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::NotFound)?;

    Ok(Json(collection))
}

#[utoipa::path(
    patch,
    path = "/api/v1/collections/{id}",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Collection id")
    ),
    request_body = UpdateCollectionRequest,
    responses(
        (status = 200, description = "Collection updated", body = crate::db::queries::collections::CollectionSummary),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn update_collection(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(collection_id): Path<String>,
    Json(payload): Json<UpdateCollectionRequest>,
) -> Result<Json<collection_queries::CollectionSummary>, AppError> {
    let name = payload
        .name
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let description = payload
        .description
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let domain = payload.domain.map(|value| {
        let value = value.trim().to_ascii_lowercase();
        match value.as_str() {
            "technical" | "electronics" | "culinary" | "legal" | "academic" | "narrative" => {
                value
            }
            _ => "technical".to_string(),
        }
    });
    let is_public = payload.is_public;

    let updated = collection_queries::update_collection(
        &state.db,
        &collection_id,
        &auth_user.user.id,
        name,
        description,
        domain,
        is_public,
    )
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::NotFound)?;

    Ok(Json(updated))
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Collection id")
    ),
    responses(
        (status = 204, description = "Collection deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn delete_collection(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(collection_id): Path<String>,
) -> Result<StatusCode, AppError> {
    let deleted = collection_queries::delete_collection(
        &state.db,
        &collection_id,
        &auth_user.user.id,
    )
        .await
        .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/collections/{id}/books",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Collection id")
    ),
    request_body = AddBooksRequest,
    responses(
        (status = 204, description = "Books added"),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 409, description = "Conflict", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn add_books_to_collection(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(collection_id): Path<String>,
    Json(payload): Json<AddBooksRequest>,
) -> Result<StatusCode, AppError> {
    let book_ids = normalize_ids(payload.book_ids);
    if book_ids.is_empty() {
        return Err(AppError::BadRequest);
    }

    for book_id in &book_ids {
        let _ = load_book_or_not_found(
            &state.db,
            book_id,
            accessible_library_id(&auth_user.user),
            Some(auth_user.user.id.as_str()),
        )
        .await?;
    }

    let outcome = collection_queries::add_books_to_collection(
        &state.db,
        &collection_id,
        &auth_user.user.id,
        &book_ids,
    )
    .await
    .map_err(|_| AppError::Internal)?;
    if outcome.inserted == 0 {
        if !outcome.allowed {
            return Err(AppError::NotFound);
        }
        return Err(AppError::Conflict);
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    delete,
    path = "/api/v1/collections/{id}/books/{book_id}",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Collection id"),
        ("book_id" = String, Path, description = "Book id")
    ),
    responses(
        (status = 204, description = "Book removed"),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn remove_book_from_collection(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((collection_id, book_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let removed = collection_queries::remove_book_from_collection(
        &state.db,
        &collection_id,
        &auth_user.user.id,
        &book_id,
    )
    .await
    .map_err(|_| AppError::Internal)?;
    if !removed {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    get,
    path = "/api/v1/collections/{id}/search/chunks",
    tag = "collections",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Collection id"),
        CollectionChunkSearchQueryParams
    ),
    responses(
        (status = 200, description = "Collection chunk search results", body = crate::api::search::ChunkSearchResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn search_collection_chunks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(collection_id): Path<String>,
    Query(query): Query<CollectionChunkSearchQueryParams>,
) -> Result<Json<crate::api::search::ChunkSearchResponse>, AppError> {
    let query_text = query.q.unwrap_or_default().trim().to_string();
    if query_text.is_empty() {
        return Err(AppError::BadRequest);
    }

    let allowed_book_ids =
        collection_book_ids_for_search(&state, &auth_user.user.id, Some(collection_id.as_str()))
            .await?;
    let allowed_book_ids = allowed_book_ids.unwrap_or_default();
    let scoped_book_ids = {
        let requested_book_ids = normalize_ids(query.book_ids);
        if requested_book_ids.is_empty() {
            Some(allowed_book_ids)
        } else {
            let mut scoped = requested_book_ids;
            scoped.retain(|book_id| allowed_book_ids.iter().any(|allowed| allowed == book_id));
            Some(scoped)
        }
    };

    let limit = query.limit.unwrap_or(10).clamp(1, MAX_CHUNK_SEARCH_RESULTS) as usize;
    let rerank = matches!(
        query.rerank
            .as_deref()
            .map(|value| value.trim().to_ascii_lowercase()),
        Some(value) if matches!(value.as_str(), "true" | "1" | "yes" | "on")
    );
    let response = run_chunk_search(
        &state,
        &auth_user,
        query_text,
        scoped_book_ids,
        query
            .chunk_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string),
        limit,
        rerank,
    )
    .await?;

    Ok(Json(response))
}

pub(crate) async fn ensure_visible_collection(
    state: &AppState,
    user_id: &str,
    collection_id: &str,
) -> Result<(), AppError> {
    let Some(collection) = collection_queries::get_collection_access(&state.db, collection_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    if collection.owner_id != user_id && !collection.is_public {
        return Err(AppError::NotFound);
    }

    Ok(())
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
