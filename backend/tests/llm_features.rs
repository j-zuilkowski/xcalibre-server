#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use axum_test::multipart::{MultipartForm, Part};
use backend::{config::AppConfig, db::queries::llm as llm_queries, llm::job_runner};
use common::{auth_header, minimal_epub_bytes, minimal_mobi_bytes, TestContext};
use sqlx::Row;
use std::time::Duration;
use wiremock::{
    matchers::{body_partial_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

const TAG_NAME: &str = "Science Fiction";

#[tokio::test]
async fn test_validate_returns_issues() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"severity\":\"warning\",\"issues\":[{\"field\":\"description\",\"severity\":\"warning\",\"message\":\"Description is too short\",\"suggestion\":\"Add a fuller synopsis\"}]}"
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let ctx = TestContext::new_with_config(llm_config(&mock_server)).await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Validation Book", "Author").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/validate", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["book_id"], book.id);
    assert_eq!(body["severity"], "warning");
    assert!(body["issues"]
        .as_array()
        .is_some_and(|issues| !issues.is_empty()));
}

#[tokio::test]
async fn test_derive_returns_content() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": "{\"summary\":\"A compelling exploration of distributed systems.\",\"related_titles\":[\"Designing Data-Intensive Applications\",\"Release It!\",\"The Pragmatic Programmer\"],\"discussion_questions\":[\"What failure modes are most dangerous?\",\"How should teams balance reliability and speed?\",\"Which trade-offs are acceptable at startup scale?\"]}"
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let ctx = TestContext::new_with_config(llm_config(&mock_server)).await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Derive Book", "Author").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/derive", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["book_id"], book.id);
    assert!(body["summary"]
        .as_str()
        .is_some_and(|summary| !summary.is_empty()));
}

#[tokio::test]
async fn test_organize_enqueues_job() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/organize")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 202);
    let body: serde_json::Value = response.json();
    let job_id = body["job_id"].as_str().expect("job_id");

    let row = sqlx::query("SELECT job_type, status FROM llm_jobs WHERE id = ?")
        .bind(job_id)
        .fetch_one(&ctx.db)
        .await
        .expect("select organize job");
    let job_type: String = row.get("job_type");
    let status: String = row.get("status");
    assert_eq!(job_type, "organize");
    assert_eq!(status, "pending");
}

#[tokio::test]
async fn test_organize_idempotent() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let first = ctx
        .server
        .post("/api/v1/organize")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    let second = ctx
        .server
        .post("/api/v1/organize")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(first, 202);
    assert_status!(second, 202);

    let pending_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(1) FROM llm_jobs WHERE job_type = 'organize' AND status = 'pending'",
    )
    .fetch_one(&ctx.db)
    .await
    .expect("count organize jobs");
    assert_eq!(pending_count, 1);
}

#[tokio::test]
async fn test_classify_job_runner() {
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
        .mount(&mock_server)
        .await;

    let ctx = TestContext::new_with_config(llm_config(&mock_server)).await;
    let book = ctx.create_book("Job Runner Book", "Author").await;
    let enqueued = llm_queries::enqueue_classify_job(&ctx.db, &book.id)
        .await
        .expect("enqueue classify job");
    assert!(enqueued);

    let processed = job_runner::process_pending_jobs_once(&ctx.state)
        .await
        .expect("process llm jobs");
    assert_eq!(processed, 1);

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
    .expect("count classified tags");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_list_chapters_epub() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_epub(&ctx, &token).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/chapters"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let chapters = body["chapters"].as_array().expect("chapters array");
    assert!(!chapters.is_empty());
    assert!(chapters[0]["title"]
        .as_str()
        .is_some_and(|title| !title.is_empty()));
    assert!(chapters[0]["word_count"].as_u64().unwrap_or_default() > 0);
}

#[tokio::test]
async fn test_get_text_full_epub() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_epub(&ctx, &token).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/text"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let text = body["text"].as_str().unwrap_or_default();
    assert!(!text.is_empty());
    let expected_count = text.split_whitespace().count();
    assert_eq!(
        body["word_count"].as_u64().unwrap_or_default() as usize,
        expected_count
    );
}

#[tokio::test]
async fn test_get_text_single_chapter() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_epub(&ctx, &token).await;

    let full = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/text"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(full, 200);
    let full_body: serde_json::Value = full.json();
    let full_text = full_body["text"].as_str().unwrap_or_default().to_string();

    let single = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/text"))
        .add_query_param("chapter", 0)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(single, 200);
    let single_body: serde_json::Value = single.json();
    let single_text = single_body["text"].as_str().unwrap_or_default();
    assert_eq!(single_body["chapter"], 0);
    assert!(single_text.len() <= full_text.len());
}

#[tokio::test]
async fn test_text_works_when_llm_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_epub(&ctx, &token).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/text"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
}

#[tokio::test]
async fn test_chapters_returns_422_no_extractable_format() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book_id = upload_mobi(&ctx, &token).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{book_id}/chapters"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 422);
    let body: serde_json::Value = response.json();
    assert_eq!(body["error"], "no_extractable_format");
}

#[tokio::test]
async fn test_upload_sets_document_type() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "choices": [{
                "message": {
                    "content": "textbook"
                }
            }]
        })))
        .mount(&mock_server)
        .await;

    let ctx = TestContext::new_with_config(llm_config(&mock_server)).await;
    let token = ctx.admin_token().await;

    let response = upload_epub_response(&ctx, &token).await;
    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["document_type"], "textbook");
}

#[tokio::test]
async fn test_upload_document_type_defaults_unknown_when_llm_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = upload_epub_response(&ctx, &token).await;
    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["document_type"], "unknown");
}

#[tokio::test]
async fn test_upload_document_type_defaults_unknown_on_llm_timeout() {
    let mock_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(11))
                .set_body_json(serde_json::json!({
                    "choices": [{
                        "message": {
                            "content": "textbook"
                        }
                    }]
                })),
        )
        .mount(&mock_server)
        .await;

    let ctx = TestContext::new_with_config(llm_config(&mock_server)).await;
    let token = ctx.admin_token().await;

    let response = upload_epub_response(&ctx, &token).await;
    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["document_type"], "unknown");
}

fn llm_config(mock_server: &MockServer) -> AppConfig {
    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.librarian.endpoint = mock_server.uri();
    config.llm.librarian.model = "test-chat-model".to_string();
    config.llm.librarian.system_prompt = "You are a strict JSON API.".to_string();
    config
}

async fn upload_epub(ctx: &TestContext, token: &str) -> String {
    let response = upload_epub_response(ctx, token).await;
    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    body["id"].as_str().expect("book id").to_string()
}

async fn upload_mobi(ctx: &TestContext, token: &str) -> String {
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(minimal_mobi_bytes())
            .file_name("minimal.mobi")
            .mime_type("application/x-mobipocket-ebook"),
    );

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .multipart(form)
        .await;
    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    body["id"].as_str().expect("book id").to_string()
}

async fn upload_epub_response(ctx: &TestContext, token: &str) -> axum_test::TestResponse {
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(minimal_epub_bytes())
            .file_name("minimal.epub")
            .mime_type("application/epub+zip"),
    );

    ctx.server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .multipart(form)
        .await
}
