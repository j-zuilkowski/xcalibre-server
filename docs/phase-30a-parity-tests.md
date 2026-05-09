# Phase 30a — OPDS Parity II + Kobo Tag Sync + Shelf Edit: Failing Tests

## Context

Rust 2021, Axum 0.7. TDD — write failing tests first.
Working dir: `~/Documents/localProject/xcalibre-server`

A post-Phase 28 parity audit found three categories of missing implementation:

1. **Kobo tag sync** (false positive) — `POST/DELETE/PUT /kobo/:token/v1/library/tags` and `POST/DELETE /v1/library/tags/:tag_id/items` are absent from `kobo.rs`. Kobo devices call these to create/manage shelf collections on the server.
2. **OPDS feeds** (false positives + new gaps) — category/tag, read/unread, shelf, formats, and UUID-lookup feeds are absent from `opds.rs`.
3. **Shelf edit** — `PATCH /api/v1/shelves/:id` to rename/toggle public is absent from `shelves.rs`.

Write tests only. All must FAIL with compile error or 404/405 before implementation.

---

## File 1 — Write to: `backend/tests/test_kobo_tags.rs`

```rust
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
    let kobo_book_id = ctx.get_kobo_book_id(book_id).await;

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
```

---

## File 2 — Write to: `backend/tests/test_opds_parity.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use common::TestContext;
use serde_json::Value;

/// Check that response body contains needle as a substring.
fn assert_body_contains(body: &str, needle: &str) {
    assert!(
        body.contains(needle),
        "Expected body to contain {needle:?}, got:\n{body}"
    );
}

#[tokio::test]
async fn test_opds_category_navigation_feed() {
    let ctx = TestContext::new().await;
    ctx.seed_book_with_tag("Sci-Fi Book", "Science Fiction").await;

    let resp = ctx.server.get("/opds/category").await;
    assert_status!(resp, 200);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.contains("atom+xml"), "expected Atom feed, got: {ct}");
    let body = resp.text();
    assert_body_contains(&body, "Science Fiction");
}

#[tokio::test]
async fn test_opds_category_books_acquisition_feed() {
    let ctx = TestContext::new().await;
    ctx.seed_book_with_tag("Dune", "Science Fiction").await;

    // First get the category id for "Science Fiction".
    let nav = ctx.server.get("/opds/category").await;
    assert_status!(nav, 200);
    let nav_body = nav.text();
    // Extract tag id from href in nav feed (format: /opds/category/<id>).
    let tag_id = extract_category_id(&nav_body).expect("category id in nav feed");

    let resp = ctx
        .server
        .get(&format!("/opds/category/{tag_id}"))
        .await;
    assert_status!(resp, 200);
    assert_body_contains(&resp.text(), "Dune");
}

#[tokio::test]
async fn test_opds_readbooks_feed_requires_token_and_filters_correctly() {
    let ctx = TestContext::new().await;
    let token = ctx.create_api_token("read-test").await;
    let book_id = ctx.seed_book("Read Book").await;
    ctx.mark_book_read(book_id, true).await;
    ctx.seed_book("Unread Book").await;

    // Without token: 401 or empty (implementation choice — must not 404/500).
    let no_auth = ctx.server.get("/opds/readbooks").await;
    assert!(
        [200u16, 401].contains(&no_auth.status_code().as_u16()),
        "unauthenticated readbooks: got {}",
        no_auth.status_code()
    );

    // With token: should see Read Book, not Unread Book.
    let authed = ctx
        .server
        .get(&format!("/opds/readbooks?token={token}"))
        .await;
    assert_status!(authed, 200);
    let body = authed.text();
    assert_body_contains(&body, "Read Book");
    assert!(
        !body.contains("Unread Book"),
        "Unread Book must not appear in readbooks feed"
    );
}

