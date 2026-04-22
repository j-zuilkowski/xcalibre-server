use crate::{
    db::queries::{books as book_queries, opds as opds_queries},
    AppError, AppState,
};
use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderValue},
    response::Response,
    routing::get,
    Router,
};
use chrono::{Duration, Utc};
use serde::Deserialize;
use std::fmt::Write as _;

pub fn router(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/", get(root_catalog))
        .route("/catalog", get(all_books))
        .route("/search", get(search))
        .route("/new", get(recently_added))
        .route("/authors", get(authors_feed))
        .route("/authors/:id", get(author_books_feed))
        .route("/series", get(series_feed))
        .route("/series/:id", get(series_books_feed))
        .route("/publishers", get(publishers_feed))
        .route("/publishers/:id", get(publisher_books_feed))
        .route("/languages", get(languages_feed))
        .route("/languages/:lang_code", get(language_books_feed))
        .route("/ratings", get(ratings_feed))
        .route("/ratings/:rating", get(rating_books_feed))
        .with_state(state)
}

#[derive(Debug, Default, Deserialize)]
struct FeedQuery {
    q: Option<String>,
    page: Option<i64>,
    page_size: Option<i64>,
}

async fn root_catalog() -> Result<Response, AppError> {
    let mut xml = String::new();
    push_feed_header(&mut xml, "Autolibre Catalog", "/opds", "navigation");
    push_opensearch_stats(&mut xml, 8, 50);
    push_navigation_entry(
        &mut xml,
        "All Books",
        "/opds/catalog",
        "Browse the full library",
        "navigation",
    );
    push_navigation_entry(
        &mut xml,
        "Recently Added",
        "/opds/new",
        "Books added in the last 30 days",
        "navigation",
    );
    push_navigation_entry(
        &mut xml,
        "Search",
        "/opds/search",
        "OpenSearch description",
        "navigation",
    );
    push_navigation_entry(
        &mut xml,
        "Authors",
        "/opds/authors",
        "Browse authors",
        "navigation",
    );
    push_navigation_entry(
        &mut xml,
        "Series",
        "/opds/series",
        "Browse series",
        "navigation",
    );
    push_navigation_entry(
        &mut xml,
        "Publishers",
        "/opds/publishers",
        "Browse publishers",
        "navigation",
    );
    push_navigation_entry(
        &mut xml,
        "Languages",
        "/opds/languages",
        "Browse languages",
        "navigation",
    );
    push_navigation_entry(
        &mut xml,
        "Ratings",
        "/opds/ratings",
        "Browse ratings",
        "navigation",
    );
    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}

async fn authors_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let total = opds_queries::count_opds_authors(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    let authors = opds_queries::list_opds_authors(&state.db, page, page_size)
        .await
        .map_err(|_| AppError::Internal)?;

    let mut xml = String::new();
    push_feed_header(&mut xml, "Authors", "/opds/authors", "navigation");
    push_opensearch_stats(&mut xml, total, page_size);
    push_pagination_links(&mut xml, "/opds/authors", page, page_size, total, &[]);

    for (author_id, author_name, book_count) in authors {
        let book_label = pluralize("book", book_count);
        push_navigation_entry(
            &mut xml,
            &author_name,
            &format!("/opds/authors/{}", urlencoding::encode(&author_id)),
            &format!("{book_count} {book_label}"),
            "navigation",
        );
    }

    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}

async fn author_books_feed(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let author_name = sqlx::query_scalar::<_, String>("SELECT name FROM authors WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let params = book_queries::ListBooksParams {
        author_id: Some(id.clone()),
        page,
        page_size,
        publisher: None,
        rating_bucket: None,
        ..Default::default()
    };
    let title = format!("Books by {author_name}");
    let xml = build_book_feed(
        &state,
        &title,
        &format!("/opds/authors/{}", urlencoding::encode(&id)),
        params,
        &[],
    )
    .await?;
    Ok(xml_response(xml))
}

