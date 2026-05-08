//! Admin book-merge endpoints for xcalibre-server.
//!
//! Routes:
//! - `POST /api/v1/admin/books/merge/preview`  — dry-run analysis of what a merge would affect
//! - `POST /api/v1/admin/books/merge`           — execute a merge with format-conflict detection
//!
//! Both routes require JWT authentication and the `admin` role, enforced via middleware
//! layers applied in `router()`.

use crate::{
    db::queries::books as book_queries,
    middleware::auth::RequireAdmin,
    AppError, AppState,
};
use axum::{
    extract::State,
    middleware,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

/// Builds the merge sub-router with JWT auth and admin-role middleware layers.
pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    let require_admin_layer = middleware::from_extractor::<crate::middleware::auth::RequireAdmin>();

    let merge_routes = Router::new()
        .route("/api/v1/admin/books/merge/preview", post(merge_preview))
        .route("/api/v1/admin/books/merge", post(merge_exec))
        .route_layer(require_admin_layer);

    Router::new().merge(merge_routes).route_layer(auth_layer)
}

// ─── Request / Response types ───────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct MergeRequest {
    source_id: String,
    target_id: String,
    #[serde(default = "default_strategy")]
    reading_progress_strategy: String,
    #[serde(default)]
    force: bool,
}

fn default_strategy() -> String {
    "keep_target".to_string()
}

#[derive(Debug, Serialize)]
struct MergePreviewResponse {
    formats_to_move: Vec<String>,
    formats_conflict: Vec<String>,
    annotations_to_move: i64,
    shelves_to_relink: Vec<String>,
    reading_progress_strategy: String,
}

#[derive(Debug, Serialize)]
struct MergeExecResponse {
    merged: bool,
    target_id: String,
    source_id: String,
}

// ─── Helpers ─────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
struct RpSnapshot {
    user_id: String,
    format_id: String,
    cfi: Option<String>,
    page: Option<i64>,
    percentage: f64,
    updated_at: String,
}

// ─── Preview endpoint ───────────────────────────────────────────────────────────

async fn merge_preview(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(payload): Json<MergeRequest>,
) -> Result<Json<MergePreviewResponse>, AppError> {
    let source_id = payload.source_id.trim();
    let target_id = payload.target_id.trim();

    if source_id == target_id {
        return Err(AppError::BadRequest);
    }

    let source = book_queries::get_book_by_id(&state.db, source_id, None, None)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let target = book_queries::get_book_by_id(&state.db, target_id, None, None)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let source_formats: Vec<String> = source.formats.iter().map(|f| f.format.clone()).collect();
    let target_formats: Vec<String> = target.formats.iter().map(|f| f.format.clone()).collect();

    let mut formats_to_move = Vec::new();
    let mut formats_conflict = Vec::new();

    for fmt in &source_formats {
        let upper = fmt.to_uppercase();
        if target_formats.iter().any(|t| t.to_uppercase() == upper) {
            formats_conflict.push(fmt.clone());
        } else {
            formats_to_move.push(fmt.clone());
        }
    }

    let annotations_count_row = sqlx::query(
        "SELECT COUNT(*) AS cnt FROM book_annotations WHERE book_id = ?",
    )
    .bind(source_id)
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;
    let annotations_to_move: i64 = annotations_count_row.get("cnt");

    let shelves_to_relink: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT s.name
        FROM shelf_books sb
        INNER JOIN shelves s ON s.id = sb.shelf_id
        WHERE sb.book_id = ?
        ORDER BY s.name ASC
        "#,
    )
    .bind(source_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(MergePreviewResponse {
        formats_to_move,
        formats_conflict,
        annotations_to_move,
        shelves_to_relink,
        reading_progress_strategy: payload.reading_progress_strategy,
    }))
}

// ─── Merge execution endpoint ───────────────────────────────────────────────────

