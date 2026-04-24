use crate::{
    db::queries::{authors as author_queries, books as book_queries},
    ingest::mobi_util,
    middleware::auth::AuthenticatedUser,
    AppError, AppState,
};
use axum::{
    body::Body,
    extract::{Extension, Multipart, Path, Query, State},
    http::{header, HeaderMap, HeaderValue},
    middleware,
    routing::{get, post},
    Json, Router,
};
use bytes::Bytes;
use image::{
    codecs::{jpeg::JpegEncoder, webp::WebPEncoder},
    imageops::FilterType,
    DynamicImage, GenericImageView, ImageEncoder,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::io::Cursor;
use utoipa::{IntoParams, ToSchema};

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    let require_admin_layer = middleware::from_extractor::<crate::middleware::auth::RequireAdmin>();

    let public_routes = Router::new()
        .route(
            "/api/v1/authors/:id/photo",
            post(upload_author_photo).get(get_author_photo),
        )
        .route("/api/v1/authors/:id", get(get_author).patch(patch_author));

    let admin_routes = Router::new()
        .route("/api/v1/admin/authors", get(list_admin_authors))
        .route("/api/v1/admin/authors/:id/merge", post(merge_author))
        .route_layer(require_admin_layer);

    Router::new()
        .merge(public_routes)
        .merge(admin_routes)
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

#[derive(Debug, Deserialize, IntoParams, ToSchema)]
pub(crate) struct AuthorPhotoQuery {
    #[serde(default)]
    size: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, ToSchema)]
struct AuthorPhotoUploadRequestDoc {
    #[schema(value_type = String, format = Binary)]
    photo: String,
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
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(24).clamp(1, 100);
    Ok(Json(
        load_author_detail(&state, &auth_user, &author_id, page, page_size).await?,
    ))
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
        return Err(AppError::Forbidden("forbidden".into()));
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
    Ok(Json(
        load_author_detail(&state, &auth_user, &author.id, 1, 24).await?,
    ))
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
    Query(query): Query<ListAuthorsQuery>,
) -> Result<Json<PaginatedResponse<crate::db::queries::authors::AdminAuthor>>, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let (items, total, page, page_size) =
        author_queries::list_admin_authors(&state.db, query.q.as_deref(), page, page_size)
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
    Path(source_id): Path<String>,
    Json(payload): Json<MergeAuthorRequest>,
) -> Result<Json<MergeAuthorResponse>, AppError> {
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

#[utoipa::path(
    post,
    path = "/api/v1/authors/{id}/photo",
    tag = "authors",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Author id")
    ),
    request_body(
        content = AuthorPhotoUploadRequestDoc,
        content_type = "multipart/form-data"
    ),
    responses(
        (status = 200, description = "Updated author detail", body = crate::db::queries::authors::AuthorDetail),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 413, description = "Payload too large", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn upload_author_photo(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(author_id): Path<String>,
    multipart: Multipart,
) -> Result<Json<crate::db::queries::authors::AuthorDetail>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_edit {
        return Err(AppError::Forbidden("forbidden".into()));
    }

    let author_id = author_id.trim().to_string();
    if author_id.is_empty() {
        return Err(AppError::BadRequest);
    }

    let Some((_author, _)) = author_queries::get_author_by_id(&state.db, &author_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let uploaded =
        parse_author_photo_upload(multipart, state.config.limits.upload_max_bytes).await?;
    let Some(variants) = render_author_photo_variants(&uploaded.bytes) else {
        return Err(AppError::Unprocessable);
    };

    let Some(bucket) = author_bucket(&author_id) else {
        return Err(AppError::BadRequest);
    };
    let photo_relative_path = format!("authors/{bucket}/{author_id}.jpg");
    let thumb_relative_path = format!("authors/{bucket}/{author_id}.thumb.jpg");
    let photo_webp_relative_path = format!("authors/{bucket}/{author_id}.webp");
    let thumb_webp_relative_path = format!("authors/{bucket}/{author_id}.thumb.webp");
    let generated_paths = [
        photo_relative_path.as_str(),
        thumb_relative_path.as_str(),
        photo_webp_relative_path.as_str(),
        thumb_webp_relative_path.as_str(),
    ];

    state
        .storage
        .put(&photo_relative_path, Bytes::from(variants.photo_jpg))
        .await
        .map_err(|_| AppError::Internal)?;
    if let Err(err) = state
        .storage
        .put(&thumb_relative_path, Bytes::from(variants.thumb_jpg))
        .await
    {
        delete_storage_paths(&state, &generated_paths).await;
        tracing::error!(error = %err, author_id = %author_id, "failed to store author photo thumbnail");
        return Err(AppError::Internal);
    }
    if let Err(err) = state
        .storage
        .put(&photo_webp_relative_path, Bytes::from(variants.photo_webp))
        .await
    {
        delete_storage_paths(&state, &generated_paths).await;
        tracing::error!(error = %err, author_id = %author_id, "failed to store author photo webp");
        return Err(AppError::Internal);
    }
    if let Err(err) = state
        .storage
        .put(&thumb_webp_relative_path, Bytes::from(variants.thumb_webp))
        .await
    {
        delete_storage_paths(&state, &generated_paths).await;
        tracing::error!(error = %err, author_id = %author_id, "failed to store author photo webp thumbnail");
        return Err(AppError::Internal);
    }

    if let Err(err) =
        author_queries::set_author_photo_path(&state.db, &author_id, &photo_relative_path).await
    {
        delete_storage_paths(&state, &generated_paths).await;
        tracing::error!(error = %err, author_id = %author_id, "failed to persist author photo path");
        return Err(AppError::Internal);
    }

    let detail = load_author_detail(&state, &auth_user, &author_id, 1, 24).await?;
    Ok(Json(detail))
}

#[utoipa::path(
    get,
    path = "/api/v1/authors/{id}/photo",
    tag = "authors",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Author id"),
        AuthorPhotoQuery
    ),
    responses(
        (status = 200, description = "Author photo or placeholder", content_type = "image/*", body = String),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn get_author_photo(
    State(state): State<AppState>,
    Path(author_id): Path<String>,
    Query(query): Query<AuthorPhotoQuery>,
    headers: HeaderMap,
) -> Result<axum::response::Response, AppError> {
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

    let Some(profile) = profile else {
        return Ok(placeholder_author_photo_response(&author.name));
    };

    if profile.photo_url.is_none() {
        return Ok(placeholder_author_photo_response(&author.name));
    }

    let Some(photo_path) = photo_path_from_author_id(&author_id) else {
        return Err(AppError::BadRequest);
    };

    let wants_thumb = match query.size.as_deref() {
        None => false,
        Some(size) if size.eq_ignore_ascii_case("thumb") => true,
        Some(_) => return Err(AppError::BadRequest),
    };
    let wants_webp = headers
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase().contains("image/webp"))
        .unwrap_or(false);

    let selected_path =
        select_author_photo_variant(&state, &photo_path, wants_thumb, wants_webp).await?;
    let bytes = state
        .storage
        .get_bytes(&selected_path)
        .await
        .map_err(|_| AppError::NotFound)?;

    let content_type = if selected_path.ends_with(".webp") {
        "image/webp"
    } else {
        "image/jpeg"
    };

    let mut response = axum::response::Response::new(Body::from(bytes));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    Ok(response)
}

