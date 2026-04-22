use crate::db::models::{
    AuthorRef, Book, FormatRef, Identifier, KoboDevice, KoboReadingState, SeriesRef, TagRef,
};
use anyhow::Context;
use chrono::Utc;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use std::collections::BTreeMap;
use uuid::Uuid;

#[derive(Clone, Debug, serde::Serialize)]
pub struct KoboDeviceListItem {
    pub id: String,
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub device_id: String,
    pub device_name: String,
    pub last_sync_at: Option<String>,
    pub created_at: String,
}

pub async fn find_device_by_id(
    db: &SqlitePool,
    device_row_id: &str,
) -> anyhow::Result<Option<KoboDevice>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, device_id, device_name, sync_token, last_sync_at, created_at
        FROM kobo_devices
        WHERE id = ?
        "#,
    )
    .bind(device_row_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(row_to_device))
}

pub async fn find_device_by_device_id(
    db: &SqlitePool,
    device_id: &str,
) -> anyhow::Result<Option<KoboDevice>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, device_id, device_name, sync_token, last_sync_at, created_at
        FROM kobo_devices
        WHERE device_id = ?
        "#,
    )
    .bind(device_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(row_to_device))
}

pub async fn list_devices(db: &SqlitePool) -> anyhow::Result<Vec<KoboDeviceListItem>> {
    let rows = sqlx::query(
        r#"
        SELECT
            d.id AS id,
            d.user_id AS user_id,
            u.username AS username,
            u.email AS email,
            d.device_id AS device_id,
            d.device_name AS device_name,
            d.last_sync_at AS last_sync_at,
            d.created_at AS created_at
        FROM kobo_devices d
        INNER JOIN users u ON u.id = d.user_id
        ORDER BY d.created_at DESC, d.id DESC
        "#,
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| KoboDeviceListItem {
            id: row.get("id"),
            user_id: row.get("user_id"),
            username: row.get("username"),
            email: row.get("email"),
            device_id: row.get("device_id"),
            device_name: row.get("device_name"),
            last_sync_at: row.get("last_sync_at"),
            created_at: row.get("created_at"),
        })
        .collect())
}

pub async fn upsert_device(
    db: &SqlitePool,
    user_id: &str,
    device_id: &str,
    device_name: &str,
) -> anyhow::Result<KoboDevice> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO kobo_devices (id, user_id, device_id, device_name, sync_token, last_sync_at, created_at)
        VALUES (?, ?, ?, ?, NULL, NULL, ?)
        ON CONFLICT(device_id) DO UPDATE SET
            user_id = excluded.user_id,
            device_name = excluded.device_name,
            sync_token = CASE
                WHEN kobo_devices.user_id != excluded.user_id THEN NULL
                ELSE kobo_devices.sync_token
            END,
            last_sync_at = CASE
                WHEN kobo_devices.user_id != excluded.user_id THEN NULL
                ELSE kobo_devices.last_sync_at
            END
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(device_id)
    .bind(device_name.trim())
    .bind(&now)
    .execute(db)
    .await?;

    find_device_by_device_id(db, device_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("kobo device missing after upsert"))
}

