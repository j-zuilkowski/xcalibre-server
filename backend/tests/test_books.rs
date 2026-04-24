#![allow(dead_code, unused_imports)]

mod common;

use axum_test::multipart::{MultipartForm, Part};
use backend::db::queries::books::{list_books, ListBooksParams};
use chrono::{Duration, Utc};
use common::{
    auth_header, epub_with_cover_bytes, minimal_epub_bytes, minimal_pdf_bytes, TestContext,
};
use futures::{future::BoxFuture, stream::BoxStream};
use sqlx::{Database, Describe, Either, Error, Execute, Executor, Row, Sqlite};
use std::fmt;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

#[derive(Clone, Copy)]
struct CountingExecutor<'a> {
    db: &'a sqlx::SqlitePool,
    queries: &'a AtomicUsize,
}

impl fmt::Debug for CountingExecutor<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CountingExecutor").finish_non_exhaustive()
    }
}

impl<'c, 'a> Executor<'c> for &'c CountingExecutor<'a>
where
    'a: 'c,
{
    type Database = Sqlite;

    fn fetch_many<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxStream<
        'e,
        Result<
            Either<<Self::Database as Database>::QueryResult, <Self::Database as Database>::Row>,
            Error,
        >,
    >
    where
        'c: 'e,
        E: 'q + Execute<'q, Self::Database>,
    {
        self.queries.fetch_add(1, Ordering::SeqCst);
        self.db.fetch_many(query)
    }

    fn fetch_optional<'e, 'q: 'e, E>(
        self,
        query: E,
    ) -> BoxFuture<'e, Result<Option<<Self::Database as Database>::Row>, Error>>
    where
        'c: 'e,
        E: 'q + Execute<'q, Self::Database>,
    {
        self.queries.fetch_add(1, Ordering::SeqCst);
        self.db.fetch_optional(query)
    }

    fn prepare_with<'e, 'q: 'e>(
        self,
        sql: &'q str,
        parameters: &'e [<Self::Database as Database>::TypeInfo],
    ) -> BoxFuture<'e, Result<<Self::Database as Database>::Statement<'q>, Error>>
    where
        'c: 'e,
    {
        self.db.prepare_with(sql, parameters)
    }

    fn describe<'e, 'q: 'e>(
        self,
        sql: &'q str,
    ) -> BoxFuture<'e, Result<Describe<Self::Database>, Error>>
    where
        'c: 'e,
    {
        self.db.describe(sql)
    }
}

async fn seed_list_books_fixtures(ctx: &TestContext, books: usize) {
    let now = Utc::now().to_rfc3339();

    for book_index in 0..books {
        let book = ctx
            .create_book(
                &format!("Book {book_index:02}"),
                &format!("Author {book_index:02}-0"),
            )
            .await;

        for author_index in 1..3 {
            let author_id = Uuid::new_v4().to_string();
            let author_name = format!("Author {book_index:02}-{author_index}");
            sqlx::query(
                "INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)",
            )
            .bind(&author_id)
            .bind(&author_name)
            .bind(&author_name)
            .bind(&now)
            .execute(&ctx.db)
            .await
            .expect("insert author");

            sqlx::query(
                "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, ?)",
            )
            .bind(&book.id)
            .bind(&author_id)
            .bind(author_index as i64)
            .execute(&ctx.db)
            .await
            .expect("insert book author");
        }

        for tag_index in 0..5 {
            let tag_id = Uuid::new_v4().to_string();
            let tag_name = format!("Tag {book_index:02}-{tag_index}");
            sqlx::query(
                "INSERT INTO tags (id, name, source, last_modified) VALUES (?, ?, 'manual', ?)",
            )
            .bind(&tag_id)
            .bind(&tag_name)
            .bind(&now)
            .execute(&ctx.db)
            .await
            .expect("insert tag");

            sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
                .bind(&book.id)
                .bind(&tag_id)
                .execute(&ctx.db)
                .await
                .expect("insert book tag");
        }
    }
}

#[tokio::test]
async fn test_list_books_empty_library() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 0);
    assert_eq!(body["items"].as_array().map(Vec::len), Some(0));
}

