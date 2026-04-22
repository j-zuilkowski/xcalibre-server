#![allow(dead_code, unused_imports)]

mod common;

use axum::http::HeaderValue;
use axum_test::multipart::{MultipartForm, Part};
use chrono::Utc;
use common::{auth_header, epub_with_cover_bytes, minimal_pdf_bytes, TestContext};

#[tokio::test]
async fn test_download_returns_full_file() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, file_path) = ctx.create_book_with_file("Downloadable", "EPUB").await;
    let bytes = b"abcdefghijklmnopqrstuvwxyz".to_vec();
    std::fs::write(&file_path, &bytes).expect("write fixture bytes");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/formats/EPUB/download", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    assert_eq!(response.as_bytes().to_vec(), bytes);
}

#[tokio::test]
async fn test_stream_supports_range_requests() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, file_path) = ctx.create_book_with_file("Streamable", "EPUB").await;
    let bytes = b"abcdefghijklmnopqrstuvwxyz".to_vec();
    std::fs::write(&file_path, &bytes).expect("write fixture bytes");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/formats/EPUB/stream", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .add_header(
            axum::http::header::RANGE,
            HeaderValue::from_static("bytes=0-0"),
        )
        .await;

    assert_status!(response, 206);
    let content_range_header = response.header(axum::http::header::CONTENT_RANGE);
    let content_range = content_range_header.to_str().expect("content-range header");
    assert!(content_range.starts_with("bytes 0-0/"));
}

#[tokio::test]
async fn test_stream_partial_content_206() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, file_path) = ctx.create_book_with_file("Partial", "EPUB").await;
    std::fs::write(&file_path, b"abcdefghijklmnopqrstuvwxyz").expect("write fixture bytes");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/formats/EPUB/stream", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .add_header(
            axum::http::header::RANGE,
            HeaderValue::from_static("bytes=5-9"),
        )
        .await;

    assert_status!(response, 206);
    assert_eq!(response.as_bytes().as_ref(), b"fghij");
}

#[tokio::test]
async fn test_cover_returns_image() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let upload = MultipartForm::new().add_part(
        "file",
        Part::bytes(epub_with_cover_bytes())
            .file_name("with-cover.epub")
            .mime_type("application/epub+zip"),
    );
    let created = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(upload)
        .await;
    assert_status!(created, 201);
    let body: serde_json::Value = created.json();
    let book_id = body["id"].as_str().expect("book id");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/cover"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let content_type_header = response.header(axum::http::header::CONTENT_TYPE);
    let content_type = content_type_header.to_str().expect("content-type");
    assert!(content_type.starts_with("image/jpeg"));
    assert!(!response.as_bytes().is_empty());
}

#[tokio::test]
async fn test_cover_missing_returns_404() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("No Cover", "Author").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/cover", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_path_traversal_rejected() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, _path) = ctx.create_book_with_file("Traversal", "EPUB").await;

    sqlx::query("UPDATE formats SET path = ? WHERE book_id = ? AND upper(format) = 'EPUB'")
        .bind("../../etc/passwd")
        .bind(&book.id)
        .execute(&ctx.db)
        .await
        .expect("update traversal path");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/formats/EPUB/download", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    let status = response.status_code().as_u16();
    assert!(
        status == 400 || status == 403,
        "expected 400/403, got {status}"
    );
}

#[tokio::test]
async fn test_download_requires_auth() {
    let ctx = TestContext::new().await;
    let (book, _path) = ctx.create_book_with_file("Needs Auth", "EPUB").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/formats/EPUB/download", book.id))
        .await;

    assert_status!(response, 401);
}

#[tokio::test]
async fn test_download_requires_download_permission() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let (book, _path) = ctx.create_book_with_file("Needs Permission", "EPUB").await;
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
        .get(&format!("/api/v1/books/{}/formats/EPUB/download", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 403);
}

#[tokio::test]
async fn test_cover_requires_download_permission() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;
    let user_token = ctx.user_token().await;
    let now = Utc::now().to_rfc3339();

    let upload = MultipartForm::new().add_part(
        "file",
        Part::bytes(epub_with_cover_bytes())
            .file_name("with-cover.epub")
            .mime_type("application/epub+zip"),
    );
    let created = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&admin_token))
        .multipart(upload)
        .await;
    assert_status!(created, 201);
    let body: serde_json::Value = created.json();
    let book_id = body["id"].as_str().expect("book id").to_string();

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
        .get(&format!("/api/v1/books/{book_id}/cover"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .await;

    assert_status!(response, 403);
}

#[tokio::test]
async fn test_text_requires_download_permission() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let (book, _path) = ctx.create_book_with_file("No Text Access", "EPUB").await;
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
        .get(&format!("/api/v1/books/{}/text", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 403);
}
