use crate::{
    db::queries::{authors as author_queries, books as book_queries},
    middleware::auth::AuthenticatedUser,
    AppError, AppState,
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::header,
    middleware,
    response::Response,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use utoipa::{IntoParams, ToSchema};

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/authors/:id", get(get_author).patch(patch_author))
        .route("/api/v1/admin/authors", get(list_admin_authors))
        .route("/api/v1/admin/authors/:id/merge", post(merge_author))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, Default, IntoParams)]
pub(crate) struct AuthorBooksQuery {
    page: Option<i64>,
    page_size: Option<i64>,
}

#[derive(Debug, Deserialize, Default, IntoParams)]
pub(crate) struct ListAuthorsQuery {
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, Default, ToSchema)]
pub(crate) struct PatchAuthorRequest {
    #[serde(default)]
    bio: Option<Value>,
    #[serde(default)]
    born: Option<Value>,
    #[serde(default)]
    died: Option<Value>,
    #[serde(default)]
    website_url: Option<Value>,
    #[serde(default)]
    openlibrary_id: Option<Value>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct MergeAuthorRequest {
    into_author_id: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct PaginatedResponse<T> {
    items: Vec<T>,
    total: i64,
    page: i64,
    page_size: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct MergeAuthorResponse {
    books_updated: usize,
    target_author: crate::db::models::AuthorRef,
}

#[utoipa::path(
    get,
    path = "/api/v1/authors/{id}",
    tag = "authors",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Author id"),
        AuthorBooksQuery
    ),
    responses(
        (status = 200, description = "Author detail", body = crate::db::queries::authors::AuthorDetail),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn get_author(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(author_id): Path<String>,
    Query(query): Query<AuthorBooksQuery>,
) -> Result<Json<crate::db::queries::authors::AuthorDetail>, AppError> {
    let author_id = author_id.trim().to_string();
    if author_id.is_empty() {
        return Err(AppError::BadRequest);
    }

    let Some((author, profile)) = author_queries::get_author_by_id(&state.db, &author_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(24).clamp(1, 100);
    let books = book_queries::list_books(
        &state.db,
        &book_queries::ListBooksParams {
            author_id: Some(author.id.clone()),
            library_id: Some(auth_user.user.default_library_id.clone()),
            page,
            page_size,
            sort: Some("pubdate".to_string()),
            order: Some("desc".to_string()),
            user_id: Some(auth_user.user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(crate::db::queries::authors::AuthorDetail {
        id: author.id,
        name: author.name,
        sort_name: author.sort_name,
        profile,
        book_count: books.total,
        books: books.items,
        page: books.page,
        page_size: books.page_size,
    }))
}

#[utoipa::path(
    patch,
    path = "/api/v1/authors/{id}",
    tag = "authors",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Author id")
    ),
    request_body = PatchAuthorRequest,
    responses(
        (status = 200, description = "Updated author detail", body = crate::db::queries::authors::AuthorDetail),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn patch_author(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(author_id): Path<String>,
    Json(payload): Json<Value>,
) -> Result<Json<crate::db::queries::authors::AuthorDetail>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_edit {
        return Err(AppError::Forbidden);
    }

    let author_id = author_id.trim().to_string();
    if author_id.is_empty() {
        return Err(AppError::BadRequest);
    }

    let Some((author, _)) = author_queries::get_author_by_id(&state.db, &author_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let patch = author_queries::AuthorProfilePatchInput {
        bio: normalize_patch_value(&payload, "bio")?,
        born: normalize_patch_value(&payload, "born")?,
        died: normalize_patch_value(&payload, "died")?,
        website_url: normalize_patch_value(&payload, "website_url")?,
        openlibrary_id: normalize_patch_value(&payload, "openlibrary_id")?,
    };

    author_queries::upsert_author_profile(&state.db, &author.id, &patch)
        .await
        .map_err(|_| AppError::Internal)?;

    let Some((author, profile)) = author_queries::get_author_by_id(&state.db, &author.id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };
    let books = book_queries::list_books(
        &state.db,
        &book_queries::ListBooksParams {
            author_id: Some(author.id.clone()),
            library_id: Some(auth_user.user.default_library_id.clone()),
            page: 1,
            page_size: 24,
            sort: Some("pubdate".to_string()),
            order: Some("desc".to_string()),
            user_id: Some(auth_user.user.id.clone()),
            ..Default::default()
        },
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(crate::db::queries::authors::AuthorDetail {
        id: author.id,
        name: author.name,
        sort_name: author.sort_name,
        profile,
        book_count: books.total,
        books: books.items,
        page: books.page,
        page_size: books.page_size,
    }))
}

#[utoipa::path(
    get,
    path = "/api/v1/admin/authors",
    tag = "authors",
    security(("bearer_auth" = [])),
    params(ListAuthorsQuery),
    responses(
        (status = 200, description = "Paginated author list", body = PaginatedResponse<crate::db::queries::authors::AdminAuthor>),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn list_admin_authors(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Query(query): Query<ListAuthorsQuery>,
) -> Result<Json<PaginatedResponse<crate::db::queries::authors::AdminAuthor>>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.is_admin() {
        return Err(AppError::Forbidden);
    }

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let (items, total, page, page_size) = author_queries::list_admin_authors(
        &state.db,
        query.q.as_deref(),
        page,
        page_size,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(PaginatedResponse {
        items,
        total,
        page,
        page_size,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/admin/authors/{id}/merge",
    tag = "authors",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Source author id")
    ),
    request_body = MergeAuthorRequest,
    responses(
        (status = 200, description = "Merge result", body = MergeAuthorResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn merge_author(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(source_id): Path<String>,
    Json(payload): Json<MergeAuthorRequest>,
) -> Result<Json<MergeAuthorResponse>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.is_admin() {
        return Err(AppError::Forbidden);
    }

    let source_id = source_id.trim().to_string();
    let target_id = payload.into_author_id.trim().to_string();
    if source_id.is_empty() || target_id.is_empty() {
        return Err(AppError::BadRequest);
    }
    if source_id == target_id {
        return Err(AppError::BadRequest);
    }

    let result = author_queries::merge_authors(&state.db, &source_id, &target_id)
        .await
        .map_err(|_| AppError::Internal)?;
    let Some(result) = result else {
        return Err(AppError::NotFound);
    };

    Ok(Json(MergeAuthorResponse {
        books_updated: result.books_updated,
        target_author: result.target_author,
    }))
}

pub async fn serve_author_photo(
    State(state): State<AppState>,
    Path((bucket, filename)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let bucket = bucket.trim();
    let filename = filename.trim();
    if bucket.len() != 2 || filename.is_empty() {
        return Err(AppError::BadRequest);
    }

    let relative_path = format!("authors/{bucket}/{filename}");
    let bytes = state
        .storage
        .get_bytes(&relative_path)
        .await
        .map_err(|_| AppError::NotFound)?;
    let content_type = mime_guess::from_path(&relative_path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();

    let mut response = Response::new(axum::body::Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        content_type.parse().map_err(|_| AppError::Internal)?,
    );
    Ok(response)
}

fn normalize_profile_value(value: String) -> Option<String> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn normalize_patch_value(payload: &Value, field: &str) -> Result<Option<Option<String>>, AppError> {
    let Some(object) = payload.as_object() else {
        return Err(AppError::BadRequest);
    };
    let Some(value) = object.get(field) else {
        return Ok(None);
    };

    match value {
        Value::Null => Ok(Some(None)),
        Value::String(value) => Ok(Some(normalize_profile_value(value.clone()))),
        _ => Err(AppError::BadRequest),
    }
}
