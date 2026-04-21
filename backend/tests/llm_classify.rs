#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::config::AppConfig;
use common::{auth_header, TestContext};
use sqlx::Row;
use wiremock::{
    matchers::{body_partial_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

const TAG_NAME: &str = "Science Fiction";

#[tokio::test]
async fn test_classify_inserts_pending_tags() {
    let (ctx, token, book, _mock_server) = setup_classify_context().await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/classify", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["book_id"], book.id);
    assert_eq!(body["suggestions"][0]["name"], TAG_NAME);
    assert_eq!(body["pending_count"], 1);

    let row = sqlx::query(
        r#"
        SELECT bt.confirmed
        FROM book_tags bt
        INNER JOIN tags t ON t.id = bt.tag_id
        WHERE bt.book_id = ? AND t.name = ?
        "#,
    )
    .bind(&book.id)
    .bind(TAG_NAME)
    .fetch_one(&ctx.db)
    .await
    .expect("select pending tag row");

    let confirmed: i64 = row.get("confirmed");
    assert_eq!(confirmed, 0);
}

#[tokio::test]
async fn test_confirm_tags_marks_confirmed() {
    let (ctx, token, book, _mock_server) = setup_classify_context().await;
    classify_book_once(&ctx, &token, &book.id).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/tags/confirm", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "confirm": [TAG_NAME],
            "reject": []
        }))
        .await;

    assert_status!(response, 200);

    let row = sqlx::query(
        r#"
        SELECT bt.confirmed
        FROM book_tags bt
        INNER JOIN tags t ON t.id = bt.tag_id
        WHERE bt.book_id = ? AND t.name = ?
        "#,
    )
    .bind(&book.id)
    .bind(TAG_NAME)
    .fetch_one(&ctx.db)
    .await
    .expect("select confirmed tag row");

    let confirmed: i64 = row.get("confirmed");
    assert_eq!(confirmed, 1);
}

#[tokio::test]
async fn test_reject_tags_removes_row() {
    let (ctx, token, book, _mock_server) = setup_classify_context().await;
    classify_book_once(&ctx, &token, &book.id).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/tags/confirm", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "confirm": [],
            "reject": [TAG_NAME]
        }))
        .await;

    assert_status!(response, 200);

    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(1)
        FROM book_tags bt
        INNER JOIN tags t ON t.id = bt.tag_id
        WHERE bt.book_id = ? AND t.name = ?
        "#,
    )
    .bind(&book.id)
    .bind(TAG_NAME)
    .fetch_one(&ctx.db)
    .await
    .expect("count tag rows");

    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_classify_returns_503_when_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("No LLM Config", "Test Author").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/classify", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 503);
    let body: serde_json::Value = response.json();
    assert_eq!(body["error"], "llm_unavailable");
}

async fn setup_classify_context() -> (TestContext, String, backend::db::models::Book, MockServer) {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .and(body_partial_json(
            serde_json::json!({ "model": "test-chat-model" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"tags\":[{\"name\":\"Science Fiction\",\"confidence\":0.92}]}"
                }
            }]
        })))
        .expect(1)
        .mount(&mock_server)
        .await;

    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.librarian.endpoint = mock_server.uri();
    config.llm.librarian.model = "test-chat-model".to_string();
    config.llm.librarian.system_prompt = "You classify books into tags.".to_string();

    let ctx = TestContext::new_with_config(config).await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Dune", "Frank Herbert").await;

    (ctx, token, book, mock_server)
}

async fn classify_book_once(ctx: &TestContext, token: &str, book_id: &str) {
    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/classify"))
        .add_header(header::AUTHORIZATION, auth_header(token))
        .await;
    assert_status!(response, 200);
}
