#![allow(dead_code, unused_imports)]

mod common;

use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum_test::{TestResponse, TestServer};
use backend::{app, config::AppConfig, AppState};
use common::{auth_header, test_db, TestContext, TEST_JWT_SECRET};

const X_CONTENT_TYPE_OPTIONS: &str = "x-content-type-options";
const X_FRAME_OPTIONS: &str = "x-frame-options";
const REFERRER_POLICY: &str = "referrer-policy";
const CONTENT_SECURITY_POLICY: &str = "content-security-policy";
const PERMISSIONS_POLICY: &str = "permissions-policy";
const X_FORWARDED_FOR: &str = "x-forwarded-for";
const EXPECTED_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob:; worker-src 'self' blob:";

fn assert_security_headers(response: &TestResponse) {
    assert_header(response, X_CONTENT_TYPE_OPTIONS, "nosniff");
    assert_header(response, X_FRAME_OPTIONS, "DENY");
    assert_header(response, REFERRER_POLICY, "strict-origin-when-cross-origin");
    assert_header(response, CONTENT_SECURITY_POLICY, EXPECTED_CSP);
    assert_header(
        response,
        PERMISSIONS_POLICY,
        "camera=(), microphone=(), geolocation=()",
    );
}

fn assert_header(response: &TestResponse, name: &'static str, expected: &'static str) {
    let header_name = HeaderName::from_static(name);
    let header_value = response.header(header_name);
    let actual = header_value.to_str().expect("header value utf-8");
    assert_eq!(actual, expected);
}

#[tokio::test]
async fn test_security_headers_present_on_all_responses() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/v1/books").await;
    assert_status!(response, 401);
    assert_security_headers(&response);
}

#[tokio::test]
async fn test_x_content_type_options_nosniff() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/v1/books").await;
    assert_status!(response, 401);
    assert_header(&response, X_CONTENT_TYPE_OPTIONS, "nosniff");
}

#[tokio::test]
async fn test_x_frame_options_deny() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/v1/books").await;
    assert_status!(response, 401);
    assert_header(&response, X_FRAME_OPTIONS, "DENY");
}

#[tokio::test]
async fn test_csp_header_present() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/v1/books").await;
    assert_status!(response, 401);
    assert_header(&response, CONTENT_SECURITY_POLICY, EXPECTED_CSP);
}

#[tokio::test]
async fn test_permissions_policy_present() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/v1/books").await;
    assert_status!(response, 401);
    assert_header(
        &response,
        PERMISSIONS_POLICY,
        "camera=(), microphone=(), geolocation=()",
    );
}

#[tokio::test]
async fn test_rate_limit_auth_after_10_requests() {
    let ctx = TestContext::new().await;
    let ip = HeaderValue::from_static("198.51.100.10");
    let forwarded_for = HeaderName::from_static(X_FORWARDED_FOR);

    for attempt in 1..=10 {
        let response = ctx
            .server
            .post("/api/v1/auth/login")
            .add_header(forwarded_for.clone(), ip.clone())
            .json(&serde_json::json!({
                "username": "missing-user",
                "password": "wrong-password"
            }))
            .await;

        let status = response.status_code();
        assert!(
            status != StatusCode::TOO_MANY_REQUESTS,
            "request {attempt} unexpectedly hit rate limit",
        );
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
}

#[tokio::test]
async fn test_rate_limit_resets_after_window() {
    let ctx = TestContext::new().await;
    let ip = HeaderValue::from_static("198.51.100.11");
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

    let blocked = ctx
        .server
        .post("/api/v1/auth/login")
        .add_header(forwarded_for.clone(), ip.clone())
        .json(&serde_json::json!({
            "username": "missing-user",
            "password": "wrong-password"
        }))
        .await;
    assert_status!(blocked, 429);

    tokio::time::sleep(std::time::Duration::from_millis(6_500)).await;

    let response_after_wait = ctx
        .server
        .post("/api/v1/auth/login")
        .add_header(forwarded_for, ip)
        .json(&serde_json::json!({
            "username": "missing-user",
            "password": "wrong-password"
        }))
        .await;

    assert_ne!(
        response_after_wait.status_code(),
        StatusCode::TOO_MANY_REQUESTS
    );
}

#[tokio::test]
async fn test_upload_over_size_limit_returns_413() {
    let storage = tempfile::tempdir().expect("tempdir");
    let db = test_db().await;
    let mut config = AppConfig::default();
    config.app.storage_path = storage.path().to_string_lossy().to_string();
    config.auth.jwt_secret = TEST_JWT_SECRET.to_string();
    config.limits.upload_max_bytes = 8;

    let server = TestServer::new(app(AppState::new(db, config).await)).expect("build test server");
    let forwarded_for = HeaderName::from_static(X_FORWARDED_FOR);
    let ip = HeaderValue::from_static("198.51.100.12");

    let register = server
        .post("/api/v1/auth/register")
        .add_header(forwarded_for.clone(), ip.clone())
        .json(&serde_json::json!({
            "username": "owner",
            "email": "owner@example.com",
            "password": "Test1234!"
        }))
        .await;
    assert_status!(register, 201);

    let login = server
        .post("/api/v1/auth/login")
        .add_header(forwarded_for.clone(), ip.clone())
        .json(&serde_json::json!({
            "username": "owner",
            "password": "Test1234!"
        }))
        .await;
    assert_status!(login, 200);
    let login_body: serde_json::Value = login.json();
    let access_token = login_body["access_token"]
        .as_str()
        .expect("access token")
        .to_string();

    let response = server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&access_token))
        .add_header(forwarded_for, ip)
        .add_header(header::CONTENT_LENGTH, HeaderValue::from_static("16"))
        .await;

    assert_status!(response, 413);
}

#[tokio::test]
async fn test_cors_allows_configured_base_url_origin() {
    let storage = tempfile::tempdir().expect("tempdir");
    let db = test_db().await;
    let mut config = AppConfig::default();
    config.app.base_url = "https://app.example.com".to_string();
    config.app.storage_path = storage.path().to_string_lossy().to_string();
    config.auth.jwt_secret = TEST_JWT_SECRET.to_string();

    let server = TestServer::new(app(AppState::new(db, config).await)).expect("build test server");
    let response = server
        .method(axum::http::Method::OPTIONS, "/api/v1/books")
        .add_header(
            header::ORIGIN,
            HeaderValue::from_static("https://app.example.com"),
        )
        .add_header(
            header::ACCESS_CONTROL_REQUEST_METHOD,
            HeaderValue::from_static("GET"),
        )
        .await;

    let status = response.status_code();
    assert!(
        status == StatusCode::OK || status == StatusCode::NO_CONTENT,
        "expected preflight success, got {status}"
    );
    assert_eq!(
        response
            .header(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .to_str()
            .expect("allow-origin"),
        "https://app.example.com"
    );
}
