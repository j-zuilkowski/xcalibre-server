use crate::db::models::{AuthorRef, Book, FormatRef, Identifier, SeriesRef, TagRef};
use anyhow::{bail, Context};
use chrono::Utc;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row, Sqlite, SqlitePool};
use std::collections::{BTreeMap, BTreeSet};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize, ToSchema)]
pub struct BookSummary {
    pub id: String,
    pub title: String,
    pub sort_title: String,
    pub authors: Vec<AuthorRef>,
    pub tags: Vec<TagRef>,
    pub series: Option<SeriesRef>,
    pub series_index: Option<f64>,
    pub cover_url: Option<String>,
    pub has_cover: bool,
    pub is_read: bool,
    pub is_archived: bool,
    pub language: Option<String>,
    pub rating: Option<i64>,
    pub document_type: String,
    pub last_modified: String,
    pub progress_percentage: f64,
}

#[derive(Clone, Debug, Default)]
pub struct BookListPage {
    pub items: Vec<BookSummary>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Deserialize)]
struct AggregatedAuthorRow {
    display_order: i64,
    id: String,
    name: String,
    sort_name: String,
}

#[derive(Debug, Deserialize)]
struct AggregatedTagRow {
    id: String,
    name: String,
    confirmed: i64,
}

#[derive(Clone, Debug, Default)]
pub struct ListBooksParams {
    pub q: Option<String>,
    pub library_id: Option<String>,
    pub author_id: Option<String>,
    pub series_id: Option<String>,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub format: Option<String>,
    pub rating_bucket: Option<i64>,
    pub sort: Option<String>,
    pub order: Option<String>,
    pub page: i64,
    pub page_size: i64,
    pub since: Option<String>,
    pub user_id: Option<String>,
    pub show_archived: Option<bool>,
    pub only_read: Option<bool>,
}

