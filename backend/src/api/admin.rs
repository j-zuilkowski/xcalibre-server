use crate::{
    auth::password::hash_password,
    db::queries::{
        api_tokens as api_token_queries, auth as auth_queries, books as book_queries,
        email_settings as email_queries, kobo as kobo_queries, libraries as library_queries,
        llm as llm_queries, scheduled_tasks as scheduled_task_queries, tags as tag_queries,
        totp as totp_queries, user_tag_restrictions as restriction_queries,
    },
    middleware::auth::AuthenticatedUser,
    scheduler, AppError, AppState,
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{delete, get, patch, post},
    Json, Router,
};
use chrono::Utc;
use rand::{rngs::OsRng, RngCore};
use reqwest::header::USER_AGENT;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{
    sync::OnceLock,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/admin/jobs", get(list_jobs))
        .route("/api/v1/admin/jobs/:id", get(get_job).delete(delete_job))
        .route(
            "/api/v1/admin/scheduled-tasks",
            get(list_scheduled_tasks).post(create_scheduled_task),
        )
        .route(
            "/api/v1/admin/scheduled-tasks/:id",
            patch(update_scheduled_task).delete(delete_scheduled_task),
        )
        .route("/api/v1/admin/update-check", get(update_check))
        .route("/api/v1/admin/roles", get(list_roles))
        .route("/api/v1/admin/users", get(list_users).post(create_user))
        .route(
            "/api/v1/admin/users/:id",
            patch(update_user).delete(delete_user),
        )
        .route(
            "/api/v1/admin/users/:id/reset-password",
            post(reset_user_password),
        )
        .route(
            "/api/v1/admin/email-settings",
            get(get_email_settings).put(update_email_settings),
        )
        .route("/api/v1/admin/kobo-devices", get(list_kobo_devices))
        .route("/api/v1/admin/kobo-devices/:id", delete(delete_kobo_device))
        .route(
            "/api/v1/admin/libraries",
            get(list_libraries).post(create_library),
        )
        .route("/api/v1/admin/libraries/:id", delete(delete_library))
        .route("/api/v1/admin/tags", get(list_tags))
        .route(
            "/api/v1/admin/tags/:id",
            patch(rename_tag).delete(delete_tag),
        )
        .route("/api/v1/admin/tags/:id/merge", post(merge_tag))
        .route(
            "/api/v1/admin/users/:id/tag-restrictions",
            get(list_user_tag_restrictions).post(set_user_tag_restriction),
        )
        .route(
            "/api/v1/admin/users/:id/tag-restrictions/:tag_id",
            delete(delete_user_tag_restriction),
        )
        .route(
            "/api/v1/admin/users/:id/totp/disable",
            post(disable_user_totp),
        )
        .route("/api/v1/admin/tokens", post(create_token).get(list_tokens))
        .route("/api/v1/admin/tokens/:id", delete(delete_token))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, Default)]
