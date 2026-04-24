use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema, PartialEq, Eq)]
pub struct ImportErrorEntry {
    pub row: i64,
    pub title: String,
    pub author: String,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct ImportLogRow {
    pub id: String,
    pub filename: String,
    pub source: String,
    pub status: String,
    pub total_rows: Option<i64>,
    pub matched: i64,
    pub unmatched: i64,
    pub errors: Vec<ImportErrorEntry>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub async fn create_import_log(
    db: &SqlitePool,
    user_id: &str,
    filename: &str,
    source: &str,
    total_rows: i64,
) -> anyhow::Result<ImportLogRow> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO goodreads_import_log (
            id, user_id, filename, source, status, total_rows, matched, unmatched, errors, created_at, completed_at
        )
        VALUES (?, ?, ?, ?, 'pending', ?, 0, 0, NULL, ?, NULL)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(filename)
    .bind(source)
    .bind(total_rows)
    .bind(&now)
    .execute(db)
    .await?;

    get_import_log(db, user_id, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("created import log not found"))
}

pub async fn get_import_log(
    db: &SqlitePool,
    user_id: &str,
    job_id: &str,
) -> anyhow::Result<Option<ImportLogRow>> {
    let row = sqlx::query(
        r#"
        SELECT
            id,
            filename,
            source,
            status,
            total_rows,
            matched,
            unmatched,
            errors,
            created_at,
            completed_at
        FROM goodreads_import_log
        WHERE id = ? AND user_id = ?
        LIMIT 1
        "#,
    )
    .bind(job_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    row.map(row_to_import_log).transpose()
}

pub async fn update_import_log(
    db: &SqlitePool,
    job_id: &str,
    status: &str,
    matched: i64,
    unmatched: i64,
    errors: &[ImportErrorEntry],
    completed_at: Option<&str>,
) -> anyhow::Result<()> {
    let errors_json = serde_json::to_string(errors)?;
    sqlx::query(
        r#"
        UPDATE goodreads_import_log
        SET status = ?,
            matched = ?,
            unmatched = ?,
            errors = ?,
            completed_at = ?
        WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(matched)
    .bind(unmatched)
    .bind(if errors.is_empty() {
        None::<String>
    } else {
        Some(errors_json)
    })
    .bind(completed_at)
    .bind(job_id)
    .execute(db)
    .await?;
    Ok(())
}

fn row_to_import_log(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<ImportLogRow> {
    let errors_json: Option<String> = row.get("errors");
    let errors = match errors_json {
        Some(value) => serde_json::from_str::<Vec<ImportErrorEntry>>(&value)?,
        None => Vec::new(),
    };

    Ok(ImportLogRow {
        id: row.get("id"),
        filename: row.get("filename"),
        source: row.get("source"),
        status: row.get("status"),
        total_rows: row.get("total_rows"),
        matched: row.get("matched"),
        unmatched: row.get("unmatched"),
        errors,
        created_at: row.get("created_at"),
        completed_at: row.get("completed_at"),
    })
}
