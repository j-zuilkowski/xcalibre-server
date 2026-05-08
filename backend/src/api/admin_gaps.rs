//! Phase 28b: Admin gaps endpoints (logs, backup, cover regenerate, task cancel, domains).
use crate::{db::queries::llm as llm_queries, middleware::auth::RequireAdmin, AppError, AppState};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    middleware,
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::Row;
use std::sync::atomic::Ordering;
use uuid::Uuid;

pub fn full_router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    let require_admin_layer = middleware::from_extractor::<RequireAdmin>();

    let routes = Router::new()
        .route("/api/v1/admin/logs", get(admin_logs))
        .route("/api/v1/admin/domains", get(list_domains))
        .route("/api/v1/admin/backup", post(admin_backup))
        .route("/api/v1/admin/domains", post(create_domain))
        .route("/api/v1/admin/covers/regenerate", post(admin_cover_regenerate))
        .route("/api/v1/admin/tasks/:task_id", delete(admin_task_cancel))
        .route("/api/v1/admin/domains/:id", delete(delete_domain))
        .route_layer(require_admin_layer);

    Router::new().merge(routes).route_layer(auth_layer)
}

#[derive(Debug, Deserialize)]
struct AdminLogsQuery {
    lines: Option<u32>,
    level: Option<String>,
}

async fn admin_logs(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Query(query): Query<AdminLogsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let log_path = state.config.log.file.as_ref().ok_or(AppError::NotFound)?.clone();
    if !std::path::Path::new(&log_path).exists() {
        return Err(AppError::NotFound);
    }
    let max_lines = query.lines.unwrap_or(100).min(500);
    if let Some(ref lvl) = query.level {
        if !["info", "warn", "error"].contains(&lvl.as_str()) {
            return Err(AppError::BadRequest);
        }
    }
    let fc = tokio::fs::read_to_string(&log_path).await.map_err(|_| AppError::Internal)?;
    let lines: Vec<&str> = fc.lines().rev().take(max_lines as usize).collect();
    let mut entries: Vec<serde_json::Value> = Vec::new();
    for line in lines.iter().rev() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            match &query.level {
                Some(lvl) if val.get("level").and_then(|v| v.as_str()) == Some(lvl) => entries.push(val),
                None => entries.push(val),
                _ => {}
            }
        }
    }
    Ok(Json(json!(entries)))
}

async fn admin_backup(
    State(state): State<AppState>,
    _admin: RequireAdmin,
) -> Result<Json<serde_json::Value>, AppError> {
    if state.backup_in_progress.swap(true, Ordering::SeqCst) {
        return Err(AppError::Conflict);
    }
    let r = do_backup(&state).await;
    state.backup_in_progress.store(false, Ordering::SeqCst);
    r
}

async fn do_backup(state: &AppState) -> Result<Json<serde_json::Value>, AppError> {
    let d = std::path::Path::new(&state.config.backup.dir);
    tokio::fs::create_dir_all(d).await.map_err(|_| AppError::Internal)?;
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let fname = format!("xcalibre-{}.db", ts);
    let dest = d.join(&fname);
    sqlx::query(&format!("VACUUM INTO '{}'", dest.display()))
        .execute(&state.db).await.map_err(|_| AppError::Internal)?;
    Ok(Json(json!({"path": fname})))
}

#[derive(Debug, Deserialize)]
struct CoverRegenRequest {
    book_ids: Vec<i64>,
}

async fn admin_cover_regenerate(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Json(payload): Json<CoverRegenRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    sqlx::query("PRAGMA foreign_keys = OFF").execute(&state.db).await.map_err(|_| AppError::Internal)?;
    let ids: Vec<String> = if payload.book_ids.is_empty() {
        sqlx::query_scalar::<_, String>("SELECT id FROM books").fetch_all(&state.db).await.unwrap_or_default()
    } else {
        payload.book_ids.iter().map(|id| id.to_string()).collect()
    };
    let now = chrono::Utc::now().to_rfc3339();
    let mut q = 0usize;
    for bid in &ids {
        let jid = Uuid::new_v4().to_string();
        let rows = sqlx::query(
            "INSERT INTO llm_jobs (id, job_type, status, book_id, created_at) SELECT ?, 'classify', 'pending', ?, ? WHERE NOT EXISTS (SELECT 1 FROM llm_jobs WHERE job_type='classify' AND book_id=? AND status IN ('pending','running'))"
        ).bind(&jid).bind(bid).bind(&now).bind(bid).execute(&state.db).await.map_err(|_| AppError::Internal)?.rows_affected();
        if rows > 0 { q += 1; }
    }
    sqlx::query("PRAGMA foreign_keys = ON").execute(&state.db).await.map_err(|_| AppError::Internal)?;
    Ok((StatusCode::ACCEPTED, Json(json!({"queued": q}))))
}