#[derive(Clone, Debug, Default, ToSchema)]
#[schema(title = "Permission")]
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

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomColumn {
    pub id: String,
    pub name: String,
    pub label: String,
    pub column_type: String,
    pub is_multiple: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BookCustomValue {
    pub column_id: String,
    pub label: String,
    pub column_type: String,
    pub value: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BookCustomValueInput {
    pub column_id: String,
    pub value: Value,
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
    pub library_id: String,
    pub title: String,
    pub sort_title: String,
    pub description: Option<String>,
    pub pubdate: Option<String>,
    pub language: Option<String>,
    pub rating: Option<i64>,
    pub document_type: String,
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

#[derive(Clone, Debug)]
pub enum BulkTagMode {
    Append,
    Overwrite,
    Remove,
}

#[derive(Clone, Debug)]
pub struct BulkTagUpdateInput {
    pub mode: BulkTagMode,
    pub values: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct BulkUpdateBooksInput {
    pub book_ids: Vec<String>,
    pub tags: Option<BulkTagUpdateInput>,
    pub series: Option<String>,
    pub rating: Option<i64>,
    pub language: Option<String>,
    pub publisher: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct BulkUpdateBooksResult {
    pub updated: i64,
    pub errors: Vec<String>,
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
pub async fn list_books<'e, E>(db: E, params: &ListBooksParams) -> anyhow::Result<BookListPage>
where
    E: Copy,
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    let page_size = clamp_page_size(params.page_size);
    let page = if params.page < 1 { 1 } else { params.page };
    let offset = (page - 1) * page_size;
    let fts_query = normalize_fts_query(params.q.as_deref());
    let user_id = params.user_id.as_deref();

    let mut total_query =
        QueryBuilder::<Sqlite>::new("SELECT COUNT(DISTINCT b.id) AS total FROM books b");
    if fts_query.is_some() {
        total_query.push(
            " INNER JOIN books_fts ON (books_fts.book_id = b.id OR books_fts.rowid = b.rowid)",
        );
    }
    if let Some(user_id) = user_id {
        total_query.push(" LEFT JOIN book_user_state bus ON bus.book_id = b.id AND bus.user_id = ");
        total_query.push_bind(user_id);
    }
    apply_list_filters(&mut total_query, params, fts_query.as_deref());
    let total: i64 = total_query
        .build_query_scalar()
        .fetch_one(db)
        .await
        .context("count books")?;

    let (sort_column, sort_default) = normalize_sort(params.sort.as_deref());
    let order = normalize_order(params.order.as_deref(), sort_default);

    let mut data_query = QueryBuilder::<Sqlite>::new("");
    append_book_summary_base(&mut data_query, user_id);
    if fts_query.is_some() {
        data_query.push(
            " INNER JOIN books_fts ON (books_fts.book_id = b.id OR books_fts.rowid = b.rowid)",
        );
    }
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
        items.push(book_summary_from_row(&row)?);
    }

    Ok(BookListPage {
        items,
        total,
        page,
        page_size,
    })
}

pub async fn list_book_summaries_by_ids<'e, E>(
    db: E,
    book_ids: &[String],
    library_id: Option<&str>,
    user_id: Option<&str>,
) -> anyhow::Result<Vec<BookSummary>>
where
    E: Copy,
    E: sqlx::Executor<'e, Database = Sqlite>,
{
    if book_ids.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = QueryBuilder::<Sqlite>::new("");
    append_book_summary_base(&mut query, user_id);
    query.push(" WHERE ");
    if let Some(library_id) = library_id {
        query.push("b.library_id = ");
        query.push_bind(library_id);
        query.push(" AND ");
    }
    query.push("b.id IN (");
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
        let summary = book_summary_from_row(&row)?;
        summaries_by_id.insert(summary.id.clone(), summary);
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

fn append_book_summary_base<'a>(query: &mut QueryBuilder<'a, Sqlite>, user_id: Option<&'a str>) {
    query.push(
        r#"
        -- EXPLAIN QUERY PLAN: one books scan plus aggregate joins for authors and tags; no per-book nested loop.
        SELECT
            b.id AS id,
            b.title AS title,
            b.sort_title AS sort_title,
            b.series_index AS series_index,
            b.has_cover AS has_cover,
            b.cover_path AS cover_path,
            COALESCE(author_agg.authors_json, '') AS authors_json,
            COALESCE(tag_agg.tags_json, '') AS tags_json,
        "#,
    );
    if user_id.is_some() {
        query.push(
            "COALESCE(bus.is_read, 0) AS is_read, COALESCE(bus.is_archived, 0) AS is_archived, ",
        );
    } else {
        query.push("0 AS is_read, 0 AS is_archived, ");
    }
    if user_id.is_some() {
        query.push("COALESCE(rp.percentage, 0.0) AS progress_percentage, ");
    } else {
        query.push("0.0 AS progress_percentage, ");
    }
    query.push(
        r#"
            b.language AS language,
            b.rating AS rating,
            b.document_type AS document_type,
            b.last_modified AS last_modified,
            s.id AS series_id,
            s.name AS series_name
        FROM books b
        LEFT JOIN (
            SELECT
                ordered_authors.book_id AS book_id,
                GROUP_CONCAT(ordered_authors.author_json) AS authors_json
            FROM (
                SELECT
                    ba.book_id AS book_id,
                    ba.display_order AS display_order,
                    a.sort_name AS sort_name,
                    json_object(
                        'display_order', ba.display_order,
                        'id', a.id,
                        'name', a.name,
                        'sort_name', a.sort_name
                    ) AS author_json
                FROM book_authors ba
                INNER JOIN authors a ON a.id = ba.author_id
                ORDER BY ba.book_id ASC, ba.display_order ASC, a.sort_name ASC, a.id ASC
            ) ordered_authors
            GROUP BY ordered_authors.book_id
        ) author_agg ON author_agg.book_id = b.id
        LEFT JOIN (
            SELECT
                ordered_tags.book_id AS book_id,
                GROUP_CONCAT(ordered_tags.tag_json) AS tags_json
            FROM (
                SELECT
                    bt.book_id AS book_id,
                    t.name AS name,
                    json_object(
                        'id', t.id,
                        'name', t.name,
                        'confirmed', CASE WHEN bt.confirmed != 0 THEN 1 ELSE 0 END
                    ) AS tag_json
                FROM book_tags bt
                INNER JOIN tags t ON t.id = bt.tag_id
                ORDER BY bt.book_id ASC, t.name ASC, t.id ASC
            ) ordered_tags
            GROUP BY ordered_tags.book_id
        ) tag_agg ON tag_agg.book_id = b.id
        "#,
    );
    if let Some(user_id) = user_id {
        query.push(" LEFT JOIN book_user_state bus ON bus.book_id = b.id AND bus.user_id = ");
        query.push_bind(user_id);
        query.push(" LEFT JOIN reading_progress rp ON rp.book_id = b.id AND rp.user_id = ");
        query.push_bind(user_id);
    }
    query.push(" LEFT JOIN series s ON s.id = b.series_id");
}

fn parse_group_concat_json<T>(raw: Option<String>, label: &str) -> anyhow::Result<Vec<T>>
where
    T: DeserializeOwned,
{
    let raw = raw.unwrap_or_default();
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }

    serde_json::from_str(&format!("[{raw}]"))
        .with_context(|| format!("parse aggregated {label} rows"))
}

fn parse_summary_authors(raw: Option<String>) -> anyhow::Result<Vec<AuthorRef>> {
    let mut authors: Vec<AggregatedAuthorRow> = parse_group_concat_json(raw, "authors")?;
    authors.sort_by(|left, right| {
        left.display_order
            .cmp(&right.display_order)
            .then_with(|| left.sort_name.cmp(&right.sort_name))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(authors
        .into_iter()
        .map(|author| AuthorRef {
            id: author.id,
            name: author.name,
            sort_name: author.sort_name,
        })
        .collect())
}

fn parse_summary_tags(raw: Option<String>) -> anyhow::Result<Vec<TagRef>> {
    let mut tags: Vec<AggregatedTagRow> = parse_group_concat_json(raw, "tags")?;
    tags.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then_with(|| left.id.cmp(&right.id))
    });

    Ok(tags
        .into_iter()
        .map(|tag| TagRef {
            id: tag.id,
            name: tag.name,
            confirmed: tag.confirmed != 0,
        })
        .collect())
}

fn book_summary_from_row(row: &SqliteRow) -> anyhow::Result<BookSummary> {
    let book_id: String = row.get("id");
    let has_cover = row.get::<i64, _>("has_cover") != 0;
    let cover_path: Option<String> = row.get("cover_path");

    Ok(BookSummary {
        id: book_id.clone(),
        title: row.get("title"),
        sort_title: row.get("sort_title"),
        authors: parse_summary_authors(row.get("authors_json"))?,
        tags: parse_summary_tags(row.get("tags_json"))?,
        series: row
            .get::<Option<String>, _>("series_id")
            .map(|id| SeriesRef {
                id,
                name: row.get("series_name"),
            }),
        series_index: row.get("series_index"),
        cover_url: to_cover_url(&book_id, has_cover, cover_path.as_deref()),
        has_cover,
        is_read: row.get::<i64, _>("is_read") != 0,
        is_archived: row.get::<i64, _>("is_archived") != 0,
        language: row.get("language"),
        rating: row.get("rating"),
        document_type: row.get("document_type"),
        last_modified: row.get("last_modified"),
        progress_percentage: row.get("progress_percentage"),
    })
}

pub async fn get_book_by_id(
    db: &SqlitePool,
    book_id: &str,
    library_id: Option<&str>,
    user_id: Option<&str>,
) -> anyhow::Result<Option<Book>> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
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
            "#,
    );
    if user_id.is_some() {
        query.push(
            "COALESCE(bus.is_read, 0) AS is_read, COALESCE(bus.is_archived, 0) AS is_archived, ",
        );
    } else {
        query.push("0 AS is_read, 0 AS is_archived, ");
    }
    query.push(
        r#"
            b.created_at AS created_at,
            b.last_modified AS last_modified,
            b.indexed_at AS indexed_at,
            s.id AS series_id,
            s.name AS series_name
        FROM books b
        "#,
    );
    if let Some(user_id) = user_id {
        query.push(" LEFT JOIN book_user_state bus ON bus.book_id = b.id AND bus.user_id = ");
        query.push_bind(user_id);
    }
    query.push(" LEFT JOIN series s ON s.id = b.series_id WHERE b.id = ");
    query.push_bind(book_id);
    if let Some(library_id) = library_id {
        query.push(" AND b.library_id = ");
        query.push_bind(library_id);
    }

    let row = query.build().fetch_optional(db).await?;

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
        document_type: row.get("document_type"),
        series: row
            .get::<Option<String>, _>("series_id")
            .map(|id| SeriesRef {
                id,
                name: row.get("series_name"),
            }),
        series_index: row.get("series_index"),
        authors: load_book_authors(db, book_id).await?,
        tags: load_book_tags(db, book_id).await?,
        formats: load_book_formats(db, book_id).await?,
        cover_url: to_cover_url(book_id, has_cover, cover_path.as_deref()),
        has_cover,
        is_read: row.get::<i64, _>("is_read") != 0,
        is_archived: row.get::<i64, _>("is_archived") != 0,
        identifiers: get_book_identifiers(db, book_id).await?,
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
    crate::db::queries::book_insert::insert_uploaded_book_impl(db, input).await
}

