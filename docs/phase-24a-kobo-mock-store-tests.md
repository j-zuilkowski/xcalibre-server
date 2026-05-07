# Phase 24a — Kobo Mock Store Endpoints Tests

## Context
Rust 2021, Axum 0.7. TDD: write failing tests first.
Working dir: `~/Documents/localProject/xcalibre-server`

calibre-web compatibility requires a set of Kobo store-adjacent mock endpoints used by some firmware during sync. These endpoints must return valid JSON and never panic.

---

## Write to: `backend/tests/test_kobo_mock_store.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::TestContext;
use serde_json::Value;

async fn kobo_token(ctx: &TestContext) -> String {
    // Reuse existing kobo test helper used by `test_kobo.rs`.
    ctx.kobo_token_for_default_user().await
}

async fn assert_json_200(ctx: &TestContext, method: &str, path: &str) {
    let resp = match method {
        "GET" => ctx.server.get(path).await,
        "POST" => ctx.server.post(path).json(&serde_json::json!({})).await,
        "DELETE" => ctx.server.delete(path).await,
        _ => panic!("unsupported method"),
    };

    assert_status!(resp, 200);
    let body = resp.text();
    let parsed: Result<Value, _> = serde_json::from_str(&body);
    assert!(parsed.is_ok());
}

#[tokio::test]
async fn test_kobo_mock_store_products_books_prices() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/products/books/prices")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_products_books_recommendations() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/products/books/recommendations")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_dailydeal() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/products/dailydeal")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_analytics_gettests() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "POST", &format!("/kobo/{token}/v1/analytics/gettests")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_deals() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/deals")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_affiliate() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "POST", &format!("/kobo/{token}/v1/affiliate")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_loyalty_benefits() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/user/loyalty/benefits")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_recommendations() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/user/recommendations")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_wishlist() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/user/wishlist")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_wishlist_items_post() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "POST", &format!("/kobo/{token}/v1/user/wishlist/items")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_wishlist_items_delete() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(
        &ctx,
        "DELETE",
        &format!("/kobo/{token}/v1/user/wishlist/items/item-1"),
    )
    .await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_profile() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/user/profile")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_product_by_id() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;
    assert_json_200(
        &ctx,
        "GET",
        &format!("/kobo/{token}/v1/products/books/product-123"),
    )
    .await;
}

#[tokio::test]
async fn test_kobo_image_route_returns_redirect_or_jpeg_placeholder() {
    let ctx = TestContext::new().await;
    let token = kobo_token(&ctx).await;

    let resp = ctx
        .server
        .get(&format!(
            "/kobo/{token}/v1/images/unknown-uuid/600/800/90/false/image.jpg"
        ))
        .await;

    let status = resp.status_code().as_u16();
    assert!(status == 302 || status == 200);

    if status == 302 {
        assert!(resp.header("location").is_some());
    } else {
        assert_eq!(resp.header("content-type"), Some("image/jpeg".to_string()));
        let bytes = resp.bytes();
        assert!(!bytes.is_empty());
        // SOI marker
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
    }
}
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_kobo_mock_store 2>&1 | tail -40
```
Expected: **BUILD FAILED** — `/kobo/:token/v1/*` mock store endpoints and image route behavior not fully implemented yet.

## Commit
```bash
git add backend/tests/test_kobo_mock_store.rs
git commit -m "Phase 24a — Kobo mock store endpoints tests (failing)"
```
