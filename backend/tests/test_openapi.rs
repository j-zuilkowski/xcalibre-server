#![allow(dead_code, unused_imports)]

mod common;

use common::TestContext;

#[tokio::test]
async fn test_openapi_json_endpoint_returns_200() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/docs/openapi.json").await;
    assert_status!(response, 200);
}

#[tokio::test]
async fn test_openapi_json_is_valid_json() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/docs/openapi.json").await;
    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body.is_object());
    assert!(body.get("openapi").is_some());
}

#[tokio::test]
async fn test_openapi_json_contains_books_path() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/docs/openapi.json").await;
    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body["paths"].get("/api/v1/books").is_some());
}

#[tokio::test]
async fn test_openapi_json_requires_no_auth() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/docs/openapi.json").await;
    assert_status!(response, 200);
}

#[tokio::test]
async fn test_swagger_ui_returns_200() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/docs/").await;
    assert_status!(response, 200);
    let body = response.text();
    assert!(body.to_lowercase().contains("swagger"));
}