async fn series_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let total = opds_queries::count_opds_series(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    let series = opds_queries::list_opds_series(&state.db, page, page_size)
        .await
        .map_err(|_| AppError::Internal)?;

    let mut xml = String::new();
    push_feed_header(&mut xml, "Series", "/opds/series", "navigation");
    push_opensearch_stats(&mut xml, total, page_size);
    push_pagination_links(&mut xml, "/opds/series", page, page_size, total, &[]);

    for (series_id, series_name, book_count) in series {
        push_navigation_entry(
            &mut xml,
            &series_name,
            &format!("/opds/series/{}", urlencoding::encode(&series_id)),
            &format!("{book_count} {}", pluralize("book", book_count)),
            "navigation",
        );
    }

    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}

async fn series_books_feed(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let series_name = sqlx::query_scalar::<_, String>("SELECT name FROM series WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let params = book_queries::ListBooksParams {
        series_id: Some(id.clone()),
        page,
        page_size,
        publisher: None,
        rating_bucket: None,
        ..Default::default()
    };
    let title = format!("Books in series {series_name}");
    let xml = build_book_feed(
        &state,
        &title,
        &format!("/opds/series/{}", urlencoding::encode(&id)),
        params,
        &[],
    )
    .await?;
    Ok(xml_response(xml))
}

async fn publishers_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let total = opds_queries::count_opds_publishers(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    let publishers = opds_queries::list_opds_publishers(&state.db, page, page_size)
        .await
        .map_err(|_| AppError::Internal)?;

    let mut xml = String::new();
    push_feed_header(&mut xml, "Publishers", "/opds/publishers", "navigation");
    push_opensearch_stats(&mut xml, total, page_size);
    push_pagination_links(&mut xml, "/opds/publishers", page, page_size, total, &[]);

    for (publisher_id, publisher_name, book_count) in publishers {
        push_navigation_entry(
            &mut xml,
            &publisher_name,
            &format!("/opds/publishers/{}", urlencoding::encode(&publisher_id)),
            &format!("{book_count} {}", pluralize("book", book_count)),
            "navigation",
        );
    }

    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}

async fn publisher_books_feed(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let params = book_queries::ListBooksParams {
        publisher: Some(id.clone()),
        page,
        page_size,
        rating_bucket: None,
        ..Default::default()
    };
    let title = format!("Books by publisher {id}");
    let xml = build_book_feed(
        &state,
        &title,
        &format!("/opds/publishers/{}", urlencoding::encode(&id)),
        params,
        &[],
    )
    .await?;
    Ok(xml_response(xml))
}

async fn languages_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let languages = opds_queries::list_opds_languages(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    let total = languages.len() as i64;
    let start = ((page - 1) * page_size).max(0) as usize;
    let end = (start + page_size as usize).min(languages.len());
    let page_items = if start >= languages.len() {
        Vec::new()
    } else {
        languages[start..end].to_vec()
    };

    let mut xml = String::new();
    push_feed_header(&mut xml, "Languages", "/opds/languages", "navigation");
    push_opensearch_stats(&mut xml, total, page_size);
    push_pagination_links(&mut xml, "/opds/languages", page, page_size, total, &[]);

    for (language_code, book_count) in page_items {
        push_navigation_entry(
            &mut xml,
            &language_code,
            &format!("/opds/languages/{}", urlencoding::encode(&language_code)),
            &format!("{book_count} {}", pluralize("book", book_count)),
            "navigation",
        );
    }

    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}

async fn language_books_feed(
    State(state): State<AppState>,
    Path(lang_code): Path<String>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let params = book_queries::ListBooksParams {
        language: Some(lang_code.clone()),
        page,
        page_size,
        publisher: None,
        rating_bucket: None,
        ..Default::default()
    };
    let title = format!("Books in {lang_code}");
    let xml = build_book_feed(
        &state,
        &title,
        &format!("/opds/languages/{}", urlencoding::encode(&lang_code)),
        params,
        &[],
    )
    .await?;
    Ok(xml_response(xml))
}

