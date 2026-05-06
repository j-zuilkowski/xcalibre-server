# Phase 21b — Google Drive Storage Backend Implementation

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite.
Phase 21a complete: failing tests in `backend/tests/test_storage_google_drive.rs`.
No new Cargo dependencies — uses `reqwest` and `oauth2` already present.

---

## 1. Migration — `backend/migrations/sqlite/0031_google_drive_files.sql`

```sql
CREATE TABLE IF NOT EXISTS google_drive_files (
    id           TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    book_id      TEXT    NOT NULL REFERENCES books(id) ON DELETE CASCADE,
    local_path   TEXT    NOT NULL,
    drive_file_id TEXT   NOT NULL,
    drive_name   TEXT    NOT NULL,
    synced_at    INTEGER NOT NULL DEFAULT (unixepoch()),
    bytes        INTEGER NOT NULL DEFAULT 0,
    status       TEXT    NOT NULL DEFAULT 'synced'  -- synced | error | deleted
);

CREATE INDEX IF NOT EXISTS idx_gdf_book_id      ON google_drive_files(book_id);
CREATE INDEX IF NOT EXISTS idx_gdf_drive_file_id ON google_drive_files(drive_file_id);
```

Also create `backend/migrations/mariadb/0031_google_drive_files.sql`:

```sql
CREATE TABLE IF NOT EXISTS google_drive_files (
    id            VARCHAR(32)  NOT NULL PRIMARY KEY DEFAULT (LOWER(HEX(RANDOM_BYTES(16)))),
    book_id       VARCHAR(36)  NOT NULL,
    local_path    TEXT         NOT NULL,
    drive_file_id VARCHAR(255) NOT NULL,
    drive_name    VARCHAR(512) NOT NULL,
    synced_at     BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
    bytes         BIGINT       NOT NULL DEFAULT 0,
    status        VARCHAR(16)  NOT NULL DEFAULT 'synced',
    CONSTRAINT fk_gdf_book FOREIGN KEY (book_id) REFERENCES books(id) ON DELETE CASCADE
);

CREATE INDEX idx_gdf_book_id       ON google_drive_files(book_id);
CREATE INDEX idx_gdf_drive_file_id ON google_drive_files(drive_file_id);
```

---

## 2. Config — add `GoogleDriveSection` to `backend/src/config.rs`

```rust
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GoogleDriveSection {
    pub enabled: bool,
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub folder_id: String,
    /// OAuth2 token endpoint — overridable in tests.
    pub token_endpoint: String,
    /// Drive API base URL — overridable in tests.
    pub drive_endpoint: String,
}

impl Default for GoogleDriveSection {
    fn default() -> Self {
        Self {
            enabled: false,
            client_id: String::new(),
            client_secret: String::new(),
            refresh_token: String::new(),
            folder_id: String::new(),
            token_endpoint: "https://oauth2.googleapis.com/token".to_string(),
            drive_endpoint: "https://www.googleapis.com".to_string(),
        }
    }
}

impl std::fmt::Debug for GoogleDriveSection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleDriveSection")
            .field("enabled", &self.enabled)
            .field("client_id", &self.client_id)
            .field("client_secret", &"[redacted]")
            .field("refresh_token", &"[redacted]")
            .field("folder_id", &self.folder_id)
            .finish()
    }
}
```

Add `pub google_drive: GoogleDriveSection` to `StorageSection`:

```rust
pub struct StorageSection {
    pub backend: String,
    pub s3: S3Section,
    pub google_drive: GoogleDriveSection,
}
```

---

## 3. Google Drive client — `backend/src/storage/google_drive.rs`

