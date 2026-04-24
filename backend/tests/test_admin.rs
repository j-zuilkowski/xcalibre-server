#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, TestContext};

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
