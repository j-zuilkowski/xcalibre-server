#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use chrono::Utc;
use common::{auth_header, TestContext};
use serde_json::Value;

async fn create_api_token(
    ctx: &TestContext,
    auth_token: &str,
    name: &str,
    expires_in_days: Option<u64>,
) -> Value {
    let mut payload = serde_json::json!({ "name": name });
    if let Some(days) = expires_in_days {
        payload["expires_in_days"] = serde_json::json!(days);
    }

    let response = ctx
        .server
        .post("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, auth_header(auth_token))
        .json(&payload)
        .await;

    assert_status!(response, 201);
    response.json()
}

#[tokio::test]
async fn test_api_token_create_returns_plain_token() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let body = create_api_token(&ctx, &token, "claude-desktop", None).await;
    let token_value = body["token"].as_str().unwrap_or_default();
    assert_eq!(token_value.len(), 64);
    assert!(body.get("token_hash").is_none());
}

#[tokio::test]
async fn test_api_token_with_no_expires_at_is_accepted_indefinitely() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;

    let body = create_api_token(&ctx, &admin_token, "mcp-client", None).await;
    let plain_token = body["token"].as_str().unwrap_or_default().to_string();

    let books_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&plain_token))
        .await;

    assert_status!(books_response, 200);
}

#[tokio::test]
async fn test_expired_api_token_returns_401() {
    let ctx = TestContext::new().await;
    let (admin_user, admin_password) = ctx.create_admin().await;
    let admin_token = ctx
        .login(&admin_user.username, &admin_password)
        .await
        .access_token;

    let body = create_api_token(&ctx, &admin_token, "expired-client", Some(1)).await;
    let token_id = body["id"].as_str().unwrap_or_default().to_string();
    let plain_token = body["token"].as_str().unwrap_or_default().to_string();

    sqlx::query(
        r#"
        UPDATE api_tokens
        SET expires_at = ?
        WHERE id = ?
        "#,
    )
    .bind(Utc::now().timestamp() - 60)
    .bind(&token_id)
    .execute(&ctx.db)
    .await
    .expect("expire token");

    let books_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&plain_token))
        .await;

    assert_status!(books_response, 401);
}

#[tokio::test]
async fn test_deleted_users_api_token_returns_401() {
    let ctx = TestContext::new().await;
    let (admin_user, admin_password) = ctx.create_admin().await;
    let admin_token = ctx
        .login(&admin_user.username, &admin_password)
        .await
        .access_token;

    let body = create_api_token(&ctx, &admin_token, "deleted-client", None).await;
    let plain_token = body["token"].as_str().unwrap_or_default().to_string();

    sqlx::query(
        r#"
        DELETE FROM users
        WHERE id = ?
        "#,
    )
    .bind(&admin_user.id)
    .execute(&ctx.db)
    .await
    .expect("delete user");

    let books_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&plain_token))
        .await;

    assert_status!(books_response, 401);
}

#[tokio::test]
async fn test_disabled_users_api_token_returns_401() {
    let ctx = TestContext::new().await;
    let (admin_user, admin_password) = ctx.create_admin().await;
    let admin_token = ctx
        .login(&admin_user.username, &admin_password)
        .await
        .access_token;

    let body = create_api_token(&ctx, &admin_token, "disabled-client", None).await;
    let plain_token = body["token"].as_str().unwrap_or_default().to_string();

    sqlx::query(
        r#"
        UPDATE users
        SET is_active = 0
        WHERE id = ?
        "#,
    )
    .bind(&admin_user.id)
    .execute(&ctx.db)
    .await
    .expect("disable user");

    let books_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&plain_token))
        .await;

    assert_status!(books_response, 401);
}

#[tokio::test]
async fn test_api_token_list_excludes_hash() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let create = create_api_token(&ctx, &token, "list-check", None).await;
    assert!(create.get("token_hash").is_none());

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
async fn test_api_token_delete_revokes_auth() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;

    let create = create_api_token(&ctx, &admin_token, "revoked-client", None).await;
    let token_id = create["id"].as_str().unwrap_or_default().to_string();
    let plain_token = create["token"].as_str().unwrap_or_default().to_string();

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
async fn test_api_tokens_require_admin() {
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
