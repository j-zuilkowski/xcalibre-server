#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::{
    config::AppConfig, db::queries::collections as collection_queries, ingest::chunker::ChunkType,
};
use common::{auth_header, TestContext};
use std::collections::HashSet;
use uuid::Uuid;

#[tokio::test]
async fn test_bm25_finds_exact_technical_token() {
    let ctx = TestContext::new_with_config(mock_llm_config("mock://bm25")).await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Oracle Errors", "Admin").await;
    insert_chunk(
        &ctx.db,
        &book.id,
        0,
        None,
        ChunkType::Reference,
        "The ORA-01555 error indicates an undo retention problem.",
        [0.9, 0.1, 0.0],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "ORA-01555")
        .add_query_param("limit", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(
        body["chunks"][0]["chunk_id"].as_str().unwrap(),
        chunk_id(&ctx.db).await
    );
    assert!(body["chunks"][0]["bm25_score"].is_number());
}

#[tokio::test]
async fn test_chunk_search_clamps_limit_to_100() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Chunk Clamp", "Admin").await;

    insert_chunk_series(
        &ctx.db,
        &book.id,
        120,
        "global clamp search chunk",
        ChunkType::Reference,
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "global clamp")
        .add_query_param("limit", 99999)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body["chunks"].as_array().unwrap().len() <= 100);
}

#[tokio::test]
async fn test_semantic_finds_conceptual_match() {
    let ctx = TestContext::new_with_config(mock_llm_config("mock://semantic")).await;
    let token = ctx.admin_token().await;
    let target = ctx.create_book("Recovery Guide", "Admin").await;
    let distractor = ctx.create_book("Unrelated Notes", "Admin").await;

    insert_chunk(
        &ctx.db,
        &target.id,
        0,
        None,
        ChunkType::Reference,
        "Snapshot too old errors are resolved by undo retention tuning.",
        [0.1, 0.2, 0.3],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &distractor.id,
        0,
        None,
        ChunkType::Reference,
        "A chapter about indexing and cache settings.",
        [0.9, 0.1, 0.0],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "snapshot too old")
        .add_query_param("limit", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["chunks"][0]["book_id"], target.id);
    assert!(body["chunks"][0]["bm25_score"].is_number());
}

#[tokio::test]
async fn test_collection_chunk_search_clamps_limit_to_100() {
    let ctx = TestContext::new().await;
    let (admin, password) = ctx.create_admin().await;
    let token = ctx.login(&admin.username, &password).await.access_token;
    let book = ctx.create_book("Collection Clamp", "Admin").await;

    let collection = collection_queries::create_collection(
        &ctx.db,
        &admin.id,
        collection_queries::CollectionInput {
            name: "Clamp Collection".to_string(),
            description: None,
            domain: "technical".to_string(),
            is_public: false,
        },
    )
    .await
    .expect("create collection");
    collection_queries::add_books_to_collection(&ctx.db, &collection.id, &[book.id.clone()])
        .await
        .expect("add book to collection");

    insert_chunk_series(
        &ctx.db,
        &book.id,
        120,
        "collection clamp search chunk",
        ChunkType::Reference,
    )
    .await;

    let response = ctx
        .server
        .get(&format!(
            "/api/v1/collections/{}/search/chunks",
            collection.id
        ))
        .add_query_param("q", "collection clamp")
        .add_query_param("limit", 99999)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body["chunks"].as_array().unwrap().len() <= 100);
}

#[tokio::test]
async fn test_hybrid_outranks_either_alone() {
    let ctx = TestContext::new_with_config(mock_llm_config("mock://hybrid")).await;
    let token = ctx.admin_token().await;
    let bm25_only = ctx.create_book("BM25 Only", "Admin").await;
    let semantic_only = ctx.create_book("Semantic Only", "Admin").await;
    let hybrid = ctx.create_book("Hybrid Winner", "Admin").await;

    insert_chunk(
        &ctx.db,
        &bm25_only.id,
        0,
        None,
        ChunkType::Procedure,
        "ORA-01555 snapshot too old alert.",
        [0.9, 0.1, 0.0],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &semantic_only.id,
        0,
        None,
        ChunkType::Procedure,
        "Completely unrelated operational note.",
        [0.1, 0.2, 0.3],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &hybrid.id,
        0,
        None,
        ChunkType::Procedure,
        "snapshot too old diagnostics.",
        [0.12, 0.22, 0.28],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "snapshot too old")
        .add_query_param("limit", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["chunks"][0]["book_id"], hybrid.id);
    let top_rrf = body["chunks"][0]["rrf_score"].as_f64().unwrap_or_default();
    let second_rrf = body["chunks"][1]["rrf_score"].as_f64().unwrap_or_default();
    assert!(top_rrf >= second_rrf);
}