async fn ratings_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let ratings = opds_queries::list_opds_ratings(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    let total = ratings.len() as i64;
    let start = ((page - 1) * page_size).max(0) as usize;
    let end = (start + page_size as usize).min(ratings.len());
    let page_items = if start >= ratings.len() {
        Vec::new()
    } else {
        ratings[start..end].to_vec()
    };

    let mut xml = String::new();
    push_feed_header(&mut xml, "Ratings", "/opds/ratings", "navigation");
    push_opensearch_stats(&mut xml, total, page_size);
    push_pagination_links(&mut xml, "/opds/ratings", page, page_size, total, &[]);

    for (rating, book_count) in page_items {
        push_navigation_entry(
            &mut xml,
            &format!("{}★", rating),
            &format!("/opds/ratings/{rating}"),
            &format!("{book_count} {}", pluralize("book", book_count)),
            "navigation",
        );
    }

    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}

async fn rating_books_feed(
    State(state): State<AppState>,
    Path(rating): Path<i64>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    if !(1..=5).contains(&rating) {
        return Err(AppError::NotFound);
    }

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(50).clamp(1, 100);
    let params = book_queries::ListBooksParams {
        rating_bucket: Some(rating),
        page,
        page_size,
        publisher: None,
        ..Default::default()
    };
    let title = format!("Books rated {}★", rating);
    let xml = build_book_feed(
        &state,
        &title,
        &format!("/opds/ratings/{rating}"),
        params,
        &[],
    )
    .await?;
    Ok(xml_response(xml))
}

async fn all_books(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(30).clamp(1, 100);
    let params = book_queries::ListBooksParams {
        q: query.q,
        page,
        page_size,
        publisher: None,
        rating_bucket: None,
        ..Default::default()
    };
    let mut extra_params = Vec::new();
    if let Some(q) = params.q.clone() {
        extra_params.push(("q", q));
    }
    let xml = build_book_feed(&state, "All Books", "/opds/catalog", params, &extra_params).await?;
    Ok(xml_response(xml))
}

async fn search(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let Some(search_terms) = query
        .q
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(xml_response(open_search_description()));
    };

    let page = query.page.unwrap_or(1).max(1);
    let page_size = query.page_size.unwrap_or(30).clamp(1, 100);
    let params = book_queries::ListBooksParams {
        q: Some(search_terms.to_string()),
        page,
        page_size,
        publisher: None,
        rating_bucket: None,
        ..Default::default()
    };
    let extra_params = vec![("q", search_terms.to_string())];
    let xml = build_book_feed(
        &state,
        &format!("Search results for {search_terms}"),
        "/opds/search",
        params,
        &extra_params,
    )
    .await?;
    Ok(xml_response(xml))
}

async fn recently_added(State(state): State<AppState>) -> Result<Response, AppError> {
    let since = (Utc::now() - Duration::days(30)).to_rfc3339();
    let params = book_queries::ListBooksParams {
        since: Some(since.clone()),
        sort: Some("added".to_string()),
        order: Some("desc".to_string()),
        page: 1,
        page_size: 30,
        publisher: None,
        rating_bucket: None,
        ..Default::default()
    };
    let extra_params = vec![("since", since)];
    let xml = build_book_feed(&state, "Recently Added", "/opds/new", params, &extra_params).await?;
    Ok(xml_response(xml))
}

