#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::{
    api::admin::clear_update_check_cache,
    config::{AppConfig, UpdaterSection},
};
use common::{auth_header, TestContext};
use serde_json::{json, Value};
use std::sync::OnceLock;
use tokio::sync::Mutex;
use wiremock::{
    matchers::{method, path, path_regex},
    Mock, MockServer, ResponseTemplate,
};

static UPDATE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

fn updater_config(enabled: bool, auto_update: bool) -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.updater.enabled = enabled;
    cfg.updater.auto_update = auto_update;
    cfg.updater.channel = "stable".to_string();
    cfg.updater.pre_update_hook = String::new();
    cfg.updater.block_if_hook_fails = true;
    cfg
}

fn updater_config_with_hook(hook: &str, block_on_fail: bool) -> AppConfig {
    let mut cfg = updater_config(true, false);
    cfg.updater.pre_update_hook = hook.to_string();
    cfg.updater.block_if_hook_fails = block_on_fail;
    cfg
}

// ── Config defaults ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_updater_enabled_by_default() {
    assert!(AppConfig::default().updater.enabled);
}

#[tokio::test]
async fn test_updater_auto_update_disabled_by_default() {
    assert!(!AppConfig::default().updater.auto_update);
}

#[tokio::test]
async fn test_updater_block_if_hook_fails_true_by_default() {
    assert!(AppConfig::default().updater.block_if_hook_fails);
}

#[tokio::test]
async fn test_updater_channel_default_stable() {
    assert_eq!(AppConfig::default().updater.channel, "stable");
}

// ── Status endpoint ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_updater_status_404_when_disabled() {
    let ctx = TestContext::new_with_config(updater_config(false, false)).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/system/update/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_updater_status_returns_config_when_enabled() {
    let ctx = TestContext::new_with_config(updater_config(true, false)).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/system/update/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    assert_eq!(body["enabled"], true);
    assert_eq!(body["auto_update"], false);
    assert_eq!(body["channel"], "stable");
    assert_eq!(body["block_if_hook_fails"], true);
}

#[tokio::test]
async fn test_updater_status_requires_admin() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .get("/api/v1/admin/system/update/status")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(resp, 403);
}

// ── Apply endpoint ────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_updater_apply_404_when_disabled() {
    let ctx = TestContext::new_with_config(updater_config(false, false)).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/system/update/apply")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({ "version": "999.0.0" }))
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_updater_apply_requires_admin() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/system/update/apply")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({ "version": "999.0.0" }))
        .await;
    assert_status!(resp, 403);
}

#[tokio::test]
async fn test_updater_apply_requires_version_field() {
    let ctx = TestContext::new_with_config(updater_config(true, false)).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/system/update/apply")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({}))
        .await;
    assert_status!(resp, 422);
}

#[tokio::test]
async fn test_updater_apply_downloads_and_verifies_checksum() {
    let _guard = UPDATE_LOCK.get_or_init(|| Mutex::new(())).lock().await;

    let mock = MockServer::start().await;

    let fake_binary = b"fake-binary-content";
    let checksum = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(fake_binary);
        hex::encode(h.finalize())
    };

    // Mock binary download
    Mock::given(method("GET"))
        .and(path_regex(r"/releases/download/v999\.0\.0/backend-.*"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fake_binary.to_vec()))
        .mount(&mock)
        .await;

    // Mock checksum file
    Mock::given(method("GET"))
        .and(path_regex(r"/releases/download/v999\.0\.0/backend-.*\.sha256"))
        .respond_with(ResponseTemplate::new(200).set_body_string(checksum.clone()))
        .mount(&mock)
        .await;

    std::env::set_var("XCS_RELEASES_DOWNLOAD_URL", mock.uri());
    let ctx = TestContext::new_with_config(updater_config(true, false)).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/system/update/apply")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({ "version": "999.0.0", "dry_run": true }))
        .await;
    std::env::remove_var("XCS_RELEASES_DOWNLOAD_URL");

    // dry_run = true: download + verify checksum but do not replace binary
    assert_status!(resp, 200);
    let body: Value = resp.json();
    assert_eq!(body["checksum_ok"], true);
}

