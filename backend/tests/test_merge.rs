#![allow(dead_code, unused_imports)]

mod common;

use chrono::Utc;
use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;

async fn insert_format(
    ctx: &TestContext,
    book_id: &str,
    format: &str,
    path: &str,
) -> anyhow::Result<String> {
    let now = Utc::now().to_rfc3339();
    let format_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&format_id)
    .bind(book_id)
    .bind(format)
    .bind(path)
    .bind(1024_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await?;
    Ok(format_id)
}

async fn insert_identifier(
    ctx: &TestContext,
    book_id: &str,
    id_type: &str,
    value: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO identifiers (id, book_id, id_type, value, last_modified)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(book_id)
    .bind(id_type)
    .bind(value)
    .bind(&now)
    .execute(&ctx.db)
    .await?;
    Ok(())
}

#[tokio::test]
async fn test_merge_transfers_formats() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let primary = ctx.create_book("Primary", "Author A").await;
    let duplicate = ctx.create_book("Duplicate", "Author B").await;

    insert_format(&ctx, &primary.id, "EPUB", "books/aa/primary.epub")
        .await
        .expect("insert primary format");
    insert_format(&ctx, &duplicate.id, "PDF", "books/bb/duplicate.pdf")
        .await
        .expect("insert duplicate format");

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/merge", primary.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "duplicate_id": duplicate.id }))
        .await;
    assert_status!(response, 204);

    let rows = sqlx::query(
        r#"
        SELECT upper(format) AS format_name
        FROM formats
        WHERE book_id = ?
        ORDER BY format_name
        "#,
    )
    .bind(&primary.id)
    .fetch_all(&ctx.db)
    .await
    .expect("load merged formats");

    let formats = rows
        .into_iter()
        .map(|row| row.get::<String, _>("format_name"))
        .collect::<Vec<_>>();
    assert_eq!(formats, vec!["EPUB".to_string(), "PDF".to_string()]);
}

#[tokio::test]
async fn test_merge_deduplicates_identifiers() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let primary = ctx.create_book("Primary", "Author A").await;
    let duplicate = ctx.create_book("Duplicate", "Author B").await;

    insert_identifier(&ctx, &primary.id, "isbn13", "9781111111111")
        .await
        .expect("insert primary isbn");
    insert_identifier(&ctx, &primary.id, "asin", "B00PRIMARY")
        .await
        .expect("insert primary asin");
    insert_identifier(&ctx, &duplicate.id, "isbn13", "9781111111111")
        .await
        .expect("insert duplicate isbn");
    insert_identifier(&ctx, &duplicate.id, "goodreads", "9999")
        .await
        .expect("insert duplicate goodreads");

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/merge", primary.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "duplicate_id": duplicate.id }))
        .await;
    assert_status!(response, 204);

    let rows = sqlx::query(
        r#"
        SELECT id_type, value
        FROM identifiers
        WHERE book_id = ?
        ORDER BY id_type
        "#,
    )
    .bind(&primary.id)
    .fetch_all(&ctx.db)
    .await
    .expect("load merged identifiers");

    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].get::<String, _>("id_type"), "asin");
    assert_eq!(rows[1].get::<String, _>("id_type"), "goodreads");
    assert_eq!(rows[2].get::<String, _>("id_type"), "isbn13");

    let duplicate_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM identifiers WHERE book_id = ?")
            .bind(&duplicate.id)
            .fetch_one(&ctx.db)
            .await
            .expect("count duplicate identifiers");
    assert_eq!(duplicate_count, 0);
}

#[tokio::test]
async fn test_merge_transfers_reading_progress() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let primary = ctx.create_book("Primary", "Author A").await;
    let duplicate = ctx.create_book("Duplicate", "Author B").await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("admin")
        .fetch_one(&ctx.db)
        .await
        .expect("load admin id");

    let primary_format_id = insert_format(&ctx, &primary.id, "EPUB", "books/aa/primary.epub")
        .await
        .expect("insert primary format");
    let duplicate_format_id = insert_format(&ctx, &duplicate.id, "EPUB", "books/bb/duplicate.epub")
        .await
        .expect("insert duplicate format");
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO reading_progress (
            id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&user_id)
    .bind(&primary.id)
    .bind(&primary_format_id)
    .bind("epubcfi(/6/2[chap]!/4/1:0)")
    .bind(10_i64)
    .bind(0.25_f64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert primary progress");

    sqlx::query(
        r#"
        INSERT INTO reading_progress (
            id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&user_id)
    .bind(&duplicate.id)
    .bind(&duplicate_format_id)
    .bind("epubcfi(/6/6[chap]!/4/1:0)")
    .bind(30_i64)
    .bind(0.75_f64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert duplicate progress");

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/merge", primary.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "duplicate_id": duplicate.id }))
        .await;
    assert_status!(response, 204);

    let row = sqlx::query(
        r#"
        SELECT book_id, format_id, percentage
        FROM reading_progress
        WHERE user_id = ? AND book_id = ?
        "#,
    )
    .bind(&user_id)
    .bind(&primary.id)
    .fetch_one(&ctx.db)
    .await
    .expect("load merged progress");
    assert_eq!(row.get::<String, _>("book_id"), primary.id);
    assert_eq!(row.get::<f64, _>("percentage"), 0.75_f64);

    let format_id = row.get::<String, _>("format_id");
    assert_eq!(format_id, primary_format_id);
}

#[tokio::test]
async fn test_merge_deletes_duplicate_book() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let primary = ctx.create_book("Primary", "Author A").await;
    let duplicate = ctx.create_book("Duplicate", "Author B").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/merge", primary.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "duplicate_id": duplicate.id }))
        .await;
    assert_status!(response, 204);

    let duplicate_exists: Option<String> = sqlx::query_scalar("SELECT id FROM books WHERE id = ?")
        .bind(&duplicate.id)
        .fetch_optional(&ctx.db)
        .await
        .expect("query duplicate book");
    assert!(duplicate_exists.is_none());
}

#[tokio::test]
async fn test_merge_requires_admin() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let primary = ctx.create_book("Primary", "Author A").await;
    let duplicate = ctx.create_book("Duplicate", "Author B").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/merge", primary.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "duplicate_id": duplicate.id }))
        .await;
    assert_status!(response, 403);
}

#[tokio::test]
async fn test_merge_same_book_returns_400() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let primary = ctx.create_book("Primary", "Author A").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/merge", primary.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "duplicate_id": primary.id }))
        .await;
    assert_status!(response, 400);
}
