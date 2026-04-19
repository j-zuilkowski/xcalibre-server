use crate::{
    db::queries::books as book_queries,
    middleware::auth::AuthenticatedUser,
    AppError, AppState,
};
use axum::{
    body::Body,
    extract::{Extension, Multipart, Path, Query, Request, State},
    http::{header, HeaderValue, StatusCode},
    middleware,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Seek},
    path::{Component, Path as FsPath, PathBuf},
};
use tower::ServiceExt;
use tower_http::services::ServeFile;
use uuid::Uuid;
use zip::ZipArchive;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/books", get(list_books).post(upload_book))
        .route(
            "/api/v1/books/:id",
            get(get_book).patch(patch_book).delete(delete_book),
        )
        .route("/api/v1/books/:id/cover", get(get_cover))
        .route("/api/v1/books/:id/formats/:format/download", get(download_format))
        .route("/api/v1/books/:id/formats/:format/stream", get(stream_format))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, Default)]
struct ListBooksQuery {
    q: Option<String>,
    author_id: Option<String>,
    series_id: Option<String>,
    tag: Option<SingleOrMany>,
    language: Option<String>,
    format: Option<String>,
    sort: Option<String>,
    order: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
    since: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum SingleOrMany {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Serialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
    total: i64,
    page: i64,
    page_size: i64,
}

#[derive(Debug, Deserialize)]
struct PatchBookRequest {
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

#[derive(Debug, Clone, Deserialize)]
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

async fn list_books(
    State(state): State<AppState>,
    Query(query): Query<ListBooksQuery>,
) -> Result<Json<PaginatedResponse<book_queries::BookSummary>>, AppError> {
    let params = book_queries::ListBooksParams {
        q: query.q,
        author_id: query.author_id,
        series_id: query.series_id,
        tags: parse_tag_query(query.tag),
        language: query.language,
        format: query.format,
        sort: query.sort,
        order: query.order,
        page: query.page.unwrap_or(1),
        page_size: query.page_size.unwrap_or(30),
        since: query.since,
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

async fn get_book(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
) -> Result<Json<crate::db::models::Book>, AppError> {
    let Some(book) = book_queries::get_book_by_id(&state.db, &book_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    Ok(Json(book))
}

async fn upload_book(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    multipart: Multipart,
) -> Result<(StatusCode, Json<crate::db::models::Book>), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_upload {
        return Err(AppError::Forbidden);
    }

    let parsed_upload = parse_upload_multipart(multipart, state.config.limits.upload_max_bytes).await?;
    let detected_format = detect_upload_format(&parsed_upload.bytes).ok_or(AppError::Unprocessable)?;
    validate_extension_matches(&parsed_upload.file_name, detected_format)?;
    let extracted_cover_source = if detected_format == UploadFormat::Epub {
        extract_epub_cover_source(&parsed_upload.bytes)
    } else {
        None
    };

    let mut ingest = extract_metadata(detected_format, &parsed_upload.file_name, &parsed_upload.bytes)?;
    ingest = apply_metadata_override(ingest, parsed_upload.metadata)?;

    let title = ingest
        .title
        .clone()
        .filter(|t| !t.trim().is_empty())
        .ok_or(AppError::Unprocessable)?;

    if let Some(rating) = ingest.rating {
        if !(0..=10).contains(&rating) {
            return Err(AppError::Unprocessable);
        }
    }

    if book_queries::has_duplicate_isbn(&state.db, &ingest.identifiers, None)
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
        .put(&relative_path, &parsed_upload.bytes)
        .map_err(|_| AppError::Internal)?;

    let insert_result = book_queries::insert_uploaded_book(
        &state.db,
        book_queries::UploadBookInput {
            title: title.clone(),
            sort_title: ingest.sort_title.clone().unwrap_or_else(|| title.clone()),
            description: ingest.description,
            pubdate: ingest.pubdate,
            language: ingest.language,
            rating: ingest.rating,
            series_id: ingest.series_id,
            series_index: ingest.series_index,
            author_names: if ingest.authors.is_empty() {
                vec!["Unknown Author".to_string()]
            } else {
                ingest.authors
            },
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
            let _ = state.storage.delete(&relative_path);
            return Err(AppError::Internal);
        }
    };

    if let Some(raw_cover_bytes) = extracted_cover_source {
        if let Some((cover_jpg, thumb_jpg)) = render_cover_variants(&raw_cover_bytes) {
            let bucket = &book.id[..2];
            let cover_relative_path = format!("covers/{bucket}/{}.jpg", book.id);
            let thumb_relative_path = format!("covers/{bucket}/{}.thumb.jpg", book.id);

            state
                .storage
                .put(&cover_relative_path, &cover_jpg)
                .map_err(|_| AppError::Internal)?;
            state
                .storage
                .put(&thumb_relative_path, &thumb_jpg)
                .map_err(|_| AppError::Internal)?;

            if let Err(err) = book_queries::set_book_cover_path(&state.db, &book.id, &cover_relative_path).await {
                let _ = state.storage.delete(&cover_relative_path);
                let _ = state.storage.delete(&thumb_relative_path);
                tracing::error!("failed to persist cover path for book {}: {err:#}", book.id);
                return Err(AppError::Internal);
            }

            if let Some(updated_book) = book_queries::get_book_by_id(&state.db, &book.id)
                .await
                .map_err(|_| AppError::Internal)?
            {
                book = updated_book;
            }
        }
    }

    Ok((StatusCode::CREATED, Json(book)))
}

async fn patch_book(
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
        return Err(AppError::Forbidden);
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

    let result = book_queries::patch_book_with_audit(&state.db, &book_id, &auth_user.user.id, patch).await;
    match result {
        Ok(Some(book)) => Ok(Json(book)),
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

async fn delete_book(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.is_admin() {
        return Err(AppError::Forbidden);
    }

    let Some(paths) = book_queries::delete_book_and_collect_paths(&state.db, &book_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    for path in paths {
        state.storage.delete(&path).map_err(|_| AppError::Internal)?;
    }

    Ok(Json(serde_json::json!({ "success": true })))
}

async fn download_format(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((book_id, format)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<axum::response::Response, AppError> {
    ensure_download_permission(&state, &auth_user.user.id).await?;

    let format_file = book_queries::find_format_file(&state.db, &book_id, &format)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let full_path = canonicalize_storage_file_path(&state, &format_file.path)?;
    let mut response = serve_file(request, full_path).await?;

    let file_name = format!("{}.{}", book_id, format_file.format.to_lowercase());
    let disposition = HeaderValue::from_str(&format!("attachment; filename=\"{file_name}\""))
        .map_err(|_| AppError::Internal)?;
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, disposition);

    Ok(response)
}

async fn stream_format(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((book_id, format)): Path<(String, String)>,
    request: Request<Body>,
) -> Result<axum::response::Response, AppError> {
    ensure_download_permission(&state, &auth_user.user.id).await?;

    let format_file = book_queries::find_format_file(&state.db, &book_id, &format)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let full_path = canonicalize_storage_file_path(&state, &format_file.path)?;
    serve_file(request, full_path).await
}

async fn get_cover(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
    request: Request<Body>,
) -> Result<axum::response::Response, AppError> {
    let cover_path = book_queries::find_book_cover_path(&state.db, &book_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let full_path = canonicalize_storage_file_path(&state, &cover_path)?;
    serve_file(request, full_path).await
}

async fn ensure_download_permission(state: &AppState, user_id: &str) -> Result<(), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_download {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

fn sanitize_relative_path(relative_path: &str) -> Result<PathBuf, AppError> {
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

fn canonicalize_storage_file_path(state: &AppState, relative_path: &str) -> Result<PathBuf, AppError> {
    let clean = sanitize_relative_path(relative_path)?;
    let storage_root = PathBuf::from(&state.config.app.storage_path);
    std::fs::create_dir_all(&storage_root).map_err(|_| AppError::Internal)?;
    let canonical_root = std::fs::canonicalize(&storage_root).map_err(|_| AppError::Internal)?;

    let joined = canonical_root.join(clean);
    let canonical_target = std::fs::canonicalize(&joined).map_err(|_| AppError::NotFound)?;
    if !canonical_target.starts_with(&canonical_root) {
        return Err(AppError::BadRequest);
    }

    Ok(canonical_target)
}

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

async fn parse_upload_multipart(
    mut multipart: Multipart,
    max_bytes: u64,
) -> Result<ParsedUpload, AppError> {
    let mut file_name: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;
    let mut metadata = UploadMetadata::default();

    while let Some(field) = multipart.next_field().await.map_err(|_| AppError::BadRequest)? {
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
                file_name = Some(field_file_name);
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

fn detect_upload_format(bytes: &[u8]) -> Option<UploadFormat> {
    if bytes.starts_with(b"%PDF") {
        return Some(UploadFormat::Pdf);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return Some(UploadFormat::Epub);
    }
    if bytes.windows(b"BOOKMOBI".len()).any(|window| window == b"BOOKMOBI")
        || bytes.windows(b"PalmDOC".len()).any(|window| window == b"PalmDOC")
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

fn validate_extension_matches(file_name: &str, detected: UploadFormat) -> Result<(), AppError> {
    if let Some(by_extension) = extension_format(file_name) {
        if by_extension != detected {
            return Err(AppError::Unprocessable);
        }
    }
    Ok(())
}

fn parse_tag_query(raw_tags: Option<SingleOrMany>) -> Vec<String> {
    let tags = match raw_tags {
        Some(SingleOrMany::One(tag)) => vec![tag],
        Some(SingleOrMany::Many(tags)) => tags,
        None => Vec::new(),
    };

    tags
        .into_iter()
        .flat_map(|tag| tag.split(',').map(ToOwned::to_owned).collect::<Vec<_>>())
        .map(trim_owned)
        .filter(|tag| !tag.is_empty())
        .collect()
}

fn trim_owned(value: String) -> String {
    value.trim().to_string()
}

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

fn parse_epub_metadata(bytes: &[u8]) -> Option<IngestMetadata> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).ok()?;

    let container_xml = read_zip_text(&mut archive, "META-INF/container.xml")?;
    let opf_path = find_opf_path(&container_xml).unwrap_or_else(|| "content.opf".to_string());
    let opf_xml = read_zip_text(&mut archive, &opf_path)?;
    parse_opf_xml(&opf_xml)
}

fn extract_epub_cover_source(bytes: &[u8]) -> Option<Vec<u8>> {
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).ok()?;

    let container_xml = read_zip_text(&mut archive, "META-INF/container.xml")?;
    let opf_path = find_opf_path(&container_xml).unwrap_or_else(|| "content.opf".to_string());
    let opf_xml = read_zip_text(&mut archive, &opf_path)?;
    let cover_path = find_cover_image_path(&opf_path, &opf_xml)?;
    read_zip_bytes(&mut archive, &cover_path)
}

fn render_cover_variants(raw_cover: &[u8]) -> Option<(Vec<u8>, Vec<u8>)> {
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

    Some((cover_writer.into_inner(), thumb_writer.into_inner()))
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

fn find_opf_path(container_xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(container_xml).ok()?;
    doc.descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "rootfile")
        .and_then(|node| node.attribute("full-path"))
        .map(ToOwned::to_owned)
}

fn find_cover_image_path(opf_path: &str, opf_xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(opf_xml).ok()?;
    let cover_href = find_cover_href_in_manifest(&doc)?;
    resolve_zip_relative_path(opf_path, &cover_href)
}

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

fn looks_like_isbn(value: &str) -> bool {
    let compact = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();
    compact.len() == 10 || compact.len() == 13
}

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
