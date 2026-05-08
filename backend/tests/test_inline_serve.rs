#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, minimal_epub_bytes, TestContext};
use serde_json::Value;

#[tokio::test]
async fn test_view_endpoint_serves_inline_with_content_type() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let upload = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .add_header(header::CONTENT_TYPE, "multipart/form-data")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("title", "Inline Book")
                .add_text("authors", "Inline Tester")
                .add_file("file", "inline.epub", "application/epub+zip", minimal_epub_bytes()),
        )
        .await;
    assert_status!(upload, 201);
    let book_id = upload.json::<Value>()["id"].as_str().unwrap_or_default().to_string();

    let resp = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/view/epub"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(resp, 200);
    let disposition = resp.header("content-disposition").unwrap_or_default();
    assert!(disposition.contains("inline"));
    assert_eq!(resp.header("content-type"), Some("application/epub+zip".to_string()));
}

#[tokio::test]
async fn test_view_endpoint_404_for_missing_format() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let upload = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .add_header(header::CONTENT_TYPE, "multipart/form-data")
        .multipart(
            axum_test::multipart::MultipartForm::new()
                .add_text("title", "Inline Book")
                .add_text("authors", "Inline Tester")
                .add_file("file", "inline.epub", "application/epub+zip", minimal_epub_bytes()),
        )
        .await;
    assert_status!(upload, 201);
    let book_id = upload.json::<Value>()["id"].as_str().unwrap_or_default().to_string();

    let resp = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/view/mobi"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(resp, 404);
}
