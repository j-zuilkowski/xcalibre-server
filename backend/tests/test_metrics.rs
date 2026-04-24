mod common;

use axum::http::{header, StatusCode};
use common::TestContext;

#[tokio::test]
async fn test_metrics_endpoint_returns_200() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/metrics").await;

    assert_status!(response, 200);
}

#[tokio::test]
async fn test_metrics_endpoint_returns_prometheus_format() {
    let ctx = TestContext::new().await;

    let _ = ctx.server.get("/health").await;
    let response = ctx.server.get("/metrics").await;

    assert_status!(response, 200);
    let content_type_header = response.header(header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content type");
    assert!(
        content_type.contains("text/plain"),
        "unexpected content type: {content_type}"
    );
    let body = response.text();
    assert!(
        body.contains("# TYPE") || body.contains("# HELP"),
        "expected Prometheus exposition format, got: {body}"
    );
}

#[tokio::test]
async fn test_metrics_requires_no_auth() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/metrics").await;

    assert_eq!(response.status_code(), StatusCode::OK);
}

#[tokio::test]
async fn test_metrics_contains_http_requests_total() {
    let ctx = TestContext::new().await;

    let _ = ctx.server.get("/health").await;
    let response = ctx.server.get("/metrics").await;

    assert_status!(response, 200);
    let body = response.text();
    assert!(
        body.contains("axum_http_requests_total"),
        "metrics output missing axum_http_requests_total: {body}"
    );
}
