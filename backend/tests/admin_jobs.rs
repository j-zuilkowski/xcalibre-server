#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use chrono::Utc;
use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;

#[tokio::test]
async fn test_list_jobs_empty() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/admin/jobs")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 0);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn test_list_jobs_filtered_by_status() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let _pending = insert_job(&ctx.db, "classify", "pending").await;
    let _completed = insert_job(&ctx.db, "organize", "completed").await;

    let response = ctx
        .server
        .get("/api/v1/admin/jobs")
        .add_query_param("status", "pending")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["items"][0]["status"], "pending");
}

#[tokio::test]
async fn test_get_job_not_found() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let missing = Uuid::new_v4().to_string();

    let response = ctx
        .server
        .get(&format!("/api/v1/admin/jobs/{missing}"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_cancel_pending_job() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let job_id = insert_job(&ctx.db, "classify", "pending").await;

    let response = ctx
        .server
        .delete(&format!("/api/v1/admin/jobs/{job_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 204);

    let row = sqlx::query("SELECT status, error_text FROM llm_jobs WHERE id = ?")
        .bind(&job_id)
        .fetch_one(&ctx.db)
        .await
        .expect("select cancelled job");
    let status: String = row.get("status");
    let error_text: Option<String> = row.get("error_text");
    assert_eq!(status, "failed");
    assert_eq!(error_text.as_deref(), Some("cancelled by admin"));
}

#[tokio::test]
async fn test_cancel_non_pending_job() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let job_id = insert_job(&ctx.db, "classify", "running").await;

    let response = ctx
        .server
        .delete(&format!("/api/v1/admin/jobs/{job_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 409);
    let body: serde_json::Value = response.json();
    assert_eq!(body["error"], "conflict");
    assert_eq!(body["message"], "Job is not in pending status");
}

async fn insert_job(db: &sqlx::SqlitePool, job_type: &str, status: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let started_at = if status == "running" || status == "completed" {
        Some(now.clone())
    } else {
        None
    };
    let completed_at = if status == "completed" {
        Some(now.clone())
    } else {
        None
    };

    sqlx::query(
        r#"
        INSERT INTO llm_jobs (
            id, job_type, status, book_id, payload_json, result_json, error_text, created_at, started_at, completed_at
        )
        VALUES (?, ?, ?, NULL, NULL, NULL, NULL, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(job_type)
    .bind(status)
    .bind(now)
    .bind(started_at)
    .bind(completed_at)
    .execute(db)
    .await
    .expect("insert llm job");

    id
}
