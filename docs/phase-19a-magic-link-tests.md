# Phase 19a — Magic Link Login Tests

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite. TDD: write failing tests first.
Working dir: ~/Documents/localProject/xcalibre-server
Phase 18 complete: pluggable search backend + Merlin RAG memory endpoint.
`cargo clippy -- -D warnings` and `cargo audit` must pass at zero warnings/vulnerabilities.

New surface introduced in Phase 19b:
- `MagicLinkSection` nested in `AuthSection` — `enabled: bool` (default `false`), `token_ttl_minutes: u32` (default `15`)
- Migration `0029_magic_link_tokens.sql` — `magic_link_tokens` table
- `backend::auth::magic_link::hash_token(raw: &str) -> String` — SHA-256 hex of raw token
- `POST /api/v1/auth/magic-link/request` — accepts `{ "email": "..." }`, returns 202 always (no user enumeration); creates token row; sends email via `lettre` (no-op in test)
- `GET  /api/v1/auth/magic-link/verify?token=<raw>` — validates token, marks used, issues JWT + refresh token (same shape as `/auth/login`)
- `DELETE /api/v1/admin/magic-link/revoke/:user_id` — admin clears all unused tokens for a user

Both endpoints return 404 when `auth.magic_link.enabled = false`.

---

## Write to: `backend/tests/test_magic_link.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::{
    auth::magic_link::hash_token,
    config::{AppConfig, MagicLinkSection},
};
use chrono::{Duration, Utc};
use common::{auth_header, TestContext};
use serde_json::{json, Value};

fn magic_link_config(enabled: bool) -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.auth.magic_link.enabled = enabled;
    cfg.auth.magic_link.token_ttl_minutes = 15;
    cfg.app.base_url = "http://localhost".to_string();
    cfg
}

async fn seed_token(ctx: &TestContext, user_id: &str, raw: &str, offset_minutes: i64) {
    let token_hash = hash_token(raw);
    let expires_at = (Utc::now() + Duration::minutes(offset_minutes)).timestamp();
    sqlx::query(
        "INSERT INTO magic_link_tokens (id, user_id, token_hash, expires_at) \
         VALUES (lower(hex(randomblob(16))), ?, ?, ?)",
    )
    .bind(user_id)
    .bind(&token_hash)
    .bind(expires_at)
    .execute(&ctx.db)
    .await
    .unwrap();
}

// ── Config defaults ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_magic_link_disabled_by_default() {
    assert!(!AppConfig::default().auth.magic_link.enabled);
}

#[tokio::test]
async fn test_magic_link_ttl_default_15_minutes() {
    assert_eq!(AppConfig::default().auth.magic_link.token_ttl_minutes, 15);
}

// ── Request endpoint ──────────────────────────────────────────────────────────

#[tokio::test]
async fn test_magic_link_request_404_when_disabled() {
    let ctx = TestContext::new_with_config(magic_link_config(false)).await;
    ctx.create_user().await;

    let resp = ctx
        .server
        .post("/api/v1/auth/magic-link/request")
        .json(&json!({ "email": "user@example.com" }))
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_magic_link_request_202_for_known_email() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;
    ctx.create_user().await;

    let resp = ctx
        .server
        .post("/api/v1/auth/magic-link/request")
        .json(&json!({ "email": "user@example.com" }))
        .await;
    assert_status!(resp, 202);
}

#[tokio::test]
async fn test_magic_link_request_202_for_unknown_email() {
    // Must not reveal whether the address exists
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;

    let resp = ctx
        .server
        .post("/api/v1/auth/magic-link/request")
        .json(&json!({ "email": "nobody@example.com" }))
        .await;
    assert_status!(resp, 202);
}

#[tokio::test]
async fn test_magic_link_request_creates_db_row() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;
    ctx.create_user().await;

    ctx.server
        .post("/api/v1/auth/magic-link/request")
        .json(&json!({ "email": "user@example.com" }))
        .await;

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM magic_link_tokens WHERE used_at IS NULL")
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_magic_link_request_missing_email_422() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;

    let resp = ctx
        .server
        .post("/api/v1/auth/magic-link/request")
        .json(&json!({}))
        .await;
    assert_status!(resp, 422);
}

