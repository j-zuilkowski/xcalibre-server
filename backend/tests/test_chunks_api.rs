#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::{ingest::chunker::ChunkType, AppConfig};
use chrono::Utc;
use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;

#[tokio::test]
async fn test_get_chunks_returns_stored_chunks() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Stored Chunks", "Ada").await;
    insert_chunk(&ctx.db, &book.id, 0, ChunkType::Text, "stored text", false).await;
    insert_chunk(&ctx.db, &book.id, 1, ChunkType::Procedure, "stored steps", false).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/chunks", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["book_id"], book.id);
    assert_eq!(body["chunk_count"], 2);
    assert_eq!(body["chunks"].as_array().unwrap().len(), 2);
}

#[tokio::test]
async fn test_get_chunks_triggers_chunking_on_empty() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, path) = ctx.create_book_with_file("Generated Chunks", "TXT").await;
    std::fs::write(&path, long_text(180)).expect("write text fixture");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/chunks", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body["chunk_count"].as_u64().unwrap_or_default() > 0);
    assert!(body["chunks"].as_array().unwrap().len() > 0);
}

#[tokio::test]
async fn test_get_chunks_filter_by_type() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Filtered Chunks", "Ada").await;
    insert_chunk(&ctx.db, &book.id, 0, ChunkType::Text, "one", false).await;
    insert_chunk(&ctx.db, &book.id, 1, ChunkType::Procedure, "two", false).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/chunks", book.id))
        .add_query_param("type", "procedure")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["chunk_count"], 1);
    assert_eq!(body["chunks"][0]["chunk_type"], "procedure");
}

#[tokio::test]
async fn test_get_chunks_respects_size_param() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, path) = ctx.create_book_with_file("Sized Chunks", "TXT").await;
    std::fs::write(&path, long_text(130)).expect("write text fixture");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/chunks", book.id))
        .add_query_param("size", 50)
        .add_query_param("overlap", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let chunks = body["chunks"].as_array().unwrap();
    assert!(chunks.len() >= 3);
    assert!(chunks
        .iter()
        .all(|chunk| chunk["word_count"].as_u64().unwrap_or_default() <= 50));
}

#[tokio::test]
async fn test_vision_pass_appends_to_ocr_text() {
    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.librarian.endpoint = "mock://vision".to_string();
    config.llm.librarian.model = "vision-model".to_string();

    let ctx = TestContext::new_with_config(config).await;
    let token = ctx.admin_token().await;
    let (book, path) = ctx.create_book_with_file("Vision Chunks", "PDF").await;
    std::fs::write(&path, fake_pdf_page("diagram labels only")).expect("write pdf fixture");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/chunks", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let text = body["chunks"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("diagram labels only"));
    assert!(text.contains("[Visual content description:]"));
    assert!(text.contains("diagram description"));
}

#[tokio::test]
async fn test_vision_pass_falls_back_gracefully_on_llm_disabled() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, path) = ctx.create_book_with_file("No Vision", "PDF").await;
    std::fs::write(&path, fake_pdf_page("diagram labels only")).expect("write pdf fixture");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/chunks", book.id))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let text = body["chunks"][0]["text"].as_str().unwrap_or_default();
    assert!(text.contains("diagram labels only"));
    assert!(!text.contains("[Visual content description:]"));
}

async fn insert_chunk(
    db: &sqlx::SqlitePool,
    book_id: &str,
    chunk_index: i64,
    chunk_type: ChunkType,
    text: &str,
    has_image: bool,
) {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO book_chunks (
            id, book_id, chunk_index, chapter_index, heading_path, chunk_type,
            text, word_count, has_image, embedding, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(book_id)
    .bind(chunk_index)
    .bind(0_i64)
    .bind(None::<String>)
    .bind(chunk_type.as_str())
    .bind(text)
    .bind(text.split_whitespace().count() as i64)
    .bind(i64::from(has_image))
    .bind(now)
    .execute(db)
    .await
    .expect("insert chunk");
}

fn long_text(word_count: usize) -> String {
    (0..word_count)
        .map(|index| format!("word{index}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn fake_pdf_page(text: &str) -> Vec<u8> {
    format!(
        "%PDF-1.4\n1 0 obj\n<< /Type /Page >>\nendobj\nBT\n({text}) Tj\nET\n"
    )
    .into_bytes()
}
