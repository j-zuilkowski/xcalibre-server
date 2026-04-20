#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::{
    config::AppConfig,
    db::queries::llm as llm_queries,
    llm::{embeddings::EmbeddingClient, job_runner},
    search::semantic::SemanticSearch,
};
use chrono::Utc;
use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;
use wiremock::{
    matchers::{body_partial_json, method, path},
    Mock, MockServer, ResponseTemplate,
};

#[tokio::test]
async fn test_semantic_index_stores_embedding() {
    let embed_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(body_partial_json(
            serde_json::json!({ "model": "test-embedding-model" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                { "embedding": [0.25, 0.5, 0.75] }
            ]
        })))
        .mount(&embed_server)
        .await;

    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.librarian.endpoint = embed_server.uri();
    config.llm.librarian.model = "test-embedding-model".to_string();

    let ctx = TestContext::new_with_config(config.clone()).await;
    let book = ctx.create_book("Rust Semantic Guide", "Ada Lovelace").await;
    let job_id = insert_semantic_job(&ctx.db, &book.id, "running").await;

    let client = EmbeddingClient::new(&config).expect("embedding client");
    let semantic = SemanticSearch::new(ctx.db.clone(), client);
    semantic
        .index_book(
            &book.id,
            &book.title,
            "Ada Lovelace",
            "A systems programming guide.",
        )
        .await
        .expect("index book");

    let embedding_row = sqlx::query(
        "SELECT model_id, length(embedding) AS embedding_bytes FROM book_embeddings WHERE book_id = ?",
    )
    .bind(&book.id)
    .fetch_one(&ctx.db)
    .await
    .expect("select embedding row");

    let model_id: String = embedding_row.get("model_id");
    let embedding_bytes: i64 = embedding_row.get("embedding_bytes");
    assert_eq!(model_id, "test-embedding-model");
    assert_eq!(embedding_bytes, 3 * std::mem::size_of::<f32>() as i64);

    let job_row = sqlx::query("SELECT status, error_text FROM llm_jobs WHERE id = ?")
        .bind(&job_id)
        .fetch_one(&ctx.db)
        .await
        .expect("select llm job");
    let status: String = job_row.get("status");
    let error_text: Option<String> = job_row.get("error_text");
    assert_eq!(status, "completed");
    assert!(error_text.is_none());
}

#[tokio::test]
async fn test_semantic_search_returns_ranked_results() {
    let embed_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .and(body_partial_json(
            serde_json::json!({ "model": "test-embedding-model" }),
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                { "embedding": [1.0, 0.0, 0.0] }
            ]
        })))
        .mount(&embed_server)
        .await;

    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.librarian.endpoint = embed_server.uri();
    config.llm.librarian.model = "test-embedding-model".to_string();

    let ctx = TestContext::new_with_config(config).await;
    let token = ctx.admin_token().await;

    let first = ctx.create_book("Rust in Production", "Ferris").await;
    let second = ctx.create_book("Bread Baking Basics", "Baker").await;

    insert_embedding(&ctx.db, &first.id, "test-embedding-model", &[1.0, 0.0, 0.0]).await;
    insert_embedding(
        &ctx.db,
        &second.id,
        "test-embedding-model",
        &[0.0, 1.0, 0.0],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/semantic")
        .add_query_param("q", "systems language")
        .add_query_param("page", 1)
        .add_query_param("page_size", 24)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 2);
    assert_eq!(body["items"][0]["id"], first.id);

    let first_score = body["items"][0]["score"].as_f64().unwrap_or_default();
    let second_score = body["items"][1]["score"].as_f64().unwrap_or_default();
    assert!(first_score >= second_score);
}

#[tokio::test]
async fn test_semantic_search_disabled_returns_503() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/search/semantic")
        .add_query_param("q", "anything")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 503);
    let body: serde_json::Value = response.json();
    assert_eq!(body["error"], "llm_unavailable");
}

#[tokio::test]
async fn test_job_runner_processes_pending_jobs() {
    let embed_server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/embeddings"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "data": [
                { "embedding": [0.9, 0.1, 0.3] }
            ]
        })))
        .expect(1)
        .mount(&embed_server)
        .await;

    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.librarian.endpoint = embed_server.uri();
    config.llm.librarian.model = "test-embedding-model".to_string();

    let ctx = TestContext::new_with_config(config).await;
    let book = ctx.create_book("Concurrent Systems", "Grace").await;
    let inserted = llm_queries::enqueue_semantic_index_job(&ctx.db, &book.id)
        .await
        .expect("enqueue semantic job");
    assert!(inserted);

    let processed = job_runner::process_pending_jobs_once(&ctx.state)
        .await
        .expect("run semantic job runner once");
    assert_eq!(processed, 1);

    let row = sqlx::query(
        "SELECT status, error_text FROM llm_jobs WHERE job_type = 'semantic_index' AND book_id = ?",
    )
    .bind(&book.id)
    .fetch_one(&ctx.db)
    .await
    .expect("select llm job");
    let status: String = row.get("status");
    let error_text: Option<String> = row.get("error_text");
    assert_eq!(status, "completed");
    assert!(error_text.is_none());

    let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM book_embeddings WHERE book_id = ?")
        .bind(&book.id)
        .fetch_one(&ctx.db)
        .await
        .expect("count embeddings");
    assert_eq!(count, 1);
}

async fn insert_embedding(db: &sqlx::SqlitePool, book_id: &str, model_id: &str, vector: &[f32]) {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO book_embeddings (book_id, model_id, embedding, created_at)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(book_id)
    .bind(model_id)
    .bind(vector_to_blob(vector))
    .bind(now)
    .execute(db)
    .await
    .expect("insert embedding");
}

async fn insert_semantic_job(db: &sqlx::SqlitePool, book_id: &str, status: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let started_at = if status == "running" {
        Some(now.clone())
    } else {
        None
    };

    sqlx::query(
        r#"
        INSERT INTO llm_jobs (
            id, job_type, status, book_id, payload_json, result_json, error_text, created_at, started_at, completed_at
        )
        VALUES (?, 'semantic_index', ?, ?, NULL, NULL, NULL, ?, ?, NULL)
        "#,
    )
    .bind(&id)
    .bind(status)
    .bind(book_id)
    .bind(now)
    .bind(started_at)
    .execute(db)
    .await
    .expect("insert semantic llm job");

    id
}

fn vector_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(std::mem::size_of_val(vector));
    for value in vector {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    blob
}