#[tokio::test]
async fn test_list_books_pagination() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let _ = ctx.create_book("Alpha", "Author A").await;
    let _ = ctx.create_book("Beta", "Author B").await;
    let _ = ctx.create_book("Gamma", "Author C").await;

    let first = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("page", 1)
        .add_query_param("page_size", 2)
        .add_query_param("sort", "title")
        .add_query_param("order", "asc")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(first, 200);
    let first_body: serde_json::Value = first.json();
    assert_eq!(first_body["total"], 3);
    assert_eq!(first_body["items"].as_array().map(Vec::len), Some(2));
    assert_eq!(first_body["items"][0]["title"], "Alpha");
    assert_eq!(first_body["items"][1]["title"], "Beta");

    let second = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("page", 2)
        .add_query_param("page_size", 2)
        .add_query_param("sort", "title")
        .add_query_param("order", "asc")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(second, 200);
    let second_body: serde_json::Value = second.json();
    assert_eq!(second_body["items"].as_array().map(Vec::len), Some(1));
    assert_eq!(second_body["items"][0]["title"], "Gamma");
}

#[tokio::test]
async fn test_list_books_does_not_n_plus_one() {
    let ctx = TestContext::new().await;
    seed_list_books_fixtures(&ctx, 10).await;

    let query_count = AtomicUsize::new(0);
    let executor = CountingExecutor {
        db: &ctx.db,
        queries: &query_count,
    };
    let params = ListBooksParams {
        sort: Some("title".to_string()),
        order: Some("asc".to_string()),
        page: 1,
        page_size: 10,
        ..Default::default()
    };

    let page = list_books(&executor, &params).await.expect("list books");

    assert_eq!(page.total, 10);
    assert_eq!(page.items.len(), 10);
    assert!(
        page.items
            .iter()
            .all(|item| item.authors.len() == 3 && item.tags.len() == 5),
        "each summary should include 3 authors and 5 tags",
    );
    assert!(
        page.items
            .iter()
            .all(|item| item.tags.iter().all(|tag| tag.confirmed)),
        "all test tags should be confirmed",
    );
    assert!(
        query_count.load(Ordering::SeqCst) <= 3,
        "expected at most 3 SQL statements, got {}",
        query_count.load(Ordering::SeqCst)
    );
}

#[tokio::test]
async fn test_list_books_filter_by_author() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let target = ctx.create_book("Match", "Target Author").await;
    let _ = ctx.create_book("Other", "Other Author").await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("author_id", target.authors[0].id.clone())
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], target.id);
}

#[tokio::test]
async fn test_list_books_filter_by_tag() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let target = ctx.create_book("Tagged", "Author A").await;
    let other = ctx.create_book("Untagged", "Author B").await;
    let now = Utc::now().to_rfc3339();
    let tag_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tags (id, name, source, last_modified) VALUES (?, ?, 'manual', ?)")
        .bind(&tag_id)
        .bind("SciFi")
        .bind(&now)
        .execute(&ctx.db)
        .await
        .expect("insert tag");
    sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
        .bind(&target.id)
        .bind(&tag_id)
        .execute(&ctx.db)
        .await
        .expect("insert book tag");

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("tag", "SciFi")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], target.id);
    assert_ne!(body["items"][0]["id"], other.id);
}

#[tokio::test]
async fn test_reading_progress_surfaces_in_library_list() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, _) = ctx.create_book_with_file("Progress Book", "EPUB").await;
    let format_id = book.formats[0].id.clone();

    let patch_response = ctx
        .server
        .patch(format!("/api/v1/reading-progress/{}", book.id).as_str())
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "format_id": format_id,
            "percentage": 42,
            "cfi": null,
            "page": null,
        }))
        .await;

    assert_status!(patch_response, 200);
    let patch_body: serde_json::Value = patch_response.json();
    assert_eq!(patch_body["percentage"], 42.0);

    let list_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(list_response, 200);
    let body: serde_json::Value = list_response.json();
    let item = body["items"]
        .as_array()
        .and_then(|items| items.iter().find(|item| item["id"] == book.id))
        .expect("book in library list");
    assert_eq!(item["progress_percentage"], 42.0);
}

#[tokio::test]
async fn test_list_books_sort_by_title() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let _ = ctx.create_book("Zulu", "Author A").await;
    let _ = ctx.create_book("Alpha", "Author B").await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("sort", "title")
        .add_query_param("order", "asc")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["items"][0]["title"], "Alpha");
    assert_eq!(body["items"][1]["title"], "Zulu");
}

