use crate::db::models::{AuthorRef, Book, FormatRef, Identifier, SeriesRef, TagRef};
use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite, SqlitePool};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize)]
pub struct BookSummary {
    pub id: String,
    pub title: String,
    pub sort_title: String,
    pub authors: Vec<AuthorRef>,
    pub series: Option<SeriesRef>,
    pub series_index: Option<f64>,
    pub cover_url: Option<String>,
    pub has_cover: bool,
    pub language: Option<String>,
    pub rating: Option<i64>,
    pub last_modified: String,
}

#[derive(Clone, Debug, Default)]
pub struct BookListPage {
    pub items: Vec<BookSummary>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Clone, Debug, Default)]
pub struct ListBooksParams {
    pub q: Option<String>,
    pub author_id: Option<String>,
    pub series_id: Option<String>,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub format: Option<String>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub page: i64,
    pub page_size: i64,
    pub since: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct RolePermissions {
    pub role_id: String,
    pub role_name: String,
    pub can_upload: bool,
    pub can_edit: bool,
    pub can_download: bool,
}

#[derive(Clone, Debug, Default)]
pub struct FormatFileRecord {
    pub id: String,
    pub path: String,
    pub format: String,
    pub size_bytes: i64,
}

impl RolePermissions {
    pub fn is_admin(&self) -> bool {
        self.role_id.eq_ignore_ascii_case("admin") || self.role_name.eq_ignore_ascii_case("admin")
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct IdentifierInput {
    pub id_type: String,
    pub value: String,
}

#[derive(Clone, Debug, Default)]
pub struct UploadBookInput {
    pub title: String,
    pub sort_title: String,
    pub description: Option<String>,
    pub pubdate: Option<String>,
    pub language: Option<String>,
    pub rating: Option<i64>,
    pub series_id: Option<String>,
    pub series_index: Option<f64>,
    pub author_names: Vec<String>,
    pub identifiers: Vec<IdentifierInput>,
    pub format: String,
    pub format_path: String,
    pub format_size_bytes: i64,
}

#[derive(Clone, Debug, Default)]
pub struct PatchBookInput {
    pub title: Option<String>,
    pub sort_title: Option<String>,
    pub description: Option<Option<String>>,
    pub pubdate: Option<Option<String>>,
    pub language: Option<Option<String>>,
    pub rating: Option<i64>,
    pub series_id: Option<Option<String>>,
    pub series_index: Option<Option<f64>>,
    pub authors: Option<Vec<String>>,
    pub identifiers: Option<Vec<IdentifierInput>>,
}

pub async fn role_permissions_for_user(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Option<RolePermissions>> {
    let row = sqlx::query(
        r#"
        SELECT
            r.id AS role_id,
            r.name AS role_name,
            r.can_upload AS can_upload,
            r.can_edit AS can_edit,
            r.can_download AS can_download
        FROM users u
        INNER JOIN roles r ON r.id = u.role_id
        WHERE u.id = ?
        "#,
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| RolePermissions {
        role_id: row.get("role_id"),
        role_name: row.get("role_name"),
        can_upload: row.get::<i64, _>("can_upload") != 0,
        can_edit: row.get::<i64, _>("can_edit") != 0,
        can_download: row.get::<i64, _>("can_download") != 0,
    }))
}

pub async fn list_books(db: &SqlitePool, params: &ListBooksParams) -> anyhow::Result<BookListPage> {
    let page_size = clamp_page_size(params.page_size);
    let page = if params.page < 1 { 1 } else { params.page };
    let offset = (page - 1) * page_size;
    let fts_query = normalize_fts_query(params.q.as_deref());

    let mut total_query = QueryBuilder::<Sqlite>::new("SELECT COUNT(DISTINCT b.id) AS total FROM books b");
    if fts_query.is_some() {
        total_query.push(
            " INNER JOIN books_fts ON (books_fts.book_id = b.id OR books_fts.rowid = b.rowid)",
        );
    }
    apply_list_filters(&mut total_query, params, fts_query.as_deref());
    let total: i64 = total_query
        .build_query_scalar()
        .fetch_one(db)
        .await
        .context("count books")?;

    let (sort_column, sort_default) = normalize_sort(params.sort.as_deref());
    let order = normalize_order(params.order.as_deref(), sort_default);

    let mut data_query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            b.id AS id,
            b.title AS title,
            b.sort_title AS sort_title,
            b.series_index AS series_index,
            b.has_cover AS has_cover,
            b.cover_path AS cover_path,
            b.language AS language,
            b.rating AS rating,
            b.last_modified AS last_modified,
            s.id AS series_id,
            s.name AS series_name
        FROM books b
        "#,
    );
    if fts_query.is_some() {
        data_query.push(
            " INNER JOIN books_fts ON (books_fts.book_id = b.id OR books_fts.rowid = b.rowid)",
        );
    }
    data_query.push(" LEFT JOIN series s ON s.id = b.series_id");
    apply_list_filters(&mut data_query, params, fts_query.as_deref());
    data_query.push(" ORDER BY ");
    data_query.push(sort_column);
    data_query.push(" ");
    data_query.push(order);
    data_query.push(", b.id ASC LIMIT ");
    data_query.push_bind(page_size);
    data_query.push(" OFFSET ");
    data_query.push_bind(offset);

    let rows = data_query
        .build()
        .fetch_all(db)
        .await
        .context("list books")?;

    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let book_id: String = row.get("id");
        let has_cover = row.get::<i64, _>("has_cover") != 0;
        let cover_path: Option<String> = row.get("cover_path");

        items.push(BookSummary {
            id: book_id.clone(),
            title: row.get("title"),
            sort_title: row.get("sort_title"),
            authors: load_book_authors(db, &book_id).await?,
            series: row.get::<Option<String>, _>("series_id").map(|id| SeriesRef {
                id,
                name: row.get("series_name"),
            }),
            series_index: row.get("series_index"),
            cover_url: to_cover_url(&book_id, has_cover, cover_path.as_deref()),
            has_cover,
            language: row.get("language"),
            rating: row.get("rating"),
            last_modified: row.get("last_modified"),
        });
    }

