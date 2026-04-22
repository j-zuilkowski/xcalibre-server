#![allow(dead_code, unused_imports)]

mod common;

use chrono::Utc;
use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;

async fn insert_custom_column(
    ctx: &TestContext,
    name: &str,
    label: &str,
    column_type: &str,
    is_multiple: bool,
) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO custom_columns (id, name, label, column_type, is_multiple, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(label)
    .bind(column_type)
    .bind(i64::from(is_multiple))
    .bind(&now)
    .execute(&ctx.db)
    .await?;
    Ok(id)
}

#[tokio::test]
async fn test_list_custom_columns() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let _ = insert_custom_column(&ctx, "Read Date", "#read_date", "datetime", false).await;
    let _ = insert_custom_column(&ctx, "Priority", "#priority", "integer", false).await;

    let response = ctx
        .server
        .get("/api/v1/books/custom-columns")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let items = body.as_array().expect("custom columns array");
    assert_eq!(items.len(), 2);
}

#[tokio::test]
async fn test_set_custom_value_for_book() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Custom Field Book", "Author A").await;
    let column_id = insert_custom_column(&ctx, "Read Date", "#read_date", "text", false)
        .await
        .expect("insert custom column");

    let patch_response = ctx
        .server
        .patch(&format!("/api/v1/books/{}/custom-values", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!([
            { "column_id": column_id, "value": "2026-04-22" }
        ]))
        .await;
    assert_status!(patch_response, 204);

    let get_response = ctx
        .server
        .get(&format!("/api/v1/books/{}/custom-values", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(get_response, 200);
    let body: serde_json::Value = get_response.json();
    let items = body.as_array().expect("custom values");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["column_id"], column_id);
    assert_eq!(items[0]["value"], "2026-04-22");
}

#[tokio::test]
async fn test_custom_value_type_validation() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Type Validation Book", "Author B").await;
    let column_id = insert_custom_column(&ctx, "Priority", "#priority", "integer", false)
        .await
        .expect("insert integer custom column");

    let response = ctx
        .server
        .patch(&format!("/api/v1/books/{}/custom-values", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!([
            { "column_id": column_id, "value": "not-a-number" }
        ]))
        .await;
    assert_status!(response, 400);
}

#[tokio::test]
async fn test_admin_can_create_column() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/books/custom-columns")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "name": "Read Date",
            "label": "#read_date",
            "column_type": "datetime",
            "is_multiple": false
        }))
        .await;
    assert_status!(response, 201);

    let body: serde_json::Value = response.json();
    let id = body["id"].as_str().expect("created custom column id");
    let row = sqlx::query(
        r#"
        SELECT id, name, label, column_type, is_multiple
        FROM custom_columns
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_one(&ctx.db)
    .await
    .expect("created custom column row");
    assert_eq!(row.get::<String, _>("name"), "Read Date");
    assert_eq!(row.get::<String, _>("label"), "#read_date");
    assert_eq!(row.get::<String, _>("column_type"), "datetime");
    assert_eq!(row.get::<i64, _>("is_multiple"), 0);
}