async fn build_book_feed(
    state: &AppState,
    title: &str,
    self_path: &str,
    params: book_queries::ListBooksParams,
    extra_query_params: &[(&str, String)],
) -> Result<String, AppError> {
    let page = book_queries::list_books(&state.db, &params)
        .await
        .map_err(|_| AppError::Internal)?;

    let mut xml = String::new();
    push_feed_header(&mut xml, title, self_path, "acquisition");
    push_opensearch_stats(&mut xml, page.total, page.page_size);
    push_pagination_links(
        &mut xml,
        self_path,
        page.page,
        page.page_size,
        page.total,
        extra_query_params,
    );

    for summary in page.items {
        if let Some(book) = book_queries::get_book_by_id(&state.db, &summary.id, None, None)
            .await
            .map_err(|_| AppError::Internal)?
        {
            push_book_entry(&mut xml, &book);
        }
    }

    push_feed_footer(&mut xml);
    Ok(xml)
}

fn xml_response(xml: String) -> Response {
    let mut response = Response::new(Body::from(xml));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/atom+xml; charset=utf-8"),
    );
    response
}

fn push_feed_header(xml: &mut String, title: &str, self_path: &str, kind: &str) {
    let _ = write!(
        xml,
        r#"<?xml version="1.0" encoding="utf-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:opensearch="http://a9.com/-/spec/opensearch/1.1/">
  <id>{}</id>
  <title>{}</title>
  <updated>{}</updated>
  <link rel="self" href="{}" type="application/atom+xml;profile=opds-catalog;kind={}" />
  <link rel="search" href="/opds/search" type="application/opensearchdescription+xml" />
"#,
        xml_escape(&format!("urn:uuid:{self_path}")),
        xml_escape(title),
        Utc::now().to_rfc3339(),
        xml_escape(self_path),
        xml_escape(kind),
    );
}

fn push_feed_footer(xml: &mut String) {
    xml.push_str("</feed>\n");
}

fn push_navigation_entry(xml: &mut String, title: &str, href: &str, summary: &str, kind: &str) {
    let _ = write!(
        xml,
        r#"  <entry>
    <title>{}</title>
    <id>{}</id>
    <updated>{}</updated>
    <summary>{}</summary>
    <link rel="subsection" href="{}" type="application/atom+xml;profile=opds-catalog;kind={}" />
  </entry>
"#,
        xml_escape(title),
        xml_escape(&format!("urn:uuid:{href}")),
        Utc::now().to_rfc3339(),
        xml_escape(summary),
        xml_escape(href),
        xml_escape(kind),
    );
}

fn push_pagination_links(
    xml: &mut String,
    self_path: &str,
    page: i64,
    page_size: i64,
    total: i64,
    extra_query_params: &[(&str, String)],
) {
    if page > 1 {
        let href = build_page_href(self_path, page - 1, page_size, extra_query_params);
        let _ = write!(
            xml,
            r#"  <link rel="previous" href="{}" />"#,
            xml_escape(&href)
        );
        xml.push('\n');
    }

    if page * page_size < total {
        let href = build_page_href(self_path, page + 1, page_size, extra_query_params);
        let _ = write!(xml, r#"  <link rel="next" href="{}" />"#, xml_escape(&href));
        xml.push('\n');
    }
}

fn build_page_href(
    self_path: &str,
    page: i64,
    page_size: i64,
    extra_query_params: &[(&str, String)],
) -> String {
    let mut href = format!("{self_path}?page={page}&page_size={page_size}");
    for (key, value) in extra_query_params {
        href.push('&');
        href.push_str(key);
        href.push('=');
        href.push_str(&urlencoding::encode(value));
    }
    href
}

fn push_opensearch_stats(xml: &mut String, total_results: i64, items_per_page: i64) {
    let _ = writeln!(
        xml,
        "  <opensearch:totalResults>{total_results}</opensearch:totalResults>"
    );
    let _ = writeln!(
        xml,
        "  <opensearch:itemsPerPage>{items_per_page}</opensearch:itemsPerPage>"
    );
}

fn pluralize(word: &str, count: i64) -> String {
    if count == 1 {
        word.to_string()
    } else {
        format!("{word}s")
    }
}

fn push_book_entry(xml: &mut String, book: &crate::db::models::Book) {
    let _ = write!(
        xml,
        r#"  <entry>
    <title>{}</title>
    <id>{}</id>
    <updated>{}</updated>
"#,
        xml_escape(&book.title),
        xml_escape(&format!("urn:uuid:{}", book.id)),
        xml_escape(&book.last_modified),
    );

    for author in &book.authors {
        let _ = writeln!(
            xml,
            r#"    <author><name>{}</name></author>"#,
            xml_escape(&author.name),
        );
    }

    if let Some(summary) = book
        .description
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        let _ = writeln!(xml, "    <summary>{}</summary>", xml_escape(summary));
    }

    if let Some(cover_url) = &book.cover_url {
        let _ = writeln!(
            xml,
            r#"    <link rel="http://opds-spec.org/image" href="{}" type="image/jpeg" />"#,
            xml_escape(cover_url),
        );
    }

    for format in &book.formats {
        let type_attr = acquisition_type_for_format(&format.format);
        let href = format!(
            "/api/v1/books/{}/formats/{}/download",
            book.id, format.format
        );
        let _ = writeln!(
            xml,
            r#"    <link rel="http://opds-spec.org/acquisition" href="{}" type="{}" />"#,
            xml_escape(&href),
            xml_escape(type_attr),
        );
    }

    xml.push_str("  </entry>\n");
}