async fn load_author_detail(
    state: &AppState,
    auth_user: &AuthenticatedUser,
    author_id: &str,
    page: i64,
    page_size: i64,
) -> Result<crate::db::queries::authors::AuthorDetail, AppError> {
    let Some((author, profile)) = author_queries::get_author_by_id(&state.db, author_id)
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

    Ok(crate::db::queries::authors::AuthorDetail {
        id: author.id,
        name: author.name,
        sort_name: author.sort_name,
        profile,
        book_count: books.total,
        books: books.items,
        page: books.page,
        page_size: books.page_size,
    })
}

async fn parse_author_photo_upload(
    mut multipart: Multipart,
    max_bytes: u64,
) -> Result<UploadedPhoto, AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest)?
    {
        let Some(name) = field.name() else {
            continue;
        };
        if name != "photo" {
            continue;
        }

        let field_bytes = field.bytes().await.map_err(|_| AppError::BadRequest)?;
        if field_bytes.len() as u64 > max_bytes {
            return Err(AppError::PayloadTooLarge);
        }
        if field_bytes.is_empty() {
            return Err(AppError::Unprocessable);
        }

        return Ok(UploadedPhoto {
            bytes: field_bytes.to_vec(),
        });
    }

    Err(AppError::BadRequest)
}

async fn select_author_photo_variant(
    state: &AppState,
    full_photo_path: &str,
    wants_thumb: bool,
    wants_webp: bool,
) -> Result<String, AppError> {
    let jpg_path = if wants_thumb {
        author_thumb_path(full_photo_path)
    } else {
        full_photo_path.to_string()
    };
    if wants_webp {
        let webp_path = if wants_thumb {
            author_thumb_webp_path(full_photo_path)
        } else {
            author_webp_path(full_photo_path)
        };
        if storage_path_exists(state, &webp_path).await {
            return Ok(webp_path);
        }
    }

    Ok(jpg_path)
}

async fn storage_path_exists(state: &AppState, relative_path: &str) -> bool {
    state
        .storage
        .get_range(relative_path, Some((0, 0)))
        .await
        .is_ok()
}

