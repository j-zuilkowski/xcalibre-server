# Phase 23a — OPDS Enhancements Tests

## Context
Rust 2021, Axum 0.7. TDD: write failing tests first.
Working dir: `~/Documents/localProject/xcalibre-server`
Phase 22 complete.

New OPDS compatibility surface for calibre-web parity:
- Cover endpoints in OPDS namespace with inline image responses and variant sizing.
- OpenSearch descriptor endpoint (`/opds/osd`).
- Path-based OPDS search compatibility (`/opds/search/<path:query>`).
- Additional OPDS feeds (`/opds/new`, `/opds/hot`, `/opds/discover`).
- Stats endpoint (`/opds/stats`) returning JSON.
- Letter-based author/series browsing.

OPDS auth remains HTTP Basic auth.

---

## Write to: `backend/tests/test_opds_enhancements.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{minimal_epub_bytes, TestContext};
use serde_json::Value;

async fn opds_basic_auth(ctx: &TestContext) -> String {
    // Reuse existing test helper used by OPDS tests; returns `Basic ...` header value.
    ctx.opds_basic_auth_header().await
}

async fn create_book_with_cover(ctx: &TestContext, title: &str) -> String {
    let admin = ctx.admin_token().await;
    let resp = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, common::auth_header(&admin))
        .add_header(header::CONTENT_TYPE, "multipart/form-data")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("title", title)
                .add_text("authors", "Tester")
                .add_file("file", "book.epub", "application/epub+zip", minimal_epub_bytes())
                .add_file("cover", "cover.jpg", "image/jpeg", include_bytes!("fixtures/cover.jpg").to_vec()),
        )
        .await;
    assert_status!(resp, 201);
    let body: Value = resp.json();
    body["id"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn test_opds_cover_inline_content_type_and_404_without_cover() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    let with_cover = create_book_with_cover(&ctx, "Cover Book").await;
    let without_cover = {
        let admin = ctx.admin_token().await;
        let resp = ctx
            .server
            .post("/api/v1/books")
            .add_header(header::AUTHORIZATION, common::auth_header(&admin))
            .add_header(header::CONTENT_TYPE, "multipart/form-data")
            .multipart(
                axum_test::multipart::MultipartForm::new()
                    .add_text("title", "No Cover")
                    .add_text("authors", "Tester")
                    .add_file("file", "book.epub", "application/epub+zip", minimal_epub_bytes()),
            )
            .await;
        assert_status!(resp, 201);
        let body: Value = resp.json();
        body["id"].as_str().unwrap_or_default().to_string()
    };

    let jpeg = ctx
        .server
        .get(&format!("/opds/cover/{with_cover}"))
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, "image/jpeg")
        .await;
    assert_status!(jpeg, 200);
    assert_eq!(jpeg.header("content-type"), Some("image/jpeg".to_string()));

    let webp = ctx
        .server
        .get(&format!("/opds/cover/{with_cover}"))
        .add_header(header::AUTHORIZATION, auth)
        .add_header(header::ACCEPT, "image/webp")
        .await;
    assert_status!(webp, 200);
    assert_eq!(webp.header("content-type"), Some("image/webp".to_string()));

    let missing = ctx.server.get(&format!("/opds/cover/{without_cover}")).await;
    assert_status!(missing, 404);
}

#[tokio::test]
async fn test_opds_cover_thumb_variant_serves_image() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let id = create_book_with_cover(&ctx, "Thumb Variant").await;

    let resp = ctx
        .server
        .get(&format!("/opds/cover/{id}/thumb"))
        .add_header(header::AUTHORIZATION, auth)
        .add_header(header::ACCEPT, "image/jpeg")
        .await;
    assert_status!(resp, 200);
    assert_eq!(resp.header("content-type"), Some("image/jpeg".to_string()));
}

#[tokio::test]
async fn test_opds_cover_large_variant_serves_image() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let id = create_book_with_cover(&ctx, "Large Variant").await;

    let resp = ctx
        .server
        .get(&format!("/opds/cover/{id}/large"))
        .add_header(header::AUTHORIZATION, auth)
        .add_header(header::ACCEPT, "image/jpeg")
        .await;
    assert_status!(resp, 200);
    assert_eq!(resp.header("content-type"), Some("image/jpeg".to_string()));
}

#[tokio::test]
async fn test_opds_osd_returns_valid_descriptor_xml() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    let resp = ctx
        .server
        .get("/opds/osd")
        .add_header(header::AUTHORIZATION, auth)
        .await;
    assert_status!(resp, 200);
    assert_eq!(
        resp.header("content-type"),
        Some("application/opensearchdescription+xml".to_string())
    );

    let body = resp.text();
    assert!(body.contains("<ShortName>"));
    assert!(body.contains("<Description>"));
    assert!(body.contains("application/atom+xml;profile=opds-catalog"));
    assert!(body.contains("/opds/search?q={searchTerms}"));
}

