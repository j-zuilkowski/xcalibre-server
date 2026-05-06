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
