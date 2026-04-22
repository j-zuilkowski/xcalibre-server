#![allow(dead_code, unused_imports)]

mod common;

use common::TestContext;
use roxmltree::Document;
use sqlx::Row;
use uuid::Uuid;

async fn create_opds_book(
    ctx: &TestContext,
    title: &str,
    author: &str,
    language: Option<&str>,
    rating: Option<i64>,
    series_name: Option<&str>,
    publisher: Option<&str>,
) -> backend::db::models::Book {
    let mut book = ctx.create_book(title, author).await;
    let now = chrono::Utc::now().to_rfc3339();

    if let Some(language) = language {
        sqlx::query("UPDATE books SET language = ?, last_modified = ? WHERE id = ?")
            .bind(language)
            .bind(&now)
            .bind(&book.id)
            .execute(&ctx.db)
            .await
            .expect("set language");
        book.language = Some(language.to_string());
    }

    if let Some(rating) = rating {
        sqlx::query("UPDATE books SET rating = ?, last_modified = ? WHERE id = ?")
            .bind(rating)
            .bind(&now)
            .bind(&book.id)
            .execute(&ctx.db)
            .await
            .expect("set rating");
        book.rating = Some(rating);
    }

    if let Some(series_name) = series_name {
        let series_id =
            match sqlx::query("SELECT id FROM series WHERE lower(name) = lower(?) LIMIT 1")
                .bind(series_name)
                .fetch_optional(&ctx.db)
                .await
                .expect("load series")
            {
                Some(row) => row.get::<String, _>("id"),
                None => {
                    let series_id = Uuid::new_v4().to_string();
                    sqlx::query(
                    "INSERT INTO series (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)",
                )
                .bind(&series_id)
                .bind(series_name)
                .bind(series_name)
                .bind(&now)
                .execute(&ctx.db)
                .await
                .expect("insert series");
                    series_id
                }
            };

        sqlx::query(
            "UPDATE books SET series_id = ?, series_index = 1, last_modified = ? WHERE id = ?",
        )
        .bind(&series_id)
        .bind(&now)
        .bind(&book.id)
        .execute(&ctx.db)
        .await
        .expect("set series");
        book.series = Some(backend::db::models::SeriesRef {
            id: series_id,
            name: series_name.to_string(),
        });
        book.series_index = Some(1.0);
    }

    if let Some(publisher) = publisher {
        sqlx::query(
            "UPDATE books SET flags = json_object('publisher', ?), last_modified = ? WHERE id = ?",
        )
        .bind(publisher)
        .bind(&now)
        .bind(&book.id)
        .execute(&ctx.db)
        .await
        .expect("set publisher");
    }

    let format_id = Uuid::new_v4().to_string();
    let file_name = format!("{}.epub", book.id);
    std::fs::write(ctx.storage.path().join(&file_name), b"fixture").expect("write format file");
    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, 'EPUB', ?, 7, ?, ?)
        "#,
    )
    .bind(&format_id)
    .bind(&book.id)
    .bind(&file_name)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert format");

    let cover_name = format!("{}.jpg", book.id);
    std::fs::write(ctx.storage.path().join(&cover_name), b"cover").expect("write cover");
    backend::db::queries::books::set_book_cover_path(&ctx.db, &book.id, &cover_name)
        .await
        .expect("set cover");

    book.formats.push(backend::db::models::FormatRef {
        id: format_id,
        format: "EPUB".to_string(),
        size_bytes: 7,
    });
    book.cover_url = Some(format!("/api/v1/books/{}/cover", book.id));
    book.has_cover = true;
    book
}

async fn create_series_book(
    ctx: &TestContext,
    title: &str,
    author: &str,
    series_name: &str,
    language: Option<&str>,
    rating: Option<i64>,
    publisher: Option<&str>,
) -> backend::db::models::Book {
    create_opds_book(
        ctx,
        title,
        author,
        language,
        rating,
        Some(series_name),
        publisher,
    )
    .await
}

fn parse_feed(body: &str) -> Document<'_> {
    Document::parse(body).expect("atom xml")
}

