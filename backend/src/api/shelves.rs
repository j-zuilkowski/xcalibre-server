use crate::{
    api::books::accessible_library_id, db::queries::shelves as shelf_queries,
    middleware::auth::AuthenticatedUser, AppError, AppState,
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    middleware,
    routing::{delete, get},
    Json, Router,
};
use serde::Deserialize;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/shelves", get(list_shelves).post(create_shelf))
        .route(
            "/api/v1/shelves/:id/books",
            get(list_shelf_books).post(add_book_to_shelf),
        )
        .route(
            "/api/v1/shelves/:id/books/:book_id",
            delete(remove_book_from_shelf),
        )
        .route("/api/v1/shelves/:id", delete(delete_shelf))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, Default)]
struct ListQuery {
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct CreateShelfRequest {
    name: String,
    #[serde(default)]
    is_public: bool,
}

#[derive(Debug, Deserialize)]
struct AddBookRequest {
    book_id: String,
}

#[derive(Debug, serde::Serialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
    total: i64,
    page: i64,
    page_size: i64,
}

async fn list_shelves(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<crate::db::models::Shelf>>, AppError> {
    let shelves = shelf_queries::list_shelves(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(shelves))
}

async fn create_shelf(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateShelfRequest>,
) -> Result<(StatusCode, Json<crate::db::models::Shelf>), AppError> {
    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest);
    }

    let shelf = shelf_queries::create_shelf(&state.db, &auth_user.user.id, name, payload.is_public)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok((StatusCode::CREATED, Json(shelf)))
}

async fn delete_shelf(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(shelf_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_owner(&state, &auth_user.user.id, &shelf_id).await?;
    let deleted = shelf_queries::delete_shelf(&state.db, &shelf_id, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn add_book_to_shelf(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(shelf_id): Path<String>,
    Json(payload): Json<AddBookRequest>,
) -> Result<StatusCode, AppError> {
    ensure_owner(&state, &auth_user.user.id, &shelf_id).await?;
    let _ = crate::api::books::load_book_or_not_found(
        &state.db,
        &payload.book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let added = shelf_queries::add_book_to_shelf(&state.db, &shelf_id, &payload.book_id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !added {
        return Err(AppError::Conflict);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_book_from_shelf(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((shelf_id, book_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    ensure_owner(&state, &auth_user.user.id, &shelf_id).await?;
    let removed = shelf_queries::remove_book_from_shelf(&state.db, &shelf_id, &book_id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !removed {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn list_shelf_books(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(shelf_id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<PaginatedResponse<crate::db::queries::books::BookSummary>>, AppError> {
    ensure_visible(&state, &auth_user.user.id, &shelf_id).await?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(30).clamp(1, 100);
    let result = shelf_queries::list_shelf_books(
        &state.db,
        &shelf_id,
        page,
        page_size,
        Some(auth_user.user.default_library_id.as_str()),
        Some(auth_user.user.id.as_str()),
    )
    .await
    .map_err(|_| AppError::Internal)?;
    Ok(Json(PaginatedResponse {
        items: result.items,
        total: result.total,
        page: result.page,
        page_size: result.page_size,
    }))
}

async fn ensure_owner(state: &AppState, user_id: &str, shelf_id: &str) -> Result<(), AppError> {
    let Some(shelf) = shelf_queries::get_shelf(&state.db, shelf_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    if shelf.user_id != user_id {
        return Err(AppError::Forbidden);
    }

    Ok(())
}

async fn ensure_visible(state: &AppState, user_id: &str, shelf_id: &str) -> Result<(), AppError> {
    let Some(shelf) = shelf_queries::get_shelf(&state.db, shelf_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    if shelf.user_id != user_id && !shelf.is_public {
        return Err(AppError::NotFound);
    }

    Ok(())
}
