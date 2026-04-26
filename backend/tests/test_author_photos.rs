#![allow(dead_code, unused_imports)]

mod common;

use axum::http::HeaderValue;
use axum_test::multipart::{MultipartForm, Part};
use common::{auth_header, TestContext};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use sqlx::Row;
use std::io::Cursor;
use uuid::Uuid;

#[tokio::test]
async fn test_upload_photo_generates_jpeg_and_webp_variants() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = author_without_profile(&ctx, "Octavia Butler").await;

    let response = upload_author_photo(
        &ctx,
        &token,
        &author_id,
        author_photo_bytes(),
        "portrait.png",
        "image/png",
    )
    .await;

    assert_status!(response, 200);

    let bucket = author_bucket(&author_id);
    let full_jpg = author_photo_file_bytes(&ctx, &bucket, &author_id, ".jpg");
    let thumb_jpg = author_photo_file_bytes(&ctx, &bucket, &author_id, ".thumb.jpg");
    let full_webp = author_photo_file_bytes(&ctx, &bucket, &author_id, ".webp");
    let thumb_webp = author_photo_file_bytes(&ctx, &bucket, &author_id, ".thumb.webp");

    let full_jpeg = image::load_from_memory(&full_jpg).expect("decode full jpeg");
    let thumb_jpeg = image::load_from_memory(&thumb_jpg).expect("decode thumb jpeg");
    let full_webp_image = image::load_from_memory(&full_webp).expect("decode full webp");
    let thumb_webp_image = image::load_from_memory(&thumb_webp).expect("decode thumb webp");
    assert_eq!((full_jpeg.width(), full_jpeg.height()), (400, 400));
    assert_eq!((thumb_jpeg.width(), thumb_jpeg.height()), (100, 100));
    assert_eq!(
        (full_webp_image.width(), full_webp_image.height()),
        (400, 400)
    );
    assert_eq!(
        (thumb_webp_image.width(), thumb_webp_image.height()),
        (100, 100)
    );
}

#[tokio::test]
async fn test_upload_photo_updates_photo_path_in_profile() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = author_without_profile(&ctx, "N.K. Jemisin").await;

    let response = upload_author_photo(
        &ctx,
        &token,
        &author_id,
        author_photo_bytes(),
        "portrait.png",
        "image/png",
    )
    .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["profile"]["photo_url"],
        format!("/api/v1/authors/{author_id}/photo")
    );

    let row = sqlx::query("SELECT photo_path FROM author_profiles WHERE author_id = ?")
        .bind(&author_id)
        .fetch_one(&ctx.db)
        .await
        .expect("load author photo path");
    let photo_path: Option<String> = row.get("photo_path");
    let expected_photo_path = format!("authors/{}/{author_id}.jpg", author_bucket(&author_id));
    assert_eq!(photo_path.as_deref(), Some(expected_photo_path.as_str()));
}

#[tokio::test]
async fn test_serve_photo_returns_webp_when_accepted() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = author_without_profile(&ctx, "Ursula K. Le Guin").await;
    upload_author_photo(
        &ctx,
        &token,
        &author_id,
        author_photo_bytes(),
        "portrait.png",
        "image/png",
    )
    .await;

    let response = ctx
        .server
        .get(&format!("/api/v1/authors/{author_id}/photo"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .add_header(
            axum::http::header::ACCEPT,
            HeaderValue::from_static("image/webp"),
        )
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(axum::http::header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert!(content_type.starts_with("image/webp"));
}

#[tokio::test]
async fn test_serve_photo_falls_back_to_jpeg_when_webp_not_accepted() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = author_without_profile(&ctx, "Terry Pratchett").await;
    upload_author_photo(
        &ctx,
        &token,
        &author_id,
        author_photo_bytes(),
        "portrait.png",
        "image/png",
    )
    .await;

    let response = ctx
        .server
        .get(&format!("/api/v1/authors/{author_id}/photo"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(axum::http::header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert!(content_type.starts_with("image/jpeg"));
}

#[tokio::test]
async fn test_serve_photo_returns_svg_placeholder_when_no_photo() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = author_without_profile(&ctx, "Robin Hobb").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/authors/{author_id}/photo"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(axum::http::header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert!(content_type.starts_with("image/svg+xml"));
    let body = response.text();
    assert!(body.contains("<svg"));
}

#[tokio::test]
async fn test_serve_photo_placeholder_varies_by_author_name() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (first_author_id, _) = author_without_profile(&ctx, "Alastair Reynolds").await;
    let (second_author_id, _) = author_without_profile(&ctx, "Becky Chambers").await;

    let first = ctx
        .server
        .get(&format!("/api/v1/authors/{first_author_id}/photo"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    let second = ctx
        .server
        .get(&format!("/api/v1/authors/{second_author_id}/photo"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(first, 200);
    assert_status!(second, 200);
    let first_body = first.text();
    let second_body = second.text();
    assert_ne!(first_body, second_body);
}

#[tokio::test]
async fn test_upload_photo_rejects_non_image_files() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = author_without_profile(&ctx, "Seanan McGuire").await;

    let response = upload_author_photo(
        &ctx,
        &token,
        &author_id,
        b"not an image".to_vec(),
        "not-image.txt",
        "text/plain",
    )
    .await;

    assert_status!(response, 422);
}

async fn upload_author_photo(
    ctx: &TestContext,
    token: &str,
    author_id: &str,
    bytes: Vec<u8>,
    file_name: &str,
    mime_type: &str,
) -> axum_test::TestResponse {
    let form = MultipartForm::new().add_part(
        "photo",
        Part::bytes(bytes).file_name(file_name).mime_type(mime_type),
    );

    ctx.server
        .post(&format!("/api/v1/authors/{author_id}/photo"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(token))
        .multipart(form)
        .await
}

async fn author_without_profile(ctx: &TestContext, name: &str) -> (String, String) {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(name)
        .bind(&now)
        .execute(&ctx.db)
        .await
        .expect("insert author");

    let book_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO books (
            id, title, sort_title, description, pubdate, language, rating, series_id, series_index,
            has_cover, cover_path, document_type, flags, library_id, indexed_at, created_at, last_modified
        )
        VALUES (?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL, 'unknown', NULL, 'default', NULL, ?, ?)
        "#,
    )
    .bind(&book_id)
    .bind(format!("Book for {name}"))
    .bind(format!("Book for {name}"))
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert book");

    sqlx::query("INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, 0)")
        .bind(&book_id)
        .bind(&id)
        .execute(&ctx.db)
        .await
        .expect("insert book author");

    (id, book_id)
}

fn author_bucket(author_id: &str) -> String {
    author_id.chars().take(2).collect()
}

fn author_photo_file_bytes(
    ctx: &TestContext,
    bucket: &str,
    author_id: &str,
    suffix: &str,
) -> Vec<u8> {
    let path = ctx
        .storage
        .path()
        .join(format!("authors/{bucket}/{author_id}{suffix}"));
    std::fs::read(path).expect("read author photo variant")
}

fn author_photo_bytes() -> Vec<u8> {
    let image = DynamicImage::ImageRgba8(ImageBuffer::from_fn(320, 240, |x, y| {
        let red = (x % 255) as u8;
        let green = (y % 255) as u8;
        let blue = 180u8;
        Rgba([red, green, blue, 255])
    }));
    let mut cursor = Cursor::new(Vec::new());
    image
        .write_to(&mut cursor, ImageFormat::Png)
        .expect("encode png");
    cursor.into_inner()
}
