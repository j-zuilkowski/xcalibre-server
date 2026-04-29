#![allow(dead_code, unused_imports)]

mod common;

use axum_test::multipart::{MultipartForm, Part};
use backend::db::queries::books::ListBooksParams;
use chrono::Utc;
use common::{auth_header, TestContext};

#[tokio::test]
async fn test_list_books_by_document_type() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let book = ctx.create_book("Reference Guide", "Author A").await;
    let other = ctx.create_book("General Book", "Author B").await;
    sqlx::query("UPDATE books SET document_type = 'Reference' WHERE id = ?")
        .bind(&book.id)
        .execute(&ctx.db)
        .await
        .expect("update document type");

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("document_type", "Reference")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["items"][0]["id"], book.id);
    assert_eq!(body["items"][0]["document_type"], "Reference");
    assert_ne!(body["items"][0]["id"], other.id);
}

#[tokio::test]
async fn test_in_progress_books_returns_started_books() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let (started_book, _) = ctx.create_book_with_file("Started Book", "EPUB").await;
    let format_id = started_book.formats[0].id.clone();
    let progress_response = ctx
        .server
        .patch(&format!("/api/v1/reading-progress/{}", started_book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "format_id": format_id,
            "percentage": 50,
            "cfi": null,
            "page": null,
        }))
        .await;
    assert_status!(progress_response, 200);

    let not_started_book = ctx.create_book("Unstarted Book", "Author B").await;

    let response = ctx
        .server
        .get("/api/v1/books/in-progress")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let items = body.as_array().expect("in-progress response array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], started_book.id);
    assert_ne!(items[0]["id"], not_started_book.id);
    assert_eq!(items[0]["progress_percentage"], 50.0);
}
