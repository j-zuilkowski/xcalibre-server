#![allow(dead_code, unused_imports)]

mod common;

use axum::{
    extract::{ConnectInfo, Request},
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use axum_test::TestServer;
use backend::{app, config::AppConfig, middleware::auth::is_trusted_proxy, AppState};
use common::{test_db, TestContext, TEST_JWT_SECRET};
use chrono::Utc;
use serde_json::Value;
use sqlx::Row;
use std::net::{IpAddr, SocketAddr};
use tempfile::TempDir;
use uuid::Uuid;

struct ProxyContext {
    db: sqlx::SqlitePool,
    server: TestServer,
    _storage: TempDir,
}

fn app_with_connect_info(state: AppState, remote_ip: IpAddr) -> axum::Router {
    app(state).layer(axum::middleware::from_fn(
        move |mut req: Request, next: Next| {
            let connect_info = ConnectInfo(SocketAddr::new(remote_ip, 12345));
            async move {
                req.extensions_mut().insert(connect_info);
                next.run(req).await
            }
        },
    ))
}

async fn proxy_context(mut config: AppConfig, remote_ip: IpAddr) -> ProxyContext {
    let storage = tempfile::tempdir().expect("tempdir");
    let db = test_db().await;
    std::env::set_var("AUTOLIBRE_DISABLE_METRICS", "1");
    config.app.storage_path = storage.path().to_string_lossy().to_string();
    if config.auth.jwt_secret.trim().is_empty() {
        config.auth.jwt_secret = TEST_JWT_SECRET.to_string();
    }

    let state = AppState::new(db.clone(), config)
        .await
        .expect("initialize app state");
    let server =
        TestServer::new(app_with_connect_info(state, remote_ip)).expect("build test server");

    ProxyContext {
        db,
        server,
        _storage: storage,
    }
}

async fn insert_legacy_proxy_user(db: &sqlx::SqlitePool, username: &str) {
    let now = Utc::now().to_rfc3339();
    let user_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("user")
    .bind("user")
    .bind(1_i64)
    .bind(1_i64)
    .bind(1_i64)
    .bind(1_i64)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await
    .expect("insert user role");

    sqlx::query(
        r#"
        INSERT INTO users (
            id, username, email, password_hash, role_id, is_active, force_pw_reset,
            login_attempts, locked_until, created_at, last_modified
        )
        VALUES (?, ?, ?, ?, ?, 1, 0, 0, NULL, ?, ?)
        "#,
    )
    .bind(&user_id)
    .bind(username)
    .bind("")
    .bind("legacy-proxy-password-hash")
    .bind("user")
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await
    .expect("insert legacy proxy user");
}

fn proxy_config() -> AppConfig {
    proxy_config_with_cidrs(vec!["127.0.0.1/32".to_string()])
}

fn proxy_config_with_cidrs(trusted_cidrs: Vec<String>) -> AppConfig {
    let mut config = AppConfig::default();
    config.auth.proxy.enabled = true;
    config.auth.proxy.header = "x-remote-user".to_string();
    config.auth.proxy.email_header = "X-Remote-Email".to_string();
    config.auth.proxy.trusted_cidrs = trusted_cidrs;
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
async fn test_proxy_auth_accepted_from_trusted_ip() {
    let ctx = proxy_context(
        proxy_config(),
        "127.0.0.1".parse::<IpAddr>().expect("loopback ip"),
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            HeaderName::from_static("x-remote-user"),
            HeaderValue::from_static("testuser"),
        )
        .add_header(
            HeaderName::from_static("x-remote-email"),
            HeaderValue::from_static("testuser@example.com"),
        )
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    assert_eq!(body["username"], "testuser");

    let row = sqlx::query("SELECT username, email FROM users WHERE username = ?")
        .bind("testuser")
        .fetch_one(&ctx.db)
        .await
        .expect("created proxy user");
    assert_eq!(row.get::<String, _>("username"), "testuser");
    assert_eq!(row.get::<String, _>("email"), "testuser@example.com");
}

#[tokio::test]
async fn test_proxy_auth_rejects_missing_email_on_provisioning() {
    let ctx = proxy_context(
        proxy_config(),
        "127.0.0.1".parse::<IpAddr>().expect("loopback ip"),
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            HeaderName::from_static("x-remote-user"),
            HeaderValue::from_static("testuser"),
        )
        .await;

    assert_status!(response, 401);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM users WHERE username = ?")
        .bind("testuser")
        .fetch_one(&ctx.db)
        .await
        .expect("count proxy users");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_proxy_auth_existing_empty_email_user_still_logs_in() {
    let ctx = proxy_context(
        proxy_config(),
        "127.0.0.1".parse::<IpAddr>().expect("loopback ip"),
    )
    .await;

    insert_legacy_proxy_user(&ctx.db, "legacy-user").await;

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            HeaderName::from_static("x-remote-user"),
            HeaderValue::from_static("legacy-user"),
        )
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    assert_eq!(body["username"], "legacy-user");

    let row = sqlx::query("SELECT username, email FROM users WHERE username = ?")
        .bind("legacy-user")
        .fetch_one(&ctx.db)
        .await
        .expect("legacy proxy user");
    assert_eq!(row.get::<String, _>("username"), "legacy-user");
    assert_eq!(row.get::<String, _>("email"), "");
}

#[tokio::test]
async fn test_proxy_auth_rejected_from_untrusted_ip() {
    let ctx = proxy_context(
        proxy_config_with_cidrs(vec!["10.0.0.0/8".to_string()]),
        "127.0.0.1".parse::<IpAddr>().expect("loopback ip"),
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            HeaderName::from_static("x-remote-user"),
            HeaderValue::from_static("admin"),
        )
        .await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_proxy_auth_ignored_with_empty_trusted_cidrs() {
    let ctx = proxy_context(
        proxy_config_with_cidrs(Vec::new()),
        "127.0.0.1".parse::<IpAddr>().expect("loopback ip"),
    )
    .await;

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

#[test]
fn test_is_trusted_proxy_cidr_matching() {
    let loopback = "127.0.0.1".parse::<IpAddr>().expect("loopback ip");
    let private = "10.1.2.3".parse::<IpAddr>().expect("private ip");
    let public = "192.168.1.1".parse::<IpAddr>().expect("public ip");
    let ipv6_loopback = "::1".parse::<IpAddr>().expect("ipv6 loopback");

    assert!(!is_trusted_proxy(loopback, &[]));
    assert!(is_trusted_proxy(loopback, &["127.0.0.1/32".to_string()]));
    assert!(is_trusted_proxy(private, &["10.0.0.0/8".to_string()]));
    assert!(!is_trusted_proxy(public, &["10.0.0.0/8".to_string()]));
    assert!(is_trusted_proxy(ipv6_loopback, &["::1/128".to_string()]));
}
