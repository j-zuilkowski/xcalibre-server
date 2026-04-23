#![allow(dead_code, unused_imports)]

mod common;

use axum::http::{header, HeaderValue};
use axum_test::multipart::{MultipartForm, Part};
use common::{auth_header, epub_with_cover_bytes, TestContext};
use sqlx::Row;

async fn upload_book_with_cover(ctx: &TestContext, token: &str) -> String {
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(epub_with_cover_bytes())
            .file_name("with-cover.epub")
            .mime_type("application/epub+zip"),
    );

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .multipart(form)
        .await;

    assert_status!(response, 201);
    response.json::<serde_json::Value>()["id"]
        .as_str()
        .expect("book id")
        .to_string()
}

async fn fetch_cover_path(ctx: &TestContext, book_id: &str) -> String {
    let row = sqlx::query("SELECT cover_path FROM books WHERE id = ?")
        .bind(book_id)
        .fetch_one(&ctx.db)
        .await
        .expect("query cover path");
    row.get::<Option<String>, _>("cover_path")
        .expect("cover path should exist")
}

#[tokio::test]
async fn test_cover_upload_generates_webp_variants() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_book_with_cover(&ctx, &token).await;
    let cover_path = fetch_cover_path(&ctx, &book_id).await;

    let thumb_jpg = cover_path.replace(".jpg", ".thumb.jpg");
    let cover_webp = cover_path.replace(".jpg", ".webp");
    let thumb_webp = cover_path.replace(".jpg", ".thumb.webp");

    assert!(ctx.storage.path().join(&cover_path).exists());
    assert!(ctx.storage.path().join(thumb_jpg).exists());
    assert!(ctx.storage.path().join(cover_webp).exists());
    assert!(ctx.storage.path().join(thumb_webp).exists());
}

#[tokio::test]
async fn test_cover_serve_returns_webp_when_accepted() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_book_with_cover(&ctx, &token).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/cover"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .add_header(
            header::ACCEPT,
            HeaderValue::from_static("image/webp,image/*;q=0.8,*/*;q=0.5"),
        )
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert!(content_type.starts_with("image/webp"));
}

#[tokio::test]
async fn test_cover_serve_falls_back_to_jpeg_when_webp_not_accepted() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_book_with_cover(&ctx, &token).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/cover"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert!(content_type.starts_with("image/jpeg"));
}

#[tokio::test]
async fn test_cover_serve_falls_back_to_jpeg_when_webp_missing() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_book_with_cover(&ctx, &token).await;
    let cover_path = fetch_cover_path(&ctx, &book_id).await;
    let webp_path = cover_path.replace(".jpg", ".webp");
    let webp_full_path = ctx.storage.path().join(&webp_path);

    assert!(
        webp_full_path.exists(),
        "webp cover should exist before deletion"
    );
    std::fs::remove_file(&webp_full_path).expect("delete webp cover");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/cover"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .add_header(header::ACCEPT, HeaderValue::from_static("image/webp"))
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert!(content_type.starts_with("image/jpeg"));
}
