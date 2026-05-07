# Phase 28a — Admin API Gaps Tests

## Context
Rust 2021, Axum 0.7. TDD: write failing tests first.
Working dir: `~/Documents/localProject/xcalibre-server`

Add tests for remaining admin parity endpoints: log viewer, backup, thumbnail regeneration queue, task cancellation, and email domain allowlist/blocklist management.

---

## Write to: `backend/tests/test_admin_gaps.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, TestContext};
use serde_json::Value;
use tempfile::tempdir;
use std::fs;

#[tokio::test]
async fn test_admin_logs_happy_path_and_level_validation() {
    let dir = tempdir().expect("tempdir");
    let log_path = dir.path().join("app.log");

    fs::write(
        &log_path,
        "{\"level\":\"info\",\"msg\":\"a\"}\n{\"level\":\"error\",\"msg\":\"b\"}\n",
    )
    .expect("write log");

    let mut cfg = backend::config::AppConfig::default();
    cfg.log.file = Some(log_path.to_string_lossy().to_string());
    let ctx = TestContext::new_with_config(cfg).await;
    let admin = ctx.admin_token().await;
    let user = ctx.user_token().await;

    let ok = ctx
        .server
        .get("/api/v1/admin/logs?lines=50&level=error")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(ok, 200);
    let ok_body: Value = ok.json();
    assert!(ok_body.is_array());

    let bad = ctx
        .server
        .get("/api/v1/admin/logs?level=nope")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(bad, 400);

    let forbidden = ctx
        .server
        .get("/api/v1/admin/logs")
        .add_header(header::AUTHORIZATION, auth_header(&user))
        .await;
    assert_status!(forbidden, 403);
}

#[tokio::test]
async fn test_admin_backup_happy_path_and_conflict_guard() {
    let dir = tempdir().expect("tempdir");
    let mut cfg = backend::config::AppConfig::default();
    cfg.backup.dir = dir.path().to_string_lossy().to_string();
    let ctx = TestContext::new_with_config(cfg).await;
    let admin = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/backup")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    let path = body["path"].as_str().unwrap_or_default().to_string();
    assert!(!path.is_empty());

    let full = dir.path().join(path);
    assert!(full.exists());

    // Simulate concurrent backup in progress.
    ctx.set_backup_in_progress(true).await;
    let conflict = ctx
        .server
        .post("/api/v1/admin/backup")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(conflict, 409);
}

#[tokio::test]
async fn test_admin_cover_regenerate_enqueue_and_auth() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;
    let user = ctx.user_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/covers/regenerate")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({ "book_ids": [1, 2, 3] }))
        .await;
    assert_status!(resp, 202);
    let body: Value = resp.json();
    assert_eq!(body["queued"], 3);

    let forbidden = ctx
        .server
        .post("/api/v1/admin/covers/regenerate")
        .add_header(header::AUTHORIZATION, auth_header(&user))
        .json(&serde_json::json!({ "book_ids": [] }))
        .await;
    assert_status!(forbidden, 403);
}

#[tokio::test]
async fn test_admin_task_cancel_happy_404_and_409() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;

    let task_id = ctx.seed_task_with_status("paused").await;

    let cancel = ctx
        .server
        .delete(&format!("/api/v1/admin/tasks/{task_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(cancel, 200);

    let missing = ctx
        .server
        .delete("/api/v1/admin/tasks/does-not-exist")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(missing, 404);

    let done_id = ctx.seed_task_with_status("completed").await;
    let done = ctx
        .server
        .delete(&format!("/api/v1/admin/tasks/{done_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(done, 409);
}

#[tokio::test]
async fn test_admin_domains_crud_and_registration_enforcement() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;
    let user = ctx.user_token().await;

    let add = ctx
        .server
        .post("/api/v1/admin/domains")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({ "domain": "allowed.example", "allow": true }))
        .await;
    assert_status!(add, 201);
    let add_body: Value = add.json();
    let id = add_body["id"].as_i64().unwrap_or_default();

    let list_allow = ctx
        .server
        .get("/api/v1/admin/domains?allow=true")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(list_allow, 200);

    let del = ctx
        .server
        .delete(&format!("/api/v1/admin/domains/{id}"))
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(del, 204);

    let forbidden = ctx
        .server
        .get("/api/v1/admin/domains?allow=true")
        .add_header(header::AUTHORIZATION, auth_header(&user))
        .await;
    assert_status!(forbidden, 403);

    // Registration enforcement (allowlist mode):
    let add_back = ctx
        .server
        .post("/api/v1/admin/domains")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({ "domain": "allowed.example", "allow": true }))
        .await;
    assert_status!(add_back, 201);

    let blocked = ctx
        .server
        .post("/api/v1/auth/register")
        .json(&serde_json::json!({
            "email": "user@blocked.example",
            "password": "Password123!"
        }))
        .await;
    assert_status!(blocked, 400);

    let allowed = ctx
        .server
        .post("/api/v1/auth/register")
        .json(&serde_json::json!({
            "email": "user@allowed.example",
            "password": "Password123!"
        }))
        .await;
    assert_status!(allowed, 201);
}
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_admin_gaps 2>&1 | tail -80
```
Expected: **BUILD FAILED** — admin gaps endpoints and registration domain enforcement not implemented yet.

## Commit
```bash
git add backend/tests/test_admin_gaps.rs
git commit -m "Phase 28a — admin API gaps tests (failing)"
```