```rust
//! Google Drive storage backend.
//!
//! Uses the Drive v3 REST API via `reqwest`. OAuth2 tokens are refreshed
//! automatically using the configured `refresh_token`.

use crate::{config::GoogleDriveSection, error::AppError};
use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone)]
pub struct GoogleDriveBackend {
    cfg: GoogleDriveSection,
    http: Client,
    /// Cached access token. Refreshed on 401 or expiry.
    token: Arc<Mutex<Option<String>>>,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
}

#[derive(Deserialize)]
pub struct DriveFile {
    pub id: String,
    pub name: String,
}

#[derive(Deserialize)]
pub struct AboutResponse {
    #[serde(rename = "storageQuota")]
    pub storage_quota: StorageQuota,
}

#[derive(Deserialize, Serialize)]
pub struct StorageQuota {
    pub limit: Option<String>,
    pub usage: Option<String>,
}

impl GoogleDriveBackend {
    pub fn new(cfg: GoogleDriveSection) -> Self {
        Self {
            cfg,
            http: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("build reqwest client"),
            token: Arc::new(Mutex::new(None)),
        }
    }

    /// Fetch or refresh the access token.
    pub async fn access_token(&self) -> Result<String> {
        let mut guard = self.token.lock().await;
        if let Some(t) = guard.as_ref() {
            return Ok(t.clone());
        }
        let resp = self
            .http
            .post(&self.cfg.token_endpoint)
            .form(&[
                ("client_id", self.cfg.client_id.as_str()),
                ("client_secret", self.cfg.client_secret.as_str()),
                ("refresh_token", self.cfg.refresh_token.as_str()),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .context("token endpoint request")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("token refresh failed: {status} — {body}");
        }

        let token_resp: TokenResponse = resp.json().await.context("parse token response")?;
        *guard = Some(token_resp.access_token.clone());
        Ok(token_resp.access_token)
    }

    /// Invalidate cached token (call after 401 from Drive API).
    pub async fn invalidate_token(&self) {
        *self.token.lock().await = None;
    }

    /// Upload a file to the configured Drive folder. Returns the Drive file ID.
    pub async fn upload_file(
        &self,
        local_path: &std::path::Path,
        file_name: &str,
    ) -> Result<DriveFile> {
        let token = self.access_token().await?;
        let data = tokio::fs::read(local_path)
            .await
            .context("read local file")?;

        let metadata = serde_json::json!({
            "name": file_name,
            "parents": [self.cfg.folder_id]
        });

        let upload_url = format!(
            "{}/upload/drive/v3/files?uploadType=multipart",
            self.cfg.drive_endpoint.trim_end_matches('/')
        );

        let form = reqwest::multipart::Form::new()
            .text("metadata", metadata.to_string())
            .part(
                "file",
                reqwest::multipart::Part::bytes(data).file_name(file_name.to_string()),
            );

        let resp = self
            .http
            .post(&upload_url)
            .bearer_auth(&token)
            .multipart(form)
            .send()
            .await
            .context("Drive upload request")?;

        if resp.status() == 401 {
            self.invalidate_token().await;
            anyhow::bail!("Drive upload: unauthorized (token may have been revoked)");
        }

        let file: DriveFile = resp.json().await.context("parse Drive upload response")?;
        Ok(file)
    }

    /// Get quota information from Drive About endpoint.
    pub async fn get_about(&self) -> Result<AboutResponse> {
        let token = self.access_token().await?;
        let url = format!(
            "{}/drive/v3/about?fields=storageQuota",
            self.cfg.drive_endpoint.trim_end_matches('/')
        );
        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .context("Drive about request")?;

        if resp.status() == 401 {
            self.invalidate_token().await;
            anyhow::bail!("Drive about: unauthorized");
        }

        resp.json().await.context("parse Drive about response")
    }
}
```

Add `pub mod google_drive;` to `backend/src/storage/mod.rs` (or create the file if it doesn't exist).

---

## 4. Route handlers — `backend/src/api/google_drive.rs`

```rust
use crate::{error::AppError, storage::google_drive::GoogleDriveBackend, AppState};
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
) -> Result<Json<Value>, AppError> {
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

    Ok(Json(json!({ "queued": queued })))
}

async fn sync_book(state: &AppState, book_id: &str) {
    let client = GoogleDriveBackend::new(state.config.storage.google_drive.clone());

    // Find formats for this book
    let formats: Vec<(String, String)> = match sqlx::query_as(
        "SELECT file_path, format FROM book_formats WHERE book_id = ?",
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

    for (file_path, _format) in formats {
        let path = std::path::Path::new(&file_path);
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("book");

        match client.upload_file(path, file_name).await {
            Ok(drive_file) => {
                let bytes = tokio::fs::metadata(path)
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
```

---

## 5. Router wiring — `backend/src/api/mod.rs`

```rust
// In the admin router (require_admin middleware):
.route("/admin/storage/google-drive/status", get(google_drive::get_status))
.route("/admin/storage/google-drive/sync",   post(google_drive::trigger_sync))
```

---

## 6. Update feature comparison

In `~/Documents/localProject/FEATURE_COMPARISON.md`:
```
| Google Drive backend | ✗ | ✓ | ✗ | ✓ configurable |
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_storage_google_drive 2>&1 | tail -20
cargo clippy -- -D warnings 2>&1 | grep "^error" | head -10
```
Expected: **all tests pass**, zero clippy errors.

## Commit
```bash
git add backend/src/storage/google_drive.rs \
        backend/src/api/google_drive.rs \
        backend/migrations/sqlite/0031_google_drive_files.sql \
        backend/migrations/mariadb/0031_google_drive_files.sql \
        backend/src/config.rs \
        backend/src/storage/mod.rs \
        backend/src/api/mod.rs
git commit -m "Phase 21b — Google Drive storage backend (configurable enable/disable)"
```
