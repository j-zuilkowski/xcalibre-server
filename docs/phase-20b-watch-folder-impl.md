# Phase 20b — Watch Folder / Auto-Add Implementation

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite.
Phase 20a complete: failing tests in `backend/tests/test_watch_folder.rs`.

New Cargo dependency: `notify = "6"` (cross-platform filesystem watcher).

---

## 1. Cargo.toml — add `notify`

In `backend/Cargo.toml`, under `[dependencies]`:

```toml
notify = { version = "6", features = ["macos_fsevent"] }
```

---

## 2. Migration — `backend/migrations/sqlite/0030_watch_folder_log.sql`

```sql
CREATE TABLE IF NOT EXISTS watch_folder_log (
    id           TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    file_path    TEXT    NOT NULL,
    file_name    TEXT    NOT NULL,
    status       TEXT    NOT NULL DEFAULT 'pending',  -- pending | ingested | duplicate | error
    error        TEXT,
    book_id      TEXT    REFERENCES books(id) ON DELETE SET NULL,
    detected_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    processed_at INTEGER
);

CREATE INDEX IF NOT EXISTS idx_watch_folder_log_detected_at  ON watch_folder_log(detected_at);
CREATE INDEX IF NOT EXISTS idx_watch_folder_log_status       ON watch_folder_log(status);
```

Also create `backend/migrations/mariadb/0030_watch_folder_log.sql`:

```sql
CREATE TABLE IF NOT EXISTS watch_folder_log (
    id           VARCHAR(32)  NOT NULL PRIMARY KEY DEFAULT (LOWER(HEX(RANDOM_BYTES(16)))),
    file_path    TEXT         NOT NULL,
    file_name    VARCHAR(512) NOT NULL,
    status       VARCHAR(16)  NOT NULL DEFAULT 'pending',
    error        TEXT,
    book_id      VARCHAR(36)  REFERENCES books(id) ON DELETE SET NULL,
    detected_at  BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
    processed_at BIGINT
);

CREATE INDEX idx_watch_folder_log_detected_at ON watch_folder_log(detected_at);
CREATE INDEX idx_watch_folder_log_status      ON watch_folder_log(status);
```

---

## 3. Config — add `WatchFolderSection` to `backend/src/config.rs`

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WatchFolderSection {
    pub enabled: bool,
    pub path: String,
    pub interval_seconds: u64,
}

impl Default for WatchFolderSection {
    fn default() -> Self {
        Self {
            enabled: false,
            path: String::new(),
            interval_seconds: 30,
        }
    }
}
```

Add `pub watch_folder: WatchFolderSection` to `AppConfig`:

```rust
pub struct AppConfig {
    // ... existing fields ...
    pub watch_folder: WatchFolderSection,
}
```

---

## 4. Watch folder service — `backend/src/watch_folder.rs`

```rust
//! Background service that monitors a directory and feeds new ebook files
//! into the existing ingest pipeline.

use crate::{config::AppConfig, db::queries::books as book_queries, AppState};
use std::{
    collections::HashSet,
    path::Path,
    time::Duration,
};
use tracing::{debug, error, info, warn};

/// File extensions accepted by the watch folder.
const EBOOK_EXTENSIONS: &[&str] = &[
    "epub", "pdf", "mobi", "azw3", "cbz", "cbr", "fb2", "djvu", "txt", "rtf", "lit",
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
        warn!(path = %watch_dir.display(), "watch folder path does not exist or is not a directory");
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

        // Spawn ingest task
        let state_clone = state.clone();
        let path_clone = path.clone();
        tokio::spawn(async move {
            ingest_file(&state_clone, &path_clone).await;
        });
    }

    Ok(queued)
}

