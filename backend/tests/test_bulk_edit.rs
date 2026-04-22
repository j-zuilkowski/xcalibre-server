#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};
use sqlx::Row;

#[tokio::test]
async fn test_bulk_append_tags() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_one = ctx.create_book("Book One", "Author One").await;
    let book_two = ctx.create_book("Book Two", "Author Two").await;

    let response = ctx
        .server
        .patch("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "book_ids": [book_one.id, book_two.id],
            "fields": {
                "tags": {
                    "mode": "append",
                    "values": ["SciFi"]
                }
            }
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["updated"], 2);
    assert_eq!(body["errors"].as_array().map(Vec::len), Some(0));

    let first = ctx
        .server
        .get(&format!("/api/v1/books/{}", book_one.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(first, 200);
    let first_body: serde_json::Value = first.json();
    assert_eq!(first_body["tags"][0]["name"], "SciFi");

    let second = ctx
        .server
        .get(&format!("/api/v1/books/{}", book_two.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(second, 200);
    let second_body: serde_json::Value = second.json();
    assert_eq!(second_body["tags"][0]["name"], "SciFi");
}

#[tokio::test]
async fn test_bulk_overwrite_series() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_one = ctx.create_book("Book One", "Author One").await;
    let book_two = ctx.create_book("Book Two", "Author Two").await;

    let response = ctx
        .server
        .patch("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "book_ids": [book_one.id, book_two.id],
            "fields": {
                "series": {
                    "mode": "overwrite",
                    "value": "Dune"
                }
            }
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["updated"], 2);

    let first = ctx
        .server
        .get(&format!("/api/v1/books/{}", book_one.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    let first_body: serde_json::Value = first.json();
    assert_eq!(first_body["series"]["name"], "Dune");
}

#[tokio::test]
async fn test_bulk_edit_requires_admin() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Book One", "Author One").await;

    let response = ctx
        .server
        .patch("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "book_ids": [book.id],
            "fields": {
                "rating": {
                    "mode": "overwrite",
                    "value": 4
                }
            }
        }))
        .await;

    assert_status!(response, 403);
}

#[tokio::test]
async fn test_bulk_edit_empty_ids_returns_400() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .patch("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "book_ids": [],
            "fields": {}
        }))
        .await;

    assert_status!(response, 400);
}