async fn admin_task_cancel(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let job = llm_queries::get_job(&state.db, &task_id).await.map_err(|_| AppError::Internal)?.ok_or(AppError::NotFound)?;
    if matches!(job.status.as_str(), "completed" | "failed" | "cancelled") {
        return Err(AppError::Conflict);
    }
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE llm_jobs SET status='cancelled', completed_at=?, error_text='cancelled by admin' WHERE id=?")
        .bind(&now).bind(&task_id).execute(&state.db).await.map_err(|_| AppError::Internal)?;
    Ok(Json(json!({"ok": true})))
}

#[derive(Debug, Deserialize)]
struct DomainQuery {
    allow: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateDomainRequest {
    domain: String,
    allow: bool,
}

#[derive(Debug, Serialize)]
struct DomainResponse {
    id: i64,
    domain: String,
    allow: bool,
    created_at: String,
}

async fn list_domains(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Query(query): Query<DomainQuery>,
) -> Result<Json<Vec<DomainResponse>>, AppError> {
    let rows = if let Some(a) = query.allow {
        sqlx::query("SELECT id, domain, allow, created_at FROM email_domains WHERE allow=? ORDER BY id").bind(i64::from(a))
            .fetch_all(&state.db).await.map_err(|_| AppError::Internal)?
    } else {
        sqlx::query("SELECT id, domain, allow, created_at FROM email_domains ORDER BY id")
            .fetch_all(&state.db).await.map_err(|_| AppError::Internal)?
    };
    Ok(Json(rows.into_iter().map(|r| DomainResponse{id:r.get("id"),domain:r.get("domain"),allow:r.get::<i64,_>("allow")!=0,created_at:r.get("created_at"),}).collect()))
}

async fn create_domain(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Json(payload): Json<CreateDomainRequest>,
) -> Result<(StatusCode, Json<DomainResponse>), AppError> {
    let dom = payload.domain.trim().to_lowercase();
    if dom.is_empty() { return Err(AppError::BadRequest); }
    if let Some(eid) = sqlx::query_scalar::<_, i64>("SELECT id FROM email_domains WHERE domain=?").bind(&dom).fetch_optional(&state.db).await.map_err(|_| AppError::Internal)? {
        let r = sqlx::query("SELECT id, domain, allow, created_at FROM email_domains WHERE id=?").bind(eid).fetch_one(&state.db).await.map_err(|_| AppError::Internal)?;
        return Ok((StatusCode::CREATED, Json(DomainResponse{id:r.get("id"),domain:r.get("domain"),allow:r.get::<i64,_>("allow")!=0,created_at:r.get("created_at")})));
    }
    let rid = sqlx::query("INSERT INTO email_domains (domain, allow) VALUES (?, ?)").bind(&dom).bind(i64::from(payload.allow))
        .execute(&state.db).await.map_err(|_| AppError::Internal)?.last_insert_rowid();
    let r = sqlx::query("SELECT id, domain, allow, created_at FROM email_domains WHERE id=?").bind(rid).fetch_one(&state.db).await.map_err(|_| AppError::Internal)?;
    Ok((StatusCode::CREATED, Json(DomainResponse{id:r.get("id"),domain:r.get("domain"),allow:r.get::<i64,_>("allow")!=0,created_at:r.get("created_at")})))
}

async fn delete_domain(
    State(state): State<AppState>,
    _admin: RequireAdmin,
    Path(id): Path<i64>,
) -> Result<StatusCode, AppError> {
    let n = sqlx::query("DELETE FROM email_domains WHERE id=?").bind(id).execute(&state.db).await.map_err(|_| AppError::Internal)?.rows_affected();
    if n == 0 { return Err(AppError::NotFound); }
    Ok(StatusCode::NO_CONTENT)
}