async fn ingest_file(state: &AppState, path: &Path) {
    let file_path_str = path.to_string_lossy().to_string();
    match crate::ingest::ingest_single_file(state, path).await {
        Ok(book_id) => {
            let now = chrono::Utc::now().timestamp();
            let _ = sqlx::query(
                "UPDATE watch_folder_log SET status='ingested', book_id=?, processed_at=? WHERE file_path=? AND status='pending'"
            )
            .bind(&book_id)
            .bind(now)
            .bind(&file_path_str)
            .execute(&state.db)
            .await;
            info!(path = %file_path_str, book_id, "watch folder: ingested");
        }
        Err(e) => {
            let now = chrono::Utc::now().timestamp();
            let _ = sqlx::query(
                "UPDATE watch_folder_log SET status='error', error=?, processed_at=? WHERE file_path=? AND status='pending'"
            )
            .bind(e.to_string())
            .bind(now)
            .bind(&file_path_str)
            .execute(&state.db)
            .await;
            error!(path = %file_path_str, error = %e, "watch folder: ingest failed");
        }
    }
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
```

Add `pub mod watch_folder;` to `backend/src/lib.rs`.

---

## 5. Route handlers — `backend/src/api/watch_folder.rs`

```rust
use crate::{error::AppError, watch_folder, AppState};
use axum::{extract::State, http::StatusCode, Json};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
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

pub async fn trigger_scan(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    feature_enabled(&state)?;
    let queued = watch_folder::scan_once(&state).await.map_err(|e| {
        tracing::error!(error = %e, "watch folder manual scan failed");
        AppError::InternalServerError
    })?;
    Ok(Json(json!({ "queued": queued })))
}

// ── GET /api/v1/admin/watch-folder/log ───────────────────────────────────────

#[derive(Deserialize)]
pub struct LogQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Serialize)]
pub struct WatchLogItem {
    pub id: String,
    pub file_name: String,
    pub status: String,
    pub error: Option<String>,
    pub book_id: Option<String>,
    pub detected_at: i64,
    pub processed_at: Option<i64>,
}

pub async fn get_log(
    State(state): State<AppState>,
    axum::extract::Query(q): axum::extract::Query<LogQuery>,
) -> Result<Json<Value>, AppError> {
    feature_enabled(&state)?;

    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);

    let items = sqlx::query_as!(
        WatchLogItem,
        r#"SELECT id, file_name, status, error, book_id, detected_at, processed_at
           FROM watch_folder_log
           ORDER BY detected_at DESC
           LIMIT ? OFFSET ?"#,
        limit,
        offset
    )
    .fetch_all(&state.db)
    .await?;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM watch_folder_log")
        .fetch_one(&state.db)
        .await
        .unwrap_or(0);

    Ok(Json(json!({ "items": items, "total": total })))
}
```

---

## 6. Router wiring — `backend/src/api/mod.rs`

```rust
// In the admin router (require_admin middleware):
.route("/admin/watch-folder/status", get(watch_folder::get_status))
.route("/admin/watch-folder/scan",   post(watch_folder::trigger_scan))
.route("/admin/watch-folder/log",    get(watch_folder::get_log))
```

---

## 7. App startup — `backend/src/main.rs`

After building `AppState` and before `axum::serve`:

```rust
if state.config.watch_folder.enabled {
    let watcher_state = state.clone();
    tokio::spawn(crate::watch_folder::start_background_watcher(watcher_state));
}
```

---

## 8. Update feature comparison

In `~/Documents/localProject/FEATURE_COMPARISON.md`:
```
| Watch folder / auto-add | ✓ | ✗ | ✗ | ✓ configurable |
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_watch_folder 2>&1 | tail -20
cargo clippy -- -D warnings 2>&1 | grep "^error" | head -10
```
Expected: **all tests pass**, zero clippy errors.

## Commit
```bash
git add backend/src/watch_folder.rs \
        backend/src/api/watch_folder.rs \
        backend/migrations/sqlite/0030_watch_folder_log.sql \
        backend/migrations/mariadb/0030_watch_folder_log.sql \
        backend/src/config.rs \
        backend/src/lib.rs \
        backend/src/api/mod.rs \
        backend/src/main.rs \
        backend/Cargo.toml
git commit -m "Phase 20b — watch folder auto-add (configurable enable/disable)"
```
