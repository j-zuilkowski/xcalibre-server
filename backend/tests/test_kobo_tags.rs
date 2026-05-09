#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, TestContext};
use serde_json::{json, Value};

/// Helper: register a Kobo device and return (token, ctx).
async fn kobo_ctx() -> (String, TestContext) {
    let ctx = TestContext::new().await;
    let token = ctx.create_api_token("kobo-device").await;
    // Trigger initialization to register device.
    let _init = ctx
        .server
        .get(&format!("/kobo/{token}/v1/initialization"))
        .await;
    (token, ctx)
}

#[tokio::test]
async fn test_kobo_create_tag_returns_tag_id() {
    let (token, ctx) = kobo_ctx().await;

    let resp = ctx
        .server
        .post(&format!("/kobo/{token}/v1/library/tags"))
        .json(&json!({
            "Name": "Favorites",
            "Items": []
        }))
        .await;
    assert_status!(resp, 201);
    let body: Value = resp.json();
    assert!(
        body["TagId"].as_str().is_some_and(|s| !s.is_empty()),
        "TagId must be a non-empty string"
    );
}

#[tokio::test]
async fn test_kobo_delete_tag_happy_and_not_found() {
    let (token, ctx) = kobo_ctx().await;

    // Create a tag first.
    let create = ctx
        .server
        .post(&format!("/kobo/{token}/v1/library/tags"))
        .json(&json!({ "Name": "ToDelete", "Items": [] }))
        .await;
    assert_status!(create, 201);
    let tag_id = create.json::<Value>()["TagId"]
        .as_str()
        .unwrap()
        .to_string();

    let del = ctx
        .server
        .delete(&format!("/kobo/{token}/v1/library/tags/{tag_id}"))
        .await;
    assert_status!(del, 200);

    // Second delete — shelf gone.
    let del2 = ctx
        .server
        .delete(&format!("/kobo/{token}/v1/library/tags/{tag_id}"))
        .await;
    assert_status!(del2, 404);
}

#[tokio::test]
async fn test_kobo_rename_tag() {
    let (token, ctx) = kobo_ctx().await;

    let create = ctx
        .server
        .post(&format!("/kobo/{token}/v1/library/tags"))
        .json(&json!({ "Name": "Original", "Items": [] }))
        .await;
    assert_status!(create, 201);
    let tag_id = create.json::<Value>()["TagId"]
        .as_str()
        .unwrap()
        .to_string();

    let rename = ctx
        .server
        .put(&format!("/kobo/{token}/v1/library/tags/{tag_id}"))
        .json(&json!({ "Name": "Renamed" }))
        .await;
    assert_status!(rename, 200);
}

#[tokio::test]
async fn test_kobo_add_and_remove_items_from_tag() {
    let (token, ctx) = kobo_ctx().await;

    // Create a book to add.
    let book_id = ctx.seed_book("Tag Test Book").await;
    let kobo_book_id = ctx.get_kobo_book_id(&book_id).await;

    let create = ctx
        .server
        .post(&format!("/kobo/{token}/v1/library/tags"))
        .json(&json!({ "Name": "Reading Now", "Items": [] }))
        .await;
    assert_status!(create, 201);
    let tag_id = create.json::<Value>()["TagId"]
        .as_str()
        .unwrap()
        .to_string();

    // Add item.
    let add = ctx
        .server
        .post(&format!(
            "/kobo/{token}/v1/library/tags/{tag_id}/items"
        ))
        .json(&json!({
            "Items": [{ "RevisionId": kobo_book_id }]
        }))
        .await;
    assert_status!(add, 201);

    // Remove item.
    let remove = ctx
        .server
        .delete(&format!(
            "/kobo/{token}/v1/library/tags/{tag_id}/items/delete"
        ))
        .json(&json!({
            "Items": [{ "RevisionId": kobo_book_id }]
        }))
        .await;
    assert_status!(remove, 200);
}

#[tokio::test]
async fn test_kobo_invalid_token_rejected_on_tag_routes() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .server
        .post("/kobo/bad-token/v1/library/tags")
        .json(&json!({ "Name": "X", "Items": [] }))
        .await;
    assert_status!(resp, 401);
}