async fn merge_exec(
    _admin: RequireAdmin,
    State(state): State<AppState>,
    Json(payload): Json<MergeRequest>,
) -> Result<Json<MergeExecResponse>, AppError> {
    let source_id = payload.source_id.trim().to_string();
    let target_id = payload.target_id.trim().to_string();

    if source_id == target_id {
        return Err(AppError::BadRequest);
    }

    match payload.reading_progress_strategy.as_str() {
        "keep_target" | "keep_source" | "merge_max" => {}
        _ => return Err(AppError::BadRequest),
    }

    let source = book_queries::get_book_by_id(&state.db, &source_id, None, None)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let target = book_queries::get_book_by_id(&state.db, &target_id, None, None)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let source_formats: Vec<String> = source.formats.iter().map(|f| f.format.clone()).collect();
    let target_formats: Vec<String> = target.formats.iter().map(|f| f.format.clone()).collect();

    let has_conflict = source_formats.iter().any(|sf| {
        target_formats
            .iter()
            .any(|tf| tf.to_uppercase() == sf.to_uppercase())
    });

    if has_conflict && !payload.force {
        return Err(AppError::Conflict);
    }

    // ── Snapshot reading progress BEFORE any format deletes ──────────────────
    // Format deletes cascade to reading_progress rows.  We capture both sides
    // now so the merge logic can apply the correct per-user strategy even after
    // conflicting target formats have been dropped.

    let source_rp: Vec<RpSnapshot> = sqlx::query(
        r#"
        SELECT user_id, format_id, cfi, page, percentage, updated_at
        FROM reading_progress
        WHERE book_id = ?
        "#,
    )
    .bind(&source_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?
    .into_iter()
    .map(|row| RpSnapshot {
        user_id: row.get("user_id"),
        format_id: row.get("format_id"),
        cfi: row.get("cfi"),
        page: row.get("page"),
        percentage: row.get("percentage"),
        updated_at: row.get("updated_at"),
    })
    .collect();

    let target_rp: Vec<RpSnapshot> = sqlx::query(
        r#"
        SELECT user_id, format_id, cfi, page, percentage, updated_at
        FROM reading_progress
        WHERE book_id = ?
        "#,
    )
    .bind(&target_id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?
    .into_iter()
    .map(|row| RpSnapshot {
        user_id: row.get("user_id"),
        format_id: row.get("format_id"),
        cfi: row.get("cfi"),
        page: row.get("page"),
        percentage: row.get("percentage"),
        updated_at: row.get("updated_at"),
    })
    .collect();

    let storage_path = state.config.app.storage_path.clone();

    // ── Begin transaction ────────────────────────────────────────────────────
    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|_| AppError::Internal)?;

    let now = chrono::Utc::now().to_rfc3339();

    // Step 1: Move non-conflicting formats.
    for fmt_name in &source_formats {
        let is_conflict = target_formats
            .iter()
            .any(|tf| tf.to_uppercase() == fmt_name.to_uppercase());

        if !is_conflict {
            sqlx::query("UPDATE formats SET book_id = ?, last_modified = ? WHERE book_id = ? AND upper(format) = upper(?)")
                .bind(&target_id)
                .bind(&now)
                .bind(&source_id)
                .bind(fmt_name)
                .execute(&mut *tx)
                .await
                .map_err(|_| AppError::Internal)?;

            let source_path = std::path::Path::new(&storage_path).join(format!("{}.{}", source_id, fmt_name.to_lowercase()));
            let target_path = std::path::Path::new(&storage_path).join(format!("{}.{}", target_id, fmt_name.to_lowercase()));
            if source_path.exists() {
                std::fs::rename(&source_path, &target_path).map_err(|_| AppError::Internal)?;
            }
        }
    }

    // Step 2: Handle conflicting formats with force=true.
    if payload.force {
        for fmt_name in &source_formats {
            let is_conflict = target_formats
                .iter()
                .any(|tf| tf.to_uppercase() == fmt_name.to_uppercase());

            if is_conflict {
                sqlx::query("DELETE FROM formats WHERE book_id = ? AND upper(format) = upper(?)")
                    .bind(&target_id)
                    .bind(fmt_name)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AppError::Internal)?;

                sqlx::query("UPDATE formats SET book_id = ?, last_modified = ? WHERE book_id = ? AND upper(format) = upper(?)")
                    .bind(&target_id)
                    .bind(&now)
                    .bind(&source_id)
                    .bind(fmt_name)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AppError::Internal)?;

                let source_path = std::path::Path::new(&storage_path).join(format!("{}.{}", source_id, fmt_name.to_lowercase()));
                let target_path = std::path::Path::new(&storage_path).join(format!("{}.{}", target_id, fmt_name.to_lowercase()));
                if source_path.exists() {
                    std::fs::rename(&source_path, &target_path).map_err(|_| AppError::Internal)?;
                }
            }
        }
    }

    // Step 3: Reparent annotations.
    sqlx::query("UPDATE book_annotations SET book_id = ?, updated_at = ? WHERE book_id = ?")
        .bind(&target_id)
        .bind(&now)
        .bind(&source_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;

    // Step 4: Relink shelves.
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO shelf_books (shelf_id, book_id, display_order, added_at)
        SELECT sb.shelf_id, ?, sb.display_order, sb.added_at
        FROM shelf_books sb
        WHERE sb.book_id = ?
        "#,
    )
    .bind(&target_id)
    .bind(&source_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    sqlx::query("DELETE FROM shelf_books WHERE book_id = ?")
        .bind(&source_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;

    // Step 5: Merge reading progress using the snapshots captured before
    // any format deletions (which cascade to reading_progress).

    // Build a lookup keyed by user_id for target's snapshot.
    let target_by_user: std::collections::HashMap<String, RpSnapshot> =
        target_rp.into_iter().map(|r| (r.user_id.clone(), r)).collect();

    for rp in source_rp {
        let target_snap = target_by_user.get(&rp.user_id);

        match payload.reading_progress_strategy.as_str() {
            "keep_target" => {
                // Resolve a valid format_id on the target book (the old
                // format may have been cascade-deleted when conflicting
                // formats were dropped).
                let target_fid: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM formats WHERE book_id = ? LIMIT 1",
                )
                .bind(&target_id)
                .fetch_optional(&mut *tx)
                .await
                .map_err(|_| AppError::Internal)?;

                if let Some(ts) = target_snap {
                    // Re-insert the target's saved progress with a valid
                    // format reference (the old one was cascade-deleted).
                    if let Some(ref fid) = target_fid {
                        let rp_id = Uuid::new_v4().to_string();
                        sqlx::query(
                            r#"
                            INSERT INTO reading_progress (id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified)
                            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                            ON CONFLICT(user_id, book_id) DO UPDATE SET
                                format_id = excluded.format_id,
                                cfi = excluded.cfi,
                                page = excluded.page,
                                percentage = excluded.percentage,
                                updated_at = excluded.updated_at,
                                last_modified = excluded.last_modified
                            "#,
                        )
                        .bind(&rp_id)
                        .bind(&ts.user_id)
                        .bind(&target_id)
                        .bind(fid)
                        .bind(ts.cfi.clone())
                        .bind(ts.page)
                        .bind(ts.percentage)
                        .bind(&ts.updated_at)
                        .bind(&now)
                        .execute(&mut *tx)
                        .await
                        .map_err(|_| AppError::Internal)?;
                    }
                } else if let Some(ref fid) = target_fid {
                    // Target had no progress for this user — insert source's.
                    let rp_id = Uuid::new_v4().to_string();
                    sqlx::query(
                        r#"
                        INSERT INTO reading_progress (id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified)
                        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                        "#,
                    )
                    .bind(&rp_id)
                    .bind(&rp.user_id)
                    .bind(&target_id)
                    .bind(fid)
                    .bind(rp.cfi)
                    .bind(rp.page)
                    .bind(rp.percentage)
                    .bind(&rp.updated_at)
                    .bind(&now)
                    .execute(&mut *tx)
                    .await
                    .map_err(|_| AppError::Internal)?;
                }
            }
            "keep_source" => {
                let rp_id = Uuid::new_v4().to_string();
                sqlx::query(
                    r#"
                    INSERT INTO reading_progress (id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(user_id, book_id) DO UPDATE SET
                        format_id = excluded.format_id,
                        cfi = excluded.cfi,
                        page = excluded.page,
                        percentage = excluded.percentage,
                        updated_at = excluded.updated_at,
                        last_modified = excluded.last_modified
                    "#,
                )
                .bind(&rp_id)
                .bind(&rp.user_id)
                .bind(&target_id)
                .bind(&rp.format_id)
                .bind(rp.cfi)
                .bind(rp.page)
                .bind(rp.percentage)
                .bind(&rp.updated_at)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(|_| AppError::Internal)?;
            }
            "merge_max" => {
                let merged_percentage = match target_snap {
                    Some(ts) => ts.percentage.max(rp.percentage),
                    None => rp.percentage,
                };
                let rp_id = Uuid::new_v4().to_string();
                sqlx::query(
                    r#"
                    INSERT INTO reading_progress (id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
                    ON CONFLICT(user_id, book_id) DO UPDATE SET
                        format_id = excluded.format_id,
                        cfi = excluded.cfi,
                        page = excluded.page,
                        percentage = excluded.percentage,
                        updated_at = excluded.updated_at,
                        last_modified = excluded.last_modified
                    "#,
                )
                .bind(&rp_id)
                .bind(&rp.user_id)
                .bind(&target_id)
                .bind(&rp.format_id)
                .bind(rp.cfi)
                .bind(rp.page)
                .bind(merged_percentage)
                .bind(&rp.updated_at)
                .bind(&now)
                .execute(&mut *tx)
                .await
                .map_err(|_| AppError::Internal)?;
            }
            _ => unreachable!(),
        }
    }

    // Clean up any remaining source reading progress rows.
    sqlx::query("DELETE FROM reading_progress WHERE book_id = ?")
        .bind(&source_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;

    // Step 6: Delete source book.
    let deleted = sqlx::query("DELETE FROM books WHERE id = ?")
        .bind(&source_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?
        .rows_affected();

    if deleted == 0 {
        tx.rollback()
            .await
            .map_err(|_| AppError::Internal)?;
        return Err(AppError::NotFound);
    }

    tx.commit().await.map_err(|_| AppError::Internal)?;

    Ok(Json(MergeExecResponse {
        merged: true,
        target_id,
        source_id,
    }))
}
