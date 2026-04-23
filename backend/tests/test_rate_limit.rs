#![allow(dead_code, unused_imports)]

mod common;

use axum::http::{HeaderName, HeaderValue};
use common::TestContext;

const X_FORWARDED_FOR: &str = "x-forwarded-for";
const X_RATE_LIMIT_LIMIT: &str = "x-ratelimit-limit";
const RETRY_AFTER: &str = "retry-after";

#[tokio::test]
async fn test_auth_endpoint_returns_ratelimit_headers() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password
        }))
        .await;

    assert_status!(response, 200);
    let header = response.header(HeaderName::from_static(X_RATE_LIMIT_LIMIT));
    let header_value = header
        .to_str()
        .expect("x-ratelimit-limit must be valid utf-8");
    assert!(!header_value.is_empty());
}

#[tokio::test]
async fn test_429_response_includes_retry_after() {
    let ctx = TestContext::new().await;
    let ip = HeaderValue::from_static("198.51.100.21");
    let forwarded_for = HeaderName::from_static(X_FORWARDED_FOR);

    for _ in 0..10 {
        let _ = ctx
            .server
            .post("/api/v1/auth/login")
            .add_header(forwarded_for.clone(), ip.clone())
            .json(&serde_json::json!({
                "username": "missing-user",
                "password": "wrong-password"
            }))
            .await;
    }

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .add_header(forwarded_for, ip)
        .json(&serde_json::json!({
            "username": "missing-user",
            "password": "wrong-password"
        }))
        .await;

    assert_status!(response, 429);
    let _ = response.header(HeaderName::from_static(RETRY_AFTER));
}

#[tokio::test]
async fn test_retry_after_value_is_positive_integer() {
    let ctx = TestContext::new().await;
    let ip = HeaderValue::from_static("198.51.100.22");
    let forwarded_for = HeaderName::from_static(X_FORWARDED_FOR);

    for _ in 0..10 {
        let _ = ctx
            .server
            .post("/api/v1/auth/login")
            .add_header(forwarded_for.clone(), ip.clone())
            .json(&serde_json::json!({
                "username": "missing-user",
                "password": "wrong-password"
            }))
            .await;
    }

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .add_header(forwarded_for, ip)
        .json(&serde_json::json!({
            "username": "missing-user",
            "password": "wrong-password"
        }))
        .await;

    assert_status!(response, 429);
    let retry_after = response
        .header(HeaderName::from_static(RETRY_AFTER))
        .to_str()
        .expect("retry-after must be valid utf-8")
        .parse::<u64>()
        .expect("retry-after must be an integer");
    assert!(retry_after > 0);
}
