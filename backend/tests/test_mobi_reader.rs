#![allow(dead_code, unused_imports)]

mod common;

use chrono::Utc;
use common::{
    auth_header, minimal_azw3_bytes, minimal_epub_bytes, minimal_mobi_bytes, TestContext,
};

async fn create_book_with_format_bytes(ctx: &TestContext, format: &str, bytes: &[u8]) -> String {
    let (book, path) = ctx.create_book_with_file("Mobi Fixture", format).await;
    std::fs::write(path, bytes).expect("write format fixture");
    book.id
}

#[tokio::test]
async fn test_mobi_to_epub_returns_epub_zip() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = create_book_with_format_bytes(&ctx, "MOBI", &minimal_mobi_bytes()).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/formats/mobi/to-epub"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(axum::http::header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert_eq!(content_type, "application/epub+zip");
    assert!(response.as_bytes().starts_with(b"PK\x03\x04"));
}

#[tokio::test]
async fn test_azw3_to_epub_returns_epub_zip() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = create_book_with_format_bytes(&ctx, "AZW3", &minimal_azw3_bytes()).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/formats/azw3/to-epub"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(axum::http::header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert_eq!(content_type, "application/epub+zip");
    assert!(response.as_bytes().starts_with(b"PK\x03\x04"));
}

#[tokio::test]
async fn test_to_epub_returns_400_for_epub_format() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = create_book_with_format_bytes(&ctx, "EPUB", &minimal_epub_bytes()).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/formats/epub/to-epub"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 400);
}

#[tokio::test]
async fn test_to_epub_returns_404_for_missing_format() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Missing Format", "Author").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/formats/mobi/to-epub", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_to_epub_requires_download_permission() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book_id = create_book_with_format_bytes(&ctx, "MOBI", &minimal_mobi_bytes()).await;
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT OR REPLACE INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
        VALUES ('no_download', 'no_download', 0, 0, 1, 0, ?, ?)
        "#,
    )
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert role");

    sqlx::query("UPDATE users SET role_id = 'no_download' WHERE username = 'user'")
        .execute(&ctx.db)
        .await
        .expect("update user role");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/formats/mobi/to-epub"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 403);
}