#[tokio::test]
async fn test_book_ids_filter_limits_results() {
    let ctx = TestContext::new_with_config(mock_llm_config("mock://book-filter")).await;
    let token = ctx.admin_token().await;
    let allowed = ctx.create_book("Allowed", "Admin").await;
    let blocked = ctx.create_book("Blocked", "Admin").await;

    insert_chunk(
        &ctx.db,
        &allowed.id,
        0,
        None,
        ChunkType::Reference,
        "ORA-01555 retention issue.",
        [0.1, 0.2, 0.3],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &blocked.id,
        0,
        None,
        ChunkType::Reference,
        "ORA-01555 retention issue.",
        [0.1, 0.2, 0.3],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "ORA-01555")
        .add_query_param("book_ids[]", allowed.id.clone())
        .add_query_param("limit", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|chunk| chunk["book_id"] == allowed.id));
}

#[tokio::test]
async fn test_collection_id_filter_spans_all_books_in_collection() {
    let ctx = TestContext::new().await;
    let (admin, password) = ctx.create_admin().await;
    let token = ctx.login(&admin.username, &password).await.access_token;
    let first = ctx.create_book("First Collection Book", "Admin").await;
    let second = ctx.create_book("Second Collection Book", "Admin").await;

    let collection = collection_queries::create_collection(
        &ctx.db,
        &admin.id,
        collection_queries::CollectionInput {
            name: "Search Collection".to_string(),
            description: None,
            domain: "technical".to_string(),
            is_public: false,
        },
    )
    .await
    .expect("create collection");
    collection_queries::add_books_to_collection(&ctx.db, &collection.id, &[first.id.clone()])
        .await
        .expect("add first book");
    collection_queries::add_books_to_collection(&ctx.db, &collection.id, &[second.id.clone()])
        .await
        .expect("add second book");

    insert_chunk(
        &ctx.db,
        &first.id,
        0,
        None,
        ChunkType::Procedure,
        "ORA-01555 first book.",
        [0.1, 0.2, 0.3],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &second.id,
        0,
        None,
        ChunkType::Procedure,
        "ORA-01555 second book.",
        [0.1, 0.2, 0.3],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "ORA-01555")
        .add_query_param("collection_id", collection.id.clone())
        .add_query_param("limit", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let book_ids = body["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|chunk| chunk["book_id"].as_str().unwrap().to_string())
        .collect::<HashSet<_>>();
    assert_eq!(
        book_ids,
        HashSet::from([first.id.clone(), second.id.clone()])
    );
}

#[tokio::test]
async fn test_chunk_type_filter_returns_only_procedures() {
    let ctx = TestContext::new_with_config(mock_llm_config("mock://chunk-type")).await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Chunk Types", "Admin").await;

    insert_chunk(
        &ctx.db,
        &book.id,
        0,
        None,
        ChunkType::Procedure,
        "ORA-01555 procedure step.",
        [0.1, 0.2, 0.3],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &book.id,
        1,
        None,
        ChunkType::Reference,
        "ORA-01555 reference note.",
        [0.1, 0.2, 0.3],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "ORA-01555")
        .add_query_param("type", "procedure")
        .add_query_param("limit", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|chunk| chunk["chunk_type"] == "procedure"));
}

#[tokio::test]
async fn test_rerank_reorders_results() {
    let ctx = TestContext::new_with_config(mock_llm_config("mock://rerank")).await;
    let token = ctx.admin_token().await;
    let first = ctx.create_book("Rerank Leader", "Admin").await;
    let second = ctx.create_book("Rerank Winner", "Admin").await;

    insert_chunk(
        &ctx.db,
        &first.id,
        0,
        None,
        ChunkType::Procedure,
        "retention policy procedure [rerank-low]",
        [0.1, 0.2, 0.3],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &second.id,
        0,
        None,
        ChunkType::Procedure,
        "retention preferred passage [rerank-high]",
        [0.09, 0.19, 0.29],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "retention")
        .add_query_param("limit", 10)
        .add_query_param("rerank", "true")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["chunks"][0]["book_id"], second.id);
    assert!(
        body["chunks"][0]["rerank_score"]
            .as_f64()
            .unwrap_or_default()
            > 0.9
    );
}

