#![allow(dead_code, unused_imports)]

mod common;

use chrono::{Datelike, TimeZone, Utc};
use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;

async fn stats_body(ctx: &TestContext, token: &str) -> serde_json::Value {
    let response = ctx
        .server
        .get("/api/v1/users/me/stats")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(token))
        .await;

    assert_status!(response, 200);
    response.json()
}

async fn user_id(ctx: &TestContext, username: &str) -> String {
    sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind(username)
        .fetch_one(&ctx.db)
        .await
        .expect("load user id")
}

fn timestamp(year: i32, month: u32, day: u32) -> String {
    Utc.with_ymd_and_hms(year, month, day, 12, 0, 0)
        .single()
        .expect("valid timestamp")
        .to_rfc3339()
}

fn last_twelve_month_labels() -> Vec<String> {
    let today = Utc::now().date_naive();
    let mut year = today.year();
    let mut month = today.month();
    let mut labels = Vec::with_capacity(12);

    for _ in 0..12 {
        labels.push(format!("{year:04}-{month:02}"));
        if month == 1 {
            year -= 1;
            month = 12;
        } else {
            month -= 1;
        }
    }

    labels.reverse();
    labels
}

fn shift_month(year: i32, month: u32, offset: i32) -> (i32, u32) {
    let total_months = year * 12 + month as i32 - 1 + offset;
    let shifted_year = total_months.div_euclid(12);
    let shifted_month = total_months.rem_euclid(12) + 1;
    (shifted_year, shifted_month as u32)
}

async fn insert_book_user_state(ctx: &TestContext, user_id: &str, book_id: &str, updated_at: &str, is_read: bool) {
    sqlx::query(
        r#"
        INSERT INTO book_user_state (user_id, book_id, is_read, is_archived, updated_at)
        VALUES (?, ?, ?, 0, ?)
        "#,
    )
    .bind(user_id)
    .bind(book_id)
    .bind(i64::from(is_read))
    .bind(updated_at)
    .execute(&ctx.db)
    .await
    .expect("insert book user state");
}

async fn insert_reading_progress(
    ctx: &TestContext,
    user_id: &str,
    book_id: &str,
    format_id: &str,
    updated_at: &str,
    percentage: f64,
) {
    sqlx::query(
        r#"
        INSERT INTO reading_progress (
            id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified
        )
        VALUES (?, ?, ?, ?, NULL, NULL, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(book_id)
    .bind(format_id)
    .bind(percentage)
    .bind(updated_at)
    .bind(updated_at)
    .execute(&ctx.db)
    .await
    .expect("insert reading progress");
}

async fn insert_tag(ctx: &TestContext, name: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO tags (id, name, source, last_modified) VALUES (?, ?, 'manual', ?)")
        .bind(&id)
        .bind(name)
        .bind(&now)
        .execute(&ctx.db)
        .await
        .expect("insert tag");
    id
}

#[tokio::test]
async fn test_stats_returns_zero_for_new_user() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let stats = stats_body(&ctx, &token).await;

    assert_eq!(stats["total_books_read"], 0);
    assert_eq!(stats["books_read_this_year"], 0);
    assert_eq!(stats["books_read_this_month"], 0);
    assert_eq!(stats["books_in_progress"], 0);
    assert_eq!(stats["total_reading_sessions"], 0);
    assert_eq!(stats["reading_streak_days"], 0);
    assert_eq!(stats["longest_streak_days"], 0);
    assert_eq!(stats["average_progress_per_session"], 0.0);
    assert!(stats["formats_read"].as_object().is_some_and(|formats| formats.is_empty()));
    assert!(stats["top_tags"].as_array().is_some_and(|tags| tags.is_empty()));
    assert!(stats["top_authors"].as_array().is_some_and(|authors| authors.is_empty()));
    assert_eq!(stats["monthly_books"].as_array().map(Vec::len), Some(12));
    assert!(stats["monthly_books"]
        .as_array()
        .is_some_and(|months| months.iter().all(|month| month["count"] == 0)));
}

#[tokio::test]
async fn test_total_books_read_counts_is_read_books() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let token = ctx.login(&user.username, &password).await.access_token;

    let read_one = ctx.create_book("Read One", "Author A").await;
    let read_two = ctx.create_book("Read Two", "Author B").await;
    let unread = ctx.create_book("Unread", "Author C").await;
    let now = Utc::now().to_rfc3339();
    let user_id = user.id.clone();

    insert_book_user_state(&ctx, &user_id, &read_one.id, &now, true).await;
    insert_book_user_state(&ctx, &user_id, &read_two.id, &now, true).await;
    insert_book_user_state(&ctx, &user_id, &unread.id, &now, false).await;

    let stats = stats_body(&ctx, &token).await;

    assert_eq!(stats["total_books_read"], 2);
}

