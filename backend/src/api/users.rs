use crate::{
    db::queries::{auth as auth_queries, libraries as library_queries, stats as stats_queries},
    db::queries::{
        book_user_state as book_state_queries, books as book_queries,
        import_logs as import_log_queries, shelves as shelf_queries,
    },
    ingest::goodreads::{parse_goodreads_csv, parse_storygraph_csv, GoodreadsRow, StorygraphRow},
    middleware::auth::AuthenticatedUser,
    AppError, AppState,
};
use axum::{
    extract::{Extension, Json, Multipart, Path, State},
    middleware,
    routing::{get, patch, post},
    Router,
};
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use utoipa::ToSchema;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/users/me", get(me).patch(patch_me))
        .route("/api/v1/users/me/stats", get(me_stats))
        .route("/api/v1/users/me/import/goodreads", post(import_goodreads))
        .route(
            "/api/v1/users/me/import/storygraph",
            post(import_storygraph),
        )
        .route("/api/v1/users/me/import/:job_id", get(get_import_status))
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

#[allow(dead_code)]
#[derive(Debug, ToSchema)]
pub(crate) struct ImportCsvRequestDoc {
    #[schema(value_type = String, format = Binary)]
    file: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ImportJobResponse {
    job_id: String,
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
    get,
    path = "/api/v1/users/me/stats",
    tag = "users",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Reading statistics", body = stats_queries::UserStats),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn me_stats(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<stats_queries::UserStats>, AppError> {
    let stats = stats_queries::get_user_stats(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(Json(stats))
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

#[utoipa::path(
    post,
    path = "/api/v1/users/me/import/goodreads",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body(
        content = ImportCsvRequestDoc,
        content_type = "multipart/form-data"
    ),
    responses(
        (status = 200, description = "Import job queued", body = ImportJobResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn import_goodreads(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    multipart: Multipart,
) -> Result<Json<ImportJobResponse>, AppError> {
    let upload = parse_csv_upload(multipart, state.config.limits.upload_max_bytes).await?;
    let rows = parse_goodreads_csv(&upload.bytes)?;
    let job = import_log_queries::create_import_log(
        &state.db,
        &auth_user.user.id,
        &upload.filename,
        "goodreads",
        rows.len() as i64,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    spawn_import_task(
        state.clone(),
        auth_user.user.id.clone(),
        job.id.clone(),
        ImportSource::Goodreads(rows),
    );

    Ok(Json(ImportJobResponse { job_id: job.id }))
}

#[utoipa::path(
    post,
    path = "/api/v1/users/me/import/storygraph",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body(
        content = ImportCsvRequestDoc,
        content_type = "multipart/form-data"
    ),
    responses(
        (status = 200, description = "Import job queued", body = ImportJobResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn import_storygraph(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    multipart: Multipart,
) -> Result<Json<ImportJobResponse>, AppError> {
    let upload = parse_csv_upload(multipart, state.config.limits.upload_max_bytes).await?;
    let rows = parse_storygraph_csv(&upload.bytes)?;
    let job = import_log_queries::create_import_log(
        &state.db,
        &auth_user.user.id,
        &upload.filename,
        "storygraph",
        rows.len() as i64,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    spawn_import_task(
        state.clone(),
        auth_user.user.id.clone(),
        job.id.clone(),
        ImportSource::Storygraph(rows),
    );

    Ok(Json(ImportJobResponse { job_id: job.id }))
}

#[utoipa::path(
    get,
    path = "/api/v1/users/me/import/{job_id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(
        ("job_id" = String, Path, description = "Import job id")
    ),
    responses(
        (status = 200, description = "Import job status", body = import_log_queries::ImportLogRow),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn get_import_status(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(job_id): Path<String>,
) -> Result<Json<import_log_queries::ImportLogRow>, AppError> {
    let Some(log) = import_log_queries::get_import_log(&state.db, &auth_user.user.id, &job_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    Ok(Json(log))
}

async fn parse_csv_upload(
    mut multipart: Multipart,
    max_bytes: u64,
) -> Result<UploadedCsv, AppError> {
    let mut filename: Option<String> = None;
    let mut bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|_| AppError::BadRequest)?
    {
        let Some(name) = field.name() else {
            continue;
        };
        if name != "file" {
            continue;
        }

        let field_name = field
            .file_name()
            .map(sanitize_upload_file_name)
            .unwrap_or_else(|| "upload.csv".to_string());
        if !field_name.to_ascii_lowercase().ends_with(".csv") {
            return Err(AppError::Unprocessable);
        }

        let field_bytes = field.bytes().await.map_err(|_| AppError::BadRequest)?;
        if field_bytes.len() as u64 > max_bytes {
            return Err(AppError::PayloadTooLarge);
        }

        filename = Some(field_name);
        bytes = Some(field_bytes.to_vec());
    }

    let Some(filename) = filename else {
        return Err(AppError::BadRequest);
    };
    let Some(bytes) = bytes else {
        return Err(AppError::BadRequest);
    };
    if bytes.is_empty() {
        return Err(AppError::Unprocessable);
    }

    Ok(UploadedCsv { filename, bytes })
}

fn spawn_import_task(state: AppState, user_id: String, job_id: String, source: ImportSource) {
    tokio::spawn(async move {
        if let Err(err) = run_import_task(state.clone(), &user_id, &job_id, source).await {
            tracing::error!(error = %err, job_id = %job_id, "csv import job failed");
            let previous = import_log_queries::get_import_log(&state.db, &user_id, &job_id)
                .await
                .ok()
                .flatten();
            let error_entries = previous
                .as_ref()
                .map(|log| log.errors.clone())
                .unwrap_or_default();
            let _ = import_log_queries::update_import_log(
                &state.db,
                &job_id,
                "failed",
                previous.as_ref().map(|log| log.matched).unwrap_or_default(),
                previous
                    .as_ref()
                    .map(|log| log.unmatched)
                    .unwrap_or_default(),
                &error_entries,
                Some(&Utc::now().to_rfc3339()),
            )
            .await;
        }
    });
}

async fn run_import_task(
    state: AppState,
    user_id: &str,
    job_id: &str,
    source: ImportSource,
) -> anyhow::Result<()> {
    import_log_queries::update_import_log(&state.db, job_id, "running", 0, 0, &[], None).await?;

    match source {
        ImportSource::Goodreads(rows) => {
            process_goodreads_rows(&state, user_id, job_id, rows).await?
        }
        ImportSource::Storygraph(rows) => {
            process_storygraph_rows(&state, user_id, job_id, rows).await?
        }
    }

    let log = import_log_queries::get_import_log(&state.db, user_id, job_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("import log missing"))?;
    import_log_queries::update_import_log(
        &state.db,
        job_id,
        "complete",
        log.matched,
        log.unmatched,
        &log.errors,
        Some(&Utc::now().to_rfc3339()),
    )
    .await?;
    Ok(())
}

async fn process_goodreads_rows(
    state: &AppState,
    user_id: &str,
    job_id: &str,
    rows: Vec<GoodreadsRow>,
) -> anyhow::Result<()> {
    let mut matched = 0_i64;
    let mut unmatched = 0_i64;
    let mut errors = Vec::new();

    for (index, row) in rows.into_iter().enumerate() {
        let row_number = (index + 1) as i64;
        if process_goodreads_row(state, user_id, &row, row_number).await? {
            matched += 1;
        } else {
            unmatched += 1;
            errors.push(import_error_from_row(row_number, &row.title, &row.author));
        }
        import_log_queries::update_import_log(
            &state.db, job_id, "running", matched, unmatched, &errors, None,
        )
        .await?;
    }

    Ok(())
}

async fn process_storygraph_rows(
    state: &AppState,
    user_id: &str,
    job_id: &str,
    rows: Vec<StorygraphRow>,
) -> anyhow::Result<()> {
    let mut matched = 0_i64;
    let mut unmatched = 0_i64;
    let mut errors = Vec::new();

    for (index, row) in rows.into_iter().enumerate() {
        let row_number = (index + 1) as i64;
        if process_storygraph_row(state, user_id, &row, row_number).await? {
            matched += 1;
        } else {
            unmatched += 1;
            errors.push(import_error_from_row(row_number, &row.title, &row.authors));
        }
        import_log_queries::update_import_log(
            &state.db, job_id, "running", matched, unmatched, &errors, None,
        )
        .await?;
    }

    Ok(())
}

async fn process_goodreads_row(
    state: &AppState,
    user_id: &str,
    row: &GoodreadsRow,
    _row_number: i64,
) -> anyhow::Result<bool> {
    let book_ids =
        book_queries::find_book_ids_by_title_and_author_like(&state.db, &row.title, &row.author)
            .await?;
    if book_ids.len() != 1 {
        return Ok(false);
    }

    let book_id = &book_ids[0];
    if row.exclusive_shelf.eq_ignore_ascii_case("read") {
        let read_at = row.date_read.clone().unwrap_or_else(now_rfc3339);
        book_state_queries::set_read_at(&state.db, user_id, book_id, &read_at)
            .await
            .map_err(|_| AppError::Internal)?;
    }

    if row.my_rating > 0 {
        let rating = i64::from(row.my_rating) * 2;
        sqlx::query("UPDATE books SET rating = ?, last_modified = ? WHERE id = ?")
            .bind(rating)
            .bind(Utc::now().to_rfc3339())
            .bind(book_id)
            .execute(&state.db)
            .await
            .map_err(|_| AppError::Internal)?;
    }

    for shelf_name in &row.bookshelves {
        let shelf_id = shelf_queries::get_or_create_shelf_id(&state.db, user_id, shelf_name)
            .await
            .map_err(|_| AppError::Internal)?;
        shelf_queries::add_book_to_shelf(&state.db, &shelf_id, book_id)
            .await
            .map_err(|_| AppError::Internal)?;
    }

    Ok(true)
}

async fn process_storygraph_row(
    state: &AppState,
    user_id: &str,
    row: &StorygraphRow,
    _row_number: i64,
) -> anyhow::Result<bool> {
    let book_ids =
        book_queries::find_book_ids_by_title_and_author_like(&state.db, &row.title, &row.authors)
            .await?;
    if book_ids.len() != 1 {
        return Ok(false);
    }

    let book_id = &book_ids[0];
    if row.read_status.eq_ignore_ascii_case("read") {
        let read_at = row.date_finished.clone().unwrap_or_else(now_rfc3339);
        book_state_queries::set_read_at(&state.db, user_id, book_id, &read_at)
            .await
            .map_err(|_| AppError::Internal)?;
    }

    if let Some(star_rating) = row.star_rating.filter(|rating| *rating > 0.0) {
        let rating = (star_rating * 2.0).round() as i64;
        sqlx::query("UPDATE books SET rating = ?, last_modified = ? WHERE id = ?")
            .bind(rating)
            .bind(Utc::now().to_rfc3339())
            .bind(book_id)
            .execute(&state.db)
            .await
            .map_err(|_| AppError::Internal)?;
    }

    for tag in &row.tags {
        let shelf_id = shelf_queries::get_or_create_shelf_id(&state.db, user_id, tag)
            .await
            .map_err(|_| AppError::Internal)?;
        shelf_queries::add_book_to_shelf(&state.db, &shelf_id, book_id)
            .await
            .map_err(|_| AppError::Internal)?;
    }

    Ok(true)
}

fn import_error_from_row(
    row: i64,
    title: &str,
    author: &str,
) -> import_log_queries::ImportErrorEntry {
    import_log_queries::ImportErrorEntry {
        row,
        title: title.to_string(),
        author: author.to_string(),
        reason: "not_in_library".to_string(),
    }
}

fn sanitize_upload_file_name(file_name: &str) -> String {
    let without_nulls = file_name.replace('\0', "");
    let final_component = without_nulls
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or("upload.csv");
    let stripped = final_component.replace("..", "");
    let trimmed = stripped.trim();
    if trimmed.is_empty() {
        "upload.csv".to_string()
    } else {
        trimmed.to_string()
    }
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339()
}

enum ImportSource {
    Goodreads(Vec<GoodreadsRow>),
    Storygraph(Vec<StorygraphRow>),
}

struct UploadedCsv {
    filename: String,
    bytes: Vec<u8>,
}
