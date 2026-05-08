#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use axum_test::multipart::{MultipartForm, Part};
use common::{auth_header, minimal_epub_bytes, TestContext};
use serde_json::Value;

#[tokio::test]
async fn test_view_endpoint_serves_inline_with_content_type() {
    let ctx = TestContext::new().await;

    // Use admin token for upload (needs can_upload permission)
    let admin_token = ctx.admin_token().await;

    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(minimal_epub_bytes())
            .file_name("inline.epub")
            .mime_type("application/epub+zip"),
    );
    let upload = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .multipart(form)
        .await;
    assert_status!(upload, 201);
    let book_id = upload.json::<Value>()["id"].as_str().unwrap_or_default().to_string();

    // Use a regular user token for viewing (needs can_download, not can_upload)
    let user_token = ctx.user_token().await;
    let resp = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/view/epub"))
        .add_header(header::AUTHORIZATION, auth_header(&user_token))
        .await;

    assert_status!(resp, 200);
    let disposition = resp.header("content-disposition");
    let disp_str = disposition.to_str().unwrap_or_default().to_string();
    assert!(disp_str.contains("inline"));
    let content_type = resp.header("content-type");
    let ct_str = content_type.to_str().unwrap_or_default().to_string();
    assert_eq!(ct_str, "application/epub+zip");
}

#[tokio::test]
async fn test_view_endpoint_404_for_missing_format() {
    let ctx = TestContext::new().await;

    // Use admin token for upload (needs can_upload permission)
    let admin_token = ctx.admin_token().await;

    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(minimal_epub_bytes())
            .file_name("inline.epub")
            .mime_type("application/epub+zip"),
    );
    let upload = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .multipart(form)
        .await;
    assert_status!(upload, 201);
    let book_id = upload.json::<Value>()["id"].as_str().unwrap_or_default().to_string();

    // Use a regular user token for viewing
    let user_token = ctx.user_token().await;
    let resp = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/view/mobi"))
        .add_header(header::AUTHORIZATION, auth_header(&user_token))
        .await;

    assert_status!(resp, 404);
}
