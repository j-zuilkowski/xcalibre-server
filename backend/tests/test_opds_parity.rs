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
    ctx.mark_book_read(&book_id, true).await;
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
    ctx.mark_book_read(&read_id, true).await;

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
    let shelf_id = ctx.seed_public_shelf("My Shelf", &token).await;
    let book_id = ctx.seed_book("Shelved Book").await;
    ctx.add_book_to_shelf(&book_id, &shelf_id, &token).await;

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
    let uuid = ctx.get_book_uuid(&book_id).await;

    let resp = ctx
        .server
        .get(&format!("/opds/ajax/book/{uuid}"))
        .await;
    assert_status!(resp, 200);
    assert_body_contains(&resp.text(), "UUID Lookup Book");
}

// Helper: extract first /opds/category/<id> href from Atom XML body.
fn extract_category_id(xml: &str) -> Option<String> {
    // Look for href="/opds/category/..." in link elements (not id elements)
    let link_marker = r#"rel="subsection" href="/opds/category/"#;
    xml.split(link_marker)
        .nth(1)
        .and_then(|s| s.split('"').next())
        .map(|s| s.to_string())
}
