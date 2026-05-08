//! Phase 23 OPDS enhancement handlers (auth-required cover, stats, discover, etc.).

use crate::{
    api::opds_cover_cache::{CoverCacheKey, CoverVariant},
    db::queries::{books as book_queries},
    AppError, AppState,
};
use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue},
    response::Response,
};
use chrono::Utc;
use image::{load_from_memory, ImageFormat};
use serde_json;
use sqlx::Row;
use std::fmt::Write as _;
use unicode_normalization::UnicodeNormalization;

/// Validates a Bearer token from the request headers.
fn validate_opds_auth(headers: &HeaderMap, state: &AppState) -> Result<(), AppError> {
    use crate::middleware::auth::{bearer_token, validate_access_token};
    let token = bearer_token(headers).ok_or(AppError::Unauthorized)?;
    let _claims = validate_access_token(token, &state.config.auth.jwt_secret)?;
    Ok(())
}

fn wants_webp(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("image/webp"))
        .unwrap_or(false)
}

fn resize_dimensions(variant: CoverVariant) -> Option<(u32, u32)> {
    match variant {
        CoverVariant::Original => None,
        CoverVariant::Thumb240 => Some((240, 240)),
        CoverVariant::Large600 => Some((600, 600)),
    }
}

async fn serve_cover(
    state: &AppState,
    book_id: &str,
    variant: CoverVariant,
    accept_webp: bool,
) -> Result<Response, AppError> {
    let cover_path = book_queries::find_book_cover_path(&state.db, book_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::NotFound)?;

    let raw = state.storage.get_bytes(&cover_path).await.map_err(|_| AppError::NotFound)?;

    let cache_key = CoverCacheKey {
        book_id: book_id.to_string(),
        variant,
        webp: accept_webp,
    };

    if let Some(cached) = state.opds_cover_cache.get(&cache_key).await {
        let ct = if accept_webp { "image/webp" } else { "image/jpeg" };
        return Ok(build_image_response(cached, ct));
    }

    let img = load_from_memory(&raw).map_err(|_| AppError::NotFound)?;
    let output_format = if accept_webp { ImageFormat::WebP } else { ImageFormat::Jpeg };

    let processed = if let Some((w, h)) = resize_dimensions(variant) {
        let resized = img.thumbnail(w, h);
        let mut buf = Vec::new();
        resized
            .write_to(&mut std::io::Cursor::new(&mut buf), output_format)
            .map_err(|_| AppError::Internal)?;
        buf
    } else {
        let mut buf = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut buf), output_format)
            .map_err(|_| AppError::Internal)?;
        buf
    };

    state.opds_cover_cache.put(cache_key, processed.clone()).await;

    let ct = if accept_webp { "image/webp" } else { "image/jpeg" };
    Ok(build_image_response(processed, ct))
}

fn build_image_response(bytes: Vec<u8>, content_type: &str) -> Response {
    let mut response = Response::new(Body::from(bytes));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type).expect("valid content type"),
    );
    response
        .headers_mut()
        .insert(header::CONTENT_DISPOSITION, HeaderValue::from_static("inline"));
    response
}

pub(super) async fn opds_cover_handler(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    serve_cover(&state, &book_id, CoverVariant::Original, wants_webp(&headers)).await
}

pub(super) async fn opds_cover_thumb(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    serve_cover(&state, &book_id, CoverVariant::Thumb240, wants_webp(&headers)).await
}

pub(super) async fn opds_cover_large(
    State(state): State<AppState>,
    Path(book_id): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    serve_cover(&state, &book_id, CoverVariant::Large600, wants_webp(&headers)).await
}

pub(super) async fn opds_osd_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <ShortName>xcalibre OPDS</ShortName>
  <Description>Search xcalibre OPDS catalog</Description>
  <Url type="application/atom+xml;profile=opds-catalog" template="/opds/search?q={searchTerms}" />
</OpenSearchDescription>
"#
    .to_string();
    let mut response = Response::new(Body::from(xml));
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/opensearchdescription+xml"),
    );
    Ok(response)
}

pub(super) async fn opds_search_path(
    State(state): State<AppState>,
    Path(query): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    let q = query.trim().to_string();
    if q.is_empty() {
        return Err(AppError::BadRequest);
    }
    let params = crate::db::queries::books::ListBooksParams {
        q: Some(q.clone()),
        page: 1,
        page_size: 30,
        publisher: None,
        rating_bucket: None,
        ..Default::default()
    };
    let xml = super::opds::build_book_feed(
        &state,
        &format!("Search results for {q}"),
        &format!("/opds/search/{q}"),
        params,
        &[("q", q.clone())],
    )
    .await?;
    Ok(super::opds::xml_response(xml))
}

pub(super) async fn opds_new_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    let params = crate::db::queries::books::ListBooksParams {
        sort: Some("added".to_string()),
        order: Some("desc".to_string()),
        page: 1,
        page_size: 30,
        publisher: None,
        rating_bucket: None,
        ..Default::default()
    };
    let xml = super::opds::build_book_feed(&state, "New Books", "/opds/new", params, &[]).await?;
    Ok(super::opds::xml_response(xml))
}

pub(super) async fn opds_hot_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    let books = sqlx::query(
        "SELECT b.id, b.title, MAX(b.created_at) AS created_at FROM books b LEFT JOIN download_history dh ON dh.book_id = b.id GROUP BY b.id ORDER BY COUNT(dh.book_id) DESC, MAX(b.created_at) DESC LIMIT 30",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let mut xml = String::new();
    super::opds::push_feed_header(&mut xml, "Hot Books", "/opds/hot", "acquisition");
    super::opds::push_opensearch_stats(&mut xml, books.len() as i64, 30);

    for row in &books {
        let book_id: String = row.get("id");
        let title: String = row.get("title");
        let _ = write!(
            xml,
            "  <entry>\n    <title>{}</title>\n    <id>{}</id>\n    <updated>{}</updated>\n  </entry>\n",
            super::opds::xml_escape(&title),
            super::opds::xml_escape(&format!("urn:uuid:{book_id}")),
            Utc::now().to_rfc3339(),
        );
    }

    super::opds::push_feed_footer(&mut xml);
    Ok(super::opds::xml_response(xml))
}