#[tokio::test]
async fn test_opds_authors_feed_returns_atom() {
    let ctx = TestContext::new().await;
    let _ = create_opds_book(&ctx, "Alpha", "Alice", Some("en"), Some(8), None, None).await;
    let _ = create_opds_book(&ctx, "Beta", "Bob", Some("fr"), Some(6), None, None).await;

    let response = ctx
        .server
        .get("/opds/authors")
        .add_query_param("page", 1)
        .add_query_param("page_size", 1)
        .await;

    assert_eq!(response.status_code().as_u16(), 200);
    let body = String::from_utf8(response.as_bytes().to_vec()).expect("xml body");
    let feed = parse_feed(&body);
    assert_eq!(feed.root_element().tag_name().name(), "feed");
    assert!(body.contains("opensearch:totalResults"));
    assert!(body.contains("rel=\"next\""));
    assert!(body.contains("kind=acquisition"));
}

#[tokio::test]
async fn test_opds_author_books_acquisition_feed() {
    let ctx = TestContext::new().await;
    let book = create_opds_book(
        &ctx,
        "Dune",
        "Frank Herbert",
        Some("en"),
        Some(10),
        None,
        None,
    )
    .await;
    let author_id = book.authors.first().expect("author").id.clone();

    let response = ctx.server.get(&format!("/opds/authors/{author_id}")).await;

    assert_eq!(response.status_code().as_u16(), 200);
    let body = String::from_utf8(response.as_bytes().to_vec()).expect("xml body");
    let feed = parse_feed(&body);
    assert_eq!(feed.root_element().tag_name().name(), "feed");
    assert!(body.contains("Dune"));
    assert!(body.contains("rel=\"http://opds-spec.org/acquisition\""));
    assert!(body.contains("rel=\"http://opds-spec.org/image\""));
}

#[tokio::test]
async fn test_opds_series_feed() {
    let ctx = TestContext::new().await;
    let _ = create_series_book(
        &ctx,
        "Foundation",
        "Isaac Asimov",
        "Foundation",
        Some("en"),
        Some(9),
        None,
    )
    .await;
    let _ = create_series_book(
        &ctx,
        "Foundation and Empire",
        "Isaac Asimov",
        "Foundation",
        Some("en"),
        Some(8),
        None,
    )
    .await;

    let response = ctx.server.get("/opds/series").await;

    assert_eq!(response.status_code().as_u16(), 200);
    let body = String::from_utf8(response.as_bytes().to_vec()).expect("xml body");
    let feed = parse_feed(&body);
    assert_eq!(feed.root_element().tag_name().name(), "feed");
    assert!(body.contains("Foundation"));
    assert!(body.contains("2 books"));
    assert!(body.contains("kind=acquisition"));
}

#[tokio::test]
async fn test_opds_language_feed() {
    let ctx = TestContext::new().await;
    let _ = create_opds_book(
        &ctx,
        "English Book",
        "Author A",
        Some("en"),
        Some(7),
        None,
        None,
    )
    .await;
    let _ = create_opds_book(
        &ctx,
        "French Book",
        "Author B",
        Some("fr"),
        Some(5),
        None,
        None,
    )
    .await;

    let response = ctx
        .server
        .get("/opds/languages")
        .add_query_param("page", 1)
        .add_query_param("page_size", 1)
        .await;

    assert_eq!(response.status_code().as_u16(), 200);
    let body = String::from_utf8(response.as_bytes().to_vec()).expect("xml body");
    let feed = parse_feed(&body);
    assert_eq!(feed.root_element().tag_name().name(), "feed");
    assert!(body.contains("opensearch:totalResults"));
    assert!(body.contains("rel=\"next\""));
    assert!(body.contains("kind=acquisition"));
}

#[tokio::test]
async fn test_opds_ratings_feed() {
    let ctx = TestContext::new().await;
    let _ = create_opds_book(
        &ctx,
        "Low Rating",
        "Author A",
        Some("en"),
        Some(1),
        None,
        None,
    )
    .await;
    let _ = create_opds_book(
        &ctx,
        "High Rating",
        "Author B",
        Some("en"),
        Some(10),
        None,
        None,
    )
    .await;

    let response = ctx.server.get("/opds/ratings").await;

    assert_eq!(response.status_code().as_u16(), 200);
    let body = String::from_utf8(response.as_bytes().to_vec()).expect("xml body");
    let feed = parse_feed(&body);
    assert_eq!(feed.root_element().tag_name().name(), "feed");
    assert!(body.contains("1★"));
    assert!(body.contains("5★"));
    assert!(body.contains("kind=acquisition"));
}
