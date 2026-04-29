#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};

#[tokio::test]
async fn test_metadata_search_returns_200_for_existing_book() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Dune", "Frank Herbert").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/metadata/search", book.id))
        .add_query_param("q", "Dune")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body.is_array());
    assert!(body.as_array().map(|items| items.len() <= 20).unwrap_or(false));
}

#[tokio::test]
async fn test_metadata_search_returns_404_for_unknown_book() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/books/nonexistent-id/metadata/search")
        .add_query_param("q", "anything")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}
