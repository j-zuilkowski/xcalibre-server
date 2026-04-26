#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::auth::password::hash_password;
use backend::ingest::chunker::ChunkType;
use chrono::Utc;
use common::{auth_header, TestContext};
use std::collections::HashSet;
use uuid::Uuid;

#[tokio::test]
async fn test_create_collection() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .post("/api/v1/collections")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "name": "Oracle Database 19c",
            "description": "Complete Oracle 19c documentation set",
            "is_public": false
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["name"], "Oracle Database 19c");
    assert_eq!(body["description"], "Complete Oracle 19c documentation set");
    assert_eq!(body["domain"], "technical");
    assert_eq!(body["is_public"], false);
    assert_eq!(body["book_count"], 0);
    assert_eq!(body["total_chunks"], 0);
}

#[tokio::test]
async fn test_add_books_to_collection() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let first = ctx.create_book("Collection Book One", "Author One").await;
    let second = ctx.create_book("Collection Book Two", "Author Two").await;
    let collection_id = create_collection(&ctx, &token, "Reading List", false).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/collections/{collection_id}/books"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "book_ids": [first.id.clone(), second.id.clone()]
        }))
        .await;

    assert_status!(response, 204);

    let detail = get_collection_detail(&ctx, &token, &collection_id).await;
    assert_eq!(detail["book_count"], 2);
    let books = detail["books"].as_array().cloned().unwrap_or_default();
    let book_ids = books
        .iter()
        .filter_map(|book| book["id"].as_str().map(ToOwned::to_owned))
        .collect::<HashSet<_>>();
    assert!(book_ids.contains(&first.id));
    assert!(book_ids.contains(&second.id));
}

#[tokio::test]
async fn test_remove_book_from_collection() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let first = ctx.create_book("Collection Book One", "Author One").await;
    let second = ctx.create_book("Collection Book Two", "Author Two").await;
    let collection_id = create_collection(&ctx, &token, "Reading List", false).await;

    add_books(
        &ctx,
        &token,
        &collection_id,
        &[first.id.clone(), second.id.clone()],
    )
    .await;

    let response = ctx
        .server
        .delete(&format!(
            "/api/v1/collections/{collection_id}/books/{}",
            first.id
        ))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 204);

    let detail = get_collection_detail(&ctx, &token, &collection_id).await;
    assert_eq!(detail["book_count"], 1);
    let books = detail["books"].as_array().cloned().unwrap_or_default();
    assert_eq!(books.len(), 1);
    assert_eq!(books[0]["id"], second.id);
}

#[tokio::test]
async fn test_add_book_to_private_collection_requires_visibility() {
    let ctx = TestContext::new().await;
    let owner_token =
        create_unique_user_token(&ctx, "owner-add-private", "owner-add-private@example.com").await;
    let other_token =
        create_unique_user_token(&ctx, "other-add-private", "other-add-private@example.com").await;
    let book = ctx.create_book("Private Add Book", "Author One").await;
    let collection_id = create_collection(&ctx, &owner_token, "Private Reading", false).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/collections/{collection_id}/books"))
        .add_header(header::AUTHORIZATION, auth_header(&other_token))
        .json(&serde_json::json!({
            "book_ids": [book.id.clone()]
        }))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_concurrent_remove_book_from_collection_is_atomic() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx
        .create_book("Concurrent Remove Book", "Author One")
        .await;
    let collection_id = create_collection(&ctx, &token, "Concurrent Reading", false).await;

    add_books(&ctx, &token, &collection_id, &[book.id.clone()]).await;

    let request_one = ctx
        .server
        .delete(&format!(
            "/api/v1/collections/{collection_id}/books/{}",
            book.id.as_str()
        ))
        .add_header(header::AUTHORIZATION, auth_header(&token));
    let request_two = ctx
        .server
        .delete(&format!(
            "/api/v1/collections/{collection_id}/books/{}",
            book.id.as_str()
        ))
        .add_header(header::AUTHORIZATION, auth_header(&token));

    let response_one = request_one.await;
    let response_two = request_two.await;
    let statuses = [
        response_one.status_code().as_u16(),
        response_two.status_code().as_u16(),
    ];
    assert!(statuses.contains(&204));
    assert!(statuses.contains(&404));
}

#[tokio::test]
async fn test_collection_search_spans_all_member_books() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let first = ctx.create_book("First Search Book", "Author One").await;
    let second = ctx.create_book("Second Search Book", "Author Two").await;
    let collection_id = create_collection(&ctx, &token, "Search Scope", false).await;

    add_books(
        &ctx,
        &token,
        &collection_id,
        &[first.id.clone(), second.id.clone()],
    )
    .await;
    insert_chunk(
        &ctx.db,
        &first.id,
        0,
        Some("Section One"),
        ChunkType::Procedure,
        "ORA-01555 snapshot too old in the first book.",
    )
    .await;
    insert_chunk(
        &ctx.db,
        &second.id,
        0,
        Some("Section Two"),
        ChunkType::Procedure,
        "ORA-01555 snapshot too old in the second book.",
    )
    .await;

    let response = ctx
        .server
        .get(&format!(
            "/api/v1/collections/{collection_id}/search/chunks"
        ))
        .add_query_param("q", "ORA-01555")
        .add_query_param("limit", 10)
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let chunks = body["chunks"].as_array().cloned().unwrap_or_default();
    assert_eq!(chunks.len(), 2);

    let book_ids = chunks
        .iter()
        .filter_map(|chunk| chunk["book_id"].as_str().map(ToOwned::to_owned))
        .collect::<HashSet<_>>();
    assert!(book_ids.contains(&first.id));
    assert!(book_ids.contains(&second.id));
}

