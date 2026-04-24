#![allow(dead_code, unused_imports)]

mod common;

use axum::http::{HeaderName, HeaderValue};
use chrono::{Duration, Utc};
use common::{auth_header, TestContext, TEST_JWT_SECRET};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use sqlx::Row;

#[tokio::test]
async fn test_register_first_user_becomes_admin() {
    let ctx = TestContext::new().await;

    let response = ctx
        .server
        .post("/api/v1/auth/register")
        .json(&serde_json::json!({
            "username": "owner",
            "email": "owner@example.com",
            "password": "Test1234!"
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["username"], "owner");
    assert_eq!(body["role"]["name"], "admin");
}

#[tokio::test]
async fn test_register_fails_if_users_exist() {
    let ctx = TestContext::new().await;

    let first = ctx
        .server
        .post("/api/v1/auth/register")
        .json(&serde_json::json!({
            "username": "owner",
            "email": "owner@example.com",
            "password": "Test1234!"
        }))
        .await;
    assert_status!(first, 201);

    let second = ctx
        .server
        .post("/api/v1/auth/register")
        .json(&serde_json::json!({
            "username": "owner2",
            "email": "owner2@example.com",
            "password": "Test1234!"
        }))
        .await;
    assert_status!(second, 409);
}

#[tokio::test]
async fn test_login_success_returns_tokens() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_admin().await;

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(!body["access_token"].as_str().unwrap_or_default().is_empty());
    assert!(!body["refresh_token"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
    assert_eq!(body["user"]["id"], user.id);
}

#[tokio::test]
async fn test_login_sets_samesite_strict_cookie() {
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
    let set_cookie = response.header(axum::http::header::SET_COOKIE);
    let cookie = set_cookie.to_str().expect("set-cookie header");
    assert!(cookie.contains("refresh_token="));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Path=/api/v1/auth"));
}

#[tokio::test]
async fn test_login_wrong_password_returns_401() {
    let ctx = TestContext::new().await;
    let (user, _) = ctx.create_user().await;

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": "wrong-password"
        }))
        .await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_login_nonexistent_user_returns_401() {
    let ctx = TestContext::new().await;

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": "missing",
            "password": "Test1234!"
        }))
        .await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_login_lockout_after_max_attempts() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let forwarded_for = HeaderName::from_static("x-forwarded-for");

    for attempt in 0..10 {
        let response = ctx
            .server
            .post("/api/v1/auth/login")
            .add_header(
                forwarded_for.clone(),
                HeaderValue::from_str(&format!("198.51.100.{}", attempt + 10)).expect("valid IP"),
            )
            .json(&serde_json::json!({
                "username": user.username,
                "password": "wrong-password"
            }))
            .await;
        assert_status!(response, 401);
    }

    let locked_response = ctx
        .server
        .post("/api/v1/auth/login")
        .add_header(forwarded_for, HeaderValue::from_static("198.51.100.250"))
        .json(&serde_json::json!({
            "username": user.username,
            "password": password
        }))
        .await;
    assert_status!(locked_response, 401);

    let row = sqlx::query("SELECT login_attempts, locked_until FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let attempts: i64 = row.get("login_attempts");
    let locked_until: Option<String> = row.get("locked_until");
    assert!(attempts >= 10);
    assert!(locked_until.is_some());
}

#[tokio::test]
async fn test_login_lockout_resets_after_duration() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let forwarded_for = HeaderName::from_static("x-forwarded-for");

    for attempt in 0..10 {
        let _ = ctx
            .server
            .post("/api/v1/auth/login")
            .add_header(
                forwarded_for.clone(),
                HeaderValue::from_str(&format!("198.51.101.{}", attempt + 10)).expect("valid IP"),
            )
            .json(&serde_json::json!({
                "username": user.username,
                "password": "wrong-password"
            }))
            .await;
    }

    let unlocked_at = (Utc::now() - Duration::minutes(20)).to_rfc3339();
    sqlx::query("UPDATE users SET locked_until = ? WHERE id = ?")
        .bind(unlocked_at)
        .bind(&user.id)
        .execute(&ctx.db)
        .await
        .expect("set lockout in past");

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .add_header(forwarded_for, HeaderValue::from_static("198.51.101.250"))
        .json(&serde_json::json!({
            "username": user.username,
            "password": password
        }))
        .await;
    assert_status!(response, 200);

    let row = sqlx::query("SELECT login_attempts, locked_until FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let attempts: i64 = row.get("login_attempts");
    let locked_until: Option<String> = row.get("locked_until");
    assert_eq!(attempts, 0);
    assert!(locked_until.is_none());
}

#[tokio::test]
async fn test_refresh_token_returns_new_pair() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let response = ctx
        .server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({
            "refresh_token": login.refresh_token
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(!body["access_token"].as_str().unwrap_or_default().is_empty());
    assert!(!body["refresh_token"]
        .as_str()
        .unwrap_or_default()
        .is_empty());
    assert_ne!(
        body["refresh_token"].as_str().unwrap_or_default(),
        login.refresh_token
    );
}

#[tokio::test]
async fn test_refresh_rotates_cookie_with_samesite_strict() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let response = ctx
        .server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({
            "refresh_token": login.refresh_token
        }))
        .await;

    assert_status!(response, 200);
    let set_cookie = response.header(axum::http::header::SET_COOKIE);
    let cookie = set_cookie.to_str().expect("set-cookie header");
    assert!(cookie.contains("refresh_token="));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Path=/api/v1/auth"));
    assert!(cookie.contains("Max-Age="));
}

#[tokio::test]
async fn test_refresh_token_revoked_after_use() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let first = ctx
        .server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({
            "refresh_token": login.refresh_token
        }))
        .await;
    assert_status!(first, 200);

    let second = ctx
        .server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({
            "refresh_token": login.refresh_token
        }))
        .await;
    assert_status!(second, 401);
}

