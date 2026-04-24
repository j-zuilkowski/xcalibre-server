#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use axum_test::multipart::{MultipartForm, Part};
use backend::ingest::goodreads::{parse_goodreads_csv, parse_storygraph_csv};
use chrono::Utc;
use common::{auth_header, TestContext};
use sqlx::Row;
use std::time::Duration;
use tokio::time::{sleep, timeout};
use uuid::Uuid;

fn goodreads_header() -> &'static str {
    "Book Id,Title,Author,Author l-f,Additional Authors,ISBN,ISBN13,My Rating,Average Rating,Publisher,Binding,Number of Pages,Year Published,Original Publication Year,Date Read,Date Added,Bookshelves,Bookshelves with positions,Exclusive Shelf,My Review,Spoiler,Private Notes,Read Count,Owned Copies"
}

fn goodreads_row(
    title: &str,
    author: &str,
    my_rating: u8,
    date_read: Option<&str>,
    bookshelves: &str,
    exclusive_shelf: &str,
) -> String {
    let mut fields = vec![String::new(); 24];
    fields[1] = csv_cell(title);
    fields[2] = csv_cell(author);
    fields[7] = csv_cell(&my_rating.to_string());
    fields[14] = csv_cell(date_read.unwrap_or_default());
    fields[16] = csv_cell(bookshelves);
    fields[18] = csv_cell(exclusive_shelf);
    fields.join(",")
}

fn goodreads_csv(
    title: &str,
    author: &str,
    my_rating: u8,
    date_read: Option<&str>,
    bookshelves: &str,
    exclusive_shelf: &str,
) -> String {
    format!(
        "{}\n{}",
        goodreads_header(),
        goodreads_row(
            title,
            author,
            my_rating,
            date_read,
            bookshelves,
            exclusive_shelf
        )
    )
}

fn storygraph_header() -> &'static str {
    "Title,Authors,Read Status,Star Rating (x/5),Review,Last Date Read,Dates Read,Tags,Owned"
}

fn storygraph_row(
    title: &str,
    authors: &str,
    read_status: &str,
    star_rating: Option<f32>,
    date_finished: Option<&str>,
    tags: &str,
) -> String {
    let mut fields = vec![String::new(); 9];
    fields[0] = csv_cell(title);
    fields[1] = csv_cell(authors);
    fields[2] = csv_cell(read_status);
    fields[3] = csv_cell(
        &star_rating
            .map(|value| value.to_string())
            .unwrap_or_default(),
    );
    fields[5] = csv_cell(date_finished.unwrap_or_default());
    fields[7] = csv_cell(tags);
    fields.join(",")
}

fn storygraph_csv(
    title: &str,
    authors: &str,
    read_status: &str,
    star_rating: Option<f32>,
    date_finished: Option<&str>,
    tags: &str,
) -> String {
    format!(
        "{}\n{}",
        storygraph_header(),
        storygraph_row(
            title,
            authors,
            read_status,
            star_rating,
            date_finished,
            tags
        )
    )
}

fn csv_cell(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') || value.contains('\r') {
        let escaped = value.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        value.to_string()
    }
}

async fn upload_goodreads_import(ctx: &TestContext, token: &str, csv: &str) -> String {
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(csv.as_bytes().to_vec())
            .file_name("goodreads.csv")
            .mime_type("text/csv"),
    );

    let response = ctx
        .server
        .post("/api/v1/users/me/import/goodreads")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .multipart(form)
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    body["job_id"].as_str().expect("job id").to_string()
}

async fn upload_storygraph_import(ctx: &TestContext, token: &str, csv: &str) -> String {
    let form = MultipartForm::new().add_part(
        "file",
        Part::bytes(csv.as_bytes().to_vec())
            .file_name("storygraph.csv")
            .mime_type("text/csv"),
    );

    let response = ctx
        .server
        .post("/api/v1/users/me/import/storygraph")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .multipart(form)
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    body["job_id"].as_str().expect("job id").to_string()
}

async fn wait_for_import_status(ctx: &TestContext, token: &str, job_id: &str) -> serde_json::Value {
    timeout(Duration::from_secs(5), async {
        loop {
            let response = ctx
                .server
                .get(&format!("/api/v1/users/me/import/{job_id}"))
                .add_header(header::AUTHORIZATION, auth_header(token))
                .await;

            assert_status!(response, 200);
            let body: serde_json::Value = response.json();
            let status = body["status"].as_str().unwrap_or_default();
            if status != "pending" && status != "running" {
                return body;
            }

            sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("import job to finish")
}

async fn query_i64(ctx: &TestContext, sql: &str, args: &[&str]) -> i64 {
    let mut query = sqlx::query_scalar::<_, i64>(sql);
    for arg in args {
        query = query.bind(arg);
    }
    query.fetch_one(&ctx.db).await.expect("query i64")
}

#[tokio::test]
async fn test_parse_goodreads_csv_extracts_title_and_author() {
    let csv = goodreads_csv("The Hobbit", "J.R.R. Tolkien", 0, None, "", "to-read");
    let rows = parse_goodreads_csv(csv.as_bytes()).expect("parse goodreads csv");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].title, "The Hobbit");
    assert_eq!(rows[0].author, "J.R.R. Tolkien");
}

#[tokio::test]
async fn test_parse_goodreads_csv_handles_empty_date_read() {
    let csv = goodreads_csv("The Hobbit", "J.R.R. Tolkien", 0, None, "", "to-read");
    let rows = parse_goodreads_csv(csv.as_bytes()).expect("parse goodreads csv");
    assert_eq!(rows[0].date_read, None);
}

#[tokio::test]
async fn test_parse_goodreads_csv_splits_bookshelves() {
    let csv = goodreads_csv(
        "The Hobbit",
        "J.R.R. Tolkien",
        0,
        None,
        "Fantasy, Classics, ",
        "to-read",
    );
    let rows = parse_goodreads_csv(csv.as_bytes()).expect("parse goodreads csv");
    assert_eq!(rows[0].bookshelves, vec!["Fantasy", "Classics"]);
}

#[tokio::test]
async fn test_import_marks_matching_book_as_read() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("The Hobbit", "J.R.R. Tolkien").await;
    let csv = goodreads_csv(
        "The Hobbit",
        "J.R.R. Tolkien",
        0,
        Some("2024/01/02"),
        "",
        "read",
    );

    let job_id = upload_goodreads_import(&ctx, &token, &csv).await;
    let _ = wait_for_import_status(&ctx, &token, &job_id).await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = 'user'")
        .fetch_one(&ctx.db)
        .await
        .expect("fetch user id");

    let row = sqlx::query(
        "SELECT is_read, updated_at FROM book_user_state WHERE user_id = ? AND book_id = ?",
    )
    .bind(&user_id)
    .bind(&book.id)
    .fetch_one(&ctx.db)
    .await
    .expect("fetch user state");
    assert_eq!(row.get::<i64, _>("is_read"), 1);
    assert_eq!(row.get::<String, _>("updated_at"), "2024/01/02");
}

