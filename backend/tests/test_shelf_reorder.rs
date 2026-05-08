#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use axum_test::multipart::{MultipartForm, Part};
use common::{auth_header, minimal_epub_bytes, TestContext};
use serde_json::Value;

async fn create_book(ctx: &TestContext, token: &str, _title: &str) -> String {
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(minimal_epub_bytes())
            .file_name("book.epub")
            .mime_type("application/epub+zip"),
    );
    let resp = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .multipart(form)
        .await;
    assert_status!(resp, 201);
    let body: Value = resp.json();
    body["id"].as_str().unwrap_or_default().to_string()
}

async fn create_shelf(ctx: &TestContext, token: &str, name: &str) -> String {
    let resp = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .json(&serde_json::json!({ "name": name, "is_public": false }))
        .await;
    assert_status!(resp, 201);
    let body: Value = resp.json();
    body["id"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn test_put_shelf_order_reorders_books() {
    let ctx = TestContext::new().await;

    // Use admin token for uploads (needs can_upload permission)
    let admin_token = ctx.admin_token().await;
    let shelf_id = create_shelf(&ctx, &admin_token, "Ordered Shelf").await;
    let b1 = create_book(&ctx, &admin_token, "One").await;
    let b2 = create_book(&ctx, &admin_token, "Two").await;
    let b3 = create_book(&ctx, &admin_token, "Three").await;
    let b4 = create_book(&ctx, &admin_token, "Four").await;

    for bid in [&b1, &b2, &b3, &b4] {
        let add = ctx
            .server
            .post(&format!("/api/v1/shelves/{shelf_id}/books/{bid}"))
            .add_header(header::AUTHORIZATION, auth_header(&admin_token))
            .await;
        assert_status!(add, 204);
    }

    let reorder = ctx
        .server
        .put(&format!("/api/v1/shelves/{shelf_id}/order"))
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .json(&serde_json::json!({
            "book_ids": [b4.clone(), b2.clone(), b1.clone(), b3.clone()]
        }))
        .await;
    assert_status!(reorder, 200);

    let get = ctx
        .server
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .await;
    assert_status!(get, 200);
    let body: Value = get.json();
    let ids: Vec<String> = body["books"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|b| b["id"].as_str().map(ToString::to_string))
        .collect();

    assert_eq!(ids, vec![b4, b2, b1, b3]);
}

#[tokio::test]
async fn test_put_shelf_order_rejects_unknown_book_ids() {
    let ctx = TestContext::new().await;

    // Use admin token for uploads (needs can_upload permission)
    let admin_token = ctx.admin_token().await;
    let shelf_id = create_shelf(&ctx, &admin_token, "Bad IDs").await;
    let b1 = create_book(&ctx, &admin_token, "One").await;

    let add = ctx
        .server
        .post(&format!("/api/v1/shelves/{shelf_id}/books/{b1}"))
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .await;
    assert_status!(add, 204);

    let resp = ctx
        .server
        .put(&format!("/api/v1/shelves/{shelf_id}/order"))
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .json(&serde_json::json!({ "book_ids": [b1, "not-on-shelf"] }))
        .await;
    assert_status!(resp, 400);
}

#[tokio::test]
async fn test_put_shelf_order_rejects_missing_shelf_members() {
    let ctx = TestContext::new().await;

    // Use admin token for uploads (needs can_upload permission)
    let admin_token = ctx.admin_token().await;
    let shelf_id = create_shelf(&ctx, &admin_token, "Missing IDs").await;
    let b1 = create_book(&ctx, &admin_token, "One").await;
    let b2 = create_book(&ctx, &admin_token, "Two").await;

    for bid in [&b1, &b2] {
        let add = ctx
            .server
            .post(&format!("/api/v1/shelves/{shelf_id}/books/{bid}"))
            .add_header(header::AUTHORIZATION, auth_header(&admin_token))
            .await;
        assert_status!(add, 204);
    }

    let resp = ctx
        .server
        .put(&format!("/api/v1/shelves/{shelf_id}/order"))
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .json(&serde_json::json!({ "book_ids": [b1] }))
        .await;
    assert_status!(resp, 400);
}

#[tokio::test]
async fn test_put_shelf_order_forbidden_for_non_owner_non_admin() {
    let ctx = TestContext::new().await;
    let owner = ctx.admin_token().await;
    let other = ctx.create_user_and_token("other-shelf-user@example.com").await;

    let shelf_id = create_shelf(&ctx, &owner, "Owner Shelf").await;
    let b1 = create_book(&ctx, &owner, "One").await;

    let add = ctx
        .server
        .post(&format!("/api/v1/shelves/{shelf_id}/books/{b1}"))
        .add_header(header::AUTHORIZATION, auth_header(&owner))
        .await;
    assert_status!(add, 204);

    let resp = ctx
        .server
        .put(&format!("/api/v1/shelves/{shelf_id}/order"))
        .add_header(header::AUTHORIZATION, auth_header(&other))
        .json(&serde_json::json!({ "book_ids": [b1] }))
        .await;
    assert_status!(resp, 403);
}

#[tokio::test]
async fn test_put_shelf_order_not_found_for_missing_shelf() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let resp = ctx
        .server
        .put("/api/v1/shelves/non-existent/order")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "book_ids": [] }))
        .await;
    assert_status!(resp, 404);
}
