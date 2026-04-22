#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, TestContext};
use serde_json::Value;

#[tokio::test]
async fn test_create_token_returns_plain_token() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "name": "claude-desktop" }))
        .await;

    assert_status!(response, 201);
    let body: Value = response.json();
    let token_value = body["token"].as_str().unwrap_or_default();
    assert_eq!(token_value.len(), 64);
    assert!(body.get("token_hash").is_none());
}

#[tokio::test]
async fn test_token_authenticates_requests() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .json(&serde_json::json!({ "name": "mcp-client" }))
        .await;

    assert_status!(response, 201);
    let body: Value = response.json();
    let plain_token = body["token"].as_str().unwrap_or_default().to_string();

    let books_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&plain_token))
        .await;

    assert_status!(books_response, 200);
}

#[tokio::test]
async fn test_list_tokens_excludes_hash() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let create = ctx
        .server
        .post("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "name": "list-check" }))
        .await;
    assert_status!(create, 201);

    let response = ctx
        .server
        .get("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    let items = body["items"].as_array().cloned().unwrap_or_default();
    for item in items {
        assert!(item.get("token_hash").is_none());
    }
}

#[tokio::test]
async fn test_delete_token_revokes_auth() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;

    let create = ctx
        .server
        .post("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .json(&serde_json::json!({ "name": "revoked-client" }))
        .await;
    assert_status!(create, 201);
    let body: Value = create.json();
    let token_id = body["id"].as_str().unwrap_or_default().to_string();
    let plain_token = body["token"].as_str().unwrap_or_default().to_string();

    let delete = ctx
        .server
        .delete(&format!("/api/v1/admin/tokens/{token_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .await;
    assert_status!(delete, 204);

    let books_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&plain_token))
        .await;

    assert_status!(books_response, 401);
}

#[tokio::test]
async fn test_tokens_require_admin() {
    let ctx = TestContext::new().await;
    let user_token = ctx.user_token().await;

    let response = ctx
        .server
        .post("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, auth_header(&user_token))
        .json(&serde_json::json!({ "name": "forbidden" }))
        .await;

    assert_status!(response, 403);
}