struct ListJobsQuery {
    status: Option<String>,
    job_type: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct CreateTokenRequest {
    name: String,
}

#[derive(Debug, Deserialize, Default)]
struct EmailSettingsRequest {
    #[serde(default)]
    smtp_host: String,
    #[serde(default = "default_smtp_port")]
    smtp_port: i64,
    #[serde(default)]
    smtp_user: String,
    #[serde(default)]
    smtp_password: String,
    #[serde(default)]
    from_address: String,
    #[serde(default = "default_use_tls")]
    use_tls: bool,
}

#[derive(Debug, Deserialize, Default)]
struct CreateUserRequest {
    username: String,
    email: String,
    password: String,
    #[serde(default)]
    role_id: String,
    #[serde(default = "default_is_active")]
    is_active: bool,
}

#[derive(Debug, Deserialize, Default)]
struct UpdateUserRequest {
    #[serde(default)]
    role_id: Option<String>,
    #[serde(default)]
    is_active: Option<bool>,
    #[serde(default)]
    force_pw_reset: Option<bool>,
}

#[derive(Debug, Serialize)]
struct CreateTokenResponse {
    id: String,
    name: String,
    token: String,
    created_at: String,
}

#[derive(Debug, Serialize)]
struct RoleResponse {
    id: String,
    name: String,
    can_upload: bool,
    can_bulk: bool,
    can_edit: bool,
    can_download: bool,
    created_at: String,
    last_modified: String,
}

#[derive(Debug, Serialize)]
struct AdminUserResponse {
    #[serde(flatten)]
    user: crate::db::models::User,
    last_login_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct ListTokensResponse {
    items: Vec<api_token_queries::ApiToken>,
}

#[derive(Debug, Serialize)]
struct EmailSettingsResponse {
    id: String,
    smtp_host: String,
    smtp_port: i64,
    smtp_user: String,
    smtp_password: String,
    from_address: String,
    use_tls: bool,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct KoboDeviceResponse {
    id: String,
    user_id: String,
    username: String,
    email: String,
    device_id: String,
    device_name: String,
    last_sync_at: Option<String>,
    created_at: String,
}

#[derive(Debug, Deserialize)]
struct CreateLibraryRequest {
    name: String,
    calibre_db_path: String,
}

#[derive(Debug, Deserialize)]
struct ListTagsQuery {
    q: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
    limit: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct TagRestrictionRequest {
    tag_id: String,
    mode: String,
}

#[derive(Debug, Serialize)]
struct LibraryResponse {
    id: String,
    name: String,
    calibre_db_path: String,
    book_count: i64,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
struct TagResponse {
    id: String,
    name: String,
    source: String,
}

#[derive(Debug, Serialize)]
struct TagWithCountResponse {
    id: String,
    name: String,
    source: String,
    book_count: i64,
    confirmed_count: i64,
}

#[derive(Debug, Deserialize)]
struct RenameTagRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct MergeTagRequest {
    into_tag_id: String,
}

#[derive(Debug, Serialize)]
struct MergeTagResponse {
    merged_book_count: usize,
    target_tag: TagResponse,
}

#[derive(Debug, Serialize)]
struct UserTagRestrictionResponse {
    user_id: String,
    tag_id: String,
    tag_name: String,
    mode: String,
}

#[derive(Debug, Serialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
    total: i64,
    page: u32,
    page_size: u32,
}

#[derive(Debug, Deserialize)]
struct CreateScheduledTaskRequest {
    name: String,
    task_type: String,
    cron_expr: String,
    enabled: bool,
}

#[derive(Debug, Deserialize, Default)]
struct UpdateScheduledTaskRequest {
    enabled: Option<bool>,
    cron_expr: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
struct UpdateCheckResponse {
    #[serde(skip_serializing_if = "Option::is_none")]
    current_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_version: Option<String>,
    update_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    release_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[derive(Clone, Debug)]
struct CachedUpdateCheck {
    status: StatusCode,
    response: UpdateCheckResponse,
    fetched_at: Instant,
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
}

static UPDATE_CHECK_CACHE: OnceLock<RwLock<Option<CachedUpdateCheck>>> = OnceLock::new();

async fn list_tags(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Query(query): Query<ListTagsQuery>,
) -> Result<Json<PaginatedResponse<TagWithCountResponse>>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query
        .page_size
        .unwrap_or(query.limit.unwrap_or(20))
        .clamp(1, 100);
    let (items, total) =
        tag_queries::list_tags_with_counts(&state.db, query.q.as_deref(), page, page_size)
            .await
            .map_err(|_| AppError::Internal)?;
    Ok(Json(PaginatedResponse {
        items: items
            .into_iter()
            .map(|tag| TagWithCountResponse {
                id: tag.id,
                name: tag.name,
                source: tag.source,
                book_count: tag.book_count,
                confirmed_count: tag.confirmed_count,
            })
            .collect(),
        total,
        page,
        page_size,
    }))
}

async fn rename_tag(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(tag_id): Path<String>,
    Json(payload): Json<RenameTagRequest>,
) -> Result<Response, Response> {
    ensure_admin(&state, &auth_user.user.id)
        .await
        .map_err(IntoResponse::into_response)?;

    let new_name = payload.name.trim();
    if new_name.is_empty() {
        return Err(AppError::BadRequest.into_response());
    }

    match tag_queries::rename_tag(&state.db, &tag_id, new_name).await {
        Ok(tag) => Ok(Json(TagResponse {
            id: tag.id,
            name: tag.name,
            source: tag.source,
        })
        .into_response()),
        Err(AppError::Conflict) => Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "tag_name_conflict",
                "message": "tag name already exists"
            })),
        )
            .into_response()),
        Err(err) => Err(err.into_response()),
    }
}