fn author_bucket(author_id: &str) -> Option<String> {
    let bucket: String = author_id.chars().take(2).collect();
    if bucket.len() == 2 {
        Some(bucket)
    } else {
        None
    }
}

fn photo_path_from_author_id(author_id: &str) -> Option<String> {
    let bucket = author_bucket(author_id)?;
    Some(format!("authors/{bucket}/{author_id}.jpg"))
}

fn author_thumb_path(full_photo_path: &str) -> String {
    full_photo_path
        .strip_suffix(".jpg")
        .map(|prefix| format!("{prefix}.thumb.jpg"))
        .unwrap_or_else(|| format!("{full_photo_path}.thumb.jpg"))
}

fn author_webp_path(full_photo_path: &str) -> String {
    full_photo_path
        .strip_suffix(".jpg")
        .map(|prefix| format!("{prefix}.webp"))
        .unwrap_or_else(|| format!("{full_photo_path}.webp"))
}

fn author_thumb_webp_path(full_photo_path: &str) -> String {
    full_photo_path
        .strip_suffix(".jpg")
        .map(|prefix| format!("{prefix}.thumb.webp"))
        .unwrap_or_else(|| format!("{full_photo_path}.thumb.webp"))
}

async fn delete_storage_paths(state: &AppState, paths: &[&str]) {
    for path in paths {
        let _ = state.storage.delete(path).await;
    }
}

fn placeholder_author_photo_response(author_name: &str) -> axum::response::Response {
    let svg = author_placeholder_svg(author_name);
    let mut response = axum::response::Response::new(Body::from(svg));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("image/svg+xml"),
    );
    response
}

fn author_placeholder_svg(author_name: &str) -> String {
    const COLORS: [&str; 8] = [
        "#27272a", "#3f3f46", "#52525b", "#1f2937", "#134e4a", "#0f766e", "#155e75", "#374151",
    ];

    let trimmed = author_name.trim();
    let first_letter = trimmed
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    let color = COLORS[hash_author_name(trimmed.as_bytes()) % COLORS.len()];

    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="400" height="400" viewBox="0 0 400 400" role="img" aria-label="{} placeholder portrait">
  <rect width="400" height="400" fill="{}"/>
  <text x="200" y="210" text-anchor="middle" dominant-baseline="middle" fill="#f4f4f5" font-family="Georgia, 'Times New Roman', serif" font-size="180" font-weight="700">{}</text>
</svg>"##,
        mobi_util::xml_escape(trimmed),
        color,
        mobi_util::xml_escape(&first_letter),
    )
}

fn hash_author_name(bytes: &[u8]) -> usize {
    let mut hash: u64 = 0;
    for byte in bytes {
        hash = hash.wrapping_mul(31).wrapping_add(u64::from(*byte));
    }
    hash as usize
}

fn render_author_photo_variants(raw_photo: &[u8]) -> Option<AuthorPhotoVariants> {
    let image = image::load_from_memory(raw_photo).ok()?;
    if image.width() == 0 || image.height() == 0 {
        return None;
    }

    let square = center_crop_square(image);
    let full = square.resize_exact(400, 400, FilterType::Lanczos3);
    let thumb = square.resize_exact(100, 100, FilterType::Lanczos3);

    Some(AuthorPhotoVariants {
        photo_jpg: encode_jpeg(&full)?,
        thumb_jpg: encode_jpeg(&thumb)?,
        photo_webp: encode_webp(&full)?,
        thumb_webp: encode_webp(&thumb)?,
    })
}

fn center_crop_square(image: DynamicImage) -> DynamicImage {
    let (width, height) = image.dimensions();
    let size = width.min(height);
    let left = (width - size) / 2;
    let top = (height - size) / 2;
    image.crop_imm(left, top, size, size)
}

fn encode_jpeg(image: &DynamicImage) -> Option<Vec<u8>> {
    let mut output = Cursor::new(Vec::new());
    let mut encoder = JpegEncoder::new_with_quality(&mut output, 85);
    encoder.encode_image(image).ok()?;
    Some(output.into_inner())
}

fn encode_webp(image: &DynamicImage) -> Option<Vec<u8>> {
    let rgba = image.to_rgba8();
    let mut output = Cursor::new(Vec::new());
    WebPEncoder::new_lossless(&mut output)
        .write_image(
            rgba.as_raw(),
            rgba.width(),
            rgba.height(),
            image::ExtendedColorType::Rgba8,
        )
        .ok()?;
    Some(output.into_inner())
}

#[derive(Debug)]
struct UploadedPhoto {
    bytes: Vec<u8>,
}

#[derive(Debug)]
struct AuthorPhotoVariants {
    photo_jpg: Vec<u8>,
    thumb_jpg: Vec<u8>,
    photo_webp: Vec<u8>,
    thumb_webp: Vec<u8>,
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