#[tokio::test]
async fn test_list_books_since_returns_only_modified() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let old = ctx.create_book("Old", "Author A").await;
    let new = ctx.create_book("New", "Author B").await;

    let old_time = (Utc::now() - Duration::days(7)).to_rfc3339();
    sqlx::query("UPDATE books SET last_modified = ? WHERE id = ?")
        .bind(old_time)
        .bind(&old.id)
        .execute(&ctx.db)
        .await
        .expect("update old timestamp");

    let since = (Utc::now() - Duration::minutes(2)).to_rfc3339();
    let response = ctx
        .server
        .get("/api/v1/books")
        .add_query_param("since", since)
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["id"], new.id);
}

#[tokio::test]
async fn test_get_book_returns_full_relations() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Rich Book", "Author A").await;
    let now = Utc::now().to_rfc3339();

    let series_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO series (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
        .bind(&series_id)
        .bind("Series 1")
        .bind("Series 1")
        .bind(&now)
        .execute(&ctx.db)
        .await
        .expect("insert series");
    sqlx::query("UPDATE books SET series_id = ?, series_index = 2.5 WHERE id = ?")
        .bind(&series_id)
        .bind(&book.id)
        .execute(&ctx.db)
        .await
        .expect("link series");

    let tag_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tags (id, name, source, last_modified) VALUES (?, ?, 'manual', ?)")
        .bind(&tag_id)
        .bind("Fantasy")
        .bind(&now)
        .execute(&ctx.db)
        .await
        .expect("insert tag");
    sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
        .bind(&book.id)
        .bind(&tag_id)
        .execute(&ctx.db)
        .await
        .expect("insert tag link");

    let format_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified) VALUES (?, ?, 'EPUB', ?, ?, ?, ?)",
    )
    .bind(&format_id)
    .bind(&book.id)
    .bind(format!("{}.epub", book.id))
    .bind(100_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert format");

    let identifier_id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO identifiers (id, book_id, id_type, value, last_modified) VALUES (?, ?, 'isbn13', '9781111111111', ?)",
    )
    .bind(&identifier_id)
    .bind(&book.id)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert identifier");

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"], book.id);
    assert_eq!(body["series"]["id"], series_id);
    assert_eq!(body["tags"][0]["name"], "Fantasy");
    assert_eq!(body["formats"][0]["format"], "EPUB");
    assert_eq!(body["identifiers"][0]["id_type"], "isbn13");
}

#[tokio::test]
async fn test_get_book_not_found_returns_404() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/books/missing-book-id")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_upload_epub_extracts_metadata() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(minimal_epub_bytes())
            .file_name("minimal.epub")
            .mime_type("application/epub+zip"),
    );

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(form)
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["title"], "Test Book");
    assert_eq!(body["authors"][0]["name"], "Test Author");
    assert_eq!(body["formats"][0]["format"], "EPUB");
}

#[tokio::test]
async fn test_upload_epub_extracts_cover() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(epub_with_cover_bytes())
            .file_name("with-cover.epub")
            .mime_type("application/epub+zip"),
    );

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(form)
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["has_cover"], true);
    assert!(body["cover_url"].as_str().is_some());

    let book_id = body["id"].as_str().expect("book id").to_string();
    let row = sqlx::query("SELECT cover_path FROM books WHERE id = ?")
        .bind(&book_id)
        .fetch_one(&ctx.db)
        .await
        .expect("query cover path");
    let cover_path: Option<String> = row.get("cover_path");
    let cover_path = cover_path.expect("cover path");
    let thumb_path = cover_path.replace(".jpg", ".thumb.jpg");
    assert!(
        ctx.storage.path().join(&cover_path).exists(),
        "cover file should exist on disk"
    );
    assert!(
        ctx.storage.path().join(&thumb_path).exists(),
        "cover thumbnail should exist on disk"
    );
}

#[tokio::test]
async fn test_upload_pdf_no_cover_ok() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(minimal_pdf_bytes())
            .file_name("Upload Title - Upload Author.pdf")
            .mime_type("application/pdf"),
    );

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(form)
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["has_cover"], false);
    assert_eq!(body["cover_url"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_upload_unknown_format_returns_422() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes("not a supported ebook".as_bytes())
            .file_name("unknown.txt")
            .mime_type("text/plain"),
    );

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(form)
        .await;

    assert_status!(response, 422);
}

#[tokio::test]
async fn test_upload_magic_bytes_mismatch_returns_422() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let fake_epub = include_bytes!("fixtures/fake.epub").to_vec();

    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(fake_epub)
            .file_name("fake.epub")
            .mime_type("application/epub+zip"),
    );

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(form)
        .await;

    assert_status!(response, 422);
}