pub(super) async fn opds_stats_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    let row = sqlx::query(
        "SELECT (SELECT COUNT(*) FROM books) AS total_books, (SELECT COUNT(DISTINCT ba.author_id) FROM book_authors ba) AS total_authors, (SELECT COUNT(DISTINCT COALESCE(series_id, '')) FROM books WHERE series_id IS NOT NULL) AS total_series, (SELECT COUNT(DISTINCT bt.tag_id) FROM book_tags bt) AS total_tags, (SELECT COUNT(DISTINCT format) FROM formats) AS total_formats",
    )
    .fetch_one(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let stats = serde_json::json!({
        "total_books": row.get::<i64, _>("total_books"),
        "total_authors": row.get::<i64, _>("total_authors"),
        "total_series": row.get::<i64, _>("total_series"),
        "total_tags": row.get::<i64, _>("total_tags"),
        "total_formats": row.get::<i64, _>("total_formats"),
    });

    let body = serde_json::to_vec(&stats).map_err(|_| AppError::Internal)?;
    let mut response = Response::new(Body::from(body));
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(response)
}

pub(super) async fn opds_discover_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    let shelves = sqlx::query("SELECT id, name FROM shelves WHERE is_public = 1 ORDER BY name ASC")
        .fetch_all(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;

    let mut xml = String::new();
    super::opds::push_feed_header(&mut xml, "Discover", "/opds/discover", "navigation");
    super::opds::push_opensearch_stats(&mut xml, shelves.len() as i64, shelves.len().max(1) as i64);

    for row in &shelves {
        let shelf_id: String = row.get("id");
        let shelf_name: String = row.get("name");
        super::opds::push_navigation_entry(
            &mut xml,
            &shelf_name,
            &format!("/opds/discover/{shelf_id}"),
            &format!("Shelf: {shelf_name}"),
            "navigation",
        );
    }

    super::opds::push_feed_footer(&mut xml);
    Ok(super::opds::xml_response(xml))
}

pub(super) async fn opds_authors_letter_handler(
    State(state): State<AppState>,
    Path(ch): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    let letter = normalized_letter(&ch).unwrap_or_default();
    if letter.is_empty() {
        return Err(AppError::BadRequest);
    }

    let authors = sqlx::query(
        "SELECT a.id, a.name, COUNT(DISTINCT ba.book_id) AS book_count FROM authors a INNER JOIN book_authors ba ON ba.author_id = a.id WHERE UPPER(SUBSTR(a.sort_name, 1, 1)) = ? GROUP BY a.id, a.name, a.sort_name ORDER BY a.sort_name ASC",
    )
    .bind(&letter)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let mut xml = String::new();
    let title = format!("Authors — {letter}");
    super::opds::push_feed_header(&mut xml, &title, &format!("/opds/authors/letter/{ch}"), "navigation");
    super::opds::push_opensearch_stats(&mut xml, authors.len() as i64, authors.len().max(1) as i64);

    for row in &authors {
        let author_id: String = row.get("id");
        let author_name: String = row.get("name");
        let book_count: i64 = row.get("book_count");
        super::opds::push_navigation_entry(
            &mut xml,
            &author_name,
            &format!("/opds/authors/{}", urlencoding::encode(&author_id)),
            &format!("{book_count} {}", super::opds::pluralize("book", book_count)),
            "navigation",
        );
    }

    super::opds::push_feed_footer(&mut xml);
    Ok(super::opds::xml_response(xml))
}

pub(super) async fn opds_series_letter_handler(
    State(state): State<AppState>,
    Path(ch): Path<String>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    validate_opds_auth(&headers, &state)?;
    let letter = ch.to_ascii_uppercase();
    if letter.is_empty() {
        return Err(AppError::BadRequest);
    }

    let series = sqlx::query(
        "SELECT s.id, s.name, COUNT(DISTINCT b.id) AS book_count FROM series s INNER JOIN books b ON b.series_id = s.id WHERE UPPER(SUBSTR(s.name, 1, 1)) = ? GROUP BY s.id, s.name, s.sort_name ORDER BY s.name ASC",
    )
    .bind(&letter)
    .fetch_all(&state.db)
    .await
    .map_err(|_| AppError::Internal)?;

    let mut xml = String::new();
    let title = format!("Series — {letter}");
    super::opds::push_feed_header(&mut xml, &title, &format!("/opds/series/letter/{ch}"), "navigation");
    super::opds::push_opensearch_stats(&mut xml, series.len() as i64, series.len().max(1) as i64);

    for row in &series {
        let series_id: String = row.get("id");
        let series_name: String = row.get("name");
        let book_count: i64 = row.get("book_count");
        super::opds::push_navigation_entry(
            &mut xml,
            &series_name,
            &format!("/opds/series/{}", urlencoding::encode(&series_id)),
            &format!("{book_count} {}", super::opds::pluralize("book", book_count)),
            "navigation",
        );
    }

    super::opds::push_feed_footer(&mut xml);
    Ok(super::opds::xml_response(xml))
}

fn normalized_letter(input: &str) -> Option<String> {
    let ch = input.nfkd().next()?;
    Some(ch.to_ascii_uppercase().to_string())
}
