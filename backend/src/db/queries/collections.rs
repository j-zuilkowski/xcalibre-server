//! Collection (curated book set) CRUD and membership queries.
//! Touches: `collections`, `collection_books`, `book_chunks`.
//!
//! Access control is embedded in the SQL: mutating queries filter by
//! `owner_id = ? OR is_public = 1` so a single query handles both ownership
//! and public-write semantics.  `get_collection_detail` builds the response
//! by first loading the `CollectionSummary`, then fetching `BookSummary` rows
//! via `list_book_summaries_by_ids` which preserves insertion order.
//!
//! `total_chunks` in `CollectionSummary` is derived by counting `book_chunks`
//! rows joined through `collection_books`; it reflects how much of the
//! collection has been semantically indexed.

use crate::db::queries::books::BookSummary;
use anyhow::Context;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub domain: String,
    pub is_public: bool,
    pub book_count: i64,
    pub total_chunks: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, ToSchema)]
pub struct CollectionDetail {
    #[serde(flatten)]
    pub collection: CollectionSummary,
    pub books: Vec<BookSummary>,
}

#[derive(Clone, Debug)]
pub struct CollectionAccess {
    pub id: String,
    pub owner_id: String,
    pub is_public: bool,
}

#[derive(Clone, Debug)]
pub struct CollectionInput {
    pub name: String,
    pub description: Option<String>,
    pub domain: String,
    pub is_public: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CollectionInsertOutcome {
    pub inserted: usize,
    pub allowed: bool,
}

pub async fn list_collections(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<CollectionSummary>> {
    let rows = sqlx::query(
        r#"
        SELECT
            c.id AS id,
            c.name AS name,
            c.description AS description,
            c.domain AS domain,
            c.is_public AS is_public,
            COUNT(DISTINCT cb.book_id) AS book_count,
            COUNT(bc.id) AS total_chunks,
            c.created_at AS created_at,
            c.updated_at AS updated_at
        FROM collections c
        LEFT JOIN collection_books cb ON cb.collection_id = c.id
        LEFT JOIN book_chunks bc ON bc.book_id = cb.book_id
        WHERE c.owner_id = ? OR c.is_public = 1
        GROUP BY c.id, c.name, c.description, c.domain, c.is_public, c.created_at, c.updated_at
        ORDER BY c.created_at DESC, c.id DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    rows.into_iter().map(row_to_summary).collect()
}

pub async fn get_collection_access(
    db: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<Option<CollectionAccess>> {
    let row = sqlx::query(
        r#"
        SELECT id, owner_id, is_public
        FROM collections
        WHERE id = ?
        "#,
    )
    .bind(collection_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| CollectionAccess {
        id: row.get("id"),
        owner_id: row.get("owner_id"),
        is_public: row.get::<i64, _>("is_public") != 0,
    }))
}

pub async fn get_collection_summary(
    db: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<Option<CollectionSummary>> {
    let row = sqlx::query(
        r#"
        SELECT
            c.id AS id,
            c.name AS name,
            c.description AS description,
            c.domain AS domain,
            c.is_public AS is_public,
            COUNT(DISTINCT cb.book_id) AS book_count,
            COUNT(bc.id) AS total_chunks,
            c.created_at AS created_at,
            c.updated_at AS updated_at
        FROM collections c
        LEFT JOIN collection_books cb ON cb.collection_id = c.id
        LEFT JOIN book_chunks bc ON bc.book_id = cb.book_id
        WHERE c.id = ?
        GROUP BY c.id, c.name, c.description, c.domain, c.is_public, c.created_at, c.updated_at
        "#,
    )
    .bind(collection_id)
    .fetch_optional(db)
    .await?;

    row.map(row_to_summary).transpose()
}

pub async fn get_collection_detail(
    db: &SqlitePool,
    collection_id: &str,
    library_id: Option<&str>,
    user_id: Option<&str>,
) -> anyhow::Result<Option<CollectionDetail>> {
    let Some(summary) = get_collection_summary(db, collection_id).await? else {
        return Ok(None);
    };
    let book_ids = list_collection_book_ids(db, collection_id).await?;
    let books =
        crate::db::queries::books::list_book_summaries_by_ids(db, &book_ids, library_id, user_id)
            .await
            .context("load collection books")?;

    Ok(Some(CollectionDetail {
        collection: summary,
        books,
    }))
}

pub async fn get_collection_book_ids(
    db: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<Vec<String>> {
    list_collection_book_ids(db, collection_id).await
}

pub async fn create_collection(
    db: &SqlitePool,
    owner_id: &str,
    input: CollectionInput,
) -> anyhow::Result<CollectionSummary> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO collections (id, name, description, domain, owner_id, is_public, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(input.name.trim())
    .bind(input.description.map(|value| value.trim().to_string()))
    .bind(normalize_domain(&input.domain))
    .bind(owner_id)
    .bind(i64::from(input.is_public))
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;

    get_collection_summary(db, &id)
        .await?
        .context("created collection not found")
}

pub async fn update_collection(
    db: &SqlitePool,
    collection_id: &str,
    user_id: &str,
    name: Option<String>,
    description: Option<String>,
    domain: Option<String>,
    is_public: Option<bool>,
) -> anyhow::Result<Option<CollectionSummary>> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        UPDATE collections
        SET name = COALESCE(?, name),
            description = COALESCE(?, description),
            domain = COALESCE(?, domain),
            is_public = COALESCE(?, is_public),
            updated_at = ?
        WHERE id = ?
          AND (owner_id = ? OR is_public = 1)
        "#,
    )
    .bind(name)
    .bind(description)
    .bind(domain)
    .bind(is_public.map(i64::from))
    .bind(&now)
    .bind(collection_id)
    .bind(user_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    get_collection_summary(db, collection_id).await
}

pub async fn delete_collection(
    db: &SqlitePool,
    collection_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
    let result =
        sqlx::query("DELETE FROM collections WHERE id = ? AND (owner_id = ? OR is_public = 1)")
            .bind(collection_id)
            .bind(user_id)
            .execute(db)
            .await?;
    Ok(result.rows_affected() > 0)
}

async fn add_book_to_collection(
    db: &SqlitePool,
    collection_id: &str,
    user_id: &str,
    book_id: &str,
) -> anyhow::Result<CollectionInsertOutcome> {
    let now = Utc::now().to_rfc3339();
    let result = sqlx::query(
        r#"
        INSERT OR IGNORE INTO collection_books (collection_id, book_id, added_at)
        SELECT ?, ?, ?
        WHERE EXISTS (
            SELECT 1
            FROM collections
            WHERE id = ? AND (owner_id = ? OR is_public = 1)
        )
        "#,
    )
    .bind(collection_id)
    .bind(book_id)
    .bind(&now)
    .bind(collection_id)
    .bind(user_id)
    .execute(db)
    .await?;

    let inserted = result.rows_affected() as usize;
    let allowed = if inserted > 0 {
        true
    } else {
        get_collection_access(db, collection_id)
            .await?
            .map(|collection| collection.owner_id == user_id || collection.is_public)
            .unwrap_or(false)
    };

    Ok(CollectionInsertOutcome { inserted, allowed })
}

/// Adds each book in `book_ids` to the collection.  Returns the total number
/// of newly-inserted rows and whether the caller has access at all.  Skips
/// books already in the collection (`INSERT OR IGNORE`).
pub async fn add_books_to_collection(
    db: &SqlitePool,
    collection_id: &str,
    user_id: &str,
    book_ids: &[String],
) -> anyhow::Result<CollectionInsertOutcome> {
    if book_ids.is_empty() {
        return Ok(CollectionInsertOutcome::default());
    }

    let mut inserted = 0usize;
    let mut allowed = false;
    for book_id in book_ids {
        let outcome = add_book_to_collection(db, collection_id, user_id, book_id).await?;
        inserted += outcome.inserted;
        allowed |= outcome.allowed;
    }

    Ok(CollectionInsertOutcome { inserted, allowed })
}

pub async fn remove_book_from_collection(
    db: &SqlitePool,
    collection_id: &str,
    user_id: &str,
    book_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM collection_books
        WHERE collection_id = ? AND book_id = ?
          AND EXISTS (
              SELECT 1
              FROM collections
              WHERE id = ? AND (owner_id = ? OR is_public = 1)
          )
        "#,
    )
    .bind(collection_id)
    .bind(book_id)
    .bind(collection_id)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_collection_book_ids(
    db: &SqlitePool,
    collection_id: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT book_id
        FROM collection_books
        WHERE collection_id = ?
        ORDER BY added_at ASC, book_id ASC
        "#,
    )
    .bind(collection_id)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(|row| row.get("book_id")).collect())
}

fn row_to_summary(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<CollectionSummary> {
    Ok(CollectionSummary {
        id: row.get("id"),
        name: row.get("name"),
        description: row.get("description"),
        domain: row.get("domain"),
        is_public: row.get::<i64, _>("is_public") != 0,
        book_count: row.get("book_count"),
        total_chunks: row.get("total_chunks"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn normalize_domain(value: &str) -> String {
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "technical" | "electronics" | "culinary" | "legal" | "academic" | "narrative" => value,
        _ => "technical".to_string(),
    }
}