async fn delete_tag(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(tag_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    tag_queries::delete_tag(&state.db, &tag_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn merge_tag(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(source_id): Path<String>,
    Json(payload): Json<MergeTagRequest>,
) -> Result<Json<MergeTagResponse>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let target_id = payload.into_tag_id.trim();
    if target_id.is_empty() || source_id == target_id {
        return Err(AppError::BadRequest);
    }

    let merged_book_count = tag_queries::merge_tags(&state.db, &source_id, target_id).await?;
    let target_tag = tag_queries::find_tag_record_by_id(&state.db, target_id)
        .await?
        .ok_or(AppError::NotFound)?;

    Ok(Json(MergeTagResponse {
        merged_book_count,
        target_tag: TagResponse {
            id: target_tag.id,
            name: target_tag.name,
            source: target_tag.source,
        },
    }))
}

async fn list_user_tag_restrictions(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(user_id): Path<String>,
) -> Result<Json<Vec<UserTagRestrictionResponse>>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let Some(_) = auth_queries::find_user_by_id(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };
    let restrictions = restriction_queries::get_restrictions(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        restrictions
            .into_iter()
            .map(|restriction| UserTagRestrictionResponse {
                user_id: restriction.user_id,
                tag_id: restriction.tag_id,
                tag_name: restriction.tag_name,
                mode: restriction.mode,
            })
            .collect(),
    ))
}