#[tokio::test]
async fn test_opds_unreadbooks_feed_requires_token_and_filters_correctly() {
    let ctx = TestContext::new().await;
    let token = ctx.create_api_token("unread-test").await;
    ctx.seed_book("Unread Book").await;
    let read_id = ctx.seed_book("Already Read").await;
    ctx.mark_book_read(read_id, true).await;

    let authed = ctx
        .server
        .get(&format!("/opds/unreadbooks?token={token}"))
        .await;
    assert_status!(authed, 200);
    let body = authed.text();
    assert_body_contains(&body, "Unread Book");
    assert!(
        !body.contains("Already Read"),
        "Already Read must not appear in unreadbooks feed"
    );
}

#[tokio::test]
async fn test_opds_shelf_index_and_per_shelf_feed() {
    let ctx = TestContext::new().await;
    // Create a public shelf via REST.
    let token = ctx.create_api_token("shelf-test").await;
    let shelf_id = ctx.seed_public_shelf("My Shelf", token.as_str()).await;
    let book_id = ctx.seed_book("Shelved Book").await;
    ctx.add_book_to_shelf(book_id, &shelf_id, token.as_str()).await;

    // Shelf index navigation feed.
    let idx = ctx.server.get("/opds/shelf").await;
    assert_status!(idx, 200);
    assert_body_contains(&idx.text(), "My Shelf");

    // Per-shelf acquisition feed.
    let shelf_feed = ctx
        .server
        .get(&format!("/opds/shelf/{shelf_id}"))
        .await;
    assert_status!(shelf_feed, 200);
    assert_body_contains(&shelf_feed.text(), "Shelved Book");
}

#[tokio::test]
async fn test_opds_formats_navigation_and_per_format_feed() {
    let ctx = TestContext::new().await;
    ctx.seed_book_with_format("EPUB Book", "epub").await;

    let nav = ctx.server.get("/opds/formats").await;
    assert_status!(nav, 200);
    assert_body_contains(&nav.text(), "epub");

    let fmt = ctx.server.get("/opds/formats/epub").await;
    assert_status!(fmt, 200);
    assert_body_contains(&fmt.text(), "EPUB Book");
}

#[tokio::test]
async fn test_opds_category_letter_feed() {
    let ctx = TestContext::new().await;
    ctx.seed_book_with_tag("A Book", "Adventure").await;
    ctx.seed_book_with_tag("B Book", "Biography").await;

    let resp = ctx.server.get("/opds/category/letter/A").await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert_body_contains(&body, "Adventure");
    assert!(
        !body.contains("Biography"),
        "Biography must not appear for letter A"
    );
}

#[tokio::test]
async fn test_opds_ajax_book_uuid_lookup() {
    let ctx = TestContext::new().await;
    let book_id = ctx.seed_book("UUID Lookup Book").await;
    let uuid = ctx.get_book_uuid(book_id).await;

    let resp = ctx
        .server
        .get(&format!("/opds/ajax/book/{uuid}"))
        .await;
    assert_status!(resp, 200);
    assert_body_contains(&resp.text(), "UUID Lookup Book");
}

// Helper: extract first /opds/category/<id> href from Atom XML body.
fn extract_category_id(xml: &str) -> Option<String> {
    let prefix = "/opds/category/";
    xml.split(prefix)
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(|s| s.to_string())
}
```

---

## File 3 — Write to: `backend/tests/test_shelf_edit.rs`

```rust
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
```

---

## Verify (all tests must FAIL — 404 / compile error expected)

```bash
cd ~/Documents/localProject/xcalibre-server
cargo test -p backend test_kobo_tags 2>&1 | tail -20
cargo test -p backend test_opds_parity 2>&1 | tail -20
cargo test -p backend test_shelf_edit 2>&1 | tail -20
```

Expected: compile errors on missing helper methods (`get_kobo_book_id`, `seed_book_with_tag`, `seed_public_shelf`, `mark_book_read`, `get_book_uuid`, `seed_book_with_format`, `add_book_to_shelf`, `create_user_token`) and 404/405 from missing routes. That is the correct failing state.

---

## Commit

```
git add backend/tests/test_kobo_tags.rs backend/tests/test_opds_parity.rs backend/tests/test_shelf_edit.rs
git commit -m "Phase 30a — Kobo tag sync + OPDS parity II + shelf edit: failing tests"
```
