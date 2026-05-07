# Phase 25a — Shelf Reordering + Inline Book Serve Tests

## Context
Rust 2021, Axum 0.7. TDD: write failing tests first.
Working dir: `~/Documents/localProject/xcalibre-server`

Two independent surfaces:
1. Shelf reordering endpoint.
2. Inline book format serving endpoint.

---

## Write to: `backend/tests/test_shelf_reorder.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, minimal_epub_bytes, TestContext};
use serde_json::Value;

async fn create_book(ctx: &TestContext, token: &str, title: &str) -> String {
    let resp = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .add_header(header::CONTENT_TYPE, "multipart/form-data")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("title", title)
                .add_text("authors", "Shelf Tester")
                .add_file("file", "book.epub", "application/epub+zip", minimal_epub_bytes()),
        )
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
    let user_token = ctx.user_token().await;

    let shelf_id = create_shelf(&ctx, &user_token, "Ordered Shelf").await;
    let b1 = create_book(&ctx, &user_token, "One").await;
    let b2 = create_book(&ctx, &user_token, "Two").await;
    let b3 = create_book(&ctx, &user_token, "Three").await;
    let b4 = create_book(&ctx, &user_token, "Four").await;

    for bid in [&b1, &b2, &b3, &b4] {
        let add = ctx
            .server
            .post(&format!("/api/v1/shelves/{shelf_id}/books/{bid}"))
            .add_header(header::AUTHORIZATION, auth_header(&user_token))
            .await;
        assert_status!(add, 204);
    }

    let reorder = ctx
        .server
        .put(&format!("/api/v1/shelves/{shelf_id}/order"))
        .add_header(header::AUTHORIZATION, auth_header(&user_token))
        .json(&serde_json::json!({
            "book_ids": [b4.clone(), b2.clone(), b1.clone(), b3.clone()]
        }))
        .await;
    assert_status!(reorder, 200);

    let get = ctx
        .server
        .get(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&user_token))
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
    let token = ctx.user_token().await;
    let shelf_id = create_shelf(&ctx, &token, "Bad IDs").await;
    let b1 = create_book(&ctx, &token, "One").await;

    let add = ctx
        .server
        .post(&format!("/api/v1/shelves/{shelf_id}/books/{b1}"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(add, 204);

    let resp = ctx
        .server
        .put(&format!("/api/v1/shelves/{shelf_id}/order"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "book_ids": [b1, "not-on-shelf"] }))
        .await;
    assert_status!(resp, 400);
}

#[tokio::test]
async fn test_put_shelf_order_rejects_missing_shelf_members() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let shelf_id = create_shelf(&ctx, &token, "Missing IDs").await;
    let b1 = create_book(&ctx, &token, "One").await;
    let b2 = create_book(&ctx, &token, "Two").await;

    for bid in [&b1, &b2] {
        let add = ctx
            .server
            .post(&format!("/api/v1/shelves/{shelf_id}/books/{bid}"))
            .add_header(header::AUTHORIZATION, auth_header(&token))
            .await;
        assert_status!(add, 204);
    }

    let resp = ctx
        .server
        .put(&format!("/api/v1/shelves/{shelf_id}/order"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "book_ids": [b1] }))
        .await;
    assert_status!(resp, 400);
}

#[tokio::test]
async fn test_put_shelf_order_forbidden_for_non_owner_non_admin() {
    let ctx = TestContext::new().await;
    let owner = ctx.user_token().await;
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
```

## Write to: `backend/tests/test_inline_serve.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, minimal_epub_bytes, TestContext};
use serde_json::Value;

#[tokio::test]
async fn test_view_endpoint_serves_inline_with_content_type() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let upload = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .add_header(header::CONTENT_TYPE, "multipart/form-data")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("title", "Inline Book")
                .add_text("authors", "Inline Tester")
                .add_file("file", "inline.epub", "application/epub+zip", minimal_epub_bytes()),
        )
        .await;
    assert_status!(upload, 201);
    let book_id = upload.json::<Value>()["id"].as_str().unwrap_or_default().to_string();

    let resp = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/view/epub"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(resp, 200);
    let disposition = resp.header("content-disposition").unwrap_or_default();
    assert!(disposition.contains("inline"));
    assert_eq!(resp.header("content-type"), Some("application/epub+zip".to_string()));
}

#[tokio::test]
async fn test_view_endpoint_404_for_missing_format() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let upload = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .add_header(header::CONTENT_TYPE, "multipart/form-data")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("title", "Inline Book")
                .add_text("authors", "Inline Tester")
                .add_file("file", "inline.epub", "application/epub+zip", minimal_epub_bytes()),
        )
        .await;
    assert_status!(upload, 201);
    let book_id = upload.json::<Value>()["id"].as_str().unwrap_or_default().to_string();

    let resp = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/view/mobi"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(resp, 404);
}
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_shelf_reorder --test test_inline_serve 2>&1 | tail -60
```
Expected: **BUILD FAILED** — `/api/v1/shelves/:id/order` and `/api/v1/books/:id/view/:format` not implemented yet.

## Commit
```bash
git add backend/tests/test_shelf_reorder.rs \
        backend/tests/test_inline_serve.rs
git commit -m "Phase 25a — shelf reorder and inline serve tests (failing)"
```