#[tokio::test]
async fn test_rerank_falls_back_on_timeout() {
    let ctx = TestContext::new_with_config(mock_llm_config("mock://timeout")).await;
    let token = ctx.admin_token().await;
    let first = ctx.create_book("Timeout Leader", "Admin").await;
    let second = ctx.create_book("Timeout Runner Up", "Admin").await;

    insert_chunk(
        &ctx.db,
        &first.id,
        0,
        None,
        ChunkType::Procedure,
        "retention policy procedure [rerank-low]",
        [0.1, 0.2, 0.3],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &second.id,
        0,
        None,
        ChunkType::Procedure,
        "retention preferred passage [rerank-high]",
        [0.09, 0.19, 0.29],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "retention")
        .add_query_param("limit", 10)
        .add_query_param("rerank", "true")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let returned_ids = body["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|chunk| chunk["book_id"].as_str().unwrap().to_string())
        .collect::<HashSet<_>>();
    assert_eq!(
        returned_ids,
        HashSet::from([first.id.clone(), second.id.clone()])
    );
    assert!(body["chunks"]
        .as_array()
        .unwrap()
        .iter()
        .all(|chunk| chunk["rerank_score"].is_null()));
}

#[tokio::test]
async fn test_response_includes_provenance_fields() {
    let ctx = TestContext::new_with_config(mock_llm_config("mock://provenance")).await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Provenance", "Admin").await;

    insert_chunk(
        &ctx.db,
        &book.id,
        0,
        Some("Chapter 1 > Section A"),
        ChunkType::Procedure,
        "ORA-01555 provenance check.",
        [0.1, 0.2, 0.3],
    )
    .await;

    let response = ctx
        .server
        .get("/api/v1/search/chunks")
        .add_query_param("q", "ORA-01555")
        .add_query_param("limit", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let chunk = &body["chunks"][0];
    for field in [
        "chunk_id",
        "book_id",
        "book_title",
        "heading_path",
        "chunk_type",
        "text",
        "word_count",
        "bm25_score",
        "cosine_score",
        "rrf_score",
        "rerank_score",
    ] {
        assert!(chunk.get(field).is_some(), "missing field {field}");
    }
}

async fn insert_chunk(
    db: &sqlx::SqlitePool,
    book_id: &str,
    chunk_index: i64,
    heading_path: Option<&str>,
    chunk_type: ChunkType,
    text: &str,
    embedding: [f32; 3],
) {
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
    .bind(Uuid::new_v4().to_string())
    .bind(book_id)
    .bind(chunk_index)
    .bind(0_i64)
    .bind(heading_path.map(str::to_string))
    .bind(chunk_type.as_str())
    .bind(text)
    .bind(text.split_whitespace().count() as i64)
    .bind(0_i64)
    .bind(vector_to_blob(&embedding))
    .bind(now)
    .execute(db)
    .await
    .expect("insert chunk");
}

async fn insert_chunk_series(
    db: &sqlx::SqlitePool,
    book_id: &str,
    count: usize,
    text_prefix: &str,
    chunk_type: ChunkType,
) {
    for chunk_index in 0..count {
        let text = format!("{text_prefix} {chunk_index}");
        insert_chunk(
            db,
            book_id,
            chunk_index as i64,
            None,
            chunk_type,
            &text,
            [0.1, 0.2, 0.3],
        )
        .await;
    }
}

fn vector_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(std::mem::size_of_val(vector));
    for value in vector {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    blob
}

fn mock_llm_config(endpoint: &str) -> AppConfig {
    let mut config = AppConfig::default();
    config.llm.enabled = true;
    config.llm.librarian.endpoint = endpoint.to_string();
    config.llm.librarian.model = "test-embedding-model".to_string();
    config.llm.librarian.system_prompt = "You are a ranking helper.".to_string();
    config
}

async fn chunk_id(db: &sqlx::SqlitePool) -> String {
    sqlx::query_scalar("SELECT id FROM book_chunks ORDER BY rowid ASC LIMIT 1")
        .fetch_one(db)
        .await
        .expect("select inserted chunk id")
}
