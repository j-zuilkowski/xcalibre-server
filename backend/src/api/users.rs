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
use chrono::Utc;
use serde::Deserialize;
use utoipa::ToSchema;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/users/me", get(me).patch(patch_me))
        .route("/api/v1/libraries", get(list_libraries))
        .route("/api/v1/users/me/library", patch(update_default_library))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize)]
struct UpdateLibraryRequest {
    library_id: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct PatchMeRequest {
    #[serde(default)]
    username: Option<String>,
    #[serde(default)]
    email: Option<String>,
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

#[utoipa::path(
    get,
    path = "/api/v1/users/me",
    tag = "users",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Current user profile", body = crate::db::models::User),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn me(
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<crate::db::models::User>, AppError> {
    Ok(Json(auth_user.user))
}

#[utoipa::path(
    patch,
    path = "/api/v1/users/me",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = PatchMeRequest,
    responses(
        (status = 200, description = "Updated user profile", body = crate::db::models::User),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn patch_me(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<PatchMeRequest>,
) -> Result<Json<crate::db::models::User>, AppError> {
    let username = payload.username.map(|value| value.trim().to_string());
    let email = payload.email.map(|value| value.trim().to_string());

    if username.as_deref().is_some_and(str::is_empty) || email.as_deref().is_some_and(str::is_empty)
    {
        return Err(AppError::BadRequest);
    }

    if username.is_none() && email.is_none() {
        let user = auth_queries::find_user_by_id(&state.db, &auth_user.user.id)
            .await
            .map_err(|_| AppError::Internal)?
            .ok_or(AppError::NotFound)?;
        return Ok(Json(user));
    }

    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE users
        SET
            username = COALESCE(?, username),
            email = COALESCE(?, email),
            last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(username)
    .bind(email)
    .bind(now)
    .bind(&auth_user.user.id)
    .execute(&state.db)
    .await;

    match result {
        Ok(_) => {
            let user = auth_queries::find_user_by_id(&state.db, &auth_user.user.id)
                .await
                .map_err(|_| AppError::Internal)?
                .ok_or(AppError::NotFound)?;
            Ok(Json(user))
        }
        Err(err) => {
            if err.to_string().to_lowercase().contains("unique") {
                Err(AppError::Conflict)
            } else {
                Err(AppError::Internal)
            }
        }
    }
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