    Ok(BookListPage {
        items,
        total,
        page,
        page_size,
    })
}

pub async fn list_book_summaries_by_ids(
    db: &SqlitePool,
    book_ids: &[String],
) -> anyhow::Result<Vec<BookSummary>> {
    if book_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            b.id AS id,
            b.title AS title,
            b.sort_title AS sort_title,
            b.series_index AS series_index,
            b.has_cover AS has_cover,
            b.cover_path AS cover_path,
            b.language AS language,
            b.rating AS rating,
            b.last_modified AS last_modified,
            s.id AS series_id,
            s.name AS series_name
        FROM books b
        LEFT JOIN series s ON s.id = b.series_id
        WHERE b.id IN (
        "#,
    );
    {
        let mut separated = query.separated(", ");
        for id in book_ids {
            separated.push_bind(id);
        }
    }
    query.push(")");

    let rows = query
        .build()
        .fetch_all(db)
        .await
        .context("list books by ids")?;

    let mut summaries_by_id = BTreeMap::new();
    for row in rows {
        let book_id: String = row.get("id");
        let has_cover = row.get::<i64, _>("has_cover") != 0;
        let cover_path: Option<String> = row.get("cover_path");

        summaries_by_id.insert(
            book_id.clone(),
            BookSummary {
                id: book_id.clone(),
                title: row.get("title"),
                sort_title: row.get("sort_title"),
                authors: load_book_authors(db, &book_id).await?,
                series: row.get::<Option<String>, _>("series_id").map(|id| SeriesRef {
                    id,
                    name: row.get("series_name"),
                }),
                series_index: row.get("series_index"),
                cover_url: to_cover_url(&book_id, has_cover, cover_path.as_deref()),
                has_cover,
                language: row.get("language"),
                rating: row.get("rating"),
                last_modified: row.get("last_modified"),
            },
        );
    }

    let mut ordered = Vec::with_capacity(book_ids.len());
    let mut seen = BTreeSet::new();
    for id in book_ids {
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(summary) = summaries_by_id.remove(id) {
            ordered.push(summary);
        }
    }

    Ok(ordered)
}

pub async fn get_book_by_id(db: &SqlitePool, book_id: &str) -> anyhow::Result<Option<Book>> {
    let row = sqlx::query(
        r#"
        SELECT
            b.id AS id,
            b.title AS title,
            b.sort_title AS sort_title,
            b.description AS description,
            b.pubdate AS pubdate,
            b.language AS language,
            b.rating AS rating,
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
        WHERE b.id = ?
        "#,
    )
    .bind(book_id)
    .fetch_optional(db)
    .await?;

    let Some(row) = row else {
        return Ok(None);
    };

    let has_cover = row.get::<i64, _>("has_cover") != 0;
    let cover_path: Option<String> = row.get("cover_path");

    Ok(Some(Book {
        id: row.get("id"),
        title: row.get("title"),
        sort_title: row.get("sort_title"),
        description: row.get("description"),
        pubdate: row.get("pubdate"),
        language: row.get("language"),
        rating: row.get("rating"),
        series: row.get::<Option<String>, _>("series_id").map(|id| SeriesRef {
            id,
            name: row.get("series_name"),
        }),
        series_index: row.get("series_index"),
        authors: load_book_authors(db, book_id).await?,
        tags: load_book_tags(db, book_id).await?,
        formats: load_book_formats(db, book_id).await?,
        cover_url: to_cover_url(book_id, has_cover, cover_path.as_deref()),
        has_cover,
        identifiers: load_book_identifiers(db, book_id).await?,
        created_at: row.get("created_at"),
        last_modified: row.get("last_modified"),
        indexed_at: row.get("indexed_at"),
    }))
}

pub async fn has_duplicate_isbn(
    db: &SqlitePool,
    identifiers: &[IdentifierInput],
    exclude_book_id: Option<&str>,
) -> anyhow::Result<bool> {
    let candidate_values: BTreeSet<String> = identifiers
        .iter()
        .filter_map(|id| normalize_isbn_candidate(&id.id_type, &id.value))
        .collect();

    if candidate_values.is_empty() {
        return Ok(false);
    }

    let mut sql = String::from(
        "SELECT value FROM identifiers WHERE lower(id_type) IN ('isbn', 'isbn10', 'isbn13')",
    );
    if exclude_book_id.is_some() {
        sql.push_str(" AND book_id <> ?");
    }

    let rows = if let Some(exclude) = exclude_book_id {
        sqlx::query(&sql).bind(exclude).fetch_all(db).await?
    } else {
        sqlx::query(&sql).fetch_all(db).await?
    };

    for row in rows {
        let value: String = row.get("value");
        if let Some(normalized) = normalize_isbn_value(&value) {
            if candidate_values.contains(&normalized) {
                return Ok(true);
            }
        }
    }

    Ok(false)
}

pub async fn insert_uploaded_book(db: &SqlitePool, input: UploadBookInput) -> anyhow::Result<Book> {
    let now = Utc::now().to_rfc3339();
    let book_id = Uuid::new_v4().to_string();
    let mut tx = db.begin().await?;

    sqlx::query(
        r#"
        INSERT INTO books (
            id, title, sort_title, description, pubdate, language, rating, series_id, series_index,
            has_cover, cover_path, flags, indexed_at, created_at, last_modified
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, NULL, NULL, NULL, ?, ?)
        "#,
    )
    .bind(&book_id)
    .bind(input.title.trim())
    .bind(input.sort_title.trim())
    .bind(optional_trimmed(input.description))
    .bind(optional_trimmed(input.pubdate))
    .bind(optional_trimmed(input.language))
    .bind(input.rating)
    .bind(optional_trimmed(input.series_id))
    .bind(input.series_index)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let authors = normalize_author_names(input.author_names);
    for (display_order, author_name) in authors.into_iter().enumerate() {
        let author_id = get_or_create_author(&mut tx, &author_name, &now).await?;
        sqlx::query(
            "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, ?)",
        )
        .bind(&book_id)
        .bind(author_id)
        .bind(display_order as i64)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(&book_id)
    .bind(input.format.trim().to_uppercase())
    .bind(input.format_path.trim())
    .bind(input.format_size_bytes)
    .bind(&now)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    let mut seen_id_types = BTreeSet::new();
    for id in input.identifiers {
        let id_type = id.id_type.trim().to_lowercase();
        let value = id.value.trim().to_string();
        if id_type.is_empty() || value.is_empty() || !seen_id_types.insert(id_type.clone()) {
            continue;
        }
        sqlx::query(
            r#"
            INSERT INTO identifiers (id, book_id, id_type, value, last_modified)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&book_id)
        .bind(id_type)
        .bind(value)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    get_book_by_id(db, &book_id)
        .await?
        .context("uploaded book missing after commit")
}

pub async fn patch_book_with_audit(
    db: &SqlitePool,
    book_id: &str,
    actor_user_id: &str,
    patch: PatchBookInput,
) -> anyhow::Result<Option<Book>> {
    let existing = get_book_by_id(db, book_id).await?;
    let Some(existing) = existing else {
        return Ok(None);
    };

    if let Some(rating) = patch.rating {
        if !(0..=10).contains(&rating) {
            anyhow::bail!("rating must be 0..=10");
        }
    }

    if let Some(ids) = patch.identifiers.as_ref() {
        if has_duplicate_isbn(db, ids, Some(book_id)).await? {
            anyhow::bail!("duplicate_isbn");
        }
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = db.begin().await?;
    let mut changes: Vec<(String, serde_json::Value, serde_json::Value)> = Vec::new();

    let mut next_title = existing.title.clone();
    if let Some(title) = patch.title {
        let trimmed = title.trim().to_string();
        if !trimmed.is_empty() && trimmed != existing.title {
            changes.push(("title".to_string(), json!(existing.title), json!(trimmed)));
            next_title = trimmed;
        }
    }

    let mut next_sort_title = existing.sort_title.clone();
    if let Some(sort_title) = patch.sort_title {
        let trimmed = sort_title.trim().to_string();
        if !trimmed.is_empty() && trimmed != existing.sort_title {
            changes.push((
                "sort_title".to_string(),
                json!(existing.sort_title),
                json!(trimmed),
            ));
            next_sort_title = trimmed;
        }
    } else if next_title != existing.title && existing.sort_title == existing.title {
        next_sort_title = next_title.clone();
        changes.push((
            "sort_title".to_string(),
            json!(existing.sort_title),
            json!(next_sort_title),
        ));
    }

    let mut next_description = existing.description.clone();
    if let Some(description) = patch.description {
        let description = description.and_then(non_empty_option);
        if description != existing.description {
            changes.push((
                "description".to_string(),
                json!(existing.description),
                json!(description),
            ));
            next_description = description;
        }
    }

    let mut next_pubdate = existing.pubdate.clone();
    if let Some(pubdate) = patch.pubdate {
        let pubdate = pubdate.and_then(non_empty_option);
        if pubdate != existing.pubdate {
            changes.push(("pubdate".to_string(), json!(existing.pubdate), json!(pubdate)));
            next_pubdate = pubdate;
        }
    }

    let mut next_language = existing.language.clone();
    if let Some(language) = patch.language {
        let language = language.and_then(non_empty_option);
        if language != existing.language {
            changes.push((
                "language".to_string(),
                json!(existing.language),
                json!(language),
            ));
            next_language = language;
        }
    }

    let mut next_rating = existing.rating;
    if let Some(rating) = patch.rating {
        if Some(rating) != existing.rating {
            changes.push(("rating".to_string(), json!(existing.rating), json!(rating)));
            next_rating = Some(rating);
        }
    }

    let mut next_series_id = existing.series.as_ref().map(|s| s.id.clone());
    if let Some(series_id) = patch.series_id {
        let normalized = series_id.and_then(non_empty_option);
        if normalized != next_series_id {
            changes.push((
                "series_id".to_string(),
                json!(next_series_id),
                json!(normalized),
            ));
            next_series_id = normalized;
        }
    }

    let mut next_series_index = existing.series_index;
    if let Some(series_index) = patch.series_index {
        if series_index != next_series_index {
            changes.push((
                "series_index".to_string(),
                json!(next_series_index),
                json!(series_index),
            ));
            next_series_index = series_index;
        }
    }

    let mut authors_changed = false;
    if let Some(author_ids) = patch.authors {
        let old_ids: Vec<String> = existing.authors.iter().map(|a| a.id.clone()).collect();
        let next_ids = dedupe_non_empty(author_ids);
        if old_ids != next_ids {
            changes.push(("authors".to_string(), json!(old_ids), json!(next_ids)));
            authors_changed = true;
            sqlx::query("DELETE FROM book_authors WHERE book_id = ?")
                .bind(book_id)
                .execute(&mut *tx)
                .await?;
            for (display_order, author_id) in next_ids.into_iter().enumerate() {
                sqlx::query(
                    "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, ?)",
                )
                .bind(book_id)
                .bind(author_id)
                .bind(display_order as i64)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    let mut identifiers_changed = false;
    if let Some(identifiers) = patch.identifiers {
        let old_ids: BTreeMap<String, String> = existing
            .identifiers
            .iter()
            .map(|id| (id.id_type.clone(), id.value.clone()))
            .collect();

        let mut next_ids: BTreeMap<String, String> = BTreeMap::new();
        for id in identifiers {
            let id_type = id.id_type.trim().to_lowercase();
            let value = id.value.trim().to_string();
            if !id_type.is_empty() && !value.is_empty() {
                next_ids.insert(id_type, value);
            }
        }

        if old_ids != next_ids {
            changes.push((
                "identifiers".to_string(),
                json!(old_ids),
                json!(next_ids.clone()),
            ));
            identifiers_changed = true;

            sqlx::query("DELETE FROM identifiers WHERE book_id = ?")
                .bind(book_id)
                .execute(&mut *tx)
                .await?;

            for (id_type, value) in next_ids {
                sqlx::query(
                    r#"
                    INSERT INTO identifiers (id, book_id, id_type, value, last_modified)
                    VALUES (?, ?, ?, ?, ?)
                    "#,
                )
                .bind(Uuid::new_v4().to_string())
                .bind(book_id)
                .bind(id_type)
                .bind(value)
                .bind(&now)
                .execute(&mut *tx)
                .await?;
            }
        }
    }

    if !changes.is_empty() || authors_changed || identifiers_changed {
        sqlx::query(
            r#"
            UPDATE books
            SET
                title = ?,
                sort_title = ?,
                description = ?,
                pubdate = ?,
                language = ?,
                rating = ?,
                series_id = ?,
                series_index = ?,
                last_modified = ?
            WHERE id = ?
            "#,
        )
        .bind(&next_title)
        .bind(&next_sort_title)
        .bind(next_description)
        .bind(next_pubdate)
        .bind(next_language)
        .bind(next_rating)
        .bind(next_series_id)
        .bind(next_series_index)
        .bind(&now)
        .bind(book_id)
        .execute(&mut *tx)
        .await?;

        for (field, old_value, new_value) in &changes {
            let diff = json!({
                "field": field,
                "old": old_value,
                "new": new_value,
            })
            .to_string();

            sqlx::query(
                r#"
                INSERT INTO audit_log (id, user_id, action, entity, entity_id, diff_json, created_at)
                VALUES (?, ?, 'update', 'book', ?, ?, ?)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(actor_user_id)
            .bind(book_id)
            .bind(diff)
            .bind(&now)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;
    get_book_by_id(db, book_id).await
}

pub async fn delete_book_and_collect_paths(
    db: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<Vec<String>>> {
    let rows = sqlx::query("SELECT path FROM formats WHERE book_id = ?")
        .bind(book_id)
        .fetch_all(db)
        .await?;
    if rows.is_empty() {
        let exists = sqlx::query("SELECT id FROM books WHERE id = ?")
            .bind(book_id)
            .fetch_optional(db)
            .await?;
        if exists.is_none() {
            return Ok(None);
        }
    }

    let format_paths = rows
        .into_iter()
        .map(|row| row.get::<String, _>("path"))
        .collect::<Vec<_>>();

    let mut tx = db.begin().await?;
    let deleted = sqlx::query("DELETE FROM books WHERE id = ?")
        .bind(book_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        tx.rollback().await?;
        return Ok(None);
    }
    tx.commit().await?;

    Ok(Some(format_paths))
}

pub async fn find_format_file(
    db: &SqlitePool,
    book_id: &str,
    format: &str,
) -> anyhow::Result<Option<FormatFileRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id, path, format, size_bytes
        FROM formats
        WHERE book_id = ?
          AND upper(format) = upper(?)
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .bind(format)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| FormatFileRecord {
        id: row.get("id"),
        path: row.get("path"),
        format: row.get("format"),
        size_bytes: row.get("size_bytes"),
    }))
}

pub async fn find_book_cover_path(db: &SqlitePool, book_id: &str) -> anyhow::Result<Option<String>> {
    let row = sqlx::query(
        r#"
        SELECT cover_path
        FROM books
        WHERE id = ?
          AND has_cover = 1
          AND cover_path IS NOT NULL
        LIMIT 1
        "#,
    )
    .bind(book_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| row.get("cover_path")))
}

pub async fn set_book_cover_path(
    db: &SqlitePool,
    book_id: &str,
    cover_path: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE books
        SET has_cover = 1,
            cover_path = ?,
            last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(cover_path)
    .bind(now)
    .bind(book_id)
    .execute(db)
    .await?;
    Ok(())
}

async fn load_book_authors(db: &SqlitePool, book_id: &str) -> anyhow::Result<Vec<AuthorRef>> {
    let rows = sqlx::query(
        r#"
        SELECT a.id, a.name, a.sort_name
        FROM book_authors ba
        INNER JOIN authors a ON a.id = ba.author_id
        WHERE ba.book_id = ?
        ORDER BY ba.display_order ASC, a.sort_name ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| AuthorRef {
            id: row.get("id"),
            name: row.get("name"),
            sort_name: row.get("sort_name"),
        })
        .collect())
}

async fn load_book_tags(db: &SqlitePool, book_id: &str) -> anyhow::Result<Vec<TagRef>> {
    let rows = sqlx::query(
        r#"
        SELECT t.id, t.name, bt.confirmed
        FROM book_tags bt
        INNER JOIN tags t ON t.id = bt.tag_id
        WHERE bt.book_id = ?
        ORDER BY t.name ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TagRef {
            id: row.get("id"),
            name: row.get("name"),
            confirmed: row.get::<i64, _>("confirmed") != 0,
        })
        .collect())
}

async fn load_book_formats(db: &SqlitePool, book_id: &str) -> anyhow::Result<Vec<FormatRef>> {
    let rows = sqlx::query(
        r#"
        SELECT id, format, size_bytes
        FROM formats
        WHERE book_id = ?
        ORDER BY format ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| FormatRef {
            id: row.get("id"),
            format: row.get("format"),
            size_bytes: row.get("size_bytes"),
        })
        .collect())
}

async fn load_book_identifiers(db: &SqlitePool, book_id: &str) -> anyhow::Result<Vec<Identifier>> {
    let rows = sqlx::query(
        r#"
        SELECT id, id_type, value
        FROM identifiers
        WHERE book_id = ?
        ORDER BY id_type ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| Identifier {
            id: row.get("id"),
            id_type: row.get("id_type"),
            value: row.get("value"),
        })
        .collect())
}

fn apply_list_filters(
    qb: &mut QueryBuilder<'_, Sqlite>,
    params: &ListBooksParams,
    fts_query: Option<&str>,
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

    if let Some(fts_query) = fts_query {
        and_where(qb);
        qb.push("books_fts MATCH ");
        qb.push_bind(fts_query.to_string());
    }

    if let Some(author_id) = params
        .author_id
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push(
            "EXISTS (SELECT 1 FROM book_authors ba WHERE ba.book_id = b.id AND ba.author_id = ",
        );
        qb.push_bind(author_id);
        qb.push(")");
    }

    if let Some(series_id) = params
        .series_id
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push("b.series_id = ");
        qb.push_bind(series_id);
    }

    if !params.tags.is_empty() {
        and_where(qb);
        qb.push(
            "EXISTS (SELECT 1 FROM book_tags bt INNER JOIN tags t ON t.id = bt.tag_id WHERE bt.book_id = b.id AND lower(t.name) IN (",
        );
        let mut separated = qb.separated(", ");
        for tag in params.tags.iter().map(|t| t.to_lowercase()) {
            separated.push_bind(tag);
        }
        qb.push("))");
    }

    if let Some(language) = params
        .language
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push("lower(b.language) = ");
        qb.push_bind(language.to_lowercase());
    }

