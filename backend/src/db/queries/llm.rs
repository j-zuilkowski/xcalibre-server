use crate::llm::classify::TagSuggestion;
use chrono::Utc;
use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct SemanticIndexJob {
    pub id: String,
    pub job_type: String,
    pub book_id: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JobRow {
    pub id: String,
    pub job_type: String,
    pub status: String,
    pub book_id: Option<String>,
    pub book_title: Option<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_text: Option<String>,
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

pub async fn enqueue_classify_job(db: &SqlitePool, book_id: &str) -> anyhow::Result<bool> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query(
        r#"
        INSERT INTO llm_jobs (
            id, job_type, status, book_id, payload_json, result_json, error_text, created_at, started_at, completed_at
        )
        SELECT ?, 'classify', 'pending', ?, NULL, NULL, NULL, ?, NULL, NULL
        WHERE NOT EXISTS (
            SELECT 1
            FROM llm_jobs
            WHERE job_type = 'classify'
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

pub async fn enqueue_organize_job(db: &SqlitePool) -> anyhow::Result<String> {
    if let Some(existing_id) = sqlx::query_scalar::<_, String>(
        r#"
        SELECT id
        FROM llm_jobs
        WHERE job_type = 'organize'
          AND status IN ('pending', 'running')
          AND book_id IS NULL
        ORDER BY created_at ASC
        LIMIT 1
        "#,
    )
    .fetch_optional(db)
    .await?
    {
        return Ok(existing_id);
    }

    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO llm_jobs (
            id, job_type, status, book_id, payload_json, result_json, error_text, created_at, started_at, completed_at
        )
        VALUES (?, 'organize', 'pending', NULL, NULL, NULL, NULL, ?, NULL, NULL)
        "#,
    )
    .bind(&id)
    .bind(now)
    .execute(db)
    .await?;

    Ok(id)
}

pub async fn claim_next_pending_job(db: &SqlitePool) -> anyhow::Result<Option<SemanticIndexJob>> {
    let now = Utc::now().to_rfc3339();
    let row = sqlx::query(
        r#"
        WITH next_job AS (
            SELECT id
            FROM llm_jobs
            WHERE status = 'pending'
            ORDER BY created_at ASC
            LIMIT 1
        )
        UPDATE llm_jobs
        SET status = 'running',
            started_at = ?,
            completed_at = NULL,
            error_text = NULL
        WHERE id IN (SELECT id FROM next_job)
        RETURNING id, job_type, book_id
        "#,
    )
    .bind(now)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| SemanticIndexJob {
        id: row.get("id"),
        job_type: row.get("job_type"),
        book_id: row.get("book_id"),
    }))
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
        RETURNING id, job_type, book_id
        "#,
    )
    .bind(now)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| SemanticIndexJob {
        id: row.get("id"),
        job_type: row.get("job_type"),
        book_id: row.get("book_id"),
    }))
}

pub async fn mark_semantic_job_completed(db: &SqlitePool, job_id: &str) -> anyhow::Result<()> {
    mark_job_completed(db, job_id).await
}

pub async fn mark_job_completed(db: &SqlitePool, job_id: &str) -> anyhow::Result<()> {
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
    mark_job_failed(db, job_id, error_text).await
}

pub async fn mark_job_failed(
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

pub async fn insert_tag_suggestions(
    db: &SqlitePool,
    book_id: &str,
    suggestions: &[TagSuggestion],
) -> anyhow::Result<usize> {
    let now = Utc::now().to_rfc3339();
    let mut inserted_book_tags = 0usize;
    let mut tx = db.begin().await?;

    for suggestion in suggestions {
        let tag_name = suggestion.name.trim();
        if tag_name.is_empty() {
            continue;
        }

        let generated_tag_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO tags (id, name, source, last_modified)
            VALUES (?, ?, 'llm', ?)
            "#,
        )
        .bind(generated_tag_id)
        .bind(tag_name)
        .bind(&now)
        .execute(&mut *tx)
        .await?;

        let tag_id: Option<String> = sqlx::query_scalar("SELECT id FROM tags WHERE name = ?")
            .bind(tag_name)
            .fetch_optional(&mut *tx)
            .await?;

        let Some(tag_id) = tag_id else {
            continue;
        };

        let result = sqlx::query(
            r#"
            INSERT OR IGNORE INTO book_tags (book_id, tag_id, confirmed)
            VALUES (?, ?, 0)
            "#,
        )
        .bind(book_id)
        .bind(tag_id)
        .execute(&mut *tx)
        .await?;

        inserted_book_tags += result.rows_affected() as usize;
    }

    tx.commit().await?;
    Ok(inserted_book_tags)
}

pub async fn list_pending_tags(
    db: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Vec<(String, String)>> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.name
        FROM book_tags bt
        JOIN tags t ON t.id = bt.tag_id
        WHERE bt.book_id = ?
          AND bt.confirmed = 0
        ORDER BY t.name ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| (row.get("id"), row.get("name")))
        .collect())
}

