#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};
use sqlx::Row;

#[tokio::test]
async fn test_mark_book_as_read() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Read Me", "Author A").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/read", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "is_read": true }))
        .await;

    assert_status!(response, 204);

    let body: serde_json::Value = ctx
        .server
        .get(&format!("/api/v1/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await
        .json();
    assert!(body["is_read"].as_bool().unwrap_or(false));
    assert!(!body["is_archived"].as_bool().unwrap_or(true));
}

#[tokio::test]
async fn test_toggle_read_false() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Unread Me", "Author A").await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("user")
        .fetch_one(&ctx.db)
        .await
        .expect("load user id");

    let mark_read = ctx
        .server
        .post(&format!("/api/v1/books/{}/read", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "is_read": true }))
        .await;
    assert_status!(mark_read, 204);

    let mark_unread = ctx
        .server
        .post(&format!("/api/v1/books/{}/read", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "is_read": false }))
        .await;
    assert_status!(mark_unread, 204);

    let row = sqlx::query(
        "SELECT is_read, is_archived FROM book_user_state WHERE user_id = ? AND book_id = ?",
    )
    .bind(&user_id)
    .bind(&book.id)
    .fetch_one(&ctx.db)
    .await
    .expect("load state");
    assert_eq!(row.get::<i64, _>("is_read"), 0);
    assert_eq!(row.get::<i64, _>("is_archived"), 0);
}

#[tokio::test]
async fn test_archive_book_hidden_from_list() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Archive Me", "Author A").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/archive", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "is_archived": true }))
        .await;
    assert_status!(response, 204);

    let list = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(list, 200);
    let body: serde_json::Value = list.json();
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn test_show_archived_filter() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Show Me", "Author A").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/archive", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "is_archived": true }))
        .await;
    assert_status!(response, 204);

    let list = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("show_archived", true)
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(list, 200);
    let body: serde_json::Value = list.json();
    assert_eq!(body["total"], 1);
    assert!(body["items"][0]["is_archived"].as_bool().unwrap_or(false));
}

#[tokio::test]
async fn test_state_is_per_user_not_global() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;
    let user_token = ctx.user_token().await;
    let book = ctx.create_book("Shared State", "Author A").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/read", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&admin_token))
        .json(&serde_json::json!({ "is_read": true }))
        .await;
    assert_status!(response, 204);

    let user_view: serde_json::Value = ctx
        .server
        .get(&format!("/api/v1/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .await
        .json();
    assert!(!user_view["is_read"].as_bool().unwrap_or(true));
    assert!(!user_view["is_archived"].as_bool().unwrap_or(true));

    let list = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .await;
    assert_status!(list, 200);
    let body: serde_json::Value = list.json();
    assert_eq!(body["total"], 1);
}
