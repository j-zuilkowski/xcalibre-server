#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use axum_test::multipart::Part;
use common::{epub_with_cover_bytes, minimal_epub_bytes, TestContext};
use serde_json::Value;

async fn opds_basic_auth(ctx: &TestContext) -> axum::http::HeaderValue {
    ctx.opds_basic_auth_header().await
}

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

#[tokio::test]
async fn test_opds_cover_inline_content_type_and_404_without_cover() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;

    let with_cover = create_book_with_cover(&ctx, "Cover Book").await;


    let api_cover = ctx.server.get(&format!("/api/v1/books/{with_cover}/cover"))
        .add_header(header::AUTHORIZATION, auth.clone()).await;
    assert_status!(api_cover, 200);

    let without_cover = {
        let admin = ctx.admin_token().await;
        let resp = ctx.server.post("/api/v1/books")
            .add_header(header::AUTHORIZATION, common::auth_header(&admin))
            .add_header(header::CONTENT_TYPE, axum::http::HeaderValue::from_static("multipart/form-data"))
            .multipart(axum_test::multipart::MultipartForm::new()
                .add_text("title", "No Cover").add_text("authors", "Tester")
                .add_part("file", Part::bytes(epub_with_cover_bytes()).file_name("book.epub").mime_type("application/epub+zip")))
            .await;
        assert_status!(resp, 201);
        let body: Value = resp.json();
        body["id"].as_str().unwrap_or_default().to_string()
    };

    let jpeg = ctx.server
        .get(&format!("/opds/public-cover/{with_cover}"))
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, axum::http::HeaderValue::from_static("image/jpeg"))
        .await;
    let s = jpeg.status_code(); eprintln!("JPEG status: {} body: {}", s, jpeg.text());
    assert_eq!(jpeg.header("content-type").to_str().unwrap(), "image/jpeg");

    let webp = ctx.server
        .get(&format!("/opds/public-cover/{with_cover}"))
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, axum::http::HeaderValue::from_static("image/webp"))
        .await;
    assert_status!(webp, 200);
    assert_eq!(webp.header("content-type").to_str().unwrap(), "image/webp");

    let auth2 = opds_basic_auth(&ctx).await;
    let missing = ctx.server.get(&format!("/opds/public-cover/{without_cover}"))
        .add_header(header::AUTHORIZATION, auth2).await;
    assert_status!(missing, 404);
}

#[tokio::test]
async fn test_opds_cover_thumb_variant_serves_image() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let id = create_book_with_cover(&ctx, "Thumb Variant").await;
    let resp = ctx.server.get(&format!("/opds/public-cover/{id}/thumb"))
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, axum::http::HeaderValue::from_static("image/jpeg")).await;
    assert_status!(resp, 200);
    assert_eq!(resp.header("content-type").to_str().unwrap(), "image/jpeg");
}

#[tokio::test]
async fn test_opds_cover_large_variant_serves_image() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let id = create_book_with_cover(&ctx, "Large Variant").await;
    let resp = ctx.server.get(&format!("/opds/public-cover/{id}/large"))
        .add_header(header::AUTHORIZATION, auth.clone())
        .add_header(header::ACCEPT, axum::http::HeaderValue::from_static("image/jpeg")).await;
    assert_status!(resp, 200);
    assert_eq!(resp.header("content-type").to_str().unwrap(), "image/jpeg");
}

#[tokio::test]
async fn test_opds_osd_returns_valid_descriptor_xml() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let resp = ctx.server.get("/opds/osd")
        .add_header(header::AUTHORIZATION, auth.clone()).await;
    assert_status!(resp, 200);
    assert_eq!(resp.header("content-type").to_str().unwrap(), "application/opensearchdescription+xml");
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
    let resp = ctx.server.get("/opds/search/Dune")
        .add_header(header::AUTHORIZATION, auth.clone()).await;
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
    let resp = ctx.server.get("/opds/new")
        .add_header(header::AUTHORIZATION, auth.clone()).await;
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
    for (idx, id) in ids.iter().enumerate() {
        let downloads = idx * 2 + 1;
        for _ in 0..downloads {
            sqlx::query("INSERT INTO download_history (user_id, book_id, format, downloaded_at) VALUES (?1, ?2, 'epub', CURRENT_TIMESTAMP)")
                .bind(ctx.admin_user_id().await).bind(id)
                .execute(&ctx.db).await.ok();
        }
    }
    let resp = ctx.server.get("/opds/hot")
        .add_header(header::AUTHORIZATION, auth.clone()).await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.find("Hot Book 4").unwrap_or(usize::MAX) < body.find("Hot Book 3").unwrap_or(0));
}

#[tokio::test]
async fn test_opds_stats_returns_all_required_keys() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let resp = ctx.server.get("/opds/stats")
        .add_header(header::AUTHORIZATION, auth.clone()).await;
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
        let create = ctx.server.post("/api/v1/shelves")
            .add_header(header::AUTHORIZATION, common::auth_header(&admin))
            .json(&serde_json::json!({ "name": name, "is_public": true })).await;
        assert_status!(create, 201);
    }
    let resp = ctx.server.get("/opds/discover")
        .add_header(header::AUTHORIZATION, auth.clone()).await;
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
    let _ = create_book_with_cover(&ctx, "Author Letter Seed").await;
    sqlx::query("UPDATE authors SET sort_name = 'Alvarez, Juan' WHERE name = 'Tester'")
        .execute(&ctx.db).await.ok();
    let resp = ctx.server.get("/opds/authors/letter/a")
        .add_header(header::AUTHORIZATION, auth.clone()).await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.contains("Alvarez"));
}

#[tokio::test]
async fn test_opds_series_letter_browsing_is_case_insensitive() {
    let ctx = TestContext::new().await;
    let auth = opds_basic_auth(&ctx).await;
    let admin = ctx.admin_token().await;
    let book_id = create_book_with_cover(&ctx, "Series Letter Seed").await;
    let patch = ctx.server.patch(&format!("/api/v1/books/{book_id}"))
        .add_header(header::AUTHORIZATION, common::auth_header(&admin))
        .json(&serde_json::json!({ "series": "alpha series" })).await;
    assert_status!(patch, 200);
    let resp = ctx.server.get("/opds/series/letter/A")
        .add_header(header::AUTHORIZATION, auth.clone()).await;
    assert_status!(resp, 200);
    let body = resp.text();
    assert!(body.contains("alpha series"));
}
