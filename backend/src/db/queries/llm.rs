use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SemanticIndexJob {
    pub id: String,
    pub book_id: Option<String>,
}

pub async fn enqueue_semantic_index_job(db: &SqlitePool, book_id: &str) -> anyhow::Result<bool> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query(
        r#"
        INSERT INTO llm_jobs (
            id, job_type, status, book_id, payload_json, result_json, error_text, created_at, started_at, completed_at
        )
        SELECT ?, 'semantic_index', 'pending', ?, NULL, NULL, NULL, ?, NULL, NULL
        WHERE NOT EXISTS (
            SELECT 1
            FROM llm_jobs
            WHERE job_type = 'semantic_index'
              AND book_id = ?
              AND status IN ('pending', 'running')
        )
        "#,
    )
    .bind(id)
    .bind(book_id)
    .bind(now)
    .bind(book_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn claim_next_semantic_index_job(
    db: &SqlitePool,
) -> anyhow::Result<Option<SemanticIndexJob>> {
    let now = Utc::now().to_rfc3339();
    let row = sqlx::query(
        r#"
        WITH next_job AS (
            SELECT id
            FROM llm_jobs
            WHERE job_type = 'semantic_index'
              AND status = 'pending'
            ORDER BY created_at ASC
            LIMIT 1
        )
        UPDATE llm_jobs
        SET status = 'running',
            started_at = ?,
            completed_at = NULL,
            error_text = NULL
        WHERE id IN (SELECT id FROM next_job)
        RETURNING id, book_id
        "#,
    )
    .bind(now)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| SemanticIndexJob {
        id: row.get("id"),
        book_id: row.get("book_id"),
    }))
}

pub async fn mark_semantic_job_completed(db: &SqlitePool, job_id: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE llm_jobs
        SET status = 'completed',
            completed_at = ?,
            error_text = NULL
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(job_id)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn mark_semantic_job_failed(
    db: &SqlitePool,
    job_id: &str,
    error_text: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE llm_jobs
        SET status = 'failed',
            completed_at = ?,
            error_text = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(error_text)
    .bind(job_id)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn mark_running_semantic_jobs_for_book_completed(
    db: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE llm_jobs
        SET status = 'completed',
            completed_at = ?,
            error_text = NULL
        WHERE job_type = 'semantic_index'
          AND book_id = ?
          AND status = 'running'
        "#,
    )
    .bind(now)
    .bind(book_id)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn mark_running_semantic_jobs_for_book_failed(
    db: &SqlitePool,
    book_id: &str,
    error_text: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE llm_jobs
        SET status = 'failed',
            completed_at = ?,
            error_text = ?
        WHERE job_type = 'semantic_index'
          AND book_id = ?
          AND status = 'running'
        "#,
    )
    .bind(now)
    .bind(error_text)
    .bind(book_id)
    .execute(db)
    .await?;

    Ok(())
}
