#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use backend::scheduler;
use chrono::{Duration, Utc};
use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;

#[tokio::test]
async fn test_create_scheduled_task() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/admin/scheduled-tasks")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "name": "Re-classify unclassified books",
            "task_type": "classify_all",
            "cron_expr": "0 2 * * 0",
            "enabled": true
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["name"], "Re-classify unclassified books");
    assert_eq!(body["task_type"], "classify_all");
    assert_eq!(body["cron_expr"], "0 2 * * 0");
    assert_eq!(body["enabled"], true);
    assert!(body["next_run_at"].as_str().unwrap_or_default().contains('T'));

    let row = sqlx::query(
        "SELECT name, task_type, cron_expr, enabled, last_run_at, next_run_at FROM scheduled_tasks WHERE id = ?",
    )
    .bind(body["id"].as_str().expect("task id"))
    .fetch_one(&ctx.db)
    .await
    .expect("select scheduled task");
    let name: String = row.get("name");
    let task_type: String = row.get("task_type");
    let cron_expr: String = row.get("cron_expr");
    let enabled: i64 = row.get("enabled");
    let last_run_at: Option<String> = row.get("last_run_at");
    let next_run_at: Option<String> = row.get("next_run_at");

    assert_eq!(name, "Re-classify unclassified books");
    assert_eq!(task_type, "classify_all");
    assert_eq!(cron_expr, "0 2 * * 0");
    assert_eq!(enabled, 1);
    assert!(last_run_at.is_none());
    assert!(next_run_at.is_some());
}

#[tokio::test]
async fn test_invalid_cron_returns_400() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .post("/api/v1/admin/scheduled-tasks")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "name": "Invalid cron",
            "task_type": "classify_all",
            "cron_expr": "not-a-cron",
            "enabled": true
        }))
        .await;

    assert_status!(response, 400);
}

#[tokio::test]
async fn test_disable_task_skips_execution() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let task_id = insert_due_scheduled_task(&ctx.db, "Skip me", "classify_all", true).await;

    let response = ctx
        .server
        .patch(&format!("/api/v1/admin/scheduled-tasks/{task_id}"))
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "enabled": false }))
        .await;

    assert_status!(response, 200);

    let processed = scheduler::process_due_scheduled_tasks_once(&ctx.state)
        .await
        .expect("run scheduler once");
    assert_eq!(processed, 0);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM llm_jobs WHERE job_type = 'organize'")
        .fetch_one(&ctx.db)
        .await
        .expect("count jobs");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_due_task_creates_llm_job() {
    let ctx = TestContext::new().await;
    let task_id = insert_due_scheduled_task(&ctx.db, "Run now", "classify_all", true).await;

    let processed = scheduler::process_due_scheduled_tasks_once(&ctx.state)
        .await
        .expect("run scheduler once");
    assert_eq!(processed, 1);

    let job_count: i64 =
        sqlx::query_scalar("SELECT COUNT(1) FROM llm_jobs WHERE job_type = 'organize' AND status = 'pending'")
            .fetch_one(&ctx.db)
            .await
            .expect("count organize jobs");
    assert_eq!(job_count, 1);

    let row = sqlx::query("SELECT last_run_at, next_run_at FROM scheduled_tasks WHERE id = ?")
        .bind(&task_id)
        .fetch_one(&ctx.db)
        .await
        .expect("select scheduled task after run");
    let last_run_at: Option<String> = row.get("last_run_at");
    let next_run_at: Option<String> = row.get("next_run_at");
    assert!(last_run_at.is_some());
    assert!(next_run_at.is_some());
}

async fn insert_due_scheduled_task(
    db: &sqlx::SqlitePool,
    name: &str,
    task_type: &str,
    enabled: bool,
) -> String {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let next_run_at = (Utc::now() - Duration::minutes(1)).to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO scheduled_tasks (
            id, name, task_type, cron_expr, enabled, last_run_at, next_run_at, created_at
        )
        VALUES (?, ?, ?, ?, ?, NULL, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(task_type)
    .bind("0 2 * * 0")
    .bind(i64::from(enabled))
    .bind(next_run_at)
    .bind(now)
    .execute(db)
    .await
    .expect("insert scheduled task");

    id
}
