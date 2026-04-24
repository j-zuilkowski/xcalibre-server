#![allow(dead_code, unused_imports)]

mod common;

use axum::http::{HeaderName, HeaderValue};
use backend::{auth::password::hash_password, db::queries::auth as auth_queries};
use chrono::{Duration, Utc};
use common::{auth_header, TestContext, TEST_JWT_SECRET};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use sqlx::Row;
use totp_rs::{Algorithm, Secret, TOTP};

const X_FORWARDED_FOR: &str = "x-forwarded-for";
const TOTP_INVALID_CODE: &str = "000000";

fn generate_code(secret_base32: &str, issuer: &str, account_name: &str) -> String {
    let secret = Secret::Encoded(secret_base32.to_string());
    let totp = TOTP::new(
        Algorithm::SHA1,
        6,
        1,
        30,
        secret.to_bytes().expect("secret bytes"),
        Some(issuer.to_string()),
        account_name.to_string(),
    )
    .expect("build totp");
    totp.generate_current().expect("generate current code")
}

async fn enable_totp_and_get_pending_token(
    ctx: &TestContext,
    username: &str,
    email: &str,
    password: &str,
) -> (String, String) {
    let login = ctx.login(username, password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    assert_status!(setup, 200);
    let setup_body: serde_json::Value = setup.json();
    let secret_base32 = setup_body["secret_base32"]
        .as_str()
        .expect("secret")
        .to_string();
    let code = generate_code(&secret_base32, "autolibre", email);

    let confirm = ctx
        .server
        .post("/api/v1/auth/totp/confirm")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({ "code": code }))
        .await;
    assert_status!(confirm, 200);

    let pending = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": username,
            "password": password,
        }))
        .await;
    assert_status!(pending, 200);
    let pending_body: serde_json::Value = pending.json();
    (
        pending_body["totp_token"]
            .as_str()
            .expect("totp token")
            .to_string(),
        secret_base32,
    )
}

async fn post_totp_verify(
    ctx: &TestContext,
    totp_token: &str,
    ip: &str,
    code: &str,
    path: &str,
) -> axum_test::TestResponse {
    ctx.server
        .post(path)
        .add_header(axum::http::header::AUTHORIZATION, auth_header(totp_token))
        .add_header(
            HeaderName::from_static(X_FORWARDED_FOR),
            HeaderValue::from_str(ip).expect("valid ip"),
        )
        .json(&serde_json::json!({ "code": code }))
        .await
}

async fn create_distinct_user(
    ctx: &TestContext,
    username: &str,
    email: &str,
    password: &str,
) -> backend::db::models::User {
    let password_hash = hash_password(password, &ctx.state.config.auth).expect("hash password");
    auth_queries::create_user(&ctx.db, username, email, "user", &password_hash)
        .await
        .expect("insert user")
}

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
async fn test_auth_totp_verify_rate_limits_same_ip_after_ten_requests() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let (totp_token, _secret_base32) =
        enable_totp_and_get_pending_token(&ctx, &user.username, &user.email, &password).await;
    let forwarded_ip = "198.51.100.10";

    for _ in 0..10 {
        let response = post_totp_verify(
            &ctx,
            &totp_token,
            forwarded_ip,
            TOTP_INVALID_CODE,
            "/api/v1/auth/totp/verify",
        )
        .await;
        assert_status!(response, 422);
    }

    let rate_limited = post_totp_verify(
        &ctx,
        &totp_token,
        forwarded_ip,
        TOTP_INVALID_CODE,
        "/api/v1/auth/totp/verify",
    )
    .await;
    assert_status!(rate_limited, 429);
}

#[tokio::test]
async fn test_auth_totp_verify_rate_limit_isolated_per_ip() {
    let ctx = TestContext::new().await;
    let (first_user, first_password) = ctx.create_user().await;
    let (first_totp_token, _first_secret_base32) = enable_totp_and_get_pending_token(
        &ctx,
        &first_user.username,
        &first_user.email,
        &first_password,
    )
    .await;

    for _ in 0..10 {
        let response = post_totp_verify(
            &ctx,
            &first_totp_token,
            "198.51.100.20",
            TOTP_INVALID_CODE,
            "/api/v1/auth/totp/verify",
        )
        .await;
        assert_status!(response, 422);
    }

    let second_user = create_distinct_user(&ctx, "user2", "user2@example.com", "Test1234!").await;
    let second_password = "Test1234!";
    let (second_totp_token, _second_secret_base32) = enable_totp_and_get_pending_token(
        &ctx,
        &second_user.username,
        &second_user.email,
        &second_password,
    )
    .await;

    let different_ip_response = post_totp_verify(
        &ctx,
        &second_totp_token,
        "198.51.100.21",
        TOTP_INVALID_CODE,
        "/api/v1/auth/totp/verify",
    )
    .await;
    assert_status!(different_ip_response, 422);
}

#[tokio::test]
async fn test_login_reauthentication_invalidates_stale_pending_totp_token() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let (pending_token_a, secret_base32) =
        enable_totp_and_get_pending_token(&ctx, &user.username, &user.email, &password).await;

    let second_login = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password,
        }))
        .await;
    assert_status!(second_login, 200);
    let second_login_body: serde_json::Value = second_login.json();
    let pending_token_b = second_login_body["totp_token"]
        .as_str()
        .expect("pending token b")
        .to_string();

    assert_ne!(pending_token_a, pending_token_b);

    let code = generate_code(&secret_base32, "autolibre", &user.email);
    let stale = post_totp_verify(
        &ctx,
        &pending_token_a,
        "198.51.100.30",
        &code,
        "/api/v1/auth/totp/verify",
    )
    .await;
    assert_status!(stale, 401);

    let fresh = post_totp_verify(
        &ctx,
        &pending_token_b,
        "198.51.100.30",
        &code,
        "/api/v1/auth/totp/verify",
    )
    .await;
    assert_status!(fresh, 200);
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
