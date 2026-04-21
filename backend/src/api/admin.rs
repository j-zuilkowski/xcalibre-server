use crate::{
    db::queries::{books as book_queries, llm as llm_queries},
    middleware::auth::AuthenticatedUser,
    AppError, AppState,
};
use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/admin/jobs", get(list_jobs))
        .route("/api/v1/admin/jobs/:id", get(get_job).delete(delete_job))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, Default)]
struct ListJobsQuery {
    status: Option<String>,
    job_type: Option<String>,
    page: Option<u32>,
    page_size: Option<u32>,
}

#[derive(Debug, Serialize)]
struct PaginatedResponse<T> {
    items: Vec<T>,
    total: i64,
    page: u32,
    page_size: u32,
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
