# Phase 20a — Watch Folder / Auto-Add Tests

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite. TDD: write failing tests first.
Working dir: ~/Documents/localProject/xcalibre-server
Phase 19b complete: magic link login.

New Cargo dependency required in Phase 20b:
- `notify = "6"` (filesystem watcher — inotify/kqueue/FSEvents)

New surface introduced in Phase 20b:
- `WatchFolderSection` in `AppConfig` — `enabled: bool` (default `false`), `path: String` (default `""`), `interval_seconds: u64` (default `30`)
- Migration `0030_watch_folder_log.sql` — `watch_folder_log` table
- `WatchFolderService` actor — started at app boot when enabled; debounced file events → ingest pipeline
- `GET  /api/v1/admin/watch-folder/status` — returns `{ enabled, path, running, queued, processed_today }`
- `POST /api/v1/admin/watch-folder/scan`   — manual one-shot scan of the watch path
- `GET  /api/v1/admin/watch-folder/log`    — paginated log of processed files (default limit 50)

Both admin routes require `require_admin` middleware.
Both return 404 when `watch_folder.enabled = false`.

---

## Write to: `backend/tests/test_watch_folder.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::config::{AppConfig, WatchFolderSection};
use common::{auth_header, TestContext};
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;

fn watch_folder_config(enabled: bool, watch_dir: &str) -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.watch_folder.enabled = enabled;
    cfg.watch_folder.path = watch_dir.to_string();
    cfg.watch_folder.interval_seconds = 30;
    cfg
}

// ── Config defaults ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_watch_folder_disabled_by_default() {
    assert!(!AppConfig::default().watch_folder.enabled);
}

#[tokio::test]
async fn test_watch_folder_path_empty_by_default() {
    assert!(AppConfig::default().watch_folder.path.is_empty());
}

#[tokio::test]
async fn test_watch_folder_interval_default_30s() {
    assert_eq!(AppConfig::default().watch_folder.interval_seconds, 30);
}

// ── Status endpoint ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_watch_folder_status_404_when_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/watch-folder/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_watch_folder_status_200_when_enabled() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = TestContext::new_with_config(watch_folder_config(true, dir.path().to_str().unwrap())).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/watch-folder/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    assert_eq!(body["enabled"], true);
    assert!(body["path"].is_string());
    assert!(body["processed_today"].is_number());
}

#[tokio::test]
async fn test_watch_folder_status_requires_admin() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = TestContext::new_with_config(watch_folder_config(true, dir.path().to_str().unwrap())).await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/watch-folder/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 403);
}

// ── Manual scan endpoint ──────────────────────────────────────────────────────

#[tokio::test]
async fn test_watch_folder_scan_404_when_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/watch-folder/scan")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_watch_folder_scan_accepts_epub_file() {
    let dir = tempfile::tempdir().unwrap();
    // Copy a minimal EPUB fixture into the watch dir
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/test.epub");
    if fixture.exists() {
        fs::copy(&fixture, dir.path().join("test.epub")).unwrap();
    } else {
        fs::write(dir.path().join("test.epub"), b"PK\x03\x04fake-epub").unwrap();
    }

    let ctx = TestContext::new_with_config(watch_folder_config(true, dir.path().to_str().unwrap())).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/watch-folder/scan")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 202);
    let body: Value = resp.json();
    assert!(body["queued"].is_number());
}

#[tokio::test]
async fn test_watch_folder_scan_ignores_non_ebook_extensions() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("README.txt"), b"not a book").unwrap();
    fs::write(dir.path().join("cover.jpg"), b"fake image").unwrap();

    let ctx = TestContext::new_with_config(watch_folder_config(true, dir.path().to_str().unwrap())).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/watch-folder/scan")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 202);
    let body: Value = resp.json();
    assert_eq!(body["queued"], 0);
}

#[tokio::test]
async fn test_watch_folder_scan_requires_admin() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = TestContext::new_with_config(watch_folder_config(true, dir.path().to_str().unwrap())).await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/watch-folder/scan")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 403);
}

// ── Log endpoint ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_watch_folder_log_404_when_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/watch-folder/log")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_watch_folder_log_returns_array_when_empty() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = TestContext::new_with_config(watch_folder_config(true, dir.path().to_str().unwrap())).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/watch-folder/log")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    assert!(body["items"].is_array());
    assert!(body["total"].is_number());
}

#[tokio::test]
async fn test_watch_folder_scan_creates_log_entry() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("book.epub"), b"PK\x03\x04fake").unwrap();

    let ctx = TestContext::new_with_config(watch_folder_config(true, dir.path().to_str().unwrap())).await;
    let token = ctx.admin_token().await;

    // Trigger a scan
    ctx.server
        .post("/api/v1/admin/watch-folder/scan")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    // Wait briefly for async processing
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let resp = ctx
        .server
        .get("/api/v1/admin/watch-folder/log")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    let total = body["total"].as_i64().unwrap_or(0);
    assert!(total >= 1, "expected at least one log entry, got {total}");
}

#[tokio::test]
async fn test_watch_folder_log_requires_admin() {
    let dir = tempfile::tempdir().unwrap();
    let ctx = TestContext::new_with_config(watch_folder_config(true, dir.path().to_str().unwrap())).await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/watch-folder/log")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 403);
}
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_watch_folder 2>&1 | tail -20
```
Expected: **BUILD FAILED** — `WatchFolderSection`, `watch_folder_log` table, and route handlers do not exist yet.

## Commit
```bash
git add backend/tests/test_watch_folder.rs
git commit -m "Phase 20a — watch folder auto-add tests (failing)"
```