#[tokio::test]
async fn test_import_sets_rating_from_goodreads_stars() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("The Hobbit", "J.R.R. Tolkien").await;
    let csv = goodreads_csv("The Hobbit", "J.R.R. Tolkien", 4, None, "", "to-read");

    let job_id = upload_goodreads_import(&ctx, &token, &csv).await;
    let _ = wait_for_import_status(&ctx, &token, &job_id).await;

    let row = sqlx::query("SELECT rating FROM books WHERE id = ?")
        .bind(&book.id)
        .fetch_one(&ctx.db)
        .await
        .expect("fetch rating");
    assert_eq!(row.get::<Option<i64>, _>("rating"), Some(8));
}

#[tokio::test]
async fn test_import_creates_shelf_if_not_exists() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let _ = ctx.create_book("The Hobbit", "J.R.R. Tolkien").await;
    let csv = goodreads_csv("The Hobbit", "J.R.R. Tolkien", 0, None, "Favorites", "read");

    let job_id = upload_goodreads_import(&ctx, &token, &csv).await;
    let _ = wait_for_import_status(&ctx, &token, &job_id).await;

    let shelf_count = query_i64(&ctx, "SELECT COUNT(1) FROM shelves WHERE name = ? AND user_id = (SELECT id FROM users WHERE username = 'user')", &["Favorites"]).await;
    assert_eq!(shelf_count, 1);
}

#[tokio::test]
async fn test_import_adds_book_to_existing_shelf() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("The Hobbit", "J.R.R. Tolkien").await;

    let shelf_response = ctx
        .server
        .post("/api/v1/shelves")
        .add_header(header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "name": "Favorites", "is_public": false }))
        .await;
    assert_status!(shelf_response, 201);

    let csv = goodreads_csv("The Hobbit", "J.R.R. Tolkien", 0, None, "Favorites", "read");
    let job_id = upload_goodreads_import(&ctx, &token, &csv).await;
    let _ = wait_for_import_status(&ctx, &token, &job_id).await;

    let row = sqlx::query(
        r#"
        SELECT COUNT(1) AS count
        FROM shelf_books sb
        INNER JOIN shelves s ON s.id = sb.shelf_id
        WHERE s.name = ? AND s.user_id = (SELECT id FROM users WHERE username = 'user') AND sb.book_id = ?
        "#,
    )
    .bind("Favorites")
    .bind(&book.id)
    .fetch_one(&ctx.db)
    .await
    .expect("fetch shelf books");
    assert_eq!(row.get::<i64, _>("count"), 1);
}

#[tokio::test]
async fn test_import_records_unmatched_books_in_errors() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let csv = goodreads_csv("Unknown Book", "Unknown Author", 0, None, "", "to-read");

    let job_id = upload_goodreads_import(&ctx, &token, &csv).await;
    let body = wait_for_import_status(&ctx, &token, &job_id).await;

    assert_eq!(body["unmatched"], 1);
    assert_eq!(body["errors"].as_array().map(Vec::len), Some(1));
    assert_eq!(body["errors"][0]["reason"], "not_in_library");
}

#[tokio::test]
async fn test_import_status_endpoint_returns_progress() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let _ = ctx.create_book("The Hobbit", "J.R.R. Tolkien").await;
    let csv = format!(
        "{}\n{}\n{}",
        goodreads_header(),
        goodreads_row(
            "The Hobbit",
            "J.R.R. Tolkien",
            4,
            Some("2024/01/02"),
            "",
            "read"
        ),
        goodreads_row("Unknown Book", "Unknown Author", 0, None, "", "to-read")
    );

    let job_id = upload_goodreads_import(&ctx, &token, &csv).await;
    let body = wait_for_import_status(&ctx, &token, &job_id).await;

    assert_eq!(body["status"], "complete");
    assert_eq!(body["total_rows"], 2);
    assert_eq!(body["matched"], 1);
    assert_eq!(body["unmatched"], 1);
}

#[tokio::test]
async fn test_parse_storygraph_csv_extracts_read_status() {
    let csv = storygraph_csv(
        "The Hobbit",
        "J.R.R. Tolkien",
        "currently-reading",
        Some(4.5),
        Some("2024-02-01"),
        "Fantasy, Classics",
    );
    let rows = parse_storygraph_csv(csv.as_bytes()).expect("parse storygraph csv");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].read_status, "currently-reading");
}
