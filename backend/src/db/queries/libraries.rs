use anyhow::Context;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug, serde::Serialize)]
pub struct Library {
    pub id: String,
    pub name: String,
    pub calibre_db_path: String,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn list_libraries(db: &SqlitePool) -> anyhow::Result<Vec<Library>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, calibre_db_path, created_at, updated_at
        FROM libraries
        ORDER BY name ASC, id ASC
        "#,
    )
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(row_to_library).collect())
}

pub async fn get_library(db: &SqlitePool, id: &str) -> anyhow::Result<Option<Library>> {
    let row = sqlx::query(
        r#"
        SELECT id, name, calibre_db_path, created_at, updated_at
        FROM libraries
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(row_to_library))
}

pub async fn create_library(
    db: &SqlitePool,
    name: &str,
    calibre_db_path: &str,
) -> anyhow::Result<Library> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO libraries (id, name, calibre_db_path, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(name.trim())
    .bind(calibre_db_path.trim())
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;

    get_library(db, &id)
        .await?
        .context("created library not found")
}

pub async fn delete_library(db: &SqlitePool, id: &str) -> anyhow::Result<bool> {
    if id == "default" {
        anyhow::bail!("default library cannot be deleted");
    }

    let books = count_books_in_library(db, id).await?;
    if books > 0 {
        anyhow::bail!("library has books assigned");
    }

    let result = sqlx::query(
        r#"
        DELETE FROM libraries
        WHERE id = ?
        "#,
    )
    .bind(id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn count_books_in_library(db: &SqlitePool, library_id: &str) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(1)
        FROM books
        WHERE library_id = ?
        "#,
    )
    .bind(library_id)
    .fetch_one(db)
    .await?;

    Ok(count)
}

pub async fn sync_default_library_path(
    db: &SqlitePool,
    calibre_db_path: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO libraries (id, name, calibre_db_path, created_at, updated_at)
        VALUES ('default', 'Default Library', ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            calibre_db_path = excluded.calibre_db_path,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(calibre_db_path.trim())
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

fn row_to_library(row: sqlx::sqlite::SqliteRow) -> Library {
    Library {
        id: row.get("id"),
        name: row.get("name"),
        calibre_db_path: row.get("calibre_db_path"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}