async fn set_user_tag_restriction(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(user_id): Path<String>,
    Json(payload): Json<TagRestrictionRequest>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let Some(_) = auth_queries::find_user_by_id(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };
    let tag_id = payload.tag_id.trim();
    if tag_id.is_empty() {
        return Err(AppError::BadRequest);
    }
    let mode = payload.mode.trim().to_lowercase();
    if mode != "allow" && mode != "block" {
        return Err(AppError::BadRequest);
    }
    let Some(_) = tag_queries::find_tag_by_id(&state.db, tag_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    restriction_queries::set_restriction(&state.db, &user_id, tag_id, &mode)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_user_tag_restriction(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path((user_id, tag_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let Some(_) = auth_queries::find_user_by_id(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };
    if tag_id.trim().is_empty() {
        return Err(AppError::BadRequest);
    }
    let removed = restriction_queries::remove_restriction(&state.db, &user_id, &tag_id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !removed {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn disable_user_totp(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let Some(_) = auth_queries::find_user_by_id(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    totp_queries::disable_totp(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_jobs(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Query(query): Query<ListJobsQuery>,
) -> Result<Json<PaginatedResponse<llm_queries::JobRow>>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(20).clamp(1, 100);
    let (items, total) = llm_queries::list_jobs(
        &state.db,
        query.status.as_deref(),
        query.job_type.as_deref(),
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

async fn list_roles(State(state): State<AppState>) -> Result<Json<Vec<RoleResponse>>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified
        FROM roles
        ORDER BY name ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let roles = rows
        .into_iter()
        .map(|row| RoleResponse {
            id: row.get("id"),
            name: row.get("name"),
            can_upload: row.get::<i64, _>("can_upload") != 0,
            can_bulk: row.get::<i64, _>("can_bulk") != 0,
            can_edit: row.get::<i64, _>("can_edit") != 0,
            can_download: row.get::<i64, _>("can_download") != 0,
            created_at: row.get("created_at"),
            last_modified: row.get("last_modified"),
        })
        .collect();

    Ok(Json(roles))
}

async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<AdminUserResponse>>, AppError> {
    let rows = sqlx::query(
        r#"
        SELECT
            u.id AS user_id,
            u.username AS username,
            u.email AS email,
            u.role_id AS role_id,
            r.name AS role_name,
            u.is_active AS is_active,
            u.force_pw_reset AS force_pw_reset,
            COALESCE(u.default_library_id, 'default') AS default_library_id,
            COALESCE(u.totp_enabled, 0) AS totp_enabled,
            u.created_at AS created_at,
            u.last_modified AS last_modified
        FROM users u
        INNER JOIN roles r ON r.id = u.role_id
        ORDER BY u.username ASC
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(
        rows.into_iter()
            .map(|row| AdminUserResponse {
                user: crate::db::models::User {
                    id: row.get("user_id"),
                    username: row.get("username"),
                    email: row.get("email"),
                    role: crate::db::models::RoleRef {
                        id: row.get("role_id"),
                        name: row.get("role_name"),
                    },
                    is_active: row.get::<i64, _>("is_active") != 0,
                    force_pw_reset: row.get::<i64, _>("force_pw_reset") != 0,
                    default_library_id: row.get("default_library_id"),
                    totp_enabled: row.get::<i64, _>("totp_enabled") != 0,
                    created_at: row.get("created_at"),
                    last_modified: row.get("last_modified"),
                },
                last_login_at: None,
            })
            .collect(),
    ))
}

async fn create_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<AdminUserResponse>), AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let username = payload.username.trim();
    let email = payload.email.trim();
    let password = payload.password.trim();
    if username.is_empty() || email.is_empty() || password.is_empty() {
        return Err(AppError::BadRequest);
    }

    let role_id = if payload.role_id.trim().is_empty() {
        "user"
    } else {
        payload.role_id.trim()
    };
    let role_exists = sqlx::query_scalar::<_, String>("SELECT id FROM roles WHERE id = ?")
        .bind(role_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    if role_exists.is_none() {
        return Err(AppError::NotFound);
    }

    let password_hash = hash_password(password, &state.config.auth)?;
    let mut user = match auth_queries::create_user(
        &state.db,
        username,
        email,
        role_id,
        &password_hash,
    )
    .await
    {
        Ok(user) => user,
        Err(err) => {
            let err_text = err.to_string();
            if err_text.contains("UNIQUE constraint failed") {
                return Err(AppError::Conflict);
            }
            return Err(AppError::Internal);
        }
    };

    if !payload.is_active {
        sqlx::query(
            r#"
            UPDATE users
            SET is_active = 0, last_modified = ?
            WHERE id = ?
            "#,
        )
        .bind(Utc::now().to_rfc3339())
        .bind(&user.id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;

        user = auth_queries::find_user_by_id(&state.db, &user.id)
            .await
            .map_err(|_| AppError::Internal)?
            .ok_or(AppError::NotFound)?;
    }

    Ok((
        StatusCode::CREATED,
        Json(AdminUserResponse {
            user,
            last_login_at: None,
        }),
    ))
}

async fn update_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(user_id): Path<String>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<Json<AdminUserResponse>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let Some(_) = auth_queries::find_user_by_id(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    if payload.role_id.is_none() && payload.is_active.is_none() && payload.force_pw_reset.is_none()
    {
        return Err(AppError::BadRequest);
    }

    if let Some(role_id) = payload.role_id.as_deref() {
        let role_id = role_id.trim();
        if role_id.is_empty() {
            return Err(AppError::BadRequest);
        }
        let role_exists = sqlx::query_scalar::<_, String>("SELECT id FROM roles WHERE id = ?")
            .bind(role_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|_| AppError::Internal)?;
        if role_exists.is_none() {
            return Err(AppError::NotFound);
        }
    }

    let mut updates = Vec::new();
    if payload.role_id.is_some() {
        updates.push("role_id = ?");
    }
    if payload.is_active.is_some() {
        updates.push("is_active = ?");
    }
    if payload.force_pw_reset.is_some() {
        updates.push("force_pw_reset = ?");
    }

    let now = Utc::now().to_rfc3339();
    let mut query = String::from("UPDATE users SET ");
    query.push_str(&updates.join(", "));
    query.push_str(", last_modified = ? WHERE id = ?");

    let mut stmt = sqlx::query(&query);
    if let Some(role_id) = payload.role_id.as_deref() {
        stmt = stmt.bind(role_id.trim());
    }
    if let Some(is_active) = payload.is_active {
        stmt = stmt.bind(i64::from(is_active));
    }
    if let Some(force_pw_reset) = payload.force_pw_reset {
        stmt = stmt.bind(i64::from(force_pw_reset));
    }
    stmt = stmt.bind(&now).bind(&user_id);
    stmt.execute(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;

    let user = auth_queries::find_user_by_id(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    Ok(Json(AdminUserResponse {
        user,
        last_login_at: None,
    }))
}

async fn delete_user(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let deleted = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::Internal)?
        .rows_affected();
    if deleted == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn reset_user_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let Some(_) = auth_queries::find_user_by_id(&state.db, &user_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    sqlx::query(
        r#"
        UPDATE users
        SET force_pw_reset = 1, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(Utc::now().to_rfc3339())
    .bind(&user_id)
    .execute(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(StatusCode::NO_CONTENT)
}

async fn get_job(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(job_id): Path<String>,
) -> Result<Json<llm_queries::JobRow>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let Some(job) = llm_queries::get_job(&state.db, &job_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    Ok(Json(job))
}

async fn delete_job(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(job_id): Path<String>,
) -> Result<StatusCode, Response> {
    ensure_admin(&state, &auth_user.user.id)
        .await
        .map_err(IntoResponse::into_response)?;

    let exists = llm_queries::get_job(&state.db, &job_id)
        .await
        .map_err(|_| AppError::Internal.into_response())?;
    if exists.is_none() {
        return Err(AppError::NotFound.into_response());
    }

    let cancelled = llm_queries::cancel_job(&state.db, &job_id)
        .await
        .map_err(|_| AppError::Internal.into_response())?;
    if !cancelled {
        return Err((
            StatusCode::CONFLICT,
            Json(json!({
                "error": "conflict",
                "message": "Job is not in pending status"
            })),
        )
            .into_response());
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn create_token(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateTokenRequest>,
) -> Result<(StatusCode, Json<CreateTokenResponse>), AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest);
    }

    let plain_token = generate_plain_token();
    let token_hash = hash_token(&plain_token);
    let token = api_token_queries::create_token(&state.db, name, &token_hash, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok((
        StatusCode::CREATED,
        Json(CreateTokenResponse {
            id: token.id,
            name: token.name,
            token: plain_token,
            created_at: token.created_at,
        }),
    ))
}

async fn list_tokens(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<ListTokensResponse>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let items = api_token_queries::list_tokens(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(ListTokensResponse { items }))
}

async fn get_email_settings(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<EmailSettingsResponse>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let settings = email_queries::get_email_settings(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    let settings = settings.unwrap_or_else(default_email_settings);
    Ok(Json(mask_email_settings(settings)))
}

async fn update_email_settings(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<EmailSettingsRequest>,
) -> Result<Json<EmailSettingsResponse>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let settings = email_queries::EmailSettings {
        id: "singleton".to_string(),
        smtp_host: payload.smtp_host,
        smtp_port: if payload.smtp_port <= 0 {
            587
        } else {
            payload.smtp_port
        },
        smtp_user: payload.smtp_user,
        smtp_password: payload.smtp_password,
        from_address: payload.from_address,
        use_tls: payload.use_tls,
        updated_at: String::new(),
    };
    let updated = email_queries::upsert_email_settings(&state.db, settings)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(mask_email_settings(updated)))
}

async fn list_kobo_devices(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<KoboDeviceResponse>>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let devices = kobo_queries::list_devices(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(
        devices
            .into_iter()
            .map(|device| KoboDeviceResponse {
                id: device.id,
                user_id: device.user_id,
                username: device.username,
                email: device.email,
                device_id: device.device_id,
                device_name: device.device_name,
                last_sync_at: device.last_sync_at,
                created_at: device.created_at,
            })
            .collect(),
    ))
}

async fn list_libraries(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<LibraryResponse>>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
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

async fn create_library(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateLibraryRequest>,
) -> Result<(StatusCode, Json<LibraryResponse>), AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let name = payload.name.trim();
    let calibre_db_path = payload.calibre_db_path.trim();
    if name.is_empty() || calibre_db_path.is_empty() {
        return Err(AppError::BadRequest);
    }

    let library = library_queries::create_library(&state.db, name, calibre_db_path)
        .await
        .map_err(|_| AppError::Conflict)?;
    Ok((
        StatusCode::CREATED,
        Json(LibraryResponse {
            id: library.id,
            name: library.name,
            calibre_db_path: library.calibre_db_path,
            book_count: 0,
            created_at: library.created_at,
            updated_at: library.updated_at,
        }),
    ))
}

async fn delete_library(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(library_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    match library_queries::delete_library(&state.db, &library_id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(AppError::NotFound),
        Err(err) => {
            let err_text = err.to_string();
            if err_text.contains("books assigned") || err_text.contains("cannot be deleted") {
                Err(AppError::Conflict)
            } else {
                Err(AppError::Internal)
            }
        }
    }
}

async fn delete_kobo_device(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(device_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let deleted = kobo_queries::revoke_device(&state.db, &device_id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn delete_token(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(token_id): Path<String>,
) -> Result<StatusCode, Response> {
    ensure_admin(&state, &auth_user.user.id)
        .await
        .map_err(IntoResponse::into_response)?;

    let deleted = api_token_queries::delete_token(&state.db, &token_id, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal.into_response())?;
    if !deleted {
        return Err(AppError::NotFound.into_response());
    }

    Ok(StatusCode::NO_CONTENT)
}

async fn list_scheduled_tasks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<scheduled_task_queries::ScheduledTask>>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let tasks = scheduled_task_queries::list_scheduled_tasks(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(tasks))
}

async fn create_scheduled_task(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateScheduledTaskRequest>,
) -> Result<(StatusCode, Json<scheduled_task_queries::ScheduledTask>), AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let name = payload.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest);
    }

    let task_type =
        normalize_scheduled_task_type(&payload.task_type).ok_or(AppError::BadRequest)?;
    let cron_expr = payload.cron_expr.trim();
    if cron_expr.is_empty() {
        return Err(AppError::BadRequest);
    }
    let next_run_at =
        scheduler::next_run_at_for_cron(cron_expr, Utc::now()).map_err(|_| AppError::BadRequest)?;

    let task = scheduled_task_queries::create_scheduled_task(
        &state.db,
        name,
        task_type,
        cron_expr,
        payload.enabled,
        &next_run_at,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok((StatusCode::CREATED, Json(task)))
}

async fn update_scheduled_task(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(task_id): Path<String>,
    Json(payload): Json<UpdateScheduledTaskRequest>,
) -> Result<Json<scheduled_task_queries::ScheduledTask>, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    let Some(_) = scheduled_task_queries::get_scheduled_task(&state.db, &task_id)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    if payload.enabled.is_none() && payload.cron_expr.is_none() {
        return Err(AppError::BadRequest);
    }

    let mut next_run_at = None;
    if let Some(cron_expr) = payload.cron_expr.as_deref() {
        let cron_expr = cron_expr.trim();
        if cron_expr.is_empty() {
            return Err(AppError::BadRequest);
        }
        next_run_at = Some(
            scheduler::next_run_at_for_cron(cron_expr, Utc::now())
                .map_err(|_| AppError::BadRequest)?,
        );
    }

    let mut tx = state.db.begin().await.map_err(|_| AppError::Internal)?;

    if let Some(enabled) = payload.enabled {
        sqlx::query(
            r#"
            UPDATE scheduled_tasks
            SET enabled = ?
            WHERE id = ?
            "#,
        )
        .bind(i64::from(enabled))
        .bind(&task_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;
    }

    if let Some(cron_expr) = payload.cron_expr.as_deref() {
        let next_run_at = next_run_at.as_deref().ok_or(AppError::BadRequest)?;
        sqlx::query(
            r#"
            UPDATE scheduled_tasks
            SET cron_expr = ?, next_run_at = ?
            WHERE id = ?
            "#,
        )
        .bind(cron_expr.trim())
        .bind(next_run_at)
        .bind(&task_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;
    }

    tx.commit().await.map_err(|_| AppError::Internal)?;

    let task = scheduled_task_queries::get_scheduled_task(&state.db, &task_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    Ok(Json(task))
}

async fn delete_scheduled_task(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(task_id): Path<String>,
) -> Result<StatusCode, AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let deleted = scheduled_task_queries::delete_scheduled_task(&state.db, &task_id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

async fn update_check(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<(StatusCode, Json<UpdateCheckResponse>), AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;

    if let Some(cached) = cached_update_check().read().await.clone() {
        if cached.fetched_at.elapsed() < Duration::from_secs(60 * 60) {
            return Ok((cached.status, Json(cached.response)));
        }
    }

    let (status, response) = fetch_update_check().await;
    let cached = CachedUpdateCheck {
        status,
        response: response.clone(),
        fetched_at: Instant::now(),
    };
    *cached_update_check().write().await = Some(cached);

    Ok((status, Json(response)))
}

fn cached_update_check() -> &'static RwLock<Option<CachedUpdateCheck>> {
    UPDATE_CHECK_CACHE.get_or_init(|| RwLock::new(None))
}

pub async fn clear_update_check_cache() {
    *cached_update_check().write().await = None;
}

async fn fetch_update_check() -> (StatusCode, UpdateCheckResponse) {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent("autolibre")
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                unreachable_update_check_response(),
            );
        }
    };

    let url = std::env::var("AUTOLIBRE_RELEASES_URL").unwrap_or_else(|_| {
        "https://api.github.com/repos/autolibre/autolibre/releases/latest".to_string()
    });

    let response = match client.get(url).header(USER_AGENT, "autolibre").send().await {
        Ok(response) if response.status().is_success() => response,
        _ => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                unreachable_update_check_response(),
            );
        }
    };

    let release = match response.json::<GitHubRelease>().await {
        Ok(release) => release,
        Err(_) => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                unreachable_update_check_response(),
            );
        }
    };

    let latest_version = release.tag_name.trim().trim_start_matches('v').to_string();
    let update_available = compare_versions(&current_version, &latest_version).unwrap_or(false);

    (
        StatusCode::OK,
        UpdateCheckResponse {
            current_version: Some(current_version),
            latest_version: Some(latest_version),
            update_available,
            release_url: Some(release.html_url),
            error: None,
        },
    )
}

fn unreachable_update_check_response() -> UpdateCheckResponse {
    UpdateCheckResponse {
        current_version: None,
        latest_version: None,
        update_available: false,
        release_url: None,
        error: Some("unreachable".to_string()),
    }
}

fn compare_versions(current: &str, latest: &str) -> anyhow::Result<bool> {
    let current = semver::Version::parse(current.trim().trim_start_matches('v'))?;
    let latest = semver::Version::parse(latest.trim().trim_start_matches('v'))?;
    Ok(latest > current)
}

fn normalize_scheduled_task_type(task_type: &str) -> Option<&'static str> {
    match task_type.trim().to_lowercase().as_str() {
        "classify_all" => Some("classify_all"),
        "semantic_index_all" => Some("semantic_index_all"),
        "backup" => Some("backup"),
        _ => None,
    }
}

async fn ensure_admin(state: &AppState, user_id: &str) -> Result<(), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.is_admin() {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

fn generate_plain_token() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}

fn mask_email_settings(settings: email_queries::EmailSettings) -> EmailSettingsResponse {
    EmailSettingsResponse {
        id: settings.id,
        smtp_host: settings.smtp_host,
        smtp_port: settings.smtp_port,
        smtp_user: settings.smtp_user,
        smtp_password: String::new(),
        from_address: settings.from_address,
        use_tls: settings.use_tls,
        updated_at: settings.updated_at,
    }
}

fn default_smtp_port() -> i64 {
    587
}

fn default_use_tls() -> bool {
    true
}

fn default_is_active() -> bool {
    true
}

fn default_email_settings() -> email_queries::EmailSettings {
    email_queries::EmailSettings {
        id: "singleton".to_string(),
        smtp_host: String::new(),
        smtp_port: default_smtp_port(),
        smtp_user: String::new(),
        smtp_password: String::new(),
        from_address: String::new(),
        use_tls: default_use_tls(),
        updated_at: String::new(),
    }
}
