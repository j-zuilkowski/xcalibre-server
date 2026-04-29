#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::{
    config::AppConfig,
    db::queries::memory_chunks as memory_queries,
    ingest::chunker::ChunkType,
    llm::embeddings::EmbeddingClient,
};
use common::{auth_header, TestContext};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

#[tokio::test]
async fn test_memory_ingest_returns_201_with_id() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": "Test memory chunk"
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert!(body["id"].as_str().is_some_and(|id| !id.is_empty()));
    assert!(body["created_at"].as_i64().is_some());
}

#[tokio::test]
async fn test_memory_ingest_stores_chunk_in_db() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let text = "Memory chunk stored in the database";

    let response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": text,
            "session_id": "session-123",
            "chunk_type": "factual"
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    let id = body["id"].as_str().expect("memory id").to_string();

    let row = sqlx::query(
        "SELECT text, chunk_type, session_id FROM memory_chunks WHERE id = ?",
    )
    .bind(&id)
    .fetch_one(&ctx.db)
    .await
    .expect("select memory chunk");

    assert_eq!(row.try_get::<String, _>("text").unwrap(), text);
    assert_eq!(row.try_get::<String, _>("chunk_type").unwrap(), "factual");
    assert_eq!(
        row.try_get::<Option<String>, _>("session_id").unwrap(),
        Some("session-123".to_string())
    );
}

#[tokio::test]
async fn test_memory_ingest_without_llm_stores_null_embedding() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": "Embedding fallback path"
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    let id = body["id"].as_str().expect("memory id").to_string();

    let embedding: Option<Vec<u8>> =
        sqlx::query_scalar("SELECT embedding FROM memory_chunks WHERE id = ?")
            .bind(&id)
            .fetch_one(&ctx.db)
            .await
            .expect("select memory embedding");

    assert!(embedding.is_none());
}

#[tokio::test]
async fn test_memory_ingest_validates_empty_text() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": ""
        }))
        .await;

    assert_status!(response, 422);
}

#[tokio::test]
async fn test_memory_ingest_validates_chunk_type() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": "Some memory",
            "chunk_type": "invalid"
        }))
        .await;

    assert_status!(response, 422);
}

#[tokio::test]
async fn test_memory_ingest_validates_text_length() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": "x".repeat(33_000)
        }))
        .await;

    assert_status!(response, 422);
}

#[tokio::test]
async fn test_memory_delete_returns_204() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": "Delete me later"
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    let id = body["id"].as_str().expect("memory id").to_string();

    let delete_path = format!("/api/v1/memory/{id}");
    let delete = ctx
        .server
        .delete(&delete_path)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(delete, 204);

    let row_count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM memory_chunks WHERE id = ?")
            .bind(&id)
            .fetch_one(&ctx.db)
            .await
            .expect("count deleted memory chunk");
    assert_eq!(row_count, 0);
}

#[tokio::test]
async fn test_memory_delete_nonexistent_returns_404() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .delete("/api/v1/memory/nonexistent-id")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_memory_requires_auth() {
    let ctx = TestContext::new().await;

    let post_response = ctx
        .server
        .post("/api/v1/memory")
        .json(&json!({
            "text": "Unauthenticated memory write"
        }))
        .await;
    assert_status!(post_response, 401);

    let delete_response = ctx.server.delete("/api/v1/memory/any-id").await;
    assert_status!(delete_response, 401);
}

#[tokio::test]
async fn test_search_chunks_source_memory_returns_memory_chunks() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let text = "dragon scales in mythology";

    let response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": text,
            "chunk_type": "factual"
        }))
        .await;
    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    let memory_id = body["id"].as_str().expect("memory id").to_string();

    let search = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "dragon scales")
        .add_query_param("source", "memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(search, 200);
    let body: serde_json::Value = search.json();
    let chunks = body["chunks"].as_array().expect("chunks array");

    assert!(
        chunks
            .iter()
            .any(|chunk| chunk["chunk_id"] == memory_id && chunk["source"] == "memory"),
        "memory chunk should be returned by memory-only search"
    );
}

