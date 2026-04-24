#![allow(dead_code, unused_imports)]

mod common;

use chrono::Utc;
use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;

#[tokio::test]
async fn test_get_author_detail_includes_books() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, first_book_id) = insert_author_with_book(&ctx, "Terry Pratchett", "Guards! Guards!", None).await;
    let (_, second_book_id) = insert_author_with_book(&ctx, "Terry Pratchett", "Mort", Some(&author_id)).await;

    let old_pubdate = "1988-10-01T00:00:00Z";
    let new_pubdate = "1994-11-01T00:00:00Z";
    sqlx::query("UPDATE books SET pubdate = ? WHERE id = ?")
        .bind(old_pubdate)
        .bind(&first_book_id)
        .execute(&ctx.db)
        .await
        .expect("update first pubdate");
    sqlx::query("UPDATE books SET pubdate = ? WHERE id = ?")
        .bind(new_pubdate)
        .bind(&second_book_id)
        .execute(&ctx.db)
        .await
        .expect("update second pubdate");

    let response = ctx
        .server
        .get(&format!("/api/v1/authors/{author_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["id"], author_id);
    assert_eq!(body["book_count"], 2);
    let books = body["books"].as_array().expect("books array");
    assert_eq!(books.len(), 2);
    assert_eq!(books[0]["id"], second_book_id);
    assert_eq!(books[1]["id"], first_book_id);
}

#[tokio::test]
async fn test_get_author_detail_includes_profile_when_present() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = insert_author_with_book(&ctx, "N.K. Jemisin", "The Fifth Season", None).await;
    insert_author_profile(
        &ctx,
        &author_id,
        Some("Award-winning author."),
        Some("1981"),
        None,
        Some("https://nkjemisin.com"),
        Some("OL123A"),
    )
    .await;

    let response = ctx
        .server
        .get(&format!("/api/v1/authors/{author_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["profile"]["bio"], "Award-winning author.");
    assert_eq!(body["profile"]["born"], "1981");
    assert_eq!(body["profile"]["website_url"], "https://nkjemisin.com");
    assert_eq!(body["profile"]["openlibrary_id"], "OL123A");
}

#[tokio::test]
async fn test_get_author_detail_profile_null_when_absent() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = insert_author_with_book(&ctx, "Ursula K. Le Guin", "A Wizard of Earthsea", None).await;

    let response = ctx
        .server
        .get(&format!("/api/v1/authors/{author_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body["profile"].is_null());
}

#[tokio::test]
async fn test_patch_author_creates_profile() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = insert_author_with_book(&ctx, "Octavia Butler", "Kindred", None).await;

    let response = ctx
        .server
        .patch(&format!("/api/v1/authors/{author_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "bio": "Science fiction pioneer.",
            "born": "1947",
            "died": "2006",
            "website_url": "https://example.org/octavia",
            "openlibrary_id": "OL456A"
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["profile"]["bio"], "Science fiction pioneer.");
    assert_eq!(body["profile"]["born"], "1947");

    let row = sqlx::query(
        "SELECT bio, born, died, website_url, openlibrary_id FROM author_profiles WHERE author_id = ?",
    )
    .bind(&author_id)
    .fetch_one(&ctx.db)
    .await
    .expect("load author profile");
    let bio: Option<String> = row.get("bio");
    let born: Option<String> = row.get("born");
    let died: Option<String> = row.get("died");
    let website_url: Option<String> = row.get("website_url");
    let openlibrary_id: Option<String> = row.get("openlibrary_id");
    assert_eq!(bio.as_deref(), Some("Science fiction pioneer."));
    assert_eq!(born.as_deref(), Some("1947"));
    assert_eq!(died.as_deref(), Some("2006"));
    assert_eq!(website_url.as_deref(), Some("https://example.org/octavia"));
    assert_eq!(openlibrary_id.as_deref(), Some("OL456A"));
}

#[tokio::test]
async fn test_patch_author_updates_existing_profile() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let (author_id, _) = insert_author_with_book(&ctx, "China Miéville", "Perdido Street Station", None).await;
    insert_author_profile(
        &ctx,
        &author_id,
        Some("Weird fiction"),
        Some("1972"),
        None,
        Some("https://old.example"),
        Some("OL999A"),
    )
    .await;

    let response = ctx
        .server
        .patch(&format!("/api/v1/authors/{author_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "bio": "Updated bio",
            "born": null,
            "website_url": "https://china-miéville.com"
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["profile"]["bio"], "Updated bio");
    assert!(body["profile"]["born"].is_null());
    assert_eq!(body["profile"]["website_url"], "https://china-miéville.com");
    assert_eq!(body["profile"]["openlibrary_id"], "OL999A");
}

#[tokio::test]
async fn test_merge_author_moves_books_to_target() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let source_id = insert_author(&ctx, "Source Author").await;
    let target_id = insert_author(&ctx, "Target Author").await;
    let book_id = insert_book_with_authors(&ctx, "Merge Me", &[&source_id], Some("2020-01-01T00:00:00Z")).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/admin/authors/{source_id}/merge"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "into_author_id": target_id }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["books_updated"], 1);
    assert_eq!(body["target_author"]["id"], target_id);

    let row = sqlx::query("SELECT author_id FROM book_authors WHERE book_id = ?")
        .bind(&book_id)
        .fetch_one(&ctx.db)
        .await
        .expect("load book author");
    let book_author_id: String = row.get("author_id");
    assert_eq!(book_author_id, target_id);
}

#[tokio::test]
async fn test_merge_author_skips_duplicate_attributions() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let source_id = insert_author(&ctx, "Duplicate Source").await;
    let target_id = insert_author(&ctx, "Duplicate Target").await;
    let book_id = insert_book_with_authors(
        &ctx,
        "Already Shared",
        &[&source_id, &target_id],
        Some("2021-01-01T00:00:00Z"),
    )
    .await;

    let response = ctx
        .server
        .post(&format!("/api/v1/admin/authors/{source_id}/merge"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "into_author_id": target_id }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["books_updated"], 0);

    let rows = sqlx::query("SELECT author_id FROM book_authors WHERE book_id = ? ORDER BY author_id ASC")
        .bind(&book_id)
        .fetch_all(&ctx.db)
        .await
        .expect("load book authors");
    let author_ids: Vec<String> = rows.into_iter().map(|row| row.get("author_id")).collect();
    assert_eq!(author_ids, vec![target_id]);
}

#[tokio::test]
async fn test_merge_author_deletes_source() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let source_id = insert_author(&ctx, "Delete Source").await;
    let target_id = insert_author(&ctx, "Delete Target").await;
    insert_author_profile(
        &ctx,
        &source_id,
        Some("Source bio"),
        Some("1900"),
        None,
        None,
        None,
    )
    .await;
    let _ = insert_book_with_authors(&ctx, "Remove Source", &[&source_id], Some("2019-01-01T00:00:00Z")).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/admin/authors/{source_id}/merge"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "into_author_id": target_id }))
        .await;

    assert_status!(response, 200);

    let source_author = sqlx::query("SELECT id FROM authors WHERE id = ?")
        .bind(&source_id)
        .fetch_optional(&ctx.db)
        .await
        .expect("check source author");
    assert!(source_author.is_none());

    let profile = sqlx::query("SELECT author_id FROM author_profiles WHERE author_id = ?")
        .bind(&source_id)
        .fetch_optional(&ctx.db)
        .await
        .expect("check source profile");
    assert!(profile.is_none());
}