#[tokio::test]
async fn test_upload_duplicate_isbn_returns_409() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let metadata = serde_json::json!({
        "title": "Book One",
        "author": "Author One",
        "identifiers": [{ "id_type": "isbn13", "value": "978-1-4028-9462-6" }]
    })
    .to_string();

    let first_form = MultipartForm::new()
        .add_part(
            "file",
            Part::bytes(minimal_pdf_bytes())
                .file_name("first.pdf")
                .mime_type("application/pdf"),
        )
        .add_text("metadata", metadata);

    let first = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(first_form)
        .await;
    assert_status!(first, 201);

    let second_metadata = serde_json::json!({
        "title": "Book Two",
        "author": "Author Two",
        "identifiers": [{ "id_type": "isbn13", "value": "9781402894626" }]
    })
    .to_string();
    let second_form = MultipartForm::new()
        .add_part(
            "file",
            Part::bytes(minimal_pdf_bytes())
                .file_name("second.pdf")
                .mime_type("application/pdf"),
        )
        .add_text("metadata", second_metadata);

    let second = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(second_form)
        .await;
    assert_status!(second, 409);
}

#[tokio::test]
async fn test_upload_requires_upload_permission() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(minimal_pdf_bytes())
            .file_name("regular-user.pdf")
            .mime_type("application/pdf"),
    );

    let response = ctx
        .server
        .post("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .multipart(form)
        .await;

    assert_status!(response, 403);
}

#[tokio::test]
async fn test_patch_book_updates_fields() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Old Title", "Old Author").await;
    let now = Utc::now().to_rfc3339();
    let new_author_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
        .bind(&new_author_id)
        .bind("Replacement Author")
        .bind("Replacement Author")
        .bind(&now)
        .execute(&ctx.db)
        .await
        .expect("insert replacement author");

    let response = ctx
        .server
        .patch(&format!("/api/v1/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "title": "New Title",
            "language": "en",
            "rating": 9,
            "authors": [new_author_id],
            "identifiers": [{ "id_type": "isbn13", "value": "9782222222222" }]
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["title"], "New Title");
    assert_eq!(body["language"], "en");
    assert_eq!(body["rating"], 9);
    assert_eq!(body["authors"][0]["name"], "Replacement Author");
    assert_eq!(body["identifiers"][0]["value"], "9782222222222");
}

#[tokio::test]
async fn test_patch_book_writes_audit_log() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let login = ctx.login(&user.username, &password).await;
    let book = ctx.create_book("Audit Book", "Author A").await;

    let response = ctx
        .server
        .patch(&format!("/api/v1/books/{}", book.id))
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&login.access_token),
        )
        .json(&serde_json::json!({
            "title": "Audit Book Updated",
            "description": "changed"
        }))
        .await;
    assert_status!(response, 200);

    let row = sqlx::query(
        "SELECT COUNT(1) AS count FROM audit_log WHERE entity = 'book' AND entity_id = ? AND action = 'update'",
    )
    .bind(&book.id)
    .fetch_one(&ctx.db)
    .await
    .expect("query audit rows");
    let count: i64 = row.get("count");
    assert!(
        count >= 2,
        "expected at least 2 field audit rows, got {count}"
    );
}

#[tokio::test]
async fn test_patch_book_not_found_returns_404() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .patch("/api/v1/books/missing-book-id")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "title": "Nope" }))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_delete_book_removes_files() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (book, file_path) = ctx.create_book_with_file("Delete Me", "EPUB").await;
    assert!(file_path.exists());

    let response = ctx
        .server
        .delete(&format!("/api/v1/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    assert!(
        !file_path.exists(),
        "format file should be removed from storage"
    );

    let row = sqlx::query("SELECT COUNT(1) AS count FROM books WHERE id = ?")
        .bind(&book.id)
        .fetch_one(&ctx.db)
        .await
        .expect("query book count");
    let count: i64 = row.get("count");
    assert_eq!(count, 0);

    let audit_row = sqlx::query(
        "SELECT COUNT(1) AS count FROM audit_log WHERE entity = 'book' AND entity_id = ? AND action = 'delete' AND diff_json LIKE '%\"event\":\"book_delete\"%'",
    )
    .bind(&book.id)
    .fetch_one(&ctx.db)
    .await
    .expect("query delete audit row");
    let audit_count: i64 = audit_row.get("count");
    assert!(audit_count >= 1);
}

#[tokio::test]
async fn test_delete_book_requires_admin() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Protected", "Author A").await;

    let response = ctx
        .server
        .delete(&format!("/api/v1/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 403);
}
