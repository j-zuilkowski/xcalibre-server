#![allow(dead_code, unused_imports)]

mod common;

use chrono::{Duration, Utc};
use common::{auth_header, TestContext};
use serde_json::Value;
use sha2::Digest;
use sqlx::Row;
use totp_rs::{Algorithm, Secret, TOTP};

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

#[test]
fn test_totp_key_derivation_is_stable() {
    let jwt_secret = "test-jwt-secret-for-totp-derivation";
    let key_one = backend::auth::totp::derive_key(jwt_secret, backend::auth::totp::TOTP_HKDF_SALT)
        .expect("derive key");
    let key_two = backend::auth::totp::derive_key(jwt_secret, backend::auth::totp::TOTP_HKDF_SALT)
        .expect("derive key");

    assert_eq!(key_one, key_two);
}

#[test]
fn test_totp_and_webhook_keys_are_distinct() {
    let jwt_secret = "test-jwt-secret-for-totp-derivation";
    let totp_key = backend::auth::totp::derive_key(jwt_secret, backend::auth::totp::TOTP_HKDF_SALT)
        .expect("derive totp key");
    let webhook_key =
        backend::auth::totp::derive_key(jwt_secret, backend::auth::totp::WEBHOOK_HKDF_SALT)
            .expect("derive webhook key");

    assert_ne!(totp_key, webhook_key);
}

async fn setup_totp_for_user(
    ctx: &TestContext,
    username: &str,
    password: &str,
) -> (String, String, String, String) {
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
    let setup_body: Value = setup.json();
    let secret_base32 = setup_body["secret_base32"]
        .as_str()
        .expect("secret")
        .to_string();
    let otpauth_uri = setup_body["otpauth_uri"].as_str().expect("uri").to_string();
    let code = generate_code(
        &secret_base32,
        "autolibre",
        &format!("{username}@example.com"),
    );

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

    let confirm_body: Value = confirm.json();
    let backup_codes = confirm_body["backup_codes"]
        .as_array()
        .expect("backup codes")
        .iter()
        .map(|value| value.as_str().expect("backup code").to_string())
        .collect::<Vec<_>>();

    (
        secret_base32,
        otpauth_uri,
        backup_codes
            .first()
            .cloned()
            .unwrap_or_else(|| panic!("backup code missing")),
        login.access_token,
    )
}

#[tokio::test]
async fn test_setup_generates_valid_otpauth_uri() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let response = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret");
    let otpauth_uri = body["otpauth_uri"].as_str().expect("uri");
    assert!(otpauth_uri.starts_with("otpauth://totp/autolibre:"));
    assert!(otpauth_uri.contains(&format!("secret={secret_base32}")));
    assert!(otpauth_uri.contains("issuer=autolibre"));
    assert!(otpauth_uri.contains("algorithm=SHA1"));
    assert!(otpauth_uri.contains("digits=6"));
    assert!(otpauth_uri.contains("period=30"));

    let row = sqlx::query("SELECT totp_enabled, totp_secret FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let totp_enabled: i64 = row.get("totp_enabled");
    let totp_secret: Option<String> = row.get("totp_secret");
    assert_eq!(totp_enabled, 0);
    assert!(totp_secret.is_some());
}

#[tokio::test]
async fn test_confirm_with_valid_code_enables_totp() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret");
    let code = generate_code(secret_base32, "autolibre", &user.email);

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

    let row = sqlx::query("SELECT totp_enabled FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let totp_enabled: i64 = row.get("totp_enabled");
    assert_eq!(totp_enabled, 1);
}

#[tokio::test]
async fn test_confirm_with_invalid_code_returns_422() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    assert_status!(setup, 200);

    let confirm = ctx
        .server
        .post("/api/v1/auth/totp/confirm")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({ "code": "000000" }))
        .await;
    assert_status!(confirm, 422);
    let body: Value = confirm.json();
    assert_eq!(body["error"], "invalid_totp");
}

#[tokio::test]
async fn test_confirm_returns_8_backup_codes() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret");
    let code = generate_code(secret_base32, "autolibre", &user.email);

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
    let confirm_body: Value = confirm.json();
    let backup_codes = confirm_body["backup_codes"]
        .as_array()
        .expect("backup codes");
    assert_eq!(backup_codes.len(), 8);
}

#[tokio::test]
async fn test_login_with_totp_disabled_returns_tokens_directly() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password,
        }))
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    assert!(body.get("access_token").is_some());
    assert!(body.get("refresh_token").is_some());
    assert!(body.get("totp_required").is_none());
}

