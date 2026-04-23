use chrono::Utc;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub task_type: String,
    pub cron_expr: String,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub next_run_at: Option<String>,
    pub created_at: String,
}

pub async fn list_scheduled_tasks(db: &SqlitePool) -> anyhow::Result<Vec<ScheduledTask>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, task_type, cron_expr, enabled, last_run_at, next_run_at, created_at
        FROM scheduled_tasks
        ORDER BY next_run_at ASC, created_at DESC
        "#,
    )
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(scheduled_task_from_row).collect())
}

pub async fn get_scheduled_task(
    db: &SqlitePool,
    task_id: &str,
) -> anyhow::Result<Option<ScheduledTask>> {
    let row = sqlx::query(
        r#"
        SELECT id, name, task_type, cron_expr, enabled, last_run_at, next_run_at, created_at
        FROM scheduled_tasks
        WHERE id = ?
        LIMIT 1
        "#,
    )
    .bind(task_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(scheduled_task_from_row))
}

pub async fn create_scheduled_task(
    db: &SqlitePool,
    name: &str,
    task_type: &str,
    cron_expr: &str,
    enabled: bool,
    next_run_at: &str,
) -> anyhow::Result<ScheduledTask> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

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
    .bind(cron_expr)
    .bind(i64::from(enabled))
    .bind(next_run_at)
    .bind(now)
    .execute(db)
    .await?;

    get_scheduled_task(db, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("scheduled task insert failed"))
}

pub async fn update_scheduled_task_enabled(
    db: &SqlitePool,
    task_id: &str,
    enabled: bool,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE scheduled_tasks
        SET enabled = ?
        WHERE id = ?
        "#,
    )
    .bind(i64::from(enabled))
    .bind(task_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn update_scheduled_task_cron_expr(
    db: &SqlitePool,
    task_id: &str,
    cron_expr: &str,
    next_run_at: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE scheduled_tasks
        SET cron_expr = ?, next_run_at = ?
        WHERE id = ?
        "#,
    )
    .bind(cron_expr)
    .bind(next_run_at)
    .bind(task_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn delete_scheduled_task(db: &SqlitePool, task_id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM scheduled_tasks
        WHERE id = ?
        "#,
    )
    .bind(task_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn list_due_scheduled_tasks(
    db: &SqlitePool,
    now: &str,
) -> anyhow::Result<Vec<ScheduledTask>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, task_type, cron_expr, enabled, last_run_at, next_run_at, created_at
        FROM scheduled_tasks
        WHERE enabled = 1
          AND next_run_at <= ?
        ORDER BY next_run_at ASC, created_at ASC
        "#,
    )
    .bind(now)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(scheduled_task_from_row).collect())
}

pub async fn mark_scheduled_task_ran(
    db: &SqlitePool,
    task_id: &str,
    last_run_at: &str,
    next_run_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE scheduled_tasks
        SET last_run_at = ?, next_run_at = ?
        WHERE id = ?
        "#,
    )
    .bind(last_run_at)
    .bind(next_run_at)
    .bind(task_id)
    .execute(db)
    .await?;

    Ok(())
}

fn scheduled_task_from_row(row: sqlx::sqlite::SqliteRow) -> ScheduledTask {
    ScheduledTask {
        id: row.get("id"),
        name: row.get("name"),
        task_type: row.get("task_type"),
        cron_expr: row.get("cron_expr"),
        enabled: row.get::<i64, _>("enabled") != 0,
        last_run_at: row.get("last_run_at"),
        next_run_at: row.get("next_run_at"),
        created_at: row.get("created_at"),
    }
}
