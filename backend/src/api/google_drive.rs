use crate::{
    error::AppError,
    storage_google_drive::GoogleDriveBackend,
    AppState,
};
use axum::{extract::State, http::StatusCode, Json};
use serde::Deserialize;
use serde_json::{json, Value};

fn feature_enabled(state: &AppState) -> Result<GoogleDriveBackend, AppError> {
    if state.config.storage.google_drive.enabled {
        Ok(GoogleDriveBackend::new(state.config.storage.google_drive.clone()))
    } else {
        Err(AppError::NotFound)
    }
}

// ── GET /api/v1/admin/storage/google-drive/status ────────────────────────────

pub async fn get_status(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let client = feature_enabled(&state)?;

    match client.get_about().await {
        Ok(about) => {
            let used = about
                .storage_quota
                .usage
                .as_deref()
                .and_then(|s| s.parse::<i64>().ok())
                .unwrap_or(0);
            let limit = about
                .storage_quota
                .limit
                .as_deref()
                .and_then(|s| s.parse::<i64>().ok());
            Ok(Json(json!({
                "connected": true,
                "quota_used_bytes": used,
                "quota_limit_bytes": limit,
                "folder_id": state.config.storage.google_drive.folder_id,
            })))
        }
        Err(e) => {
            tracing::warn!(error = %e, "Google Drive status check failed");
            Ok(Json(json!({ "connected": false, "error": e.to_string() })))
        }
    }
}

// ── POST /api/v1/admin/storage/google-drive/sync ─────────────────────────────

#[derive(Deserialize, Default)]
pub struct SyncRequest {
    /// If empty, syncs all books that have no Drive mapping yet.
    pub book_ids: Option<Vec<String>>,
}

pub async fn trigger_sync(
    State(state): State<AppState>,
    Json(body): Json<SyncRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    feature_enabled(&state)?; // validate feature enabled before spawning

    // Determine which book ids to sync
    let ids: Vec<String> = match body.book_ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => {
            // All books without a Drive mapping
            sqlx::query_scalar(
                "SELECT id FROM books WHERE id NOT IN (SELECT book_id FROM google_drive_files)",
            )
            .fetch_all(&state.db)
            .await
            .unwrap_or_default()
        }
    };

    let queued = ids.len() as i64;

    // Spawn background uploads
    for book_id in ids {
        let state_clone = state.clone();
        tokio::spawn(async move {
            sync_book(&state_clone, &book_id).await;
        });
    }

    Ok((StatusCode::ACCEPTED, Json(json!({ "queued": queued }))))
}

async fn sync_book(state: &AppState, book_id: &str) {
    let client = GoogleDriveBackend::new(state.config.storage.google_drive.clone());

    // Find formats for this book
    let formats: Vec<(String, String)> = match sqlx::query_as(
        "SELECT path, format FROM formats WHERE book_id = ?",
    )
    .bind(book_id)
    .fetch_all(&state.db)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(book_id, error = %e, "gdrive sync: failed to fetch formats");
            return;
        }
    };

    let storage_root = std::path::Path::new(&state.config.app.storage_path);
    for (file_path, _format) in formats {
        let path = storage_root.join(&file_path);
        let file_name = std::path::Path::new(&file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("book");

        match client.upload_file(&path, file_name).await {
            Ok(drive_file) => {
                let bytes = tokio::fs::metadata(&path)
                    .await
                    .map(|m| m.len() as i64)
                    .unwrap_or(0);
                let _ = sqlx::query(
                    "INSERT OR REPLACE INTO google_drive_files \
                     (book_id, local_path, drive_file_id, drive_name, bytes) \
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(book_id)
                .bind(&file_path)
                .bind(&drive_file.id)
                .bind(&drive_file.name)
                .bind(bytes)
                .execute(&state.db)
                .await;
                tracing::info!(book_id, drive_id = %drive_file.id, "gdrive sync: uploaded");
            }
            Err(e) => {
                tracing::warn!(book_id, error = %e, "gdrive sync: upload failed");
            }
        }
    }
}