#[tokio::test]
async fn test_books_read_this_year_excludes_prior_years() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let token = ctx.login(&user.username, &password).await.access_token;
    let user_id = user.id.clone();
    let current_year = Utc::now().year();

    let old_book = ctx.create_book("Old Book", "Author A").await;
    let current_book = ctx.create_book("Current Book", "Author B").await;

    insert_book_user_state(
        &ctx,
        &user_id,
        &old_book.id,
        &timestamp(current_year - 1, 12, 31),
        true,
    )
    .await;
    insert_book_user_state(
        &ctx,
        &user_id,
        &current_book.id,
        &timestamp(current_year, 1, 15),
        true,
    )
    .await;

    let stats = stats_body(&ctx, &token).await;

    assert_eq!(stats["books_read_this_year"], 1);
}

#[tokio::test]
async fn test_streak_is_zero_with_no_activity() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let stats = stats_body(&ctx, &token).await;

    assert_eq!(stats["reading_streak_days"], 0);
    assert_eq!(stats["longest_streak_days"], 0);
}

#[tokio::test]
async fn test_streak_counts_consecutive_days() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let token = ctx.login(&user.username, &password).await.access_token;
    let user_id = user.id.clone();
    let today = Utc::now().date_naive();

    for offset in 0..3 {
        let date = today - chrono::Duration::days(offset);
        let (book, _) = ctx.create_book_with_file(&format!("Book {offset}"), "EPUB").await;
        let format_id = book.formats[0].id.clone();
        insert_reading_progress(
            &ctx,
            &user_id,
            &book.id,
            &format_id,
            &timestamp(date.year(), date.month(), date.day()),
            35.0,
        )
        .await;
    }

    let stats = stats_body(&ctx, &token).await;

    assert_eq!(stats["reading_streak_days"], 3);
    assert_eq!(stats["longest_streak_days"], 3);
}

#[tokio::test]
async fn test_streak_resets_on_gap() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let token = ctx.login(&user.username, &password).await.access_token;
    let user_id = user.id.clone();
    let today = Utc::now().date_naive();

    let day_offsets = [0_i64, 2, 3];
    for offset in day_offsets {
        let date = today - chrono::Duration::days(offset);
        let (book, _) = ctx.create_book_with_file(&format!("Book {offset}"), "EPUB").await;
        let format_id = book.formats[0].id.clone();
        insert_reading_progress(
            &ctx,
            &user_id,
            &book.id,
            &format_id,
            &timestamp(date.year(), date.month(), date.day()),
            50.0,
        )
        .await;
    }

    let stats = stats_body(&ctx, &token).await;

    assert_eq!(stats["reading_streak_days"], 1);
    assert_eq!(stats["longest_streak_days"], 2);
}

