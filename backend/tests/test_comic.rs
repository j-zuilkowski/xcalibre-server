#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};
use image::{DynamicImage, ImageBuffer, ImageFormat, Rgba};
use sqlx::Row;
use std::io::{Cursor, Write};
use uuid::Uuid;
use zip::{write::FileOptions, CompressionMethod, ZipWriter};

fn make_png(color: [u8; 4]) -> Vec<u8> {
    let image = ImageBuffer::from_fn(1, 1, |_x, _y| Rgba(color));
    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode png");
    bytes
}

async fn create_cbz_book(ctx: &TestContext) -> (String, Vec<u8>) {
    let book = ctx.create_book("Comic Book", "Comic Artist").await;
    let now = chrono::Utc::now().to_rfc3339();
    let file_name = format!("{}.cbz", book.id);
    let path = ctx.storage.path().join(&file_name);

    let file = std::fs::File::create(&path).expect("create cbz");
    let mut writer = ZipWriter::new(file);
    let options = FileOptions::default().compression_method(CompressionMethod::Stored);
    let page_one = make_png([255, 0, 0, 255]);
    let page_two = make_png([0, 0, 255, 255]);
    writer.start_file("002.png", options).expect("start file");
    writer.write_all(&page_two).expect("write page two");
    writer.start_file("001.png", options).expect("start file");
    writer.write_all(&page_one).expect("write page one");
    writer.finish().expect("finish cbz");

    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, 'CBZ', ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&book.id)
    .bind(&file_name)
    .bind(0_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert cbz format");

    (book.id, make_png([255, 0, 0, 255]))
}

#[tokio::test]
async fn test_comic_pages_returns_page_list() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Comic Book", "Comic Artist").await;
    let now = chrono::Utc::now().to_rfc3339();
    let file_name = format!("{}.cbz", book.id);
    let path = ctx.storage.path().join(&file_name);

    let file = std::fs::File::create(&path).expect("create cbz");
    let mut writer = ZipWriter::new(file);
    let options = FileOptions::default().compression_method(CompressionMethod::Stored);
    let page_one = make_png([255, 0, 0, 255]);
    let page_two = make_png([0, 0, 255, 255]);
    writer.start_file("002.png", options).expect("start file");
    writer.write_all(&page_two).expect("write page two");
    writer.start_file("001.png", options).expect("start file");
    writer.write_all(&page_one).expect("write page one");
    writer.finish().expect("finish cbz");

    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, 'CBZ', ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&book.id)
    .bind(&file_name)
    .bind(0_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert cbz format");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/comic/pages", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total_pages"], 2);
    assert_eq!(body["pages"][0]["index"], 0);
    assert!(body["pages"][0]["url"]
        .as_str()
        .unwrap_or_default()
        .contains("/comic/page/0"));
}

#[tokio::test]
async fn test_comic_page_returns_image_bytes() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Comic Book", "Comic Artist").await;
    let now = chrono::Utc::now().to_rfc3339();
    let file_name = format!("{}.cbz", book.id);
    let path = ctx.storage.path().join(&file_name);

    let file = std::fs::File::create(&path).expect("create cbz");
    let mut writer = ZipWriter::new(file);
    let options = FileOptions::default().compression_method(CompressionMethod::Stored);
    let page_one = make_png([255, 0, 0, 255]);
    writer.start_file("001.png", options).expect("start file");
    writer.write_all(&page_one).expect("write page one");
    writer.finish().expect("finish cbz");

    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, 'CBZ', ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&book.id)
    .bind(&file_name)
    .bind(0_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert cbz format");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/comic/page/0", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(axum::http::header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content type");
    assert!(content_type.starts_with("image/png"));
    assert!(!response.as_bytes().is_empty());
}

#[tokio::test]
async fn test_comic_page_out_of_range_returns_404() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Comic Book", "Comic Artist").await;
    let now = chrono::Utc::now().to_rfc3339();
    let file_name = format!("{}.cbz", book.id);
    let path = ctx.storage.path().join(&file_name);

    let file = std::fs::File::create(&path).expect("create cbz");
    let mut writer = ZipWriter::new(file);
    let options = FileOptions::default().compression_method(CompressionMethod::Stored);
    let page_one = make_png([255, 0, 0, 255]);
    writer.start_file("001.png", options).expect("start file");
    writer.write_all(&page_one).expect("write page one");
    writer.finish().expect("finish cbz");

    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, 'CBZ', ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&book.id)
    .bind(&file_name)
    .bind(0_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert cbz format");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/comic/page/9", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}