// ── Verify endpoint ───────────────────────────────────────────────────────────

#[tokio::test]
async fn test_magic_link_verify_404_when_disabled() {
    let ctx = TestContext::new_with_config(magic_link_config(false)).await;

    let resp = ctx
        .server
        .get("/api/v1/auth/magic-link/verify?token=anything")
        .await;
    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_magic_link_verify_issues_jwt_for_valid_token() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;
    let (user, _) = ctx.create_user().await;
    seed_token(&ctx, &user.id, "valid-raw-token-abc123", 15).await;

    let resp = ctx
        .server
        .get("/api/v1/auth/magic-link/verify?token=valid-raw-token-abc123")
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
}

#[tokio::test]
async fn test_magic_link_verify_single_use() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;
    let (user, _) = ctx.create_user().await;
    seed_token(&ctx, &user.id, "single-use-token-xyz789", 15).await;

    let r1 = ctx
        .server
        .get("/api/v1/auth/magic-link/verify?token=single-use-token-xyz789")
        .await;
    assert_status!(r1, 200);

    let r2 = ctx
        .server
        .get("/api/v1/auth/magic-link/verify?token=single-use-token-xyz789")
        .await;
    assert_status!(r2, 401);
}

#[tokio::test]
async fn test_magic_link_verify_expired_token_401() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;
    let (user, _) = ctx.create_user().await;
    seed_token(&ctx, &user.id, "expired-token-aaabbbccc", -1).await; // expired 1 min ago

    let resp = ctx
        .server
        .get("/api/v1/auth/magic-link/verify?token=expired-token-aaabbbccc")
        .await;
    assert_status!(resp, 401);
}

#[tokio::test]
async fn test_magic_link_verify_unknown_token_401() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;

    let resp = ctx
        .server
        .get("/api/v1/auth/magic-link/verify?token=no-such-token-ever")
        .await;
    assert_status!(resp, 401);
}

#[tokio::test]
async fn test_magic_link_verify_marks_token_used() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;
    let (user, _) = ctx.create_user().await;
    seed_token(&ctx, &user.id, "mark-used-token-111222", 15).await;

    ctx.server
        .get("/api/v1/auth/magic-link/verify?token=mark-used-token-111222")
        .await;

    let used_at: Option<i64> =
        sqlx::query_scalar("SELECT used_at FROM magic_link_tokens WHERE user_id = ?")
            .bind(&user.id)
            .fetch_one(&ctx.db)
            .await
            .unwrap();
    assert!(used_at.is_some(), "used_at should be set after successful verify");
}

// ── Admin revoke ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_magic_link_admin_revoke_clears_pending_tokens() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;
    let (user, _) = ctx.create_user().await;
    let admin_tok = ctx.admin_token().await;

    seed_token(&ctx, &user.id, "rev-tok-1", 15).await;
    seed_token(&ctx, &user.id, "rev-tok-2", 15).await;

    let resp = ctx
        .server
        .delete(&format!("/api/v1/admin/magic-link/revoke/{}", user.id))
        .add_header(header::AUTHORIZATION, auth_header(&admin_tok))
        .await;
    assert_status!(resp, 204);

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM magic_link_tokens WHERE used_at IS NULL AND user_id = ?",
    )
    .bind(&user.id)
    .fetch_one(&ctx.db)
    .await
    .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_magic_link_revoke_requires_admin() {
    let ctx = TestContext::new_with_config(magic_link_config(true)).await;
    let (user, _) = ctx.create_user().await;
    let user_tok = ctx.user_token().await;

    let resp = ctx
        .server
        .delete(&format!("/api/v1/admin/magic-link/revoke/{}", user.id))
        .add_header(header::AUTHORIZATION, auth_header(&user_tok))
        .await;
    assert_status!(resp, 403);
}
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_magic_link 2>&1 | tail -20
```
Expected: **BUILD FAILED** — `MagicLinkSection`, `magic_link_tokens` table, `hash_token`, and route handlers do not exist yet.

## Commit
```bash
git add backend/tests/test_magic_link.rs
git commit -m "Phase 19a — magic link login tests (failing)"
```
