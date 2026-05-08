#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use axum_test::multipart::Part;
use common::{epub_with_cover_bytes, minimal_epub_bytes, TestContext};
use serde_json::Value;

async fn opds_basic_auth(ctx: &TestContext) -> axum::http::HeaderValue {
    ctx.opds_basic_auth_header().await
}

/// Creates a book with an embedded cover (via EPUB with cover bytes).
async fn create_book_with_cover(ctx: &TestContext, title: &str) -> String {
    let admin = ctx.admin_token().await;
    let resp = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, common::auth_header(&admin))
        .add_header(header::CONTENT_TYPE, axum::http::HeaderValue::from_static("multipart/form-data"))
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("title", title)
                .add_text("authors", "Tester")
                .add_part("file", Part::bytes(epub_with_cover_bytes()).file_name("book.epub").mime_type("application/epub+zip")),
        )
        .await;
    assert_status!(resp, 201);
    let body: Value = resp.json();
    body["id"].as_str().unwrap_or_default().to_string()
}

/// Creates a book without a cover (using minimal EPUB).
async fn create_book_without_cover(ctx: &TestContext, title: &str) -> String {
    let admin = ctx.admin_token().await;
    let resp = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, common::auth_header(&admin))
        .add_header(header::CONTENT_TYPE, axum::http::HeaderValue::from_static("multipart/form-data"))
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("title", title)
                .add_text("authors", "Tester")
                .add_part("file", Part::bytes(minimal_epub_bytes()).file_name("book.epub").mime_type("application/epub+zip")),
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
    let without_cover = create_book_without_cover(&ctx, "No Cover Book").await;

    let jpeg = ctx
        .server
        .get(&format!("/opds/cover/{with_cover}"))
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, axum::http::HeaderValue::from_static("image/jpeg"))
        .await;
    assert_status!(jpeg, 200);
    assert_eq!(jpeg.header("content-type").to_str().unwrap(), "image/jpeg");
    assert!(jpeg.header("content-disposition").to_str().unwrap().contains("inline"));

    let webp = ctx
        .server
        .get(&format!("/opds/cover/{with_cover}"))
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, axum::http::HeaderValue::from_static("image/webp"))
        .await;
    assert_status!(webp, 200);
    assert_eq!(webp.header("content-type").to_str().unwrap(), "image/webp");

    let auth2 = opds_basic_auth(&ctx).await;
    let missing = ctx.server.get(&format!("/opds/cover/{without_cover}"))
        .add_header(header::AUTHORIZATION, auth2).await;
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
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, axum::http::HeaderValue::from_static("image/jpeg"))
        .await;
    assert_status!(resp, 200);
    assert_eq!(resp.header("content-type").to_str().unwrap(), "image/jpeg");
}

#[tokio::test]
async fn test_opds_cover_large_variant_serves_image() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let id = create_book_with_cover(&ctx, "Large Variant").await;

    let resp = ctx
        .server
        .get(&format!("/opds/cover/{id}/large"))
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, axum::http::HeaderValue::from_static("image/jpeg"))
        .await;
    assert_status!(resp, 200);
    assert_eq!(resp.header("content-type").to_str().unwrap(), "image/jpeg");
}

#[tokio::test]
async fn test_opds_osd_returns_valid_descriptor_xml() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    let resp = ctx
        .server
        .get("/opds/osd")
        .add_header(header::AUTHORIZATION, auth.clone())
        .await;
    assert_status!(resp, 200);
    assert_eq!(
        resp.header("content-type").to_str().unwrap(),
        "application/opensearchdescription+xml"
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
        .add_header(header::AUTHORIZATION, auth.clone())
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
        let _ = create_book_without_cover(&ctx, &format!("New Book {i:02}")).await;
    }

    let resp = ctx
        .server
        .get("/opds/new")
        .add_header(header::AUTHORIZATION, auth.clone())
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    let entries = body.matches("<entry>").count();
    assert!(entries <= 30);
    assert!(entries > 0, "expected at least one entry in new feed");
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
        let downloads = idx * 2 + 1;  // idx 4 → 9, idx 3 → 7, idx 2 → 5, idx 1 → 3, idx 0 → 1
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
        .add_header(header::AUTHORIZATION, auth.clone())
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    // Hot Book 4 (last in ids) should have most downloads, appear first
    // All books have title "Cover Test Book" from EPUB metadata, check by IDs
    let id0_idx = body.find(&ids[0][..8]).unwrap_or(usize::MAX);
    let id3_idx = body.find(&ids[3][..8]).unwrap_or(usize::MAX);
    let id4_idx = body.find(&ids[4][..8]).unwrap_or(usize::MAX);
    assert!(id4_idx < id3_idx, "book4 (9 downloads) should appear before book3 (7 downloads) in hot feed");
    assert!(id3_idx < id0_idx, "book3 (7 downloads) should appear before book0 (1 download) in hot feed");
}

#[tokio::test]
async fn test_opds_stats_returns_all_required_keys() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    let resp = ctx
        .server
        .get("/opds/stats")
        .add_header(header::AUTHORIZATION, auth.clone())
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
        .add_header(header::AUTHORIZATION, auth.clone())
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

    let _book_id = create_book_with_cover(&ctx, "Author Letter Seed").await;
    // EPUB metadata sets author name to 'Cover Test Author' — update both name and sort_name
    sqlx::query("UPDATE authors SET name = 'Alvarez, Juan', sort_name = 'Alvarez, Juan' WHERE name = 'Cover Test Author'")
        .execute(&ctx.db)
        .await
        .ok();

    let resp = ctx
        .server
        .get("/opds/authors/letter/a")
        .add_header(header::AUTHORIZATION, auth.clone())
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.contains("Alvarez"), "body does not contain Alvarez: {body}");
}

#[tokio::test]
async fn test_opds_series_letter_browsing_is_case_insensitive() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let book_id = create_book_with_cover(&ctx, "Series Letter Seed").await;

    // Directly set up a series record and link the book to it
    let now = chrono::Utc::now().to_rfc3339();
    let series_id = uuid::Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO series (id, name, sort_name, last_modified) VALUES (?1, ?2, ?3, ?4)")
        .bind(&series_id)
        .bind("alpha series")
        .bind("alpha series")
        .bind(&now)
        .execute(&ctx.db)
        .await
        .ok();
    sqlx::query("UPDATE books SET series_id = ?1 WHERE id = ?2")
        .bind(&series_id)
        .bind(&book_id)
        .execute(&ctx.db)
        .await
        .ok();

    let resp = ctx
        .server
        .get("/opds/series/letter/A")
        .add_header(header::AUTHORIZATION, auth.clone())
        .await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.contains("alpha series"));
}
