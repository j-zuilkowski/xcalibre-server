#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};
use sqlx::Row;
use std::time::Duration;
use tokio::time::sleep;

async fn wait_for_history_count(ctx: &TestContext, user_id: &str, expected: i64) {
    for _ in 0..50 {
        let count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM download_history WHERE user_id = ?")
                .bind(user_id)
                .fetch_one(&ctx.db)
                .await
                .expect("count download history");
        if count == expected {
            return;
        }
        sleep(Duration::from_millis(20)).await;
    }

    panic!("download history count did not reach {expected}");
}

#[tokio::test]
async fn test_download_records_history_entry() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("user")
        .fetch_one(&ctx.db)
        .await
        .expect("load user id");
    let book = ctx.create_book_with_file("Download Me", "EPUB").await.0;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/formats/EPUB/download", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(response, 200);

    wait_for_history_count(&ctx, &user_id, 1).await;

    let row = sqlx::query("SELECT book_id, format FROM download_history WHERE user_id = ? LIMIT 1")
        .bind(&user_id)
        .fetch_one(&ctx.db)
        .await
        .expect("load history row");
    assert_eq!(row.get::<String, _>("book_id"), book.id);
    assert_eq!(row.get::<String, _>("format"), "EPUB");
}

#[tokio::test]
async fn test_download_history_is_per_user() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;
    let user_token = ctx.user_token().await;
    let admin_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("admin")
        .fetch_one(&ctx.db)
        .await
        .expect("load admin id");
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("user")
        .fetch_one(&ctx.db)
        .await
        .expect("load user id");
    let book = ctx
        .create_book_with_file("Private Download", "EPUB")
        .await
        .0;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/formats/EPUB/download", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&admin_token))
        .await;
    assert_status!(response, 200);

    wait_for_history_count(&ctx, &admin_id, 1).await;

    let admin_history = ctx
        .server
        .get("/api/v1/books/downloads")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&admin_token))
        .await;
    assert_status!(admin_history, 200);
    let admin_body: serde_json::Value = admin_history.json();
    assert_eq!(admin_body["total"], 1);

    let user_history = ctx
        .server
        .get("/api/v1/books/downloads")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .await;
    assert_status!(user_history, 200);
    let user_body: serde_json::Value = user_history.json();
    assert_eq!(user_body["total"], 0);

    let user_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM download_history WHERE user_id = ?")
            .bind(&user_id)
            .fetch_one(&ctx.db)
            .await
            .expect("count user history");
    assert_eq!(user_count, 0);
}

#[tokio::test]
async fn test_download_history_pagination() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("user")
        .fetch_one(&ctx.db)
        .await
        .expect("load user id");
    let first = ctx.create_book_with_file("Alpha", "EPUB").await.0;
    sleep(Duration::from_millis(5)).await;
    let second = ctx.create_book_with_file("Beta", "EPUB").await.0;
    sleep(Duration::from_millis(5)).await;
    let third = ctx.create_book_with_file("Gamma", "EPUB").await.0;

    for book in [&first, &second, &third] {
        let response = ctx
            .server
            .get(&format!("/api/v1/books/{}/formats/EPUB/download", book.id))
            .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
            .await;
        assert_status!(response, 200);
        sleep(Duration::from_millis(20)).await;
    }

    wait_for_history_count(&ctx, &user_id, 3).await;

    let first_page = ctx
        .server
        .get("/api/v1/books/downloads")
        .add_query_param("page", 1)
        .add_query_param("page_size", 2)
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(first_page, 200);
    let first_body: serde_json::Value = first_page.json();
    assert_eq!(first_body["total"], 3);
    assert_eq!(first_body["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(first_body["items"][0]["title"], "Gamma");
    assert_eq!(first_body["items"][1]["title"], "Beta");

    let second_page = ctx
        .server
        .get("/api/v1/books/downloads")
        .add_query_param("page", 2)
        .add_query_param("page_size", 2)
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(second_page, 200);
    let second_body: serde_json::Value = second_page.json();
    assert_eq!(second_body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(second_body["items"][0]["title"], "Alpha");
}