#[tokio::test]
async fn test_opds_search_supports_path_query_variant() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    let resp = ctx
        .server
        .get("/opds/search/Dune")
        .add_header(header::AUTHORIZATION, auth)
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.contains("<feed"));
}

#[tokio::test]
async fn test_opds_new_returns_most_recent_books_max_30() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    for i in 0..35 {
        let _ = create_book_with_cover(&ctx, &format!("New Book {i:02}")).await;
    }

    let resp = ctx
        .server
        .get("/opds/new")
        .add_header(header::AUTHORIZATION, auth)
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    let entries = body.matches("<entry>").count();
    assert!(entries <= 30);
}

#[tokio::test]
async fn test_opds_hot_orders_by_download_count_desc() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    let mut ids = Vec::new();
    for i in 0..5 {
        ids.push(create_book_with_cover(&ctx, &format!("Hot Book {i}")).await);
    }

    // simulate download counts: book4=9, book3=7, book2=5, book1=3, book0=1
    for (idx, id) in ids.iter().enumerate() {
        let downloads = idx * 2 + 1;
        for _ in 0..downloads {
            sqlx::query(
                "INSERT INTO download_history (user_id, book_id, format, downloaded_at) VALUES (?1, ?2, 'epub', CURRENT_TIMESTAMP)",
            )
            .bind(ctx.admin_user_id().await)
            .bind(id)
            .execute(&ctx.db)
            .await
            .ok();
        }
    }

    let resp = ctx
        .server
        .get("/opds/hot")
        .add_header(header::AUTHORIZATION, auth)
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.find("Hot Book 4").unwrap_or(usize::MAX) < body.find("Hot Book 3").unwrap_or(0));
}

#[tokio::test]
async fn test_opds_stats_returns_all_required_keys() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    let resp = ctx
        .server
        .get("/opds/stats")
        .add_header(header::AUTHORIZATION, auth)
        .await;
    assert_status!(resp, 200);
    let body: Value = resp.json();

    assert!(body.get("total_books").is_some());
    assert!(body.get("total_authors").is_some());
    assert!(body.get("total_series").is_some());
    assert!(body.get("total_tags").is_some());
    assert!(body.get("total_formats").is_some());
}

#[tokio::test]
async fn test_opds_discover_lists_all_shelves_as_navigation_entries() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let admin = ctx.admin_token().await;

    for name in ["Favourites", "Unread", "Sci-Fi"] {
        let create = ctx
            .server
            .post("/api/v1/shelves")
            .add_header(header::AUTHORIZATION, common::auth_header(&admin))
            .json(&serde_json::json!({ "name": name, "is_public": true }))
            .await;
        assert_status!(create, 201);
    }

    let resp = ctx
        .server
        .get("/opds/discover")
        .add_header(header::AUTHORIZATION, auth)
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.contains("Favourites"));
    assert!(body.contains("Unread"));
    assert!(body.contains("Sci-Fi"));
}

#[tokio::test]
async fn test_opds_authors_letter_browsing_is_case_insensitive_and_nfkd_aware() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    // Seed authors where displayed first letter should normalize to A.
    let _ = create_book_with_cover(&ctx, "Author Letter Seed").await;
    sqlx::query("UPDATE authors SET sort = 'Álvarez, Juan' WHERE name = 'Tester'")
        .execute(&ctx.db)
        .await
        .ok();

    let resp = ctx
        .server
        .get("/opds/authors/letter/a")
        .add_header(header::AUTHORIZATION, auth)
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.contains("Álvarez") || body.contains("Alvarez"));
}

#[tokio::test]
async fn test_opds_series_letter_browsing_is_case_insensitive() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let admin = ctx.admin_token().await;
    let book_id = create_book_with_cover(&ctx, "Series Letter Seed").await;

    let patch = ctx
        .server
        .patch(&format!("/api/v1/books/{book_id}"))
        .add_header(header::AUTHORIZATION, common::auth_header(&admin))
        .json(&serde_json::json!({ "series": "alpha series" }))
        .await;
    assert_status!(patch, 200);

    let resp = ctx
        .server
        .get("/opds/series/letter/A")
        .add_header(header::AUTHORIZATION, auth)
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.contains("alpha series"));
}
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_opds_enhancements 2>&1 | tail -40
```
Expected: **BUILD FAILED** — new OPDS endpoints (`/opds/cover/*`, `/opds/osd`, `/opds/hot`, `/opds/stats`, `/opds/discover`, letter browsing, path search variant) not implemented yet.

## Commit
```bash
git add backend/tests/test_opds_enhancements.rs
git commit -m "Phase 23a — OPDS enhancements tests (failing)"
```
