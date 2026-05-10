#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, TestContext};
use tempfile::tempdir;

#[tokio::test]
async fn test_admin_users_requires_authentication() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/v1/admin/users").await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_admin_users_rejects_non_admin_users() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .get("/api/v1/admin/users")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 403);
}

#[tokio::test]
async fn test_admin_users_allows_admin_users() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/admin/users")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body.as_array().is_some(), "expected a JSON array");
}

#[tokio::test]
async fn test_admin_authors_requires_authentication() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/v1/admin/authors").await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_admin_authors_rejects_non_admin_users() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .get("/api/v1/admin/authors")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 403);
}

#[tokio::test]
async fn test_admin_authors_allows_admin_users() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/admin/authors")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(
        body["items"].is_array(),
        "expected a paginated response body"
    );
}

#[tokio::test]
async fn test_backup_creates_valid_sqlite_file() {
    let db_dir     = tempdir().expect("db tempdir");
    let backup_dir = tempdir().expect("backup tempdir");
    let db_path = format!("sqlite://{}/library.db", db_dir.path().display());

    let mut config = backend::config::AppConfig::default();
    config.backup.dir = backup_dir.path().to_string_lossy().to_string();
    let ctx = TestContext::new_with_file_db(&db_path, config).await;

    let token = ctx.admin_token().await;
    let response = ctx.server.post("/api/v1/admin/backup")
        .add_header(header::AUTHORIZATION, format!("Bearer {token}").parse().unwrap())
        .await;
    assert_status!(response, 200);

    let body: serde_json::Value = response.json();
    let fname = body["path"].as_str().expect("response must contain 'path'");
    assert!(fname.ends_with(".db"), "got {fname}");

    let dest = backup_dir.path().join(fname);
    assert!(dest.exists(), "backup file not created at {dest:?}");
    let bytes = std::fs::read(&dest).expect("read backup file");
    assert!(bytes.len() >= 16, "file too small: {} bytes", bytes.len());
    assert_eq!(&bytes[..16], b"SQLite format 3\0", "not a SQLite file");
}

