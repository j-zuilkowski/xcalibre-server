//! Author CRUD, profile management, and merge queries.
//! Touches: `authors`, `author_profiles`, `book_authors`.
//!
//! `merge_authors` reassigns all `book_authors` rows from `source_id` to
//! `target_id` using a NOT EXISTS guard to skip books that already list the
//! target author, then deletes the source author and its profile.
//!
//! `upsert_author_profile` performs a read-modify-write: it loads the existing
//! profile first so that `None` patch fields keep their current values rather
//! than being overwritten with NULL.  Photo path is never cleared by this
//! function; use `set_author_photo_path` separately.

use crate::db::models::AuthorRef;
use crate::db::queries::books::BookSummary;
use anyhow::Context;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use utoipa::ToSchema;

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct AuthorProfile {
    pub bio: Option<String>,
    pub photo_url: Option<String>,
    pub born: Option<String>,
    pub died: Option<String>,
    pub website_url: Option<String>,
    pub openlibrary_id: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct AuthorDetail {
    pub id: String,
    pub name: String,
    pub sort_name: String,
    pub profile: Option<AuthorProfile>,
    pub book_count: i64,
    pub books: Vec<BookSummary>,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct AdminAuthor {
    pub id: String,
    pub name: String,
    pub sort_name: String,
    pub book_count: i64,
    pub has_profile: bool,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct MergeAuthorResponse {
    pub books_updated: usize,
    pub target_author: AuthorRef,
}

#[derive(Clone, Debug, Default)]
pub struct AuthorProfilePatchInput {
    pub bio: Option<Option<String>>,
    pub born: Option<Option<String>>,
    pub died: Option<Option<String>>,
    pub website_url: Option<Option<String>>,
    pub openlibrary_id: Option<Option<String>>,
}

fn clamp_page(page: i64) -> i64 {
    if page < 1 {
        1
    } else {
        page
    }
}

fn clamp_page_size(page_size: i64) -> i64 {
    match page_size {
        n if n < 1 => 20,
        n if n > 100 => 100,
        n => n,
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|candidate| {
        let trimmed = candidate.trim().to_string();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn photo_url_from_path(author_id: &str, photo_path: Option<String>) -> Option<String> {
    normalize_optional(photo_path).map(|_| format!("/api/v1/authors/{author_id}/photo"))
}

/// Returns the author row and its optional profile (LEFT JOIN on
/// `author_profiles`).  `profile` is `None` when no profile row exists.
pub async fn get_author_by_id(
    db: &SqlitePool,
    author_id: &str,
) -> anyhow::Result<Option<(AuthorRef, Option<AuthorProfile>)>> {
    let row = sqlx::query(
        r#"
        SELECT
            a.id AS id,
            a.name AS name,
            a.sort_name AS sort_name,
            ap.author_id AS profile_author_id,
            ap.bio AS bio,
            ap.photo_path AS photo_path,
            ap.born AS born,
            ap.died AS died,
            ap.website_url AS website_url,
            ap.openlibrary_id AS openlibrary_id
        FROM authors a
        LEFT JOIN author_profiles ap ON ap.author_id = a.id
        WHERE a.id = ?
        LIMIT 1
        "#,
    )
    .bind(author_id)
    .fetch_optional(db)
    .await
    .context("load author by id")?;

    Ok(row.map(|row| {
        let author = AuthorRef {
            id: row.get("id"),
            name: row.get("name"),
            sort_name: row.get("sort_name"),
        };
        let profile = row
            .get::<Option<String>, _>("profile_author_id")
            .map(|_| AuthorProfile {
                bio: row.get("bio"),
                photo_url: photo_url_from_path(author_id, row.get("photo_path")),
                born: row.get("born"),
                died: row.get("died"),
                website_url: row.get("website_url"),
                openlibrary_id: row.get("openlibrary_id"),
            });
        (author, profile)
    }))
}

pub async fn upsert_author_profile(
    db: &SqlitePool,
    author_id: &str,
    patch: &AuthorProfilePatchInput,
) -> anyhow::Result<()> {
    let existing = sqlx::query(
        r#"
        SELECT bio, photo_path, born, died, website_url, openlibrary_id
        FROM author_profiles
        WHERE author_id = ?
        LIMIT 1
        "#,
    )
    .bind(author_id)
    .fetch_optional(db)
    .await
    .context("load existing author profile")?;

    let existing_bio = existing
        .as_ref()
        .map(|row| row.get::<Option<String>, _>("bio"))
        .unwrap_or(None);
    let existing_photo_path = existing
        .as_ref()
        .map(|row| row.get::<Option<String>, _>("photo_path"))
        .unwrap_or(None);
    let existing_born = existing
        .as_ref()
        .map(|row| row.get::<Option<String>, _>("born"))
        .unwrap_or(None);
    let existing_died = existing
        .as_ref()
        .map(|row| row.get::<Option<String>, _>("died"))
        .unwrap_or(None);
    let existing_website_url = existing
        .as_ref()
        .map(|row| row.get::<Option<String>, _>("website_url"))
        .unwrap_or(None);
    let existing_openlibrary_id = existing
        .as_ref()
        .map(|row| row.get::<Option<String>, _>("openlibrary_id"))
        .unwrap_or(None);

    let bio = match patch.bio.as_ref() {
        Some(value) => value.clone(),
        None => existing_bio,
    };
    let born = match patch.born.as_ref() {
        Some(value) => value.clone(),
        None => existing_born,
    };
    let died = match patch.died.as_ref() {
        Some(value) => value.clone(),
        None => existing_died,
    };
    let website_url = match patch.website_url.as_ref() {
        Some(value) => value.clone(),
        None => existing_website_url,
    };
    let openlibrary_id = match patch.openlibrary_id.as_ref() {
        Some(value) => value.clone(),
        None => existing_openlibrary_id,
    };
    let updated_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO author_profiles (
            author_id, bio, photo_path, born, died, website_url, openlibrary_id, updated_at
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(author_id) DO UPDATE SET
            bio = excluded.bio,
            photo_path = excluded.photo_path,
            born = excluded.born,
            died = excluded.died,
            website_url = excluded.website_url,
            openlibrary_id = excluded.openlibrary_id,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(author_id)
    .bind(bio)
    .bind(existing_photo_path)
    .bind(born)
    .bind(died)
    .bind(website_url)
    .bind(openlibrary_id)
    .bind(updated_at)
    .execute(db)
    .await
    .context("upsert author profile")?;

    Ok(())
}

pub async fn set_author_photo_path(
    db: &SqlitePool,
    author_id: &str,
    photo_path: &str,
) -> anyhow::Result<()> {
    let updated_at = chrono::Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO author_profiles (author_id, photo_path, updated_at)
        VALUES (?, ?, ?)
        ON CONFLICT(author_id) DO UPDATE SET
            photo_path = excluded.photo_path,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(author_id)
    .bind(photo_path)
    .bind(updated_at)
    .execute(db)
    .await
    .context("set author photo path")?;

    Ok(())
}

pub async fn list_admin_authors(
    db: &SqlitePool,
    q: Option<&str>,
    page: i64,
    page_size: i64,
) -> anyhow::Result<(Vec<AdminAuthor>, i64, i64, i64)> {
    let page = clamp_page(page);
    let page_size = clamp_page_size(page_size);
    let offset = (page - 1) * page_size;
    let query = q
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let mut count_sql = String::from("SELECT COUNT(*) AS total FROM authors a");
    let mut data_sql = String::from(
        r#"
        SELECT
            a.id AS id,
            a.name AS name,
            a.sort_name AS sort_name,
            COUNT(DISTINCT ba.book_id) AS book_count,
            CASE WHEN ap.author_id IS NULL THEN 0 ELSE 1 END AS has_profile
        FROM authors a
        LEFT JOIN book_authors ba ON ba.author_id = a.id
        LEFT JOIN author_profiles ap ON ap.author_id = a.id
        "#,
    );

    if query.is_some() {
        let filter = " WHERE (lower(a.name) LIKE '%' || lower(?) || '%' OR lower(a.sort_name) LIKE '%' || lower(?) || '%')";
        count_sql.push_str(filter);
        data_sql.push_str(filter);
    }

    data_sql.push_str(
        " GROUP BY a.id, a.name, a.sort_name, ap.author_id ORDER BY a.sort_name ASC, a.name ASC, a.id ASC LIMIT ? OFFSET ?",
    );

    let total = if let Some(query) = query.as_deref() {
        sqlx::query_scalar::<_, i64>(&count_sql)
            .bind(query)
            .bind(query)
            .fetch_one(db)
            .await
            .context("count admin authors")?
    } else {
        sqlx::query_scalar::<_, i64>(&count_sql)
            .fetch_one(db)
            .await
            .context("count admin authors")?
    };

    let rows = if let Some(query) = query.as_deref() {
        sqlx::query(&data_sql)
            .bind(query)
            .bind(query)
            .bind(page_size)
            .bind(offset)
            .fetch_all(db)
            .await
            .context("list admin authors")?
    } else {
        sqlx::query(&data_sql)
            .bind(page_size)
            .bind(offset)
            .fetch_all(db)
            .await
            .context("list admin authors")?
    };

    let items = rows
        .into_iter()
        .map(|row| AdminAuthor {
            id: row.get("id"),
            name: row.get("name"),
            sort_name: row.get("sort_name"),
            book_count: row.get("book_count"),
            has_profile: row.get::<i64, _>("has_profile") != 0,
        })
        .collect();

    Ok((items, total, page, page_size))
}

/// Merges `source_id` into `target_id`.  Returns `None` if either author is
/// not found, or `Some(MergeAuthorResponse)` with the number of books
/// reassigned.  The source author row and its profile are deleted after
/// all book links are moved; books that already list the target author are
/// not duplicated (NOT EXISTS guard on the UPDATE).
pub async fn merge_authors(
    db: &SqlitePool,
    source_id: &str,
    target_id: &str,
) -> anyhow::Result<Option<MergeAuthorResponse>> {
    if source_id == target_id {
        anyhow::bail!("cannot merge an author into itself");
    }

    let mut tx = db.begin().await.context("begin author merge transaction")?;

    let source = sqlx::query(
        r#"
        SELECT id, name, sort_name
        FROM authors
        WHERE id = ?
        LIMIT 1
        "#,
    )
    .bind(source_id)
    .fetch_optional(&mut *tx)
    .await
    .context("load source author")?;
    let Some(_source) = source else {
        return Ok(None);
    };

    let target = sqlx::query(
        r#"
        SELECT id, name, sort_name
        FROM authors
        WHERE id = ?
        LIMIT 1
        "#,
    )
    .bind(target_id)
    .fetch_optional(&mut *tx)
    .await
    .context("load target author")?;
    let Some(target) = target else {
        return Ok(None);
    };

    let books_updated = sqlx::query(
        r#"
        UPDATE book_authors
        SET author_id = ?
        WHERE author_id = ?
          AND NOT EXISTS (
              SELECT 1
              FROM book_authors AS existing
              WHERE existing.book_id = book_authors.book_id
                AND existing.author_id = ?
          )
        "#,
    )
    .bind(target_id)
    .bind(source_id)
    .bind(target_id)
    .execute(&mut *tx)
    .await
    .context("reassign book authors")?
    .rows_affected() as usize;

    sqlx::query("DELETE FROM book_authors WHERE author_id = ?")
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .context("delete source author book links")?;

    sqlx::query("DELETE FROM author_profiles WHERE author_id = ?")
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .context("delete source author profile")?;

    sqlx::query("DELETE FROM authors WHERE id = ?")
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .context("delete source author")?;

    tx.commit()
        .await
        .context("commit author merge transaction")?;

    Ok(Some(MergeAuthorResponse {
        books_updated,
        target_author: AuthorRef {
            id: target.get("id"),
            name: target.get("name"),
            sort_name: target.get("sort_name"),
        },
    }))
}