#[tokio::test]
async fn test_monthly_books_covers_last_12_months() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let token = ctx.login(&user.username, &password).await.access_token;
    let user_id = user.id.clone();
    let today = Utc::now().date_naive();

    let current_month = timestamp(today.year(), today.month(), 3);
    let (previous_year, previous_month) = shift_month(today.year(), today.month(), -1);
    let (old_year, old_month) = shift_month(today.year(), today.month(), -13);
    let previous_month_date = timestamp(previous_year, previous_month, 18);
    let old_date = timestamp(old_year, old_month, 18);

    let (current_book, _) = ctx.create_book_with_file("Current Month", "EPUB").await;
    let current_format = current_book.formats[0].id.clone();
    insert_book_user_state(&ctx, &user_id, &current_book.id, &current_month, true).await;
    insert_reading_progress(
        &ctx,
        &user_id,
        &current_book.id,
        &current_format,
        &current_month,
        45.0,
    )
    .await;

    let (previous_book, _) = ctx.create_book_with_file("Previous Month", "EPUB").await;
    let previous_format = previous_book.formats[0].id.clone();
    insert_book_user_state(
        &ctx,
        &user_id,
        &previous_book.id,
        &previous_month_date,
        true,
    )
    .await;
    insert_reading_progress(
        &ctx,
        &user_id,
        &previous_book.id,
        &previous_format,
        &previous_month_date,
        50.0,
    )
    .await;

    let (old_book, _) = ctx.create_book_with_file("Old Month", "EPUB").await;
    let old_format = old_book.formats[0].id.clone();
    insert_book_user_state(&ctx, &user_id, &old_book.id, &old_date, true).await;
    insert_reading_progress(&ctx, &user_id, &old_book.id, &old_format, &old_date, 30.0).await;

    let stats = stats_body(&ctx, &token).await;
    let months = stats["monthly_books"].as_array().expect("monthly books array");

    assert_eq!(months.len(), 12);

    let expected_labels = last_twelve_month_labels();
    let labels: Vec<String> = months
        .iter()
        .map(|month| month["month"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(labels, expected_labels);

    let current_label = format!("{:04}-{:02}", today.year(), today.month());
    let previous_label = if today.month() == 1 {
        format!("{:04}-12", today.year() - 1)
    } else {
        format!("{:04}-{:02}", today.year(), today.month() - 1)
    };

    let counts: std::collections::HashMap<String, i64> = months
        .iter()
        .map(|month| {
            (
                month["month"].as_str().unwrap_or_default().to_string(),
                month["count"].as_i64().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(counts.get(&current_label), Some(&1));
    assert_eq!(counts.get(&previous_label), Some(&1));
    assert!(!counts.contains_key(&format!("{:04}-{:02}", old_year, old_month)));
}

#[tokio::test]
async fn test_top_tags_ordered_by_count() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let token = ctx.login(&user.username, &password).await.access_token;
    let user_id = user.id.clone();
    let now = Utc::now().to_rfc3339();
    let fiction_tag = insert_tag(&ctx, "Fiction").await;
    let scifi_tag = insert_tag(&ctx, "Science Fiction").await;

    let first = ctx.create_book("First", "Author A").await;
    let second = ctx.create_book("Second", "Author B").await;
    let third = ctx.create_book("Third", "Author C").await;

    for book_id in [&first.id, &second.id, &third.id] {
        insert_book_user_state(&ctx, &user_id, book_id, &now, true).await;
    }

    sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
        .bind(&first.id)
        .bind(&fiction_tag)
        .execute(&ctx.db)
        .await
        .expect("insert first fiction tag");
    sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
        .bind(&second.id)
        .bind(&fiction_tag)
        .execute(&ctx.db)
        .await
        .expect("insert second fiction tag");
    sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
        .bind(&first.id)
        .bind(&scifi_tag)
        .execute(&ctx.db)
        .await
        .expect("insert scifi tag");

    let stats = stats_body(&ctx, &token).await;
    let top_tags = stats["top_tags"].as_array().expect("top tags array");

    assert_eq!(top_tags[0]["name"], "Fiction");
    assert_eq!(top_tags[0]["count"], 2);
    assert_eq!(top_tags[1]["name"], "Science Fiction");
    assert_eq!(top_tags[1]["count"], 1);
}

#[tokio::test]
async fn test_formats_read_groups_by_format() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let token = ctx.login(&user.username, &password).await.access_token;
    let user_id = user.id.clone();
    let now = Utc::now().to_rfc3339();

    let (first_book, _) = ctx.create_book_with_file("First", "EPUB").await;
    let epub_format_id = first_book.formats[0].id.clone();
    let pdf_format_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, ?, ?, 0, ?, ?)
        "#,
    )
    .bind(&pdf_format_id)
    .bind(&first_book.id)
    .bind("PDF")
    .bind(format!("{}.pdf", first_book.id))
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert pdf format");

    let (second_book, _) = ctx.create_book_with_file("Second", "EPUB").await;
    let second_epub_format_id = second_book.formats[0].id.clone();

    insert_book_user_state(&ctx, &user_id, &first_book.id, &now, true).await;
    insert_book_user_state(&ctx, &user_id, &second_book.id, &now, true).await;
    insert_reading_progress(&ctx, &user_id, &first_book.id, &epub_format_id, &now, 60.0).await;
    insert_reading_progress(
        &ctx,
        &user_id,
        &second_book.id,
        &second_epub_format_id,
        &now,
        40.0,
    )
    .await;

    let stats = stats_body(&ctx, &token).await;
    let formats = stats["formats_read"].as_object().expect("formats map");

    assert_eq!(formats.get("epub"), Some(&serde_json::Value::from(2)));
    assert_eq!(formats.get("pdf"), Some(&serde_json::Value::from(1)));
}
