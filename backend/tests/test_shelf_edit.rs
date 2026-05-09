#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, TestContext};
use serde_json::{json, Value};

#[tokio::test]
async fn test_patch_shelf_rename_and_toggle_public() {
    let ctx = TestContext::new().await;
    let user = ctx.user_token().await;

    // Create shelf.
    let create = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(header::AUTHORIZATION, auth_header(&user))
        .json(&json!({ "name": "Original Name", "public": false }))
        .await;
    assert_status!(create, 201);
    let shelf: Value = create.json();
    let shelf_id = shelf["id"].as_str().unwrap().to_string();

    // Rename + toggle public.
    let patch = ctx
        .server
        .patch(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&user))
        .json(&json!({ "name": "New Name", "public": true }))
        .await;
    assert_status!(patch, 200);
    let updated: Value = patch.json();
    assert_eq!(updated["name"], "New Name");
    assert_eq!(updated["public"], true);
}

#[tokio::test]
async fn test_patch_shelf_partial_update_name_only() {
    let ctx = TestContext::new().await;
    let user = ctx.user_token().await;

    let create = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(header::AUTHORIZATION, auth_header(&user))
        .json(&json!({ "name": "Keep Public", "public": true }))
        .await;
    assert_status!(create, 201);
    let shelf_id = create.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    // Only rename; public should remain true.
    let patch = ctx
        .server
        .patch(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&user))
        .json(&json!({ "name": "Renamed Only" }))
        .await;
    assert_status!(patch, 200);
    let updated: Value = patch.json();
    assert_eq!(updated["name"], "Renamed Only");
    assert_eq!(updated["public"], true);
}

#[tokio::test]
async fn test_patch_shelf_non_owner_forbidden() {
    let ctx = TestContext::new().await;
    let owner = ctx.user_token().await;
    let other = ctx.create_user_token("other@example.com").await;

    let create = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(header::AUTHORIZATION, auth_header(&owner))
        .json(&json!({ "name": "Owner Shelf", "public": false }))
        .await;
    assert_status!(create, 201);
    let shelf_id = create.json::<Value>()["id"]
        .as_str()
        .unwrap()
        .to_string();

    let patch = ctx
        .server
        .patch(&format!("/api/v1/shelves/{shelf_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&other))
        .json(&json!({ "name": "Stolen" }))
        .await;
    assert_status!(patch, 403);
}
