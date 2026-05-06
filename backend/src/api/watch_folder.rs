use crate::{error::AppError, watch_folder, AppState};
use axum::{extract::State, http::StatusCode, Json};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};

fn feature_enabled(state: &AppState) -> Result<(), AppError> {
    if state.config.watch_folder.enabled {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

// ── GET /api/v1/admin/watch-folder/status ────────────────────────────────────

pub async fn get_status(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    feature_enabled(&state)?;

    let today_start = (Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc())
    .timestamp();

    let processed_today: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM watch_folder_log WHERE status = 'ingested' AND processed_at >= ?",
    )
    .bind(today_start)
    .fetch_one(&state.db)
    .await
    .unwrap_or(0);

    let queued: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM watch_folder_log WHERE status = 'pending'")
            .fetch_one(&state.db)
            .await
            .unwrap_or(0);

    Ok(Json(json!({
        "enabled": true,
        "path": state.config.watch_folder.path,
        "running": true,
        "queued": queued,
        "processed_today": processed_today,
    })))
}

// ── POST /api/v1/admin/watch-folder/scan ─────────────────────────────────────

pub async fn trigger_scan(State(state): State<AppState>) -> Result<(StatusCode, Json<Value>), AppError> {
    feature_enabled(&state)?;
    let queued = watch_folder::scan_once(&state).await.map_err(|e| {
        tracing::error!(error = %e, "watch folder manual scan failed");
        AppError::Internal
    })?;
    Ok((StatusCode::ACCEPTED, Json(json!({ "queued": queued }))))
}

// ── GET /api/v1/admin/watch-folder/log ───────────────────────────────────────

#[derive(Deserialize)]
pub struct LogQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn get_log(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<LogQuery>,
) -> Result<Json<Value>, AppError> {
    feature_enabled(&state)?;

    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);

    let items: Vec<serde_json::Value> = sqlx::query_as::<_, (String, String, String, Option<String>, Option<String>, i64, Option<i64>)>(
        "SELECT id, file_name, status, error, book_id, detected_at, processed_at \
         FROM watch_folder_log \
         ORDER BY detected_at DESC \
         LIMIT ? OFFSET ?",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?
    .into_iter()
    .map(|(id, file_name, status, error, book_id, detected_at, processed_at)| {
        json!({
            "id": id,
            "file_name": file_name,
            "status": status,
            "error": error,
            "book_id": book_id,
            "detected_at": detected_at,
            "processed_at": processed_at,
        })
    })
    .collect();

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM watch_folder_log")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    Ok(Json(json!({ "items": items, "total": total })))
}
