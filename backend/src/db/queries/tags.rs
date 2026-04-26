//! Tag lifecycle queries: rename, merge, delete, and lookup.
//! Touches: `tags`, `book_tags`.
//!
//! `merge_tags` reassigns `book_tags` rows from `source_id` to `target_id`
//! using a NOT EXISTS guard to prevent duplicate `(book_id, tag_id)` pairs.
//! When both source and target already exist on a book, a second UPDATE
//! preserves `confirmed = 1` if either side had it confirmed.  The source
//! `tags` row is hard-deleted after its `book_tags` links are cleaned up.
//!
//! `rename_tag` checks for case-insensitive name conflicts before updating.
//! `delete_tag` cascades by first removing `book_tags` entries.

use crate::AppError;
use anyhow::Context;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug, Serialize)]
pub struct TagLookupItem {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TagRecord {
    pub id: String,
    pub name: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct TagWithCount {
    pub id: String,
    pub name: String,
    pub source: String,
    pub book_count: i64,
    pub confirmed_count: i64,
}

pub async fn search_tags(
    db: &SqlitePool,
    query: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<TagLookupItem>> {
    let limit = limit.clamp(1, 50);
    let mut sql = String::from("SELECT id, name FROM tags");
    let trimmed = query.map(str::trim).filter(|value| !value.is_empty());
    if trimmed.is_some() {
        sql.push_str(" WHERE lower(name) LIKE lower(?)");
    }
    sql.push_str(" ORDER BY name ASC LIMIT ?");

    let mut statement = sqlx::query(&sql);
    if let Some(value) = trimmed {
        statement = statement.bind(format!("%{value}%"));
    }
    let rows = statement
        .bind(limit)
        .fetch_all(db)
        .await
        .context("search tags")?;

    Ok(rows
        .into_iter()
        .map(|row| TagLookupItem {
            id: row.get("id"),
            name: row.get("name"),
        })
        .collect())
}

pub async fn list_tags_with_counts(
    db: &SqlitePool,
    query: Option<&str>,
    page: u32,
    page_size: u32,
) -> anyhow::Result<(Vec<TagWithCount>, i64)> {
    let page = page.max(1);
    let page_size = page_size.clamp(1, 100);
    let offset_u64 = u64::from(page.saturating_sub(1)).saturating_mul(u64::from(page_size));
    let offset = if offset_u64 > i64::MAX as u64 {
        i64::MAX
    } else {
        offset_u64 as i64
    };
    let pattern = format!("%{}%", query.map(str::trim).unwrap_or_default());

    let total: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE lower(name) LIKE lower(?)")
            .bind(&pattern)
            .fetch_one(db)
            .await
            .context("count tags with filter")?;

    let rows = sqlx::query(
        r#"
        SELECT
            t.id,
            t.name,
            t.source,
            COUNT(bt.book_id) AS book_count,
            COALESCE(SUM(CASE WHEN bt.confirmed = 1 THEN 1 ELSE 0 END), 0) AS confirmed_count
        FROM tags t
        LEFT JOIN book_tags bt ON bt.tag_id = t.id
        WHERE lower(t.name) LIKE lower(?)
        GROUP BY t.id, t.name, t.source
        ORDER BY book_count DESC, t.name ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(&pattern)
    .bind(i64::from(page_size))
    .bind(offset)
    .fetch_all(db)
    .await
    .context("list tags with counts")?;

    Ok((
        rows.into_iter()
            .map(|row| TagWithCount {
                id: row.get("id"),
                name: row.get("name"),
                source: row.get("source"),
                book_count: row.get("book_count"),
                confirmed_count: row.get("confirmed_count"),
            })
            .collect(),
        total,
    ))
}

pub async fn find_tag_record_by_id(
    db: &SqlitePool,
    tag_id: &str,
) -> Result<Option<TagRecord>, AppError> {
    let row = sqlx::query("SELECT id, name, source FROM tags WHERE id = ?")
        .bind(tag_id)
        .fetch_optional(db)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(row.map(|row| TagRecord {
        id: row.get("id"),
        name: row.get("name"),
        source: row.get("source"),
    }))
}

/// Renames `tag_id` to `new_name`.  Returns `AppError::Conflict` if another
/// tag with the same name (case-insensitive) already exists, `AppError::NotFound`
/// if `tag_id` is unknown.
pub async fn rename_tag(
    db: &SqlitePool,
    tag_id: &str,
    new_name: &str,
) -> Result<TagRecord, AppError> {
    let normalized_name = new_name.trim();
    if normalized_name.is_empty() {
        return Err(AppError::BadRequest);
    }

    let conflict = sqlx::query_scalar::<_, i64>(
        "SELECT 1 FROM tags WHERE lower(name) = lower(?) AND id <> ? LIMIT 1",
    )
    .bind(normalized_name)
    .bind(tag_id)
    .fetch_optional(db)
    .await
    .map_err(|_| AppError::Internal)?;
    if conflict.is_some() {
        return Err(AppError::Conflict);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let updated = sqlx::query("UPDATE tags SET name = ?, last_modified = ? WHERE id = ?")
        .bind(normalized_name)
        .bind(&now)
        .bind(tag_id)
        .execute(db)
        .await
        .map_err(|_| AppError::Internal)?
        .rows_affected();
    if updated == 0 {
        return Err(AppError::NotFound);
    }

    find_tag_record_by_id(db, tag_id)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn delete_tag(db: &SqlitePool, tag_id: &str) -> Result<(), AppError> {
    let mut tx = db.begin().await.map_err(|_| AppError::Internal)?;

    sqlx::query("DELETE FROM book_tags WHERE tag_id = ?")
        .bind(tag_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;

    let deleted = sqlx::query("DELETE FROM tags WHERE id = ?")
        .bind(tag_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?
        .rows_affected();
    if deleted == 0 {
        tx.rollback().await.map_err(|_| AppError::Internal)?;
        return Err(AppError::NotFound);
    }

    tx.commit().await.map_err(|_| AppError::Internal)?;
    Ok(())
}

/// Merges `source_id` into `target_id`.  Returns the number of unique books
/// whose tag was reassigned.  Books that already have `target_id` are skipped
/// (NOT EXISTS guard); `confirmed = 1` is propagated when the source was
/// confirmed on overlapping books.  The source `tags` row is deleted.
pub async fn merge_tags(
    db: &SqlitePool,
    source_id: &str,
    target_id: &str,
) -> Result<usize, AppError> {
    if source_id == target_id {
        return Err(AppError::BadRequest);
    }

    let mut tx = db.begin().await.map_err(|_| AppError::Internal)?;

    let source_exists = sqlx::query_scalar::<_, String>("SELECT id FROM tags WHERE id = ?")
        .bind(source_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;
    if source_exists.is_none() {
        tx.rollback().await.map_err(|_| AppError::Internal)?;
        return Err(AppError::NotFound);
    }

    let target_exists = sqlx::query_scalar::<_, String>("SELECT id FROM tags WHERE id = ?")
        .bind(target_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;
    if target_exists.is_none() {
        tx.rollback().await.map_err(|_| AppError::Internal)?;
        return Err(AppError::NotFound);
    }

    let moved_count_i64: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM book_tags src
        WHERE src.tag_id = ?
          AND NOT EXISTS (
              SELECT 1
              FROM book_tags tgt
              WHERE tgt.book_id = src.book_id
                AND tgt.tag_id = ?
          )
        "#,
    )
    .bind(source_id)
    .bind(target_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    sqlx::query(
        r#"
        INSERT INTO book_tags (book_id, tag_id, confirmed)
        SELECT src.book_id, ?, src.confirmed
        FROM book_tags src
        WHERE src.tag_id = ?
          AND NOT EXISTS (
              SELECT 1
              FROM book_tags tgt
              WHERE tgt.book_id = src.book_id
                AND tgt.tag_id = ?
          )
        "#,
    )
    .bind(target_id)
    .bind(source_id)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    // Preserve confirmed tags when both source and target exist on a book.
    sqlx::query(
        r#"
        UPDATE book_tags
        SET confirmed = 1
        WHERE tag_id = ?
          AND confirmed = 0
          AND EXISTS (
              SELECT 1
              FROM book_tags src
              WHERE src.book_id = book_tags.book_id
                AND src.tag_id = ?
                AND src.confirmed = 1
          )
        "#,
    )
    .bind(target_id)
    .bind(source_id)
    .execute(&mut *tx)
    .await
    .map_err(|_| AppError::Internal)?;

    sqlx::query("DELETE FROM book_tags WHERE tag_id = ?")
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?;

    let deleted_source = sqlx::query("DELETE FROM tags WHERE id = ?")
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(|_| AppError::Internal)?
        .rows_affected();
    if deleted_source == 0 {
        tx.rollback().await.map_err(|_| AppError::Internal)?;
        return Err(AppError::NotFound);
    }

    tx.commit().await.map_err(|_| AppError::Internal)?;

    let moved_count = if moved_count_i64 <= 0 {
        0
    } else {
        match usize::try_from(moved_count_i64) {
            Ok(value) => value,
            Err(_) => usize::MAX,
        }
    };
    Ok(moved_count)
}

pub async fn find_tag_by_id(
    db: &SqlitePool,
    tag_id: &str,
) -> anyhow::Result<Option<TagLookupItem>> {
    let row = sqlx::query("SELECT id, name FROM tags WHERE id = ?")
        .bind(tag_id)
        .fetch_optional(db)
        .await?;

    Ok(row.map(|row| TagLookupItem {
        id: row.get("id"),
        name: row.get("name"),
    }))
}