#[tokio::test]
async fn test_merge_author_is_atomic() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let source_id = insert_author(&ctx, "Atomic Source").await;
    let book_id = insert_book_with_authors(&ctx, "Atomic Book", &[&source_id], Some("2018-01-01T00:00:00Z")).await;
    let missing_target_id = Uuid::new_v4().to_string();

    let response = ctx
        .server
        .post(&format!("/api/v1/admin/authors/{source_id}/merge"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "into_author_id": missing_target_id }))
        .await;

    assert_status!(response, 404);

    let source_author = sqlx::query("SELECT id FROM authors WHERE id = ?")
        .bind(&source_id)
        .fetch_optional(&ctx.db)
        .await
        .expect("check source author");
    assert!(source_author.is_some());

    let row = sqlx::query("SELECT author_id FROM book_authors WHERE book_id = ?")
        .bind(&book_id)
        .fetch_one(&ctx.db)
        .await
        .expect("check book author");
    let author_id: String = row.get("author_id");
    assert_eq!(author_id, source_id);
}

async fn insert_author(ctx: &TestContext, name: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(name)
        .bind(&now)
        .execute(&ctx.db)
        .await
        .expect("insert author");
    id
}

async fn insert_author_with_book(
    ctx: &TestContext,
    author_name: &str,
    title: &str,
    author_id_override: Option<&str>,
) -> (String, String) {
    let author_id = if let Some(author_id) = author_id_override {
        author_id.to_string()
    } else {
        insert_author(ctx, author_name).await
    };
    let book_id = insert_book_with_authors(ctx, title, &[&author_id], None).await;
    (author_id, book_id)
}

async fn insert_book_with_authors(
    ctx: &TestContext,
    title: &str,
    author_ids: &[&str],
    pubdate: Option<&str>,
) -> String {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO books (
            id, title, sort_title, description, pubdate, language, rating, series_id, series_index,
            has_cover, cover_path, document_type, flags, library_id, indexed_at, created_at, last_modified
        )
        VALUES (?, ?, ?, NULL, ?, NULL, NULL, NULL, NULL, 0, NULL, 'unknown', NULL, 'default', NULL, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(title)
    .bind(title)
    .bind(pubdate)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert book");

    for (index, author_id) in author_ids.iter().enumerate() {
        sqlx::query(
            "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, ?)",
        )
        .bind(&id)
        .bind(author_id)
        .bind(index as i64)
        .execute(&ctx.db)
        .await
        .expect("insert book author");
    }

    id
}

async fn insert_author_profile(
    ctx: &TestContext,
    author_id: &str,
    bio: Option<&str>,
    born: Option<&str>,
    died: Option<&str>,
    website_url: Option<&str>,
    openlibrary_id: Option<&str>,
) {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO author_profiles (
            author_id, bio, photo_path, born, died, website_url, openlibrary_id, updated_at
        )
        VALUES (?, ?, NULL, ?, ?, ?, ?, ?)
        ON CONFLICT(author_id) DO UPDATE SET
            bio = excluded.bio,
            born = excluded.born,
            died = excluded.died,
            website_url = excluded.website_url,
            openlibrary_id = excluded.openlibrary_id,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(author_id)
    .bind(bio)
    .bind(born)
    .bind(died)
    .bind(website_url)
    .bind(openlibrary_id)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert author profile");
}
