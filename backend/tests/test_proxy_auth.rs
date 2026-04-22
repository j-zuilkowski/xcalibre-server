#![allow(dead_code, unused_imports)]

mod common;

use axum::http::{HeaderName, HeaderValue};
use backend::config::AppConfig;
use common::TestContext;
use serde_json::Value;
use sqlx::Row;

fn proxy_config() -> AppConfig {
    let mut config = AppConfig::default();
    config.auth.proxy.enabled = true;
    config.auth.proxy.header = "X-Remote-User".to_string();
    config.auth.proxy.email_header = "X-Remote-Email".to_string();
    config
}

#[tokio::test]
async fn test_proxy_auth_disabled_ignores_header() {
    let ctx = TestContext::new().await;

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            HeaderName::from_static("x-remote-user"),
            HeaderValue::from_static("proxy-user"),
        )
        .await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_proxy_auth_creates_user_on_first_request() {
    let ctx = TestContext::new_with_config(proxy_config()).await;

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            HeaderName::from_static("x-remote-user"),
            HeaderValue::from_static("proxy-user"),
        )
        .add_header(
            HeaderName::from_static("x-remote-email"),
            HeaderValue::from_static("proxy-user@example.com"),
        )
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    assert_eq!(body["username"], "proxy-user");

    let row = sqlx::query("SELECT username, email FROM users WHERE username = ?")
        .bind("proxy-user")
        .fetch_one(&ctx.db)
        .await
        .expect("created proxy user");
    assert_eq!(row.get::<String, _>("username"), "proxy-user");
    assert_eq!(row.get::<String, _>("email"), "proxy-user@example.com");
}

#[tokio::test]
async fn test_proxy_auth_reuses_existing_user() {
    let ctx = TestContext::new_with_config(proxy_config()).await;

    let first = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            HeaderName::from_static("x-remote-user"),
            HeaderValue::from_static("proxy-user"),
        )
        .add_header(
            HeaderName::from_static("x-remote-email"),
            HeaderValue::from_static("proxy-user@example.com"),
        )
        .await;
    assert_status!(first, 200);
    let first_body: Value = first.json();

    let second = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            HeaderName::from_static("x-remote-user"),
            HeaderValue::from_static("proxy-user"),
        )
        .add_header(
            HeaderName::from_static("x-remote-email"),
            HeaderValue::from_static("proxy-user@example.com"),
        )
        .await;
    assert_status!(second, 200);
    let second_body: Value = second.json();

    assert_eq!(first_body["id"], second_body["id"]);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM users WHERE username = ?")
        .bind("proxy-user")
        .fetch_one(&ctx.db)
        .await
        .expect("count proxy users");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_proxy_auth_requires_header_presence() {
    let ctx = TestContext::new_with_config(proxy_config()).await;

    let response = ctx.server.get("/api/v1/auth/me").await;

    assert_status!(response, 401);
}