    if let Some(format) = params
        .format
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push(
            "EXISTS (SELECT 1 FROM formats f WHERE f.book_id = b.id AND upper(f.format) = ",
        );
        qb.push_bind(format.to_uppercase());
        qb.push(")");
    }

    if let Some(since) = params
        .since
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push("b.last_modified > ");
        qb.push_bind(since);
    }
}

fn clamp_page_size(page_size: i64) -> i64 {
    match page_size {
        n if n < 1 => 30,
        n if n > 100 => 100,
        n => n,
    }
}

fn normalize_sort(sort: Option<&str>) -> (&'static str, &'static str) {
    match sort.unwrap_or_default().trim().to_lowercase().as_str() {
        "title" => ("b.sort_title", "ASC"),
        "author" => (
            "(SELECT MIN(a.sort_name) FROM book_authors ba INNER JOIN authors a ON a.id = ba.author_id WHERE ba.book_id = b.id)",
            "ASC",
        ),
        "pubdate" => ("b.pubdate", "DESC"),
        "added" => ("b.created_at", "DESC"),
        "rating" => ("b.rating", "DESC"),
        _ => ("b.sort_title", "ASC"),
    }
}

fn normalize_order(order: Option<&str>, default_order: &'static str) -> &'static str {
    match order.unwrap_or_default().trim().to_lowercase().as_str() {
        "asc" => "ASC",
        "desc" => "DESC",
        _ => default_order,
    }
}

fn to_cover_url(book_id: &str, has_cover: bool, cover_path: Option<&str>) -> Option<String> {
    if has_cover || cover_path.is_some() {
        Some(format!("/api/v1/books/{book_id}/cover"))
    } else {
        None
    }
}

fn optional_trimmed(value: Option<String>) -> Option<String> {
    value.and_then(non_empty_option)
}

fn non_empty_option(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn dedupe_non_empty(values: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for value in values {
        let trimmed = value.trim();
        if !trimmed.is_empty() && seen.insert(trimmed.to_string()) {
            result.push(trimmed.to_string());
        }
    }
    result
}

fn normalize_author_names(author_names: Vec<String>) -> Vec<String> {
    let normalized = author_names
        .into_iter()
        .flat_map(|name| split_authors(&name))
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>();

    if normalized.is_empty() {
        vec!["Unknown Author".to_string()]
    } else {
        dedupe_non_empty(normalized)
    }
}

fn split_authors(raw: &str) -> Vec<String> {
    raw.split(';')
        .flat_map(|chunk| chunk.split('&'))
        .flat_map(|chunk| chunk.split(','))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn normalize_fts_query(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }

    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_alphanumeric() || ch.is_whitespace() || ch == '*' {
            sanitized.push(ch);
        } else {
            sanitized.push(' ');
        }
    }

    let terms = sanitized
        .split_whitespace()
        .map(|term| term.trim_matches('*'))
        .filter(|term| !term.is_empty())
        .map(|term| format!("{term}*"))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

async fn get_or_create_author(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    author_name: &str,
    now: &str,
) -> anyhow::Result<String> {
    let existing = sqlx::query("SELECT id FROM authors WHERE lower(name) = lower(?) LIMIT 1")
        .bind(author_name)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(row) = existing {
        return Ok(row.get("id"));
    }

    let author_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
        .bind(&author_id)
        .bind(author_name)
        .bind(author_name)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(author_id)
}

fn normalize_isbn_candidate(id_type: &str, value: &str) -> Option<String> {
    let id_type_lc = id_type.trim().to_lowercase();
    if id_type_lc.contains("isbn") || looks_like_isbn(value) {
        normalize_isbn_value(value)
    } else {
        None
    }
}

fn normalize_isbn_value(value: &str) -> Option<String> {
    let normalized = value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect::<String>();
    if normalized.len() == 10 || normalized.len() == 13 {
        Some(normalized)
    } else {
        None
    }
}

fn looks_like_isbn(value: &str) -> bool {
    normalize_isbn_value(value).is_some()
}

#[allow(dead_code)]
fn _debug_row(_row: &SqliteRow) {}