#[tokio::test]
async fn test_public_collection_visible_to_other_users() {
    let ctx = TestContext::new().await;
    let owner_token =
        create_unique_user_token(&ctx, "owner-public", "owner-public@example.com").await;
    let other_token =
        create_unique_user_token(&ctx, "other-public", "other-public@example.com").await;

    let collection_id = create_collection(&ctx, &owner_token, "Public Reading", true).await;

    let response = ctx
        .server
        .get("/api/v1/collections")
        .add_header(header::AUTHORIZATION, auth_header(&other_token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let collections = body.as_array().cloned().unwrap_or_default();
    assert!(collections
        .iter()
        .any(|collection| collection["id"] == collection_id));

    let detail = ctx
        .server
        .get(&format!("/api/v1/collections/{collection_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&other_token))
        .await;
    assert_status!(detail, 200);
}

#[tokio::test]
async fn test_private_collection_not_visible_to_other_users() {
    let ctx = TestContext::new().await;
    let owner_token =
        create_unique_user_token(&ctx, "owner-private", "owner-private@example.com").await;
    let other_token =
        create_unique_user_token(&ctx, "other-private", "other-private@example.com").await;

    let collection_id = create_collection(&ctx, &owner_token, "Private Reading", false).await;

    let response = ctx
        .server
        .get("/api/v1/collections")
        .add_header(header::AUTHORIZATION, auth_header(&other_token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let collections = body.as_array().cloned().unwrap_or_default();
    assert!(!collections
        .iter()
        .any(|collection| collection["id"] == collection_id));

    let detail = ctx
        .server
        .get(&format!("/api/v1/collections/{collection_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&other_token))
        .await;
    assert_status!(detail, 404);
}

#[tokio::test]
async fn test_delete_collection_does_not_delete_books() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx
        .create_book("Disposable Collection Book", "Author")
        .await;
    let collection_id = create_collection(&ctx, &token, "Temp Collection", false).await;

    add_books(&ctx, &token, &collection_id, &[book.id.clone()]).await;

    let response = ctx
        .server
        .delete(&format!("/api/v1/collections/{collection_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 204);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM books WHERE id = ?")
        .bind(&book.id)
        .fetch_one(&ctx.db)
        .await
        .expect("count books");
    assert_eq!(count, 1);
}

async fn create_collection(ctx: &TestContext, token: &str, name: &str, is_public: bool) -> String {
    let response = ctx
        .server
        .post("/api/v1/collections")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .json(&serde_json::json!({
            "name": name,
            "is_public": is_public
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    body["id"].as_str().expect("collection id").to_string()
}

async fn create_unique_user_token(ctx: &TestContext, username: &str, email: &str) -> String {
    let password = "Test1234!".to_string();
    let now = Utc::now().to_rfc3339();
    let password_hash = hash_password(&password, &ctx.state.config.auth).expect("hash password");

    let _ = sqlx::query(
        r#"
        INSERT OR IGNORE INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("user")
    .bind("user")
    .bind(0_i64)
    .bind(0_i64)
    .bind(1_i64)
    .bind(1_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("seed role");

    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, password_hash, role_id, is_active, force_pw_reset, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind("user")
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert user");

    ctx.login(username, &password).await.access_token
}

async fn add_books(ctx: &TestContext, token: &str, collection_id: &str, book_ids: &[String]) {
    let response = ctx
        .server
        .post(&format!("/api/v1/collections/{collection_id}/books"))
        .add_header(header::AUTHORIZATION, auth_header(token))
        .json(&serde_json::json!({ "book_ids": book_ids }))
        .await;

    assert_status!(response, 204);
}

async fn get_collection_detail(
    ctx: &TestContext,
    token: &str,
    collection_id: &str,
) -> serde_json::Value {
    let response = ctx
        .server
        .get(&format!("/api/v1/collections/{collection_id}"))
        .add_header(header::AUTHORIZATION, auth_header(token))
        .await;

    assert_status!(response, 200);
    response.json()
}

async fn insert_chunk(
    db: &sqlx::SqlitePool,
    book_id: &str,
    chunk_index: i64,
    heading_path: Option<&str>,
    chunk_type: ChunkType,
    text: &str,
) {
    let now = Utc::now().to_rfc3339();
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
    .bind(vector_to_blob(&[0.9, 0.1, 0.0]))
    .bind(now)
    .execute(db)
    .await
    .expect("insert chunk");
}

fn vector_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut blob = Vec::with_capacity(std::mem::size_of_val(vector));
    for value in vector {
        blob.extend_from_slice(&value.to_le_bytes());
    }
    blob
}
