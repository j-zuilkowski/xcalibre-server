use crate::db::models::Shelf;
use anyhow::Context;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct ShelfBookPage {
    pub items: Vec<crate::db::queries::books::BookSummary>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

pub async fn list_shelves(db: &SqlitePool, user_id: &str) -> anyhow::Result<Vec<Shelf>> {
    let rows = sqlx::query(
        r#"
        SELECT
            s.id AS id,
            s.name AS name,
            s.is_public AS is_public,
            COUNT(sb.book_id) AS book_count,
            s.created_at AS created_at,
            s.last_modified AS last_modified
        FROM shelves s
        LEFT JOIN shelf_books sb ON sb.shelf_id = s.id
        WHERE s.user_id = ? OR s.is_public = 1
        GROUP BY s.id, s.name, s.is_public, s.created_at, s.last_modified
        ORDER BY s.created_at DESC, s.id DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(row_to_shelf)
        .collect::<anyhow::Result<Vec<_>>>()
}

pub async fn get_shelf(db: &SqlitePool, shelf_id: &str) -> anyhow::Result<Option<ShelfRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, name, is_public, created_at, last_modified
        FROM shelves
        WHERE id = ?
        "#,
    )
    .bind(shelf_id)
    .fetch_optional(db)
    .await?;

    row.map(row_to_shelf_record).transpose()
}

pub async fn create_shelf(
    db: &SqlitePool,
    user_id: &str,
    name: &str,
    is_public: bool,
) -> anyhow::Result<Shelf> {
    let now = Utc::now().to_rfc3339();
    let shelf_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO shelves (id, user_id, name, is_public, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&shelf_id)
    .bind(user_id)
    .bind(name.trim())
    .bind(i64::from(is_public))
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;

    get_shelf(db, &shelf_id)
        .await?
        .map(|record| record.into_shelf())
        .context("created shelf not found")
}

pub async fn find_shelf_id_by_name(
    db: &SqlitePool,
    user_id: &str,
    name: &str,
) -> anyhow::Result<Option<String>> {
    let row = sqlx::query_scalar(
        r#"
        SELECT id
        FROM shelves
        WHERE user_id = ?
          AND lower(trim(name)) = lower(trim(?))
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(name)
    .fetch_optional(db)
    .await?;

    Ok(row)
}

pub async fn get_or_create_shelf_id(
    db: &SqlitePool,
    user_id: &str,
    name: &str,
) -> anyhow::Result<String> {
    if let Some(id) = find_shelf_id_by_name(db, user_id, name).await? {
        return Ok(id);
    }

    let shelf = create_shelf(db, user_id, name, false).await?;
    Ok(shelf.id)
}

pub async fn delete_shelf(db: &SqlitePool, shelf_id: &str, user_id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM shelves
        WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(shelf_id)
    .bind(user_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn add_book_to_shelf(
    db: &SqlitePool,
    shelf_id: &str,
    book_id: &str,
) -> anyhow::Result<bool> {
    let now = Utc::now().to_rfc3339();
    let next_order: i64 = sqlx::query(
        r#"
        SELECT COALESCE(MAX(display_order), -1) + 1 AS next_order
        FROM shelf_books
        WHERE shelf_id = ?
        "#,
    )
    .bind(shelf_id)
    .fetch_one(db)
    .await?
    .get("next_order");

    let result = sqlx::query(
        r#"
        INSERT OR IGNORE INTO shelf_books (shelf_id, book_id, display_order, added_at)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(shelf_id)
    .bind(book_id)
    .bind(next_order)
    .bind(&now)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn remove_book_from_shelf(
    db: &SqlitePool,
    shelf_id: &str,
    book_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM shelf_books
        WHERE shelf_id = ? AND book_id = ?
        "#,
    )
    .bind(shelf_id)
    .bind(book_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn list_shelf_books(
    db: &SqlitePool,
    shelf_id: &str,
    page: i64,
    page_size: i64,
    library_id: Option<&str>,
    user_id: Option<&str>,
) -> anyhow::Result<ShelfBookPage> {
    let page_size = page_size.clamp(1, 100);
    let page = page.max(1);
    let offset = (page - 1) * page_size;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM shelf_books WHERE shelf_id = ?
        "#,
    )
    .bind(shelf_id)
    .fetch_one(db)
    .await?;

    let rows = sqlx::query(
        r#"
        SELECT book_id
        FROM shelf_books
        WHERE shelf_id = ?
        ORDER BY display_order ASC, added_at ASC, book_id ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(shelf_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(db)
    .await?;

    let mut book_ids = Vec::with_capacity(rows.len());
    for row in rows {
        book_ids.push(row.get::<String, _>("book_id"));
    }

    let items =
        crate::db::queries::books::list_book_summaries_by_ids(db, &book_ids, library_id, user_id)
            .await?;

    Ok(ShelfBookPage {
        items,
        total,
        page,
        page_size,
    })
}

#[derive(Clone, Debug)]
pub struct ShelfRecord {
    pub id: String,
    pub user_id: String,
    pub name: String,
    pub is_public: bool,
    pub created_at: String,
    pub last_modified: String,
    pub book_count: i64,
}

impl ShelfRecord {
    pub fn into_shelf(self) -> Shelf {
        Shelf {
            id: self.id,
            name: self.name,
            is_public: self.is_public,
            book_count: self.book_count,
            created_at: self.created_at,
            last_modified: self.last_modified,
        }
    }
}

fn row_to_shelf(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<Shelf> {
    Ok(Shelf {
        id: row.get("id"),
        name: row.get("name"),
        is_public: row.get::<i64, _>("is_public") != 0,
        book_count: row.get("book_count"),
        created_at: row.get("created_at"),
        last_modified: row.get("last_modified"),
    })
}

fn row_to_shelf_record(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<ShelfRecord> {
    Ok(ShelfRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        name: row.get("name"),
        is_public: row.get::<i64, _>("is_public") != 0,
        created_at: row.get("created_at"),
        last_modified: row.get("last_modified"),
        book_count: 0,
    })
}