pub async fn confirm_tags(
    db: &SqlitePool,
    book_id: &str,
    confirm_names: &[String],
    reject_names: &[String],
) -> anyhow::Result<usize> {
    let mut tx = db.begin().await?;
    let mut confirmed_rows = 0usize;

    if !confirm_names.is_empty() {
        let mut query =
            QueryBuilder::<Sqlite>::new("UPDATE book_tags SET confirmed = 1 WHERE book_id = ");
        query.push_bind(book_id);
        query.push(" AND tag_id IN (SELECT id FROM tags WHERE name IN (");
        {
            let mut separated = query.separated(", ");
            for name in confirm_names {
                separated.push_bind(name);
            }
        }
        query.push("))");

        let result = query.build().execute(&mut *tx).await?;
        confirmed_rows = result.rows_affected() as usize;
    }

    if !reject_names.is_empty() {
        let mut query = QueryBuilder::<Sqlite>::new("DELETE FROM book_tags WHERE book_id = ");
        query.push_bind(book_id);
        query.push(" AND tag_id IN (SELECT id FROM tags WHERE name IN (");
        {
            let mut separated = query.separated(", ");
            for name in reject_names {
                separated.push_bind(name);
            }
        }
        query.push("))");

        query.build().execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(confirmed_rows)
}

pub async fn confirm_all_pending_tags(db: &SqlitePool, book_id: &str) -> anyhow::Result<usize> {
    let result = sqlx::query(
        r#"
        UPDATE book_tags
        SET confirmed = 1
        WHERE book_id = ?
          AND confirmed = 0
        "#,
    )
    .bind(book_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() as usize)
}

pub async fn list_jobs(
    db: &SqlitePool,
    status: Option<&str>,
    job_type: Option<&str>,
    page: u32,
    page_size: u32,
) -> anyhow::Result<(Vec<JobRow>, i64)> {
    let page = page.max(1);
    let page_size = page_size.max(1);
    let offset = ((page - 1) as i64) * (page_size as i64);

    let status = status
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let job_type = job_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);

    let mut count_query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT COUNT(1) AS total
        FROM llm_jobs lj
        LEFT JOIN books b ON b.id = lj.book_id
        "#,
    );
    apply_job_filters(&mut count_query, status.as_deref(), job_type.as_deref());
    let total: i64 = count_query.build_query_scalar().fetch_one(db).await?;

    let mut data_query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            lj.id,
            lj.job_type,
            lj.status,
            lj.book_id,
            b.title AS book_title,
            lj.created_at,
            lj.started_at,
            lj.completed_at,
            lj.error_text
        FROM llm_jobs lj
        LEFT JOIN books b ON b.id = lj.book_id
        "#,
    );
    apply_job_filters(&mut data_query, status.as_deref(), job_type.as_deref());
    data_query.push(" ORDER BY lj.created_at DESC LIMIT ");
    data_query.push_bind(page_size as i64);
    data_query.push(" OFFSET ");
    data_query.push_bind(offset);

    let rows = data_query.build().fetch_all(db).await?;
    let items = rows
        .into_iter()
        .map(|row| JobRow {
            id: row.get("id"),
            job_type: row.get("job_type"),
            status: row.get("status"),
            book_id: row.get("book_id"),
            book_title: row.get("book_title"),
            created_at: row.get("created_at"),
            started_at: row.get("started_at"),
            completed_at: row.get("completed_at"),
            error_text: row.get("error_text"),
        })
        .collect::<Vec<_>>();

    Ok((items, total))
}

pub async fn get_job(db: &SqlitePool, job_id: &str) -> anyhow::Result<Option<JobRow>> {
    let row = sqlx::query(
        r#"
        SELECT
            lj.id,
            lj.job_type,
            lj.status,
            lj.book_id,
            b.title AS book_title,
            lj.created_at,
            lj.started_at,
            lj.completed_at,
            lj.error_text
        FROM llm_jobs lj
        LEFT JOIN books b ON b.id = lj.book_id
        WHERE lj.id = ?
        LIMIT 1
        "#,
    )
    .bind(job_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| JobRow {
        id: row.get("id"),
        job_type: row.get("job_type"),
        status: row.get("status"),
        book_id: row.get("book_id"),
        book_title: row.get("book_title"),
        created_at: row.get("created_at"),
        started_at: row.get("started_at"),
        completed_at: row.get("completed_at"),
        error_text: row.get("error_text"),
    }))
}

pub async fn cancel_job(db: &SqlitePool, job_id: &str) -> anyhow::Result<bool> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE llm_jobs
        SET status = 'failed',
            error_text = 'cancelled by admin',
            completed_at = ?
        WHERE id = ?
          AND status = 'pending'
        "#,
    )
    .bind(now)
    .bind(job_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

fn apply_job_filters(
    qb: &mut QueryBuilder<'_, Sqlite>,
    status: Option<&str>,
    job_type: Option<&str>,
) {
    let mut where_added = false;
    let mut and_where = |qb: &mut QueryBuilder<'_, Sqlite>| {
        if !where_added {
            qb.push(" WHERE ");
            where_added = true;
        } else {
            qb.push(" AND ");
        }
    };

    if let Some(status) = status {
        and_where(qb);
        qb.push("lj.status = ");
        qb.push_bind(status.to_string());
    }
    if let Some(job_type) = job_type {
        and_where(qb);
        qb.push("lj.job_type = ");
        qb.push_bind(job_type.to_string());
    }
}