pub async fn patch_book_with_audit(
    db: &SqlitePool,
    book_id: &str,
    actor_user_id: &str,
    library_id: Option<&str>,
    user_id: Option<&str>,
    patch: PatchBookInput,
) -> anyhow::Result<Option<Book>> {
    let existing = get_book_by_id(db, book_id, library_id, user_id).await?;
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
            changes.push((
                "pubdate".to_string(),
                json!(existing.pubdate),
                json!(pubdate),
            ));
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
    get_book_by_id(db, book_id, library_id, user_id).await
}

pub async fn bulk_update_books(
    db: &SqlitePool,
    input: BulkUpdateBooksInput,
) -> anyhow::Result<BulkUpdateBooksResult> {
    let book_ids = dedupe_non_empty(input.book_ids);
    if book_ids.is_empty() {
        return Ok(BulkUpdateBooksResult::default());
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = db.begin().await?;
    let mut updated = 0_i64;
    let mut errors = Vec::new();

    for book_id in book_ids {
        let exists = sqlx::query("SELECT id FROM books WHERE id = ?")
            .bind(&book_id)
            .fetch_optional(&mut *tx)
            .await?;
        if exists.is_none() {
            errors.push(format!("{book_id}: not found"));
            continue;
        }

        let mut changed = false;

        if let Some(rating) = input.rating {
            if !(0..=10).contains(&rating) {
                errors.push(format!("{book_id}: invalid rating"));
                continue;
            }

            sqlx::query("UPDATE books SET rating = ?, last_modified = ? WHERE id = ?")
                .bind(rating)
                .bind(&now)
                .bind(&book_id)
                .execute(&mut *tx)
                .await?;
            changed = true;
        }

        if let Some(language) = input.language.as_ref() {
            let next_language = non_empty_option(language.clone());
            sqlx::query("UPDATE books SET language = ?, last_modified = ? WHERE id = ?")
                .bind(next_language)
                .bind(&now)
                .bind(&book_id)
                .execute(&mut *tx)
                .await?;
            changed = true;
        }

        if let Some(series) = input.series.as_ref() {
            let next_series = non_empty_option(series.clone());
            let next_series_id = match next_series {
                Some(ref series_name) => {
                    Some(get_or_create_series(&mut tx, series_name, &now).await?)
                }
                None => None,
            };
            sqlx::query("UPDATE books SET series_id = ?, last_modified = ? WHERE id = ?")
                .bind(next_series_id)
                .bind(&now)
                .bind(&book_id)
                .execute(&mut *tx)
                .await?;
            changed = true;
        }

        if let Some(publisher) = input.publisher.as_ref() {
            update_book_publisher(&mut tx, &book_id, publisher, &now).await?;
            changed = true;
        }

        if let Some(tags) = input.tags.as_ref() {
            apply_bulk_tags(&mut tx, &book_id, tags, &now).await?;
            changed = true;
        }

        if changed {
            updated += 1;
        }
    }

    tx.commit().await?;

    Ok(BulkUpdateBooksResult { updated, errors })
}

pub async fn merge_books(
    db: &SqlitePool,
    primary_id: &str,
    duplicate_id: &str,
) -> anyhow::Result<()> {
    let primary_id = primary_id.trim();
    let duplicate_id = duplicate_id.trim();
    if primary_id.is_empty() || duplicate_id.is_empty() {
        bail!("book id must not be empty");
    }
    if primary_id == duplicate_id {
        bail!("cannot merge a book into itself");
    }

    let mut tx = db.begin().await?;

    let primary_exists: Option<String> = sqlx::query_scalar("SELECT id FROM books WHERE id = ?")
        .bind(primary_id)
        .fetch_optional(&mut *tx)
        .await?;
    if primary_exists.is_none() {
        bail!("primary_not_found");
    }

    let duplicate_exists: Option<String> = sqlx::query_scalar("SELECT id FROM books WHERE id = ?")
        .bind(duplicate_id)
        .fetch_optional(&mut *tx)
        .await?;
    if duplicate_exists.is_none() {
        bail!("duplicate_not_found");
    }

    // Step 1: move non-conflicting formats to the primary book.
    sqlx::query(
        r#"
        UPDATE formats
        SET book_id = ?
        WHERE book_id = ?
          AND NOT EXISTS (
              SELECT 1
              FROM formats f2
              WHERE f2.book_id = ?
                AND upper(f2.format) = upper(formats.format)
          )
        "#,
    )
    .bind(primary_id)
    .bind(duplicate_id)
    .bind(primary_id)
    .execute(&mut *tx)
    .await?;

    // Step 2: merge identifiers (dedupe by (book_id, id_type)).
    sqlx::query(
        r#"
        UPDATE identifiers
        SET book_id = ?
        WHERE book_id = ?
          AND NOT EXISTS (
              SELECT 1
              FROM identifiers i2
              WHERE i2.book_id = ?
                AND lower(i2.id_type) = lower(identifiers.id_type)
          )
        "#,
    )
    .bind(primary_id)
    .bind(duplicate_id)
    .bind(primary_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM identifiers WHERE book_id = ?")
        .bind(duplicate_id)
        .execute(&mut *tx)
        .await?;

    // Step 3: merge authors (dedupe by (book_id, author_id)).
    let starting_order: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(display_order), -1) + 1 FROM book_authors WHERE book_id = ?",
    )
    .bind(primary_id)
    .fetch_one(&mut *tx)
    .await?;
    let duplicate_author_rows = sqlx::query(
        r#"
        SELECT author_id
        FROM book_authors
        WHERE book_id = ?
        ORDER BY display_order ASC, author_id ASC
        "#,
    )
    .bind(duplicate_id)
    .fetch_all(&mut *tx)
    .await?;
    for (index, row) in duplicate_author_rows.into_iter().enumerate() {
        let author_id: String = row.get("author_id");
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO book_authors (book_id, author_id, display_order)
            VALUES (?, ?, ?)
            "#,
        )
        .bind(primary_id)
        .bind(author_id)
        .bind(starting_order + index as i64)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM book_authors WHERE book_id = ?")
        .bind(duplicate_id)
        .execute(&mut *tx)
        .await?;

    // Step 4: merge tags and preserve confirmed tags when either side confirmed.
    sqlx::query(
        r#"
        INSERT INTO book_tags (book_id, tag_id, confirmed)
        SELECT ?, tag_id, confirmed
        FROM book_tags
        WHERE book_id = ?
        ON CONFLICT(book_id, tag_id) DO UPDATE SET
            confirmed = MAX(book_tags.confirmed, excluded.confirmed)
        "#,
    )
    .bind(primary_id)
    .bind(duplicate_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM book_tags WHERE book_id = ?")
        .bind(duplicate_id)
        .execute(&mut *tx)
        .await?;

    // Step 5: reassign reading progress and keep the furthest percentage per user.
    let duplicate_progress_rows = sqlx::query(
        r#"
        SELECT user_id, format_id, cfi, page, percentage, updated_at, last_modified
        FROM reading_progress
        WHERE book_id = ?
        "#,
    )
    .bind(duplicate_id)
    .fetch_all(&mut *tx)
    .await?;

    for row in duplicate_progress_rows {
        let user_id: String = row.get("user_id");
        let duplicate_format_id: String = row.get("format_id");
        let duplicate_cfi: Option<String> = row.get("cfi");
        let duplicate_page: Option<i64> = row.get("page");
        let duplicate_percentage: f64 = row.get("percentage");
        let duplicate_updated_at: String = row.get("updated_at");
        let duplicate_last_modified: String = row.get("last_modified");

        let mapped_format_id: String = sqlx::query_scalar(
            r#"
            SELECT COALESCE(
                (
                    SELECT pf.id
                    FROM formats df
                    JOIN formats pf
                      ON pf.book_id = ?
                     AND upper(pf.format) = upper(df.format)
                    WHERE df.id = ?
                    LIMIT 1
                ),
                ?
            )
            "#,
        )
        .bind(primary_id)
        .bind(&duplicate_format_id)
        .bind(&duplicate_format_id)
        .fetch_one(&mut *tx)
        .await?;

        let existing_primary_row = sqlx::query(
            r#"
            SELECT percentage
            FROM reading_progress
            WHERE user_id = ? AND book_id = ?
            LIMIT 1
            "#,
        )
        .bind(&user_id)
        .bind(primary_id)
        .fetch_optional(&mut *tx)
        .await?;

        if let Some(existing_primary_row) = existing_primary_row {
            let existing_percentage: f64 = existing_primary_row.get("percentage");
            if duplicate_percentage >= existing_percentage {
                sqlx::query(
                    r#"
                    UPDATE reading_progress
                    SET
                        format_id = ?,
                        cfi = ?,
                        page = ?,
                        percentage = ?,
                        updated_at = ?,
                        last_modified = ?
                    WHERE user_id = ? AND book_id = ?
                    "#,
                )
                .bind(&mapped_format_id)
                .bind(duplicate_cfi)
                .bind(duplicate_page)
                .bind(duplicate_percentage)
                .bind(&duplicate_updated_at)
                .bind(&duplicate_last_modified)
                .bind(&user_id)
                .bind(primary_id)
                .execute(&mut *tx)
                .await?;
            }
            continue;
        }

        sqlx::query(
            r#"
            UPDATE reading_progress
            SET
                book_id = ?,
                format_id = ?,
                cfi = ?,
                page = ?,
                percentage = ?,
                updated_at = ?,
                last_modified = ?
            WHERE user_id = ? AND book_id = ?
            "#,
        )
        .bind(primary_id)
        .bind(&mapped_format_id)
        .bind(duplicate_cfi)
        .bind(duplicate_page)
        .bind(duplicate_percentage)
        .bind(&duplicate_updated_at)
        .bind(&duplicate_last_modified)
        .bind(&user_id)
        .bind(duplicate_id)
        .execute(&mut *tx)
        .await?;
    }
    sqlx::query("DELETE FROM reading_progress WHERE book_id = ?")
        .bind(duplicate_id)
        .execute(&mut *tx)
        .await?;

    // Step 6: reassign shelf links.
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO shelf_books (shelf_id, book_id, display_order, added_at)
        SELECT shelf_id, ?, display_order, added_at
        FROM shelf_books
        WHERE book_id = ?
        "#,
    )
    .bind(primary_id)
    .bind(duplicate_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM shelf_books WHERE book_id = ?")
        .bind(duplicate_id)
        .execute(&mut *tx)
        .await?;

    // Step 7: reassign per-user book state.
    sqlx::query(
        r#"
        INSERT INTO book_user_state (user_id, book_id, is_read, is_archived, updated_at)
        SELECT user_id, ?, is_read, is_archived, updated_at
        FROM book_user_state
        WHERE book_id = ?
        ON CONFLICT(user_id, book_id) DO UPDATE SET
            is_read = MAX(book_user_state.is_read, excluded.is_read),
            is_archived = MAX(book_user_state.is_archived, excluded.is_archived),
            updated_at = CASE
                WHEN excluded.updated_at >= book_user_state.updated_at THEN excluded.updated_at
                ELSE book_user_state.updated_at
            END
        "#,
    )
    .bind(primary_id)
    .bind(duplicate_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query("DELETE FROM book_user_state WHERE book_id = ?")
        .bind(duplicate_id)
        .execute(&mut *tx)
        .await?;

    // Step 8: delete duplicate book (remaining related rows cascade).
    let deleted = sqlx::query("DELETE FROM books WHERE id = ?")
        .bind(duplicate_id)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if deleted == 0 {
        bail!("duplicate_not_found");
    }

    tx.commit().await?;
    Ok(())
}

pub async fn delete_book_and_collect_paths(
    db: &SqlitePool,
    book_id: &str,
    actor_user_id: &str,
    library_id: Option<&str>,
) -> anyhow::Result<Option<Vec<String>>> {
    let rows = if let Some(library_id) = library_id {
        sqlx::query(
            r#"
            SELECT f.path
            FROM formats f
            INNER JOIN books b ON b.id = f.book_id
            WHERE f.book_id = ? AND b.library_id = ?
            "#,
        )
        .bind(book_id)
        .bind(library_id)
        .fetch_all(db)
        .await?
    } else {
        sqlx::query("SELECT path FROM formats WHERE book_id = ?")
            .bind(book_id)
            .fetch_all(db)
            .await?
    };
    if rows.is_empty() {
        let exists = if let Some(library_id) = library_id {
            sqlx::query(
                r#"
                SELECT b.id
                FROM books b
                WHERE b.id = ? AND b.library_id = ?
                "#,
            )
            .bind(book_id)
            .bind(library_id)
            .fetch_optional(db)
            .await?
        } else {
            sqlx::query("SELECT id FROM books WHERE id = ?")
                .bind(book_id)
                .fetch_optional(db)
                .await?
        };
        if exists.is_none() {
            return Ok(None);
        }
    }

    let format_paths = rows
        .into_iter()
        .map(|row| row.get::<String, _>("path"))
        .collect::<Vec<_>>();

    let now = Utc::now().to_rfc3339();
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
    sqlx::query(
        r#"
        INSERT INTO audit_log (id, user_id, action, entity, entity_id, diff_json, created_at)
        VALUES (?, ?, 'delete', 'book', ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(actor_user_id)
    .bind(book_id)
    .bind(json!({"event":"book_delete"}).to_string())
    .bind(&now)
    .execute(&mut *tx)
    .await?;
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

pub async fn find_book_cover_path(
    db: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<String>> {
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

pub async fn find_book_ids_by_title_and_author_like(
    db: &SqlitePool,
    title: &str,
    author: &str,
) -> anyhow::Result<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT DISTINCT b.id
        FROM books b
        WHERE (
            lower(b.title) LIKE '%' || lower(?) || '%'
            OR lower(b.sort_title) LIKE '%' || lower(?) || '%'
        )
          AND EXISTS (
              SELECT 1
              FROM book_authors ba
              INNER JOIN authors a ON a.id = ba.author_id
              WHERE ba.book_id = b.id
                AND (
                    lower(a.name) LIKE '%' || lower(?) || '%'
                    OR lower(a.sort_name) LIKE '%' || lower(?) || '%'
                )
          )
        ORDER BY b.title ASC, b.id ASC
        LIMIT 2
        "#,
    )
    .bind(title)
    .bind(title)
    .bind(author)
    .bind(author)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(|row| row.get("id")).collect())
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

pub async fn get_book_identifiers(
    db: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Vec<Identifier>> {
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

pub async fn list_custom_columns(db: &SqlitePool) -> anyhow::Result<Vec<CustomColumn>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, label, column_type, is_multiple
        FROM custom_columns
        ORDER BY lower(label) ASC, id ASC
        "#,
    )
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| CustomColumn {
            id: row.get("id"),
            name: row.get("name"),
            label: row.get("label"),
            column_type: row.get("column_type"),
            is_multiple: row.get::<i64, _>("is_multiple") != 0,
        })
        .collect())
}

pub async fn create_custom_column(
    db: &SqlitePool,
    name: &str,
    label: &str,
    column_type: &str,
    is_multiple: bool,
) -> anyhow::Result<CustomColumn> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO custom_columns (id, name, label, column_type, is_multiple, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(label)
    .bind(column_type)
    .bind(i64::from(is_multiple))
    .bind(&now)
    .execute(db)
    .await?;

    Ok(CustomColumn {
        id,
        name: name.to_string(),
        label: label.to_string(),
        column_type: column_type.to_string(),
        is_multiple,
    })
}

pub async fn delete_custom_column(db: &SqlitePool, column_id: &str) -> anyhow::Result<bool> {
    let result = sqlx::query("DELETE FROM custom_columns WHERE id = ?")
        .bind(column_id)
        .execute(db)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn get_book_custom_values(
    db: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Vec<BookCustomValue>> {
    let rows = sqlx::query(
        r#"
        SELECT
            cc.id AS column_id,
            cc.label AS label,
            cc.column_type AS column_type,
            bcv.value_text AS value_text,
            bcv.value_int AS value_int,
            bcv.value_float AS value_float,
            bcv.value_bool AS value_bool
        FROM custom_columns cc
        LEFT JOIN book_custom_values bcv
            ON bcv.column_id = cc.id
           AND bcv.book_id = ?
        ORDER BY lower(cc.label) ASC, cc.id ASC
        "#,
    )
    .bind(book_id)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let column_type: String = row.get("column_type");
            BookCustomValue {
                column_id: row.get("column_id"),
                label: row.get("label"),
                value: decode_custom_value(&column_type, &row),
                column_type,
            }
        })
        .collect())
}

pub async fn upsert_book_custom_values(
    db: &SqlitePool,
    book_id: &str,
    values: &[BookCustomValueInput],
) -> anyhow::Result<()> {
    if values.is_empty() {
        return Ok(());
    }

    let mut deduped_by_column: BTreeMap<String, Value> = BTreeMap::new();
    for value in values {
        let column_id = value.column_id.trim();
        if column_id.is_empty() {
            bail!("invalid_column_id");
        }
        deduped_by_column.insert(column_id.to_string(), value.value.clone());
    }
    if deduped_by_column.is_empty() {
        return Ok(());
    }

    let mut tx = db.begin().await?;
    let book_exists: Option<String> = sqlx::query_scalar("SELECT id FROM books WHERE id = ?")
        .bind(book_id)
        .fetch_optional(&mut *tx)
        .await?;
    if book_exists.is_none() {
        bail!("book_not_found");
    }

    let column_ids = deduped_by_column.keys().cloned().collect::<Vec<_>>();
    let mut query = QueryBuilder::<Sqlite>::new(
        "SELECT id, column_type, is_multiple FROM custom_columns WHERE id IN (",
    );
    {
        let mut separated = query.separated(", ");
        for column_id in &column_ids {
            separated.push_bind(column_id);
        }
    }
    query.push(")");

    let rows = query.build().fetch_all(&mut *tx).await?;
    let definitions = rows
        .into_iter()
        .map(|row| {
            (
                row.get::<String, _>("id"),
                CustomColumnDefinition {
                    column_type: row.get("column_type"),
                    is_multiple: row.get::<i64, _>("is_multiple") != 0,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();

    if definitions.len() != deduped_by_column.len() {
        bail!("column_not_found");
    }

    for (column_id, value) in deduped_by_column {
        let definition = definitions
            .get(&column_id)
            .ok_or_else(|| anyhow::anyhow!("column_not_found"))?;
        let parsed = parse_custom_value_input(
            definition.column_type.as_str(),
            definition.is_multiple,
            &value,
        )?;

        if parsed.is_empty() {
            sqlx::query("DELETE FROM book_custom_values WHERE book_id = ? AND column_id = ?")
                .bind(book_id)
                .bind(&column_id)
                .execute(&mut *tx)
                .await?;
            continue;
        }

        sqlx::query(
            r#"
            INSERT INTO book_custom_values (
                id, book_id, column_id, value_text, value_int, value_float, value_bool
            ) VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(book_id, column_id) DO UPDATE SET
                value_text = excluded.value_text,
                value_int = excluded.value_int,
                value_float = excluded.value_float,
                value_bool = excluded.value_bool
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(book_id)
        .bind(&column_id)
        .bind(parsed.value_text)
        .bind(parsed.value_int)
        .bind(parsed.value_float)
        .bind(parsed.value_bool)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;
    Ok(())
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
        qb.push("EXISTS (SELECT 1 FROM formats f WHERE f.book_id = b.id AND upper(f.format) = ");
        qb.push_bind(format.to_uppercase());
        qb.push(")");
    }

    if let Some(publisher) = params
        .publisher
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push("lower(trim(coalesce(json_extract(b.flags, '$.publisher'), ''))) = lower(trim(");
        qb.push_bind(publisher);
        qb.push("))");
    }

    if let Some(rating_bucket) = params.rating_bucket.filter(|value| (1..=5).contains(value)) {
        and_where(qb);
        let min_rating = (rating_bucket - 1) * 2 + 1;
        let max_rating = rating_bucket * 2;
        qb.push("b.rating BETWEEN ");
        qb.push_bind(min_rating);
        qb.push(" AND ");
        qb.push_bind(max_rating);
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

    if params.user_id.is_some() {
        if !params.show_archived.unwrap_or(false) {
            and_where(qb);
            qb.push("COALESCE(bus.is_archived, 0) = 0");
        }

        if let Some(only_read) = params.only_read {
            and_where(qb);
            qb.push("COALESCE(bus.is_read, 0) = ");
            qb.push_bind(i64::from(only_read));
        }

        if let Some(user_id) = params
            .user_id
            .as_ref()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        {
            let user_id_for_allow = user_id.clone();
            and_where(qb);
            qb.push(
                "NOT EXISTS (SELECT 1 FROM book_tags bt2 JOIN user_tag_restrictions r ON r.tag_id = bt2.tag_id WHERE bt2.book_id = b.id AND r.user_id = ",
            );
            qb.push_bind(user_id.clone());
            qb.push(" AND r.mode = 'block')");

            and_where(qb);
            qb.push("(");
            qb.push("NOT EXISTS (SELECT 1 FROM user_tag_restrictions r WHERE r.user_id = ");
            qb.push_bind(user_id_for_allow.clone());
            qb.push(" AND r.mode = 'allow') OR EXISTS (SELECT 1 FROM book_tags bt2 JOIN user_tag_restrictions r ON r.tag_id = bt2.tag_id WHERE bt2.book_id = b.id AND r.user_id = ");
            qb.push_bind(user_id_for_allow);
            qb.push(" AND r.mode = 'allow'))");
        }
    }

    if let Some(library_id) = params
        .library_id
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push("b.library_id = ");
        qb.push_bind(library_id);
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

pub(crate) fn optional_trimmed(value: Option<String>) -> Option<String> {
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

pub(crate) fn normalize_author_names(author_names: Vec<String>) -> Vec<String> {
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

pub(crate) async fn get_or_create_author(
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

async fn get_or_create_series(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    series_name: &str,
    now: &str,
) -> anyhow::Result<String> {
    let existing = sqlx::query("SELECT id FROM series WHERE lower(name) = lower(?) LIMIT 1")
        .bind(series_name)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(row) = existing {
        return Ok(row.get("id"));
    }

    let series_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO series (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
        .bind(&series_id)
        .bind(series_name)
        .bind(series_name)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(series_id)
}

async fn get_or_create_tag(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    tag_name: &str,
    now: &str,
) -> anyhow::Result<String> {
    let existing = sqlx::query("SELECT id FROM tags WHERE lower(name) = lower(?) LIMIT 1")
        .bind(tag_name)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(row) = existing {
        return Ok(row.get("id"));
    }

    let tag_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tags (id, name, source, last_modified) VALUES (?, ?, 'manual', ?)")
        .bind(&tag_id)
        .bind(tag_name)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    Ok(tag_id)
}

async fn update_book_publisher(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    book_id: &str,
    publisher: &str,
    now: &str,
) -> anyhow::Result<()> {
    let flags: Option<String> = sqlx::query_scalar("SELECT flags FROM books WHERE id = ?")
        .bind(book_id)
        .fetch_optional(&mut **tx)
        .await?;
    let mut value = match flags {
        Some(raw) => serde_json::from_str::<serde_json::Value>(&raw).unwrap_or_default(),
        None => serde_json::Value::Null,
    };
    if !matches!(value, serde_json::Value::Object(_)) {
        value = serde_json::Value::Object(serde_json::Map::new());
    }

    if let serde_json::Value::Object(ref mut object) = value {
        let trimmed = publisher.trim();
        if trimmed.is_empty() {
            object.remove("publisher");
        } else {
            object.insert(
                "publisher".to_string(),
                serde_json::Value::String(trimmed.to_string()),
            );
        }
    }

    sqlx::query("UPDATE books SET flags = ?, last_modified = ? WHERE id = ?")
        .bind(value.to_string())
        .bind(now)
        .bind(book_id)
        .execute(&mut **tx)
        .await?;

    Ok(())
}

async fn apply_bulk_tags(
    tx: &mut sqlx::Transaction<'_, Sqlite>,
    book_id: &str,
    tags: &BulkTagUpdateInput,
    now: &str,
) -> anyhow::Result<()> {
    let tag_names = dedupe_non_empty(tags.values.clone());
    if tag_names.is_empty() {
        if matches!(tags.mode, BulkTagMode::Overwrite) {
            sqlx::query("DELETE FROM book_tags WHERE book_id = ?")
                .bind(book_id)
                .execute(&mut **tx)
                .await?;
        }
        return Ok(());
    }

    match tags.mode {
        BulkTagMode::Overwrite => {
            sqlx::query("DELETE FROM book_tags WHERE book_id = ?")
                .bind(book_id)
                .execute(&mut **tx)
                .await?;
        }
        BulkTagMode::Remove => {
            for tag_name in tag_names {
                if let Some(tag_id) =
                    sqlx::query("SELECT id FROM tags WHERE lower(name) = lower(?) LIMIT 1")
                        .bind(&tag_name)
                        .fetch_optional(&mut **tx)
                        .await?
                        .map(|row| row.get::<String, _>("id"))
                {
                    sqlx::query("DELETE FROM book_tags WHERE book_id = ? AND tag_id = ?")
                        .bind(book_id)
                        .bind(tag_id)
                        .execute(&mut **tx)
                        .await?;
                }
            }
            return Ok(());
        }
        BulkTagMode::Append => {}
    }

    for tag_name in tag_names {
        let tag_id = get_or_create_tag(tx, &tag_name, now).await?;
        sqlx::query(
            r#"
            INSERT OR IGNORE INTO book_tags (book_id, tag_id, confirmed)
            VALUES (?, ?, 1)
            "#,
        )
        .bind(book_id)
        .bind(tag_id)
        .execute(&mut **tx)
        .await?;
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct CustomColumnDefinition {
    column_type: String,
    is_multiple: bool,
}

#[derive(Clone, Debug, Default)]
struct ParsedCustomValue {
    value_text: Option<String>,
    value_int: Option<i64>,
    value_float: Option<f64>,
    value_bool: Option<i64>,
}

impl ParsedCustomValue {
    fn is_empty(&self) -> bool {
        self.value_text.is_none()
            && self.value_int.is_none()
            && self.value_float.is_none()
            && self.value_bool.is_none()
    }
}

fn decode_custom_value(column_type: &str, row: &SqliteRow) -> Option<Value> {
    match normalize_custom_column_type(column_type) {
        "integer" => row.get::<Option<i64>, _>("value_int").map(Value::from),
        "float" => row.get::<Option<f64>, _>("value_float").map(Value::from),
        "bool" => row
            .get::<Option<i64>, _>("value_bool")
            .map(|value| Value::Bool(value != 0)),
        _ => row
            .get::<Option<String>, _>("value_text")
            .map(Value::String),
    }
}

fn normalize_custom_column_type(column_type: &str) -> &'static str {
    match column_type.trim().to_ascii_lowercase().as_str() {
        "int" | "integer" => "integer",
        "float" => "float",
        "bool" | "boolean" => "bool",
        "datetime" => "datetime",
        "text" | "tags" => "text",
        _ => "text",
    }
}

fn parse_custom_value_input(
    column_type: &str,
    is_multiple: bool,
    value: &Value,
) -> anyhow::Result<ParsedCustomValue> {
    if value.is_null() {
        return Ok(ParsedCustomValue::default());
    }

    // For multi-valued columns we store values as JSON text in value_text.
    if is_multiple {
        return match value {
            Value::Array(items) if items.is_empty() => Ok(ParsedCustomValue::default()),
            Value::Array(_) => Ok(ParsedCustomValue {
                value_text: Some(value.to_string()),
                ..ParsedCustomValue::default()
            }),
            Value::String(text) if text.trim().is_empty() => Ok(ParsedCustomValue::default()),
            Value::String(text) => Ok(ParsedCustomValue {
                value_text: Some(text.trim().to_string()),
                ..ParsedCustomValue::default()
            }),
            _ => bail!("invalid_custom_value_type"),
        };
    }

    match normalize_custom_column_type(column_type) {
        "integer" => {
            if let Some(number) = value.as_i64() {
                return Ok(ParsedCustomValue {
                    value_int: Some(number),
                    ..ParsedCustomValue::default()
                });
            }
            if let Some(text) = value.as_str() {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Ok(ParsedCustomValue::default());
                }
                let parsed = trimmed
                    .parse::<i64>()
                    .map_err(|_| anyhow::anyhow!("invalid_integer"))?;
                return Ok(ParsedCustomValue {
                    value_int: Some(parsed),
                    ..ParsedCustomValue::default()
                });
            }
            bail!("invalid_integer");
        }
        "float" => {
            if let Some(number) = value.as_f64() {
                return Ok(ParsedCustomValue {
                    value_float: Some(number),
                    ..ParsedCustomValue::default()
                });
            }
            if let Some(text) = value.as_str() {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Ok(ParsedCustomValue::default());
                }
                let parsed = trimmed
                    .parse::<f64>()
                    .map_err(|_| anyhow::anyhow!("invalid_float"))?;
                return Ok(ParsedCustomValue {
                    value_float: Some(parsed),
                    ..ParsedCustomValue::default()
                });
            }
            bail!("invalid_float");
        }
        "bool" => {
            if let Some(boolean) = value.as_bool() {
                return Ok(ParsedCustomValue {
                    value_bool: Some(i64::from(boolean)),
                    ..ParsedCustomValue::default()
                });
            }
            if let Some(number) = value.as_i64() {
                if number == 0 || number == 1 {
                    return Ok(ParsedCustomValue {
                        value_bool: Some(number),
                        ..ParsedCustomValue::default()
                    });
                }
            }
            if let Some(text) = value.as_str() {
                let normalized = text.trim().to_ascii_lowercase();
                if normalized.is_empty() {
                    return Ok(ParsedCustomValue::default());
                }
                let parsed = match normalized.as_str() {
                    "1" | "true" | "yes" => Some(1_i64),
                    "0" | "false" | "no" => Some(0_i64),
                    _ => None,
                };
                if let Some(parsed) = parsed {
                    return Ok(ParsedCustomValue {
                        value_bool: Some(parsed),
                        ..ParsedCustomValue::default()
                    });
                }
            }
            bail!("invalid_bool");
        }
        "datetime" | "text" => {
            if let Some(text) = value.as_str() {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    return Ok(ParsedCustomValue::default());
                }
                return Ok(ParsedCustomValue {
                    value_text: Some(trimmed.to_string()),
                    ..ParsedCustomValue::default()
                });
            }
            bail!("invalid_text");
        }
        _ => bail!("invalid_custom_value_type"),
    }
}

fn looks_like_isbn(value: &str) -> bool {
    normalize_isbn_value(value).is_some()
}

#[allow(dead_code)]
fn _debug_row(_row: &SqliteRow) {}