#[tokio::test]
async fn test_search_chunks_source_books_excludes_memory() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Book Search Scope", "Author").await;
    let search_text = "unique_memory_phrase_xyz";

    let memory_response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": search_text
        }))
        .await;
    assert_status!(memory_response, 201);
    let memory_body: serde_json::Value = memory_response.json();
    let memory_id = memory_body["id"].as_str().expect("memory id").to_string();

    insert_book_chunk(&ctx.db, &book.id, search_text).await;

    let search = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", search_text)
        .add_query_param("source", "books")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(search, 200);
    let body: serde_json::Value = search.json();
    let chunks = body["chunks"].as_array().expect("chunks array");

    assert!(
        !chunks.iter().any(|chunk| chunk["chunk_id"] == memory_id),
        "books-only search must exclude memory chunks"
    );
    assert!(
        chunks.iter().all(|chunk| chunk["source"] == "books"),
        "books-only search must only return book chunks"
    );
}

#[tokio::test]
async fn test_search_chunks_source_all_returns_both() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Combined Search Book", "Author").await;
    let search_text = "combined_search_term_abc";

    let memory_response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": search_text
        }))
        .await;
    assert_status!(memory_response, 201);
    let memory_body: serde_json::Value = memory_response.json();
    let memory_id = memory_body["id"].as_str().expect("memory id").to_string();

    let book_id = insert_book_chunk(&ctx.db, &book.id, search_text).await;

    let search = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", search_text)
        .add_query_param("source", "all")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(search, 200);
    let body: serde_json::Value = search.json();
    let chunks = body["chunks"].as_array().expect("chunks array");

    assert!(
        chunks.iter().any(|chunk| chunk["chunk_id"] == memory_id && chunk["source"] == "memory"),
        "combined search should include memory results"
    );
    assert!(
        chunks.iter().any(|chunk| chunk["chunk_id"] == book_id && chunk["source"] == "books"),
        "combined search should include book results"
    );
}

#[tokio::test]
async fn test_search_chunks_invalid_source_returns_422() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "test")
        .add_query_param("source", "invalid")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 422);
}

#[tokio::test]
async fn test_search_chunks_project_path_filter() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let search_text = "project scoped memory token";

    let alpha_response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": search_text,
            "project_path": "/project/alpha"
        }))
        .await;
    assert_status!(alpha_response, 201);
    let alpha_body: serde_json::Value = alpha_response.json();
    let alpha_id = alpha_body["id"].as_str().expect("alpha id").to_string();

    let beta_response = ctx
        .server
        .post("/api/v1/memory")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "text": search_text,
            "project_path": "/project/beta"
        }))
        .await;
    assert_status!(beta_response, 201);
    let beta_body: serde_json::Value = beta_response.json();
    let beta_id = beta_body["id"].as_str().expect("beta id").to_string();

    let search = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", search_text)
        .add_query_param("source", "memory")
        .add_query_param("project_path", "/project/alpha")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(search, 200);
    let body: serde_json::Value = search.json();
    let chunk_ids = body["chunks"]
        .as_array()
        .expect("chunks array")
        .iter()
        .filter_map(|chunk| chunk["chunk_id"].as_str().map(ToString::to_string))
        .collect::<Vec<_>>();

    assert!(chunk_ids.contains(&alpha_id));
    assert!(!chunk_ids.contains(&beta_id));
}

#[tokio::test]
async fn test_embedding_model_config_field_exists() {
    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.embedding_model = Some("nomic-embed-text-v1.5".to_string());
    config.llm.librarian.model = "phi-3-mini".to_string();

    let client = EmbeddingClient::new(&config).expect("build embedding client");
    assert_eq!(client.model_id(), "nomic-embed-text-v1.5");

    let mut fallback_config = AppConfig::default();
    fallback_config.llm.enabled = true;
    fallback_config.llm.embedding_model = None;
    fallback_config.llm.librarian.model = "phi-3-mini".to_string();

    let fallback_client = EmbeddingClient::new(&fallback_config).expect("build fallback client");
    assert_eq!(fallback_client.model_id(), "phi-3-mini");
}

async fn insert_book_chunk(db: &sqlx::SqlitePool, book_id: &str, text: &str) -> String {
    let chunk_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO book_chunks (
            id, book_id, chunk_index, chapter_index, heading_path, chunk_type,
            text, word_count, has_image, embedding, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&chunk_id)
    .bind(book_id)
    .bind(0_i64)
    .bind(0_i64)
    .bind(None::<String>)
    .bind(ChunkType::Reference.as_str())
    .bind(text)
    .bind(text.split_whitespace().count() as i64)
    .bind(0_i64)
    .bind(None::<Vec<u8>>)
    .bind(now)
    .execute(db)
    .await
    .expect("insert book chunk");

    chunk_id
}