fn acquisition_type_for_format(format: &str) -> &'static str {
    match format.trim().to_ascii_uppercase().as_str() {
        "EPUB" => "application/epub+zip",
        "PDF" => "application/pdf",
        "CBZ" => "application/vnd.comicbook+zip",
        "CBR" => "application/x-cbr",
        "MOBI" | "AZW3" => "application/vnd.amazon.ebook",
        _ => "application/octet-stream",
    }
}

fn xml_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&apos;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

fn open_search_description() -> String {
    r#"<?xml version="1.0" encoding="utf-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <ShortName>Autolibre</ShortName>
  <Description>Search the Autolibre library</Description>
  <InputEncoding>UTF-8</InputEncoding>
  <Image width="16" height="16" type="image/png">/assets/favicon.png</Image>
  <Url type="application/atom+xml" template="/opds/search?q={searchTerms}" />
</OpenSearchDescription>
"#
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::AppConfig,
        db::{connect_sqlite_pool, models::Book},
        AppState,
    };
    use axum_test::TestServer;
    use chrono::Utc;
    use tempfile::TempDir;
    use uuid::Uuid;

    struct OpdsTestContext {
        db: sqlx::SqlitePool,
        server: TestServer,
        _storage: TempDir,
    }

    impl OpdsTestContext {
        async fn new() -> Self {
            let storage = tempfile::tempdir().expect("storage tempdir");
            let db = connect_sqlite_pool("sqlite::memory:", 1)
                .await
                .expect("connect sqlite");
            let migration_path =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/sqlite");
            let migrator = sqlx::migrate::Migrator::new(migration_path.as_path())
                .await
                .expect("load migrations");
            migrator.run(&db).await.expect("run migrations");

            let mut config = AppConfig::default();
            config.app.storage_path = storage.path().to_string_lossy().to_string();
            if config.auth.jwt_secret.trim().is_empty() {
                config.auth.jwt_secret = "test-secret-test-secret-test-secret-test".to_string();
            }

            let state = AppState::new(db.clone(), config).await;
            let server = TestServer::new(crate::app(state)).expect("build test server");
            Self {
                db,
                server,
                _storage: storage,
            }
        }

        async fn create_book_with_file(&self, title: &str, format: &str) -> Book {
            let now = Utc::now().to_rfc3339();
            let id = Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO books (id, title, sort_title, description, pubdate, language, rating, series_id, series_index, has_cover, cover_path, flags, indexed_at, created_at, last_modified)
                VALUES (?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL, NULL, ?, ?)
                "#,
            )
            .bind(&id)
            .bind(title)
            .bind(title)
            .bind(&now)
            .bind(&now)
            .execute(&self.db)
            .await
            .expect("insert book");

            let author_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)",
            )
            .bind(&author_id)
            .bind("Author")
            .bind("Author")
            .bind(&now)
            .execute(&self.db)
            .await
            .expect("insert author");

            sqlx::query(
                "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, 0)",
            )
            .bind(&id)
            .bind(&author_id)
            .execute(&self.db)
            .await
            .expect("insert book author");

            let file_name = format!("{}.{}", id, format.to_lowercase());
            let path = self._storage.path().join(&file_name);
            std::fs::write(&path, b"fixture").expect("write format file");

            sqlx::query(
                r#"
                INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(Uuid::new_v4().to_string())
            .bind(&id)
            .bind(format)
            .bind(&file_name)
            .bind(7_i64)
            .bind(&now)
            .bind(&now)
            .execute(&self.db)
            .await
            .expect("insert format");

            let cover_file_name = format!("{id}.jpg");
            let cover_path = self._storage.path().join(&cover_file_name);
            std::fs::write(&cover_path, b"cover").expect("write cover file");

            crate::db::queries::books::set_book_cover_path(&self.db, &id, &cover_file_name)
                .await
                .expect("set cover path");

            crate::db::queries::books::get_book_by_id(&self.db, &id, None, None)
                .await
                .expect("load book")
                .expect("book exists")
        }
    }

    #[tokio::test]
    async fn test_opds_root_returns_atom_xml() {
        let ctx = OpdsTestContext::new().await;

        let response = ctx.server.get("/opds").await;
        assert_eq!(response.status_code().as_u16(), 200);
        let content_type_header = response.header(header::CONTENT_TYPE);
        let content_type = content_type_header.to_str().expect("content type");
        assert!(content_type.starts_with("application/atom+xml"));
        let body = String::from_utf8(response.as_bytes().to_vec()).expect("xml");
        assert!(body.contains("<feed"));
    }

    #[tokio::test]
    async fn test_opds_catalog_paginated() {
        let ctx = OpdsTestContext::new().await;
        let _ = ctx.create_book_with_file("Alpha", "EPUB").await;
        let _ = ctx.create_book_with_file("Beta", "EPUB").await;
        let _ = ctx.create_book_with_file("Gamma", "EPUB").await;

        let response = ctx
            .server
            .get("/opds/catalog")
            .add_query_param("page", 1)
            .add_query_param("page_size", 2)
            .await;

        assert_eq!(response.status_code().as_u16(), 200);
        let body = String::from_utf8(response.as_bytes().to_vec()).expect("xml");
        assert!(body.contains("rel=\"next\""));
        assert!(body.contains("rel=\"http://opds-spec.org/acquisition\""));
        assert!(body.contains("rel=\"http://opds-spec.org/image\""));
    }

    #[tokio::test]
    async fn test_opds_search_returns_results() {
        let ctx = OpdsTestContext::new().await;
        let book = ctx.create_book_with_file("Searchable Dune", "EPUB").await;

        let response = ctx
            .server
            .get("/opds/search")
            .add_query_param("q", "Searchable")
            .await;

        assert_eq!(response.status_code().as_u16(), 200);
        let body = String::from_utf8(response.as_bytes().to_vec()).expect("xml");
        assert!(body.contains("Searchable Dune"));
        assert!(body.contains("rel=\"http://opds-spec.org/acquisition\""));
        assert!(body.contains(&book.id));
    }

    #[tokio::test]
    async fn test_opds_download_requires_auth() {
        let ctx = OpdsTestContext::new().await;
        let book = ctx.create_book_with_file("Downloadable", "EPUB").await;

        let response = ctx
            .server
            .get(&format!("/api/v1/books/{}/formats/EPUB/download", book.id))
            .await;

        assert_eq!(response.status_code().as_u16(), 401);
    }
}
