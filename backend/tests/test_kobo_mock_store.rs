#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::TestContext;
use serde_json::Value;

async fn create_kobo_token(ctx: &TestContext) -> String {
    let admin_token = ctx.admin_token().await;
    let response = ctx
        .server
        .post("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, common::auth_header(&admin_token))
        .json(&serde_json::json!({ "name": "kobo-device" }))
        .await;
    assert_status!(response, 201);
    let body: Value = response.json();
    body["token"].as_str().unwrap_or_default().to_string()
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
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/products/books/prices")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_products_books_recommendations() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/products/books/recommendations")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_dailydeal() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/products/dailydeal")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_analytics_gettests() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "POST", &format!("/kobo/{token}/v1/analytics/gettests")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_deals() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/deals")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_affiliate() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "POST", &format!("/kobo/{token}/v1/affiliate")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_loyalty_benefits() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/user/loyalty/benefits")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_recommendations() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/user/recommendations")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_wishlist() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/user/wishlist")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_wishlist_items_post() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "POST", &format!("/kobo/{token}/v1/user/wishlist/items")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_user_wishlist_items_delete() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
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
    let token = create_kobo_token(&ctx).await;
    assert_json_200(&ctx, "GET", &format!("/kobo/{token}/v1/user/profile")).await;
}

#[tokio::test]
async fn test_kobo_mock_store_product_by_id() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
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
    let token = create_kobo_token(&ctx).await;

    let resp = ctx
        .server
        .get(&format!(
            "/kobo/{token}/v1/images/unknown-uuid/600/800/90/false/image.jpg"
        ))
        .await;

    let status = resp.status_code().as_u16();
    assert!(status == 302 || status == 200);

    if status == 302 {
        let loc_hdr = resp.header("location");
        let loc = loc_hdr.to_str().unwrap_or_default();
        assert!(!loc.is_empty(), "expected non-empty Location header on redirect");
    } else {
        assert_eq!(
            resp.header("content-type").to_str().unwrap_or_default(),
            "image/jpeg"
        );
        let bytes = resp.as_bytes();
        assert!(!bytes.is_empty());
        // SOI marker
        assert_eq!(bytes[0], 0xFF);
        assert_eq!(bytes[1], 0xD8);
    }
}
