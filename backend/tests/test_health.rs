#![allow(dead_code, unused_imports)]

mod common;

use common::TestContext;

#[tokio::test]
async fn test_health_returns_200_with_ok_status() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/health").await;
    assert_status!(response, 200);

    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "ok");
    assert_eq!(body["db"]["status"], "ok");
}

#[tokio::test]
async fn test_health_includes_version_string() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/health").await;
    assert_status!(response, 200);

    let body: serde_json::Value = response.json();
    let version = body["version"].as_str().unwrap_or_default();
    assert!(!version.is_empty());
    assert_eq!(version, env!("CARGO_PKG_VERSION"));
}

#[tokio::test]
async fn test_health_reports_search_disabled_when_meilisearch_not_configured() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/health").await;
    assert_status!(response, 200);

    let body: serde_json::Value = response.json();
    assert_eq!(body["search"]["status"], "disabled");
    assert!(body["search"]["error"].is_null());
}

#[tokio::test]
async fn test_health_requires_no_auth() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/health").await;
    assert_status!(response, 200);
}