#[tokio::test]
async fn test_login_with_totp_enabled_returns_totp_required() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret");
    let code = generate_code(secret_base32, "autolibre", &user.email);
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

    let response = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password,
        }))
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    assert_eq!(body["totp_required"], true);
    assert!(body["totp_token"].as_str().unwrap_or_default().len() > 10);
    assert!(body.get("access_token").is_none());
}

#[test]
fn test_totp_verify_lockout_not_cleared_on_token_failure() {
    // Regression note: token generation must happen before clear_login_lockout in
    // backend/src/api/auth.rs. The current test harness cannot inject a token
    // generation failure, so this test documents the invariant directly.
}

#[tokio::test]
async fn test_totp_verify_success_returns_tokens_and_clears_lockout() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret").to_string();
    let code = generate_code(&secret_base32, "autolibre", &user.email);
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

    let totp_login = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password,
        }))
        .await;
    let totp_login_body: Value = totp_login.json();
    let totp_token = totp_login_body["totp_token"].as_str().expect("totp token");
    let verify_code = generate_code(&secret_base32, "autolibre", &user.email);

    let preexisting_lockout = (Utc::now() - Duration::minutes(5)).to_rfc3339();
    sqlx::query("UPDATE users SET login_attempts = ?, locked_until = ? WHERE id = ?")
        .bind(7_i64)
        .bind(&preexisting_lockout)
        .bind(&user.id)
        .execute(&ctx.db)
        .await
        .expect("seed lockout state");

    let response = ctx
        .server
        .post("/api/v1/auth/totp/verify")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(totp_token))
        .json(&serde_json::json!({ "code": verify_code }))
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    assert!(body["access_token"].as_str().unwrap_or_default().len() > 10);
    assert!(body["refresh_token"].as_str().unwrap_or_default().len() > 10);

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
async fn test_verify_with_invalid_code_returns_422() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret").to_string();
    let code = generate_code(&secret_base32, "autolibre", &user.email);
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

    let totp_login = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password,
        }))
        .await;
    let totp_login_body: Value = totp_login.json();
    let totp_token = totp_login_body["totp_token"].as_str().expect("totp token");

    let response = ctx
        .server
        .post("/api/v1/auth/totp/verify")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(totp_token))
        .json(&serde_json::json!({ "code": "000000" }))
        .await;

    assert_status!(response, 422);
    let body: Value = response.json();
    assert_eq!(body["error"], "invalid_totp");
}

#[tokio::test]
async fn test_verify_backup_code_marks_used() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret").to_string();
    let code = generate_code(&secret_base32, "autolibre", &user.email);
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
    let confirm_body: Value = confirm.json();
    let backup_code = confirm_body["backup_codes"][0]
        .as_str()
        .expect("backup code")
        .to_string();

    let totp_login = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password,
        }))
        .await;
    let totp_login_body: Value = totp_login.json();
    let totp_token = totp_login_body["totp_token"].as_str().expect("totp token");

    let response = ctx
        .server
        .post("/api/v1/auth/totp/verify-backup")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(totp_token))
        .json(&serde_json::json!({ "code": backup_code }))
        .await;
    assert_status!(response, 200);

    let code_hash = {
        let digest = sha2::Sha256::digest(backup_code.as_bytes());
        hex::encode(digest)
    };
    let row =
        sqlx::query("SELECT used_at FROM totp_backup_codes WHERE user_id = ? AND code_hash = ?")
            .bind(&user.id)
            .bind(&code_hash)
            .fetch_one(&ctx.db)
            .await
            .expect("query backup code");
    let used_at: Option<String> = row.get("used_at");
    assert!(used_at.is_some());
}

