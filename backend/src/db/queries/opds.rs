use anyhow::Context;
use sqlx::{Row, SqlitePool};

fn clamp_page(page: i64) -> i64 {
    if page < 1 {
        1
    } else {
        page
    }
}

fn clamp_page_size(page_size: i64) -> i64 {
    match page_size {
        n if n < 1 => 50,
        n if n > 100 => 100,
        n => n,
    }
}

pub async fn list_opds_authors(
    db: &SqlitePool,
    page: i64,
    page_size: i64,
) -> anyhow::Result<Vec<(String, String, i64)>> {
    let page = clamp_page(page);
    let page_size = clamp_page_size(page_size);
    let offset = (page - 1) * page_size;
    let rows = sqlx::query(
        r#"
        SELECT
            a.id AS author_id,
            a.name AS author_name,
            COUNT(DISTINCT ba.book_id) AS book_count
        FROM authors a
        INNER JOIN book_authors ba ON ba.author_id = a.id
        GROUP BY a.id, a.name, a.sort_name
        ORDER BY a.sort_name ASC, a.name ASC, a.id ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(db)
    .await
    .context("list opds authors")?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("author_id"),
                row.get::<String, _>("author_name"),
                row.get::<i64, _>("book_count"),
            )
        })
        .collect())
}

pub async fn list_opds_series(
    db: &SqlitePool,
    page: i64,
    page_size: i64,
) -> anyhow::Result<Vec<(String, String, i64)>> {
    let page = clamp_page(page);
    let page_size = clamp_page_size(page_size);
    let offset = (page - 1) * page_size;
    let rows = sqlx::query(
        r#"
        SELECT
            s.id AS series_id,
            s.name AS series_name,
            COUNT(DISTINCT b.id) AS book_count
        FROM series s
        INNER JOIN books b ON b.series_id = s.id
        GROUP BY s.id, s.name, s.sort_name
        ORDER BY s.sort_name ASC, s.name ASC, s.id ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(db)
    .await
    .context("list opds series")?;

    Ok(rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("series_id"),
                row.get::<String, _>("series_name"),
                row.get::<i64, _>("book_count"),
            )
        })
        .collect())
}

pub async fn list_opds_publishers(
    db: &SqlitePool,
    page: i64,
    page_size: i64,
) -> anyhow::Result<Vec<(String, String, i64)>> {
    let page = clamp_page(page);
    let page_size = clamp_page_size(page_size);
    let offset = (page - 1) * page_size;
    let rows = sqlx::query(
        r#"
        SELECT
            MIN(TRIM(json_extract(b.flags, '$.publisher'))) AS publisher_name,
            COUNT(DISTINCT b.id) AS book_count
        FROM books b
        WHERE json_extract(b.flags, '$.publisher') IS NOT NULL
          AND TRIM(json_extract(b.flags, '$.publisher')) <> ''
        GROUP BY lower(TRIM(json_extract(b.flags, '$.publisher')))
        ORDER BY publisher_name COLLATE NOCASE ASC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(page_size)
    .bind(offset)
    .fetch_all(db)
    .await
    .context("list opds publishers")?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let publisher_name: String = row.get("publisher_name");
            (
                publisher_name.clone(),
                publisher_name,
                row.get("book_count"),
            )
        })
        .collect())
}

pub async fn list_opds_languages(db: &SqlitePool) -> anyhow::Result<Vec<(String, i64)>> {
    let rows = sqlx::query(
        r#"
        SELECT lower(TRIM(language)) AS language_code, COUNT(*) AS book_count
        FROM books
        WHERE language IS NOT NULL
          AND TRIM(language) <> ''
        GROUP BY lower(TRIM(language))
        ORDER BY language_code ASC
        "#,
    )
    .fetch_all(db)
    .await
    .context("list opds languages")?;

    Ok(rows
        .into_iter()
        .map(|row| (row.get("language_code"), row.get("book_count")))
        .collect())
}

pub async fn list_opds_ratings(db: &SqlitePool) -> anyhow::Result<Vec<(i64, i64)>> {
    let rows = sqlx::query(
        r#"
        SELECT ((rating + 1) / 2) AS rating_bucket, COUNT(*) AS book_count
        FROM books
        WHERE rating IS NOT NULL
          AND rating BETWEEN 1 AND 10
        GROUP BY ((rating + 1) / 2)
        ORDER BY rating_bucket ASC
        "#,
    )
    .fetch_all(db)
    .await
    .context("list opds ratings")?;

    Ok(rows
        .into_iter()
        .map(|row| (row.get("rating_bucket"), row.get("book_count")))
        .collect())
}

pub async fn count_opds_authors(db: &SqlitePool) -> anyhow::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT a.id)
        FROM authors a
        INNER JOIN book_authors ba ON ba.author_id = a.id
        "#,
    )
    .fetch_one(db)
    .await
    .context("count opds authors")
}

pub async fn count_opds_series(db: &SqlitePool) -> anyhow::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT b.series_id)
        FROM books b
        WHERE b.series_id IS NOT NULL
        "#,
    )
    .fetch_one(db)
    .await
    .context("count opds series")
}

pub async fn count_opds_publishers(db: &SqlitePool) -> anyhow::Result<i64> {
    sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM (
            SELECT lower(TRIM(json_extract(b.flags, '$.publisher'))) AS publisher_key
            FROM books b
            WHERE json_extract(b.flags, '$.publisher') IS NOT NULL
              AND TRIM(json_extract(b.flags, '$.publisher')) <> ''
            GROUP BY lower(TRIM(json_extract(b.flags, '$.publisher')))
        ) publishers
        "#,
    )
    .fetch_one(db)
    .await
    .context("count opds publishers")
}
