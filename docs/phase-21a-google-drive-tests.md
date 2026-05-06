# Phase 21a — Google Drive Storage Backend Tests

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite. TDD: write failing tests first.
Working dir: ~/Documents/localProject/xcalibre-server
Phase 20b complete: watch folder auto-add.

No new Cargo dependencies required — uses `reqwest` and `oauth2` already in the codebase.

New surface introduced in Phase 21b:
- `GoogleDriveSection` added to `StorageSection` — `enabled: bool` (default `false`), `client_id`, `client_secret`, `refresh_token`, `folder_id`, `token_endpoint` (overridable for tests)
- `storage.backend` gains a new valid value: `"google_drive"`
- `GoogleDriveBackend` implements the existing `StorageBackend` trait
- Migration `0031_google_drive_files.sql` — `google_drive_files` mapping table (local path ↔ Drive file ID)
- `GET  /api/v1/admin/storage/google-drive/status` — auth check + quota info
- `POST /api/v1/admin/storage/google-drive/sync`   — re-sync all books to Drive (queues background job)

When `storage.backend = "google_drive"` and `google_drive.enabled = false`, file operations fall through to the local backend (safe default).

---

## Write to: `backend/tests/test_storage_google_drive.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::config::{AppConfig, GoogleDriveSection, StorageSection};
use common::{auth_header, TestContext};
use serde_json::{json, Value};
use wiremock::{
    matchers::{method, path_regex},
    Mock, MockServer, ResponseTemplate,
};

fn google_drive_config(enabled: bool, token_endpoint: &str, drive_endpoint: &str) -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.storage.backend = if enabled {
        "google_drive".to_string()
    } else {
        "local".to_string()
    };
    cfg.storage.google_drive.enabled = enabled;
    cfg.storage.google_drive.client_id = "test-client-id".to_string();
    cfg.storage.google_drive.client_secret = "test-client-secret".to_string();
    cfg.storage.google_drive.refresh_token = "test-refresh-token".to_string();
    cfg.storage.google_drive.folder_id = "test-folder-id".to_string();
    cfg.storage.google_drive.token_endpoint = token_endpoint.to_string();
    cfg.storage.google_drive.drive_endpoint = drive_endpoint.to_string();
    cfg
}

fn mock_token_response() -> serde_json::Value {
    json!({
        "access_token": "ya29.test-access-token",
        "token_type": "Bearer",
        "expires_in": 3600
    })
}

// ── Config defaults ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_google_drive_disabled_by_default() {
    assert!(!AppConfig::default().storage.google_drive.enabled);
}

#[tokio::test]
async fn test_google_drive_storage_backend_default_is_local() {
    assert_eq!(AppConfig::default().storage.backend, "local");
}

// ── Status endpoint ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_google_drive_status_404_when_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/storage/google-drive/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_google_drive_status_returns_connected_when_token_ok() {
    let mock = MockServer::start().await;

    // Mock Google OAuth token endpoint
    Mock::given(method("POST"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(mock_token_response()),
        )
        .mount(&mock)
        .await;

    // Mock Drive about endpoint (quota info)
    Mock::given(method("GET"))
        .and(path_regex(r"/drive/v3/about"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "storageQuota": {
                "limit": "16106127360",
                "usage": "1073741824"
            }
        })))
        .mount(&mock)
        .await;

    let cfg = google_drive_config(true, &format!("{}/token", mock.uri()), &mock.uri());
    let ctx = TestContext::new_with_config(cfg).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/storage/google-drive/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    assert_eq!(body["connected"], true);
    assert!(body["quota_used_bytes"].is_number());
    assert!(body["quota_limit_bytes"].is_number());
}

#[tokio::test]
async fn test_google_drive_status_requires_admin() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/storage/google-drive/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 403);
}

// ── File upload routing ───────────────────────────────────────────────────────

#[tokio::test]
async fn test_google_drive_file_stored_locally_when_disabled() {
    // When backend = "local", uploads go to local storage (existing behaviour unchanged)
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let epub_bytes = include_bytes!("fixtures/test.epub");
    let resp = ctx
        .server
        .post("/api/v1/books/upload")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .add_header(
            header::CONTENT_TYPE,
            "multipart/form-data; boundary=----TestBoundary",
        )
        .bytes(
            format!(
                "------TestBoundary\r\nContent-Disposition: form-data; name=\"file\"; filename=\"test.epub\"\r\nContent-Type: application/epub+zip\r\n\r\n{}\r\n------TestBoundary--\r\n",
                std::str::from_utf8(epub_bytes).unwrap_or("")
            )
            .into_bytes(),
        )
        .await;

    // 200 or 201 — book stored locally, no Drive interaction
    assert!(
        resp.status_code().as_u16() < 300,
        "upload should succeed locally when GDrive disabled"
    );
}

#[tokio::test]
async fn test_google_drive_mapping_row_created_after_upload() {
    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_regex(r"/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(mock_token_response()),
        )
        .mount(&mock)
        .await;

    // Mock multipart upload
    Mock::given(method("POST"))
        .and(path_regex(r"/upload/drive/v3/files"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "id": "drive-file-id-abc123",
            "name": "test.epub"
        })))
        .mount(&mock)
        .await;

    let cfg = google_drive_config(true, &format!("{}/token", mock.uri()), &mock.uri());
    let ctx = TestContext::new_with_config(cfg).await;
    let token = ctx.admin_token().await;

    // Ingest a book so there's a file to map
    let book = ctx.create_book("Drive Book", "Test Author").await;
    let fake_path = format!("{}/drive_book.epub", ctx.storage.path().to_str().unwrap());
    std::fs::write(&fake_path, b"PK\x03\x04fake-epub").unwrap();

    // Trigger sync for this specific book
    let resp = ctx
        .server
        .post("/api/v1/admin/storage/google-drive/sync")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({ "book_ids": [book.id] }))
        .await;
    assert_status!(resp, 202);

    // Wait briefly for async upload
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM google_drive_files WHERE book_id = ?")
            .bind(&book.id)
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert!(count >= 1, "expected at least one google_drive_files row for book");
}

// ── Sync endpoint ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_google_drive_sync_404_when_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/storage/google-drive/sync")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({}))
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_google_drive_sync_requires_admin() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/storage/google-drive/sync")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({}))
        .await;
    assert_status!(resp, 403);
}

#[tokio::test]
async fn test_google_drive_sync_full_returns_202() {
    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path_regex(r"/token"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(mock_token_response()),
        )
        .mount(&mock)
        .await;

    let cfg = google_drive_config(true, &format!("{}/token", mock.uri()), &mock.uri());
    let ctx = TestContext::new_with_config(cfg).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/storage/google-drive/sync")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({}))
        .await;
    assert_status!(resp, 202);
    let body: Value = resp.json();
    assert!(body["queued"].is_number());
}
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_storage_google_drive 2>&1 | tail -20
```
Expected: **BUILD FAILED** — `GoogleDriveSection`, `google_drive_files` table, and route handlers do not exist yet.

## Commit
```bash
git add backend/tests/test_storage_google_drive.rs
git commit -m "Phase 21a — Google Drive storage backend tests (failing)"
```