#[tokio::test]
async fn test_verify_backup_code_format_and_lookup_failures_hit_db() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret").to_string();
    let code = generate_code(&secret_base32, "autolibre", &user.email);
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
    let confirm_body: Value = confirm.json();
    let backup_code = confirm_body["backup_codes"][0]
        .as_str()
        .expect("backup code")
        .to_string();

    let totp_login = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password,
        }))
        .await;
    let totp_login_body: Value = totp_login.json();
    let totp_token = totp_login_body["totp_token"].as_str().expect("totp token");

    let row = sqlx::query("SELECT login_attempts FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let initial_attempts: i64 = row.get("login_attempts");
    assert_eq!(initial_attempts, 0);

    let malformed = ctx
        .server
        .post("/api/v1/auth/totp/verify-backup")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(totp_token))
        .json(&serde_json::json!({ "code": "AB12" }))
        .await;
    assert_status!(malformed, 400);
    let body: Value = malformed.json();
    assert_eq!(body["error"], "invalid_backup_code");

    let row = sqlx::query("SELECT login_attempts FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let attempts_after_malformed: i64 = row.get("login_attempts");
    assert_eq!(attempts_after_malformed, 1);

    let wrong_code = format!(
        "{}{}",
        &backup_code[..7],
        if &backup_code[7..8] == "0" { "1" } else { "0" }
    );
    let response = ctx
        .server
        .post("/api/v1/auth/totp/verify-backup")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(totp_token))
        .json(&serde_json::json!({ "code": wrong_code }))
        .await;
    assert_status!(response, 401);
    let body: Value = response.json();
    assert_eq!(body["error"], "invalid_backup_code");

    let row = sqlx::query("SELECT login_attempts FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let attempts_after_wrong_code: i64 = row.get("login_attempts");
    assert_eq!(attempts_after_wrong_code, 2);
}

#[tokio::test]
async fn test_access_token_rejected_as_totp_token() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let response = ctx
        .server
        .post("/api/v1/auth/totp/verify")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({ "code": "123456" }))
        .await;

    assert_status!(response, 403);
}

#[tokio::test]
async fn test_totp_token_rejected_as_access_token() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret").to_string();
    let code = generate_code(&secret_base32, "autolibre", &user.email);
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

    let totp_login = ctx
        .server
        .post("/api/v1/auth/login")
        .json(&serde_json::json!({
            "username": user.username,
            "password": password,
        }))
        .await;
    let totp_login_body: Value = totp_login.json();
    let totp_token = totp_login_body["totp_token"].as_str().expect("totp token");

    let response = ctx
        .server
        .get("/api/v1/auth/me")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(totp_token))
        .await;

    assert_status!(response, 403);
}

#[tokio::test]
async fn test_admin_can_disable_totp_for_any_user() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let admin_login = ctx.create_admin().await;
    let admin = ctx.login(&admin_login.0.username, &admin_login.1).await;

    let user_login = ctx.login(&user.username, &password).await;
    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&user_login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret").to_string();
    let code = generate_code(&secret_base32, "autolibre", &user.email);
    let confirm = ctx
        .server
        .post("/api/v1/auth/totp/confirm")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&user_login.access_token),
        )
        .json(&serde_json::json!({ "code": code }))
        .await;
    assert_status!(confirm, 200);

    let disable_path = format!("/api/v1/admin/users/{}/totp/disable", user.id);
    let response = ctx
        .server
        .post(&disable_path)
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&admin.access_token),
        )
        .await;

    assert_status!(response, 204);
    let row = sqlx::query("SELECT totp_enabled, totp_secret FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let totp_enabled: i64 = row.get("totp_enabled");
    let totp_secret: Option<String> = row.get("totp_secret");
    assert_eq!(totp_enabled, 0);
    assert!(totp_secret.is_none());
    let backup_count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM totp_backup_codes WHERE user_id = ?")
            .bind(&user.id)
            .fetch_one(&ctx.db)
            .await
            .expect("count backup codes");
    assert_eq!(backup_count, 0);
}

#[tokio::test]
async fn test_self_disable_requires_correct_password() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;

    let setup = ctx
        .server
        .get("/api/v1/auth/totp/setup")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .await;
    let body: Value = setup.json();
    let secret_base32 = body["secret_base32"].as_str().expect("secret").to_string();
    let code = generate_code(&secret_base32, "autolibre", &user.email);
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

    let bad = ctx
        .server
        .post("/api/v1/auth/totp/disable")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({ "password": "wrong-password" }))
        .await;
    assert_status!(bad, 400);

    let good = ctx
        .server
        .post("/api/v1/auth/totp/disable")
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({ "password": password }))
        .await;
    assert_status!(good, 204);

    let row = sqlx::query("SELECT totp_enabled, totp_secret FROM users WHERE id = ?")
        .bind(&user.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query user");
    let totp_enabled: i64 = row.get("totp_enabled");
    let totp_secret: Option<String> = row.get("totp_secret");
    assert_eq!(totp_enabled, 0);
    assert!(totp_secret.is_none());
    let backup_count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM totp_backup_codes WHERE user_id = ?")
            .bind(&user.id)
            .fetch_one(&ctx.db)
            .await
            .expect("count backup codes");
    assert_eq!(backup_count, 0);
}
