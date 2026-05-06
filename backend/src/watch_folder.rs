//! Background service that monitors a directory and feeds new ebook files
//! into the existing ingest pipeline.

use crate::AppState;
use std::{path::Path, time::Duration};
use tracing::{debug, error, info, warn};

/// File extensions accepted by the watch folder.
const EBOOK_EXTENSIONS: &[&str] = &[
    "epub", "pdf", "mobi", "azw3", "cbz", "cbr", "fb2", "djvu", "rtf", "lit",
];

pub fn is_ebook(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| EBOOK_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Scan the watch folder once and queue new ebook files for ingestion.
/// Returns the number of files queued.
pub async fn scan_once(state: &AppState) -> anyhow::Result<u64> {
    let cfg = &state.config.watch_folder;
    if cfg.path.is_empty() {
        return Ok(0);
    }

    let watch_dir = std::path::PathBuf::from(&cfg.path);
    if !watch_dir.is_dir() {
        warn!(
            path = %watch_dir.display(),
            "watch folder path does not exist or is not a directory"
        );
        return Ok(0);
    }

    let mut queued: u64 = 0;

    for entry in std::fs::read_dir(&watch_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() || !is_ebook(&path) {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let file_path_str = path.to_string_lossy().to_string();

        // Skip if already in log (any status)
        let already_logged: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM watch_folder_log WHERE file_path = ?",
        )
        .bind(&file_path_str)
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

        if already_logged > 0 {
            continue;
        }

        // Insert pending log entry
        sqlx::query(
            "INSERT INTO watch_folder_log (file_path, file_name, status) VALUES (?, ?, 'pending')",
        )
        .bind(&file_path_str)
        .bind(&file_name)
        .execute(&state.db)
        .await?;

        queued += 1;

        debug!(path = %file_path_str, "watch folder: queued for ingest");
    }

    Ok(queued)
}

/// Start the background polling loop. Call once at server startup when enabled.
pub async fn start_background_watcher(state: AppState) {
    let interval = Duration::from_secs(state.config.watch_folder.interval_seconds);
    info!(
        path = %state.config.watch_folder.path,
        interval_secs = interval.as_secs(),
        "watch folder: background watcher started"
    );
    loop {
        if let Err(e) = scan_once(&state).await {
            error!(error = %e, "watch folder: scan error");
        }
        tokio::time::sleep(interval).await;
    }
}