#[tokio::test]
async fn test_updater_apply_aborts_on_checksum_mismatch() {
    let _guard = UPDATE_LOCK.get_or_init(|| Mutex::new(())).lock().await;

    let mock = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path_regex(r"/releases/download/v999\.0\.0/backend-.*"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"fake-binary".to_vec()))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path_regex(r"/releases/download/v999\.0\.0/backend-.*\.sha256"))
        .respond_with(ResponseTemplate::new(200).set_body_string("deadbeefdeadbeef".to_string()))
        .mount(&mock)
        .await;

    std::env::set_var("XCS_RELEASES_DOWNLOAD_URL", mock.uri());
    let ctx = TestContext::new_with_config(updater_config(true, false)).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/system/update/apply")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({ "version": "999.0.0", "dry_run": true }))
        .await;
    std::env::remove_var("XCS_RELEASES_DOWNLOAD_URL");

    assert_status!(resp, 422);
    let body: Value = resp.json();
    assert_eq!(body["checksum_ok"], false);
}

#[tokio::test]
async fn test_updater_pre_hook_failure_blocks_update_when_configured() {
    let _guard = UPDATE_LOCK.get_or_init(|| Mutex::new(())).lock().await;

    let mock = MockServer::start().await;
    let fake_binary = b"fake-binary-content";
    let checksum = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(fake_binary);
        hex::encode(h.finalize())
    };
    Mock::given(method("GET"))
        .and(path_regex(r"/releases/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fake_binary.to_vec()))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"\.sha256"))
        .respond_with(ResponseTemplate::new(200).set_body_string(checksum))
        .mount(&mock)
        .await;

    std::env::set_var("XCS_RELEASES_DOWNLOAD_URL", mock.uri());
    // Hook exits non-zero; block_if_hook_fails = true
    let cfg = updater_config_with_hook("exit 1", true);
    let ctx = TestContext::new_with_config(cfg).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/system/update/apply")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({ "version": "999.0.0" }))
        .await;
    std::env::remove_var("XCS_RELEASES_DOWNLOAD_URL");

    assert_status!(resp, 409);
    let body: Value = resp.json();
    assert!(body["error"].as_str().unwrap_or("").contains("pre-update hook"));
}

#[tokio::test]
async fn test_updater_pre_hook_failure_does_not_block_when_not_configured() {
    // With block_if_hook_fails = false, a failing hook is a warning, not an abort.
    // We use dry_run=true so no binary replacement happens in the test.
    let _guard = UPDATE_LOCK.get_or_init(|| Mutex::new(())).lock().await;

    let mock = MockServer::start().await;
    let fake_binary = b"fake-binary-content";
    let checksum = {
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(fake_binary);
        hex::encode(h.finalize())
    };
    Mock::given(method("GET"))
        .and(path_regex(r"/releases/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(fake_binary.to_vec()))
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .and(path_regex(r"\.sha256"))
        .respond_with(ResponseTemplate::new(200).set_body_string(checksum))
        .mount(&mock)
        .await;

    std::env::set_var("XCS_RELEASES_DOWNLOAD_URL", mock.uri());
    let cfg = updater_config_with_hook("exit 1", false); // block = false
    let ctx = TestContext::new_with_config(cfg).await;
    let token = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/system/update/apply")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({ "version": "999.0.0", "dry_run": true }))
        .await;
    std::env::remove_var("XCS_RELEASES_DOWNLOAD_URL");

    // Should succeed despite hook failure (block_if_hook_fails = false)
    assert_status!(resp, 200);
    let body: Value = resp.json();
    assert_eq!(body["hook_ok"], false);
    assert_eq!(body["checksum_ok"], true);
}