pub async fn revoke_device(db: &SqlitePool, device_row_id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM kobo_devices
        WHERE id = ?
        "#,
    )
    .bind(device_row_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn update_device_sync_token(
    db: &SqlitePool,
    device_row_id: &str,
    sync_token: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE kobo_devices
        SET sync_token = ?, last_sync_at = ?
        WHERE id = ?
        "#,
    )
    .bind(sync_token)
    .bind(&now)
    .bind(device_row_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn upsert_reading_state(
    db: &SqlitePool,
    device_row_id: &str,
    book_id: &str,
    kobo_position: Option<&str>,
    percent_read: Option<f64>,
    last_modified: &str,
) -> anyhow::Result<KoboReadingState> {
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO kobo_reading_state (
            id, device_id, book_id, kobo_position, percent_read, last_modified
        ) VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(device_id, book_id) DO UPDATE SET
            kobo_position = excluded.kobo_position,
            percent_read = excluded.percent_read,
            last_modified = excluded.last_modified
        "#,
    )
    .bind(&id)
    .bind(device_row_id)
    .bind(book_id)
    .bind(kobo_position)
    .bind(percent_read)
    .bind(last_modified)
    .execute(db)
    .await?;

    let row = sqlx::query(
        r#"
        SELECT id, device_id, book_id, kobo_position, percent_read, last_modified
        FROM kobo_reading_state
        WHERE device_id = ? AND book_id = ?
        "#,
    )
    .bind(device_row_id)
    .bind(book_id)
    .fetch_one(db)
    .await?;

    Ok(KoboReadingState {
        id: row.get("id"),
        device_id: row.get("device_id"),
        book_id: row.get("book_id"),
        kobo_position: row.get("kobo_position"),
        percent_read: row.get("percent_read"),
        last_modified: row.get("last_modified"),
    })
}

pub async fn list_kobo_books_since(
    db: &SqlitePool,
    since: Option<&str>,
    page: i64,
    page_size: i64,
    library_id: Option<&str>,
) -> anyhow::Result<(Vec<Book>, i64)> {
    let page = if page < 1 { 1 } else { page };
    let page_size = match page_size {
        n if n < 1 => 30,
        n if n > 100 => 100,
        n => n,
    };
    let offset = (page - 1) * page_size;
    let since = since
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    let mut total_query =
        QueryBuilder::<Sqlite>::new("SELECT COUNT(DISTINCT b.id) AS total FROM books b");
    let mut where_added = false;
    if let Some(ref library_id) = library_id {
        total_query.push(" WHERE b.library_id = ");
        total_query.push_bind(library_id);
        where_added = true;
    }
    if let Some(ref since) = since {
        if where_added {
            total_query.push(" AND ");
        } else {
            total_query.push(" WHERE ");
        }
        total_query.push("b.last_modified > ");
        total_query.push_bind(since);
    }
    let total: i64 = total_query
        .build_query_scalar()
        .fetch_one(db)
        .await
        .context("count kobo books")?;

    let mut data_query = QueryBuilder::<Sqlite>::new(
        r#"
        WITH paged_books AS (
            SELECT
                b.id AS id,
                b.title AS title,
                b.sort_title AS sort_title,
                b.description AS description,
                b.pubdate AS pubdate,
                b.language AS language,
                b.rating AS rating,
                b.document_type AS document_type,
                b.series_index AS series_index,
                b.has_cover AS has_cover,
                b.cover_path AS cover_path,
                b.created_at AS created_at,
                b.last_modified AS last_modified,
                b.indexed_at AS indexed_at,
                s.id AS series_id,
                s.name AS series_name
            FROM books b
            LEFT JOIN series s ON s.id = b.series_id
        "#,
    );
    let mut where_added = false;
    if let Some(ref library_id) = library_id {
        data_query.push(" WHERE b.library_id = ");
        data_query.push_bind(library_id);
        where_added = true;
    }
    if let Some(ref since) = since {
        if where_added {
            data_query.push(" AND ");
        } else {
            data_query.push(" WHERE ");
        }
        data_query.push("b.last_modified > ");
        data_query.push_bind(since);
    }
    data_query.push(
        r#"
            ORDER BY b.last_modified ASC, b.id ASC
            LIMIT "#,
    );
    data_query.push_bind(page_size);
    data_query.push(" OFFSET ");
    data_query.push_bind(offset);
    data_query.push(
        r#"
        )
        SELECT
            pb.id AS id,
            pb.title AS title,
            pb.sort_title AS sort_title,
            pb.description AS description,
            pb.pubdate AS pubdate,
            pb.language AS language,
            pb.rating AS rating,
            pb.document_type AS document_type,
            pb.series_index AS series_index,
            pb.has_cover AS has_cover,
            pb.cover_path AS cover_path,
            pb.created_at AS created_at,
            pb.last_modified AS last_modified,
            pb.indexed_at AS indexed_at,
            pb.series_id AS series_id,
            pb.series_name AS series_name,
            ba.display_order AS author_display_order,
            a.id AS author_id,
            a.name AS author_name,
            a.sort_name AS author_sort_name,
            t.id AS tag_id,
            t.name AS tag_name,
            bt.confirmed AS tag_confirmed,
            f.id AS format_id,
            f.format AS format_format,
            f.size_bytes AS format_size_bytes,
            i.id AS identifier_id,
            i.id_type AS identifier_id_type,
            i.value AS identifier_value
        FROM paged_books pb
        LEFT JOIN book_authors ba ON ba.book_id = pb.id
        LEFT JOIN authors a ON a.id = ba.author_id
        LEFT JOIN book_tags bt ON bt.book_id = pb.id
        LEFT JOIN tags t ON t.id = bt.tag_id
        LEFT JOIN formats f ON f.book_id = pb.id
        LEFT JOIN identifiers i ON i.book_id = pb.id
        ORDER BY pb.last_modified ASC, pb.id ASC, ba.display_order ASC, a.sort_name ASC, t.name ASC, f.format ASC, i.id_type ASC, i.value ASC
        "#,
    );

    let rows = data_query
        .build()
        .fetch_all(db)
        .await
        .context("list kobo books")?;

    let mut books: Vec<KoboBookAggregate> = Vec::new();
    let mut index_by_id: BTreeMap<String, usize> = BTreeMap::new();
    for row in rows {
        let book_id: String = row.get("id");
        let has_cover = row.get::<i64, _>("has_cover") != 0;
        let cover_path: Option<String> = row.get("cover_path");

        let entry_index = if let Some(index) = index_by_id.get(&book_id).copied() {
            index
        } else {
            let index = books.len();
            books.push(KoboBookAggregate {
                book: Book {
                    id: book_id.clone(),
                    title: row.get("title"),
                    sort_title: row.get("sort_title"),
                    description: row.get("description"),
                    pubdate: row.get("pubdate"),
                    language: row.get("language"),
                    rating: row.get("rating"),
                    document_type: row.get("document_type"),
                    series: row
                        .get::<Option<String>, _>("series_id")
                        .map(|id| SeriesRef {
                            id,
                            name: row.get("series_name"),
                        }),
                    series_index: row.get("series_index"),
                    authors: Vec::new(),
                    tags: Vec::new(),
                    formats: Vec::new(),
                    cover_url: to_cover_url(&book_id, has_cover, cover_path.as_deref()),
                    has_cover,
                    is_read: false,
                    is_archived: false,
                    identifiers: Vec::new(),
                    created_at: row.get("created_at"),
                    last_modified: row.get("last_modified"),
                    indexed_at: row.get("indexed_at"),
                },
                authors: BTreeMap::new(),
                tags: BTreeMap::new(),
                formats: BTreeMap::new(),
                identifiers: BTreeMap::new(),
            });
            index_by_id.insert(book_id.clone(), index);
            index
        };
        let entry = &mut books[entry_index];

        if let Some(author_id) = row.get::<Option<String>, _>("author_id") {
            let display_order = row
                .get::<Option<i64>, _>("author_display_order")
                .unwrap_or(i64::MAX);
            let author = AuthorRef {
                id: author_id.clone(),
                name: row.get("author_name"),
                sort_name: row.get("author_sort_name"),
            };
            entry
                .authors
                .entry(author_id)
                .and_modify(|existing| {
                    if display_order < existing.0 {
                        existing.0 = display_order;
                        existing.1 = author.clone();
                    }
                })
                .or_insert((display_order, author));
        }

        if let Some(tag_id) = row.get::<Option<String>, _>("tag_id") {
            entry.tags.entry(tag_id.clone()).or_insert(TagRef {
                id: tag_id,
                name: row.get("tag_name"),
                confirmed: row.get::<Option<i64>, _>("tag_confirmed").unwrap_or(0) != 0,
            });
        }

        if let Some(format_id) = row.get::<Option<String>, _>("format_id") {
            entry.formats.entry(format_id.clone()).or_insert(FormatRef {
                id: format_id,
                format: row.get("format_format"),
                size_bytes: row.get("format_size_bytes"),
            });
        }

        if let Some(identifier_id) = row.get::<Option<String>, _>("identifier_id") {
            entry
                .identifiers
                .entry(identifier_id.clone())
                .or_insert(Identifier {
                    id: identifier_id,
                    id_type: row.get("identifier_id_type"),
                    value: row.get("identifier_value"),
                });
        }
    }

    let mut ordered_books = Vec::with_capacity(books.len());
    for mut aggregate in books {
        let mut authors: Vec<(i64, AuthorRef)> = aggregate.authors.into_values().collect();
        authors.sort_by(|left, right| {
            left.0
                .cmp(&right.0)
                .then(left.1.sort_name.cmp(&right.1.sort_name))
                .then(left.1.id.cmp(&right.1.id))
        });
        aggregate.book.authors = authors.into_iter().map(|(_, author)| author).collect();
        aggregate.book.tags = aggregate.tags.into_values().collect();
        aggregate
            .book
            .tags
            .sort_by(|left, right| left.name.cmp(&right.name).then(left.id.cmp(&right.id)));
        aggregate.book.formats = aggregate.formats.into_values().collect();
        aggregate
            .book
            .formats
            .sort_by(|left, right| left.format.cmp(&right.format).then(left.id.cmp(&right.id)));
        aggregate.book.identifiers = aggregate.identifiers.into_values().collect();
        aggregate.book.identifiers.sort_by(|left, right| {
            left.id_type
                .cmp(&right.id_type)
                .then(left.id.cmp(&right.id))
        });
        ordered_books.push(aggregate.book);
    }

    Ok((ordered_books, total))
}

struct KoboBookAggregate {
    book: Book,
    authors: BTreeMap<String, (i64, AuthorRef)>,
    tags: BTreeMap<String, TagRef>,
    formats: BTreeMap<String, FormatRef>,
    identifiers: BTreeMap<String, Identifier>,
}

fn row_to_device(row: sqlx::sqlite::SqliteRow) -> KoboDevice {
    KoboDevice {
        id: row.get("id"),
        user_id: row.get("user_id"),
        device_id: row.get("device_id"),
        device_name: row.get("device_name"),
        sync_token: row.get("sync_token"),
        last_sync_at: row.get("last_sync_at"),
        created_at: row.get("created_at"),
    }
}

fn to_cover_url(book_id: &str, has_cover: bool, cover_path: Option<&str>) -> Option<String> {
    if has_cover || cover_path.is_some() {
        Some(format!("/api/v1/books/{book_id}/cover"))
    } else {
        None
    }
}
