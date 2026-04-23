#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::api::admin::clear_update_check_cache;
use common::{auth_header, TestContext};
use serde_json::json;
use std::sync::OnceLock;
use tokio::sync::Mutex;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

static RELEASES_URL_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[tokio::test]
async fn test_update_check_returns_current_version() {
    let _guard = RELEASES_URL_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .await;
    clear_update_check_cache().await;

    let mock_server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/autolibre/autolibre/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "tag_name": "v999.0.0",
            "html_url": "https://github.com/autolibre/autolibre/releases/tag/v999.0.0"
        })))
        .mount(&mock_server)
        .await;

    std::env::set_var(
        "AUTOLIBRE_RELEASES_URL",
        format!(
            "{}/repos/autolibre/autolibre/releases/latest",
            mock_server.uri()
        ),
    );

    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/admin/update-check")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    std::env::remove_var("AUTOLIBRE_RELEASES_URL");
    clear_update_check_cache().await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["current_version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(body["latest_version"], "999.0.0");
    assert_eq!(body["update_available"], true);
    assert_eq!(
        body["release_url"],
        "https://github.com/autolibre/autolibre/releases/tag/v999.0.0"
    );
}

#[tokio::test]
async fn test_update_check_github_unreachable_returns_503_gracefully() {
    let _guard = RELEASES_URL_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .await;
    clear_update_check_cache().await;

    std::env::set_var(
        "AUTOLIBRE_RELEASES_URL",
        "http://127.0.0.1:9/repos/autolibre/autolibre/releases/latest",
    );

    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/admin/update-check")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    std::env::remove_var("AUTOLIBRE_RELEASES_URL");
    clear_update_check_cache().await;

    assert_status!(response, 503);
    let body: serde_json::Value = response.json();
    assert_eq!(body["update_available"], false);
    assert_eq!(body["error"], "unreachable");
}

#[tokio::test]
async fn test_update_check_requires_admin() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .get("/api/v1/admin/update-check")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 403);
}