#[tokio::test]
async fn test_refresh_invalid_token_returns_401() {
    let ctx = TestContext::new().await;

    let response = ctx
        .server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({
            "refresh_token": "not-a-token"
        }))
        .await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_logout_revokes_refresh_token() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let logout = ctx
        .server
        .post("/api/v1/auth/logout")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({
            "refresh_token": login.refresh_token
        }))
        .await;
    assert_status!(logout, 200);

    let refresh = ctx
        .server
        .post("/api/v1/auth/refresh")
        .json(&serde_json::json!({
            "refresh_token": login.refresh_token
        }))
        .await;
    assert_status!(refresh, 401);
}

#[tokio::test]
async fn test_me_returns_current_user() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"], user.id);
    assert_eq!(body["username"], user.username);
}

#[tokio::test]
async fn test_me_without_token_returns_401() {
    let ctx = TestContext::new().await;

    let response = ctx.server.get("/api/v1/auth/me").await;
    assert_status!(response, 401);
}

#[derive(Debug, Serialize)]
struct ExpiredClaims {
    sub: String,
    iat: usize,
    exp: usize,
}

#[tokio::test]
async fn test_me_with_expired_token_returns_401() {
    let ctx = TestContext::new().await;
    let (user, _) = ctx.create_user().await;

    let now = Utc::now();
    let token = encode(
        &Header::default(),
        &ExpiredClaims {
            sub: user.id,
            iat: (now - Duration::minutes(10)).timestamp() as usize,
            exp: (now - Duration::minutes(5)).timestamp() as usize,
        },
        &EncodingKey::from_secret(ctx.jwt_secret().as_bytes()),
    )
    .expect("encode expired token");

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_change_password_success() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let change = ctx
        .server
        .patch("/api/v1/auth/me/password")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({
            "current_password": password,
            "new_password": "NewPass123!"
        }))
        .await;
    assert_status!(change, 200);

    let old_login = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": "Test1234!"
        }))
        .await;
    assert_status!(old_login, 401);

    let new_login = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": "NewPass123!"
        }))
        .await;
    assert_status!(new_login, 200);
}

#[tokio::test]
async fn test_change_password_wrong_current_returns_400() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let response = ctx
        .server
        .patch("/api/v1/auth/me/password")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({
            "current_password": "wrong-password",
            "new_password": "NewPass123!"
        }))
        .await;

    assert_status!(response, 400);
}

#[tokio::test]
async fn test_login_success_writes_audit_log() {
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

    let row = sqlx::query(
        "SELECT COUNT(1) AS count FROM audit_log WHERE entity = 'user' AND entity_id = ? AND diff_json LIKE '%\"event\":\"login_success\"%'",
    )
    .bind(&user.id)
    .fetch_one(&ctx.db)
    .await
    .expect("query login success audit");
    let count: i64 = row.get("count");
    assert!(count >= 1);
}

#[tokio::test]
async fn test_login_failure_writes_audit_log() {
    let ctx = TestContext::new().await;
    let (user, _) = ctx.create_user().await;

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": "wrong-password"
        }))
        .await;
    assert_status!(response, 401);

    let row = sqlx::query(
        "SELECT COUNT(1) AS count FROM audit_log WHERE entity = 'user' AND entity_id = ? AND diff_json LIKE '%\"event\":\"login_failure\"%'",
    )
    .bind(&user.id)
    .fetch_one(&ctx.db)
    .await
    .expect("query login failure audit");
    let count: i64 = row.get("count");
    assert!(count >= 1);
}

#[tokio::test]
async fn test_change_password_writes_audit_log() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let response = ctx
        .server
        .patch("/api/v1/auth/me/password")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({
            "current_password": password,
            "new_password": "Password123!"
        }))
        .await;
    assert_status!(response, 200);

    let row = sqlx::query(
        "SELECT COUNT(1) AS count FROM audit_log WHERE entity = 'user' AND entity_id = ? AND diff_json LIKE '%\"event\":\"password_change\"%'",
    )
    .bind(&user.id)
    .fetch_one(&ctx.db)
    .await
    .expect("query password-change audit");
    let count: i64 = row.get("count");
    assert!(count >= 1);
}

#[tokio::test]
async fn test_login_sets_secure_refresh_cookie_when_base_url_is_https() {
    let mut config = backend::config::AppConfig::default();
    config.app.base_url = "https://library.example.com".to_string();
    config.auth.jwt_secret = TEST_JWT_SECRET.to_string();
    let ctx = TestContext::new_with_config(config).await;
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

    let set_cookie = response.header(axum::http::header::SET_COOKIE);
    let cookie = set_cookie.to_str().expect("set-cookie header");
    assert!(cookie.contains("refresh_token="));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("Secure"));
}

#[tokio::test]
async fn test_role_change_writes_audit_log() {
    let ctx = TestContext::new().await;
    let (user, _password) = ctx.create_user().await;
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT OR IGNORE INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
        VALUES ('audited_admin', 'audited_admin', 1, 1, 1, 1, ?, ?)
        "#,
    )
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert audited_admin role");

    sqlx::query("UPDATE users SET role_id = 'audited_admin' WHERE id = ?")
        .bind(&user.id)
        .execute(&ctx.db)
        .await
        .expect("update user role");

    let row = sqlx::query(
        "SELECT COUNT(1) AS count FROM audit_log WHERE entity = 'user' AND entity_id = ? AND action = 'update' AND diff_json LIKE '%\"event\":\"role_change\"%'",
    )
    .bind(&user.id)
    .fetch_one(&ctx.db)
    .await
    .expect("query role-change audit");
    let count: i64 = row.get("count");
    assert!(count >= 1);
}
