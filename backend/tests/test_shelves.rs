#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};

#[tokio::test]
async fn test_create_shelf() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "name": "Favorites",
            "is_public": true
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["name"], "Favorites");
    assert_eq!(body["is_public"], true);
}

#[tokio::test]
async fn test_add_book_to_shelf() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Shelf Book", "Author").await;

    let shelf = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "name": "Reading",
            "is_public": false
        }))
        .await;
    assert_status!(shelf, 201);
    let shelf_body: serde_json::Value = shelf.json();
    let shelf_id = shelf_body["id"].as_str().expect("shelf id");

    let response = ctx
        .server
        .post(&format!("/api/v1/shelves/{shelf_id}/books"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "book_id": book.id
        }))
        .await;

    assert_status!(response, 204);
}

#[tokio::test]
async fn test_remove_book_from_shelf() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Shelf Book", "Author").await;

    let shelf = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "name": "Reading",
            "is_public": false
        }))
        .await;
    let shelf_body: serde_json::Value = shelf.json();
    let shelf_id = shelf_body["id"].as_str().expect("shelf id");

    let added = ctx
        .server
        .post(&format!("/api/v1/shelves/{shelf_id}/books"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "book_id": book.id
        }))
        .await;
    assert_status!(added, 204);

    let response = ctx
        .server
        .delete(&format!("/api/v1/shelves/{shelf_id}/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 204);
}

#[tokio::test]
async fn test_list_shelf_books() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book_one = ctx.create_book("Shelf Book One", "Author").await;
    let book_two = ctx.create_book("Shelf Book Two", "Author").await;

    let shelf = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "name": "Reading",
            "is_public": false
        }))
        .await;
    let shelf_body: serde_json::Value = shelf.json();
    let shelf_id = shelf_body["id"].as_str().expect("shelf id");

    let added_one = ctx
        .server
        .post(&format!("/api/v1/shelves/{shelf_id}/books"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "book_id": book_one.id }))
        .await;
    assert_status!(added_one, 204);

    let added_two = ctx
        .server
        .post(&format!("/api/v1/shelves/{shelf_id}/books"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "book_id": book_two.id }))
        .await;
    assert_status!(added_two, 204);

    let response = ctx
        .server
        .get(&format!("/api/v1/shelves/{shelf_id}/books"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 2);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(2));
}
