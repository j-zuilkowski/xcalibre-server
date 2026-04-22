use crate::{
    db::queries::{auth as auth_queries, libraries as library_queries},
    middleware::auth::AuthenticatedUser,
    AppError, AppState,
};
use axum::{
    extract::{Extension, Json, State},
    middleware,
    routing::{get, patch},
    Router,
};
use serde::Deserialize;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/libraries", get(list_libraries))
        .route("/api/v1/users/me/library", patch(update_default_library))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize)]
struct UpdateLibraryRequest {
    library_id: String,
}

#[derive(Debug, serde::Serialize)]
struct LibraryResponse {
    id: String,
    name: String,
    calibre_db_path: String,
    book_count: i64,
    created_at: String,
    updated_at: String,
}

async fn list_libraries(
    State(state): State<AppState>,
) -> Result<Json<Vec<LibraryResponse>>, AppError> {
    let libraries = library_queries::list_libraries(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    let mut response = Vec::with_capacity(libraries.len());
    for library in libraries {
        let book_count = library_queries::count_books_in_library(&state.db, &library.id)
            .await
            .map_err(|_| AppError::Internal)?;
        response.push(LibraryResponse {
            id: library.id,
            name: library.name,
            calibre_db_path: library.calibre_db_path,
            book_count,
            created_at: library.created_at,
            updated_at: library.updated_at,
        });
    }
    Ok(Json(response))
}

async fn update_default_library(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<UpdateLibraryRequest>,
) -> Result<Json<crate::db::models::User>, AppError> {
    let library_id = payload.library_id.trim();
    if library_id.is_empty() {
        return Err(AppError::BadRequest);
    }

    let Some(_) = library_queries::get_library(&state.db, library_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let Some(user) =
        auth_queries::set_user_default_library(&state.db, &auth_user.user.id, library_id)
            .await
            .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    Ok(Json(user))
}
