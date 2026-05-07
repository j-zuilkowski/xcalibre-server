# Phase 26a — OAuth Account Linking Tests

## Context
Rust 2021, Axum 0.7. TDD: write failing tests first.
Working dir: `~/Documents/localProject/xcalibre-server`

Current OAuth flow supports login and account creation. This phase adds post-login link/unlink for authenticated users.

---

## Write to: `backend/tests/test_oauth_linking.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, TestContext};
use serde_json::Value;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

fn oauth_linking_config(mock: &MockServer) -> backend::config::AppConfig {
    let mut cfg = backend::config::AppConfig::default();
    cfg.oauth.github.enabled = true;
    cfg.oauth.github.client_id = "test-client".to_string();
    cfg.oauth.github.client_secret = "test-secret".to_string();
    cfg.oauth.github.authorize_url = format!("{}/authorize", mock.uri());
    cfg.oauth.github.token_url = format!("{}/token", mock.uri());
    cfg.oauth.github.userinfo_url = format!("{}/userinfo", mock.uri());
    cfg.oauth.google.enabled = true;
    cfg
}

#[tokio::test]
async fn test_me_oauth_providers_initially_all_available_none_linked() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .get("/api/v1/me/oauth/providers")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(resp, 200);
    let body: Value = resp.json();

    let linked = body["linked"].as_array().cloned().unwrap_or_default();
    let available = body["available"].as_array().cloned().unwrap_or_default();

    assert!(linked.is_empty());
    assert!(available.iter().any(|v| v == "github"));
    assert!(available.iter().any(|v| v == "google"));
}

#[tokio::test]
async fn test_oauth_link_flow_links_github_and_updates_provider_list() {
    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "gh-access",
            "token_type": "bearer"
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 987654,
            "login": "linked-user",
            "email": "linked-user@example.com"
        })))
        .mount(&mock)
        .await;

    let ctx = TestContext::new_with_config(oauth_linking_config(&mock)).await;
    let token = ctx.user_token().await;

    let start = ctx
        .server
        .get("/auth/oauth/github/link")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(start, 302);

    let location = start.header("location").unwrap_or_default();
    assert!(location.contains("state="));

    let state = location
        .split("state=")
        .nth(1)
        .unwrap_or_default()
        .split('&')
        .next()
        .unwrap_or_default()
        .to_string();

    let callback = ctx
        .server
        .get(&format!("/auth/oauth/github/link/callback?code=test-code&state={state}"))
        .await;
    assert_status!(callback, 200);
    let callback_body: Value = callback.json();
    assert_eq!(callback_body["linked"], true);

    let list = ctx
        .server
        .get("/api/v1/me/oauth/providers")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(list, 200);
    let list_body: Value = list.json();
    let linked = list_body["linked"].as_array().cloned().unwrap_or_default();
    assert!(linked.iter().any(|v| v == "github"));
}

#[tokio::test]
async fn test_unlink_github_returns_200_when_user_has_password() {
    let mock = MockServer::start().await;
    let ctx = TestContext::new_with_config(oauth_linking_config(&mock)).await;
    let token = ctx.user_token().await;

    // Assume helper links provider row for current user directly for test setup.
    ctx.link_oauth_for_current_user("github", "provider-account-1").await;

    let unlink = ctx
        .server
        .delete("/api/v1/me/oauth/github")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(unlink, 200);
}

#[tokio::test]
async fn test_unlink_only_auth_method_returns_400() {
    let mock = MockServer::start().await;
    let ctx = TestContext::new_with_config(oauth_linking_config(&mock)).await;

    let token = ctx
        .create_oauth_only_user_and_token("oauth-only@example.com", "github", "gh-only-1")
        .await;

    let unlink = ctx
        .server
        .delete("/api/v1/me/oauth/github")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(unlink, 400);
}

#[tokio::test]
async fn test_link_callback_conflict_when_provider_account_already_linked_to_other_user() {
    let mock = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/token"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "access_token": "gh-access",
            "token_type": "bearer"
        })))
        .mount(&mock)
        .await;

    Mock::given(method("GET"))
        .and(path("/userinfo"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": 424242,
            "login": "conflict-user",
            "email": "conflict@example.com"
        })))
        .mount(&mock)
        .await;

    let ctx = TestContext::new_with_config(oauth_linking_config(&mock)).await;
    let token = ctx.user_token().await;

    // Pre-link provider account to a different user.
    ctx.link_oauth_for_other_user("github", "424242").await;

    let start = ctx
        .server
        .get("/auth/oauth/github/link")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(start, 302);

    let location = start.header("location").unwrap_or_default();
    let state = location
        .split("state=")
        .nth(1)
        .unwrap_or_default()
        .split('&')
        .next()
        .unwrap_or_default()
        .to_string();

    let callback = ctx
        .server
        .get(&format!("/auth/oauth/github/link/callback?code=test-code&state={state}"))
        .await;

    assert_status!(callback, 409);
}
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_oauth_linking 2>&1 | tail -60
```
Expected: **BUILD FAILED** — OAuth link/unlink routes and state verification flow for linking are not implemented yet.

## Commit
```bash
git add backend/tests/test_oauth_linking.rs
git commit -m "Phase 26a — OAuth account linking tests (failing)"
```
