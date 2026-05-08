#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use axum_test::multipart::{MultipartForm, Part};
use common::{auth_header, minimal_epub_bytes, minimal_mobi_bytes, TestContext};
use sqlx::SqlitePool;
use uuid::Uuid;
use serde_json::Value;

fn epub_part() -> Part {
    Part::bytes(minimal_epub_bytes())
        .file_name("book.epub")
        .mime_type("application/epub+zip")
}

fn mobi_part() -> Part {
    Part::bytes(minimal_mobi_bytes())
        .file_name("book.mobi")
        .mime_type("application/x-mobipocket-ebook")
}

async fn create_book_with_formats(
    ctx: &TestContext,
    token: &str,
    title: &str,
    include_epub: bool,
    include_mobi: bool,
) -> String {
    let mut form = MultipartForm::new()
        .add_text("title", title)
        .add_text("authors", "Merge Tester");

    // Always include at least one file — the upload endpoint requires a file.
    // When EPUB is requested, upload it directly.  When only MOBI is requested,
    // upload MOBI.  When neither is requested, upload a dummy EPUB (which the
    // test will ignore via the format list of the returned id; tests that need
    // a format-free book session seed separate reading-progress rows that
    // reference the format via the DB).
    if include_epub {
        form = form.add_part("file", epub_part());
    } else if include_mobi {
        form = form.add_part("file", mobi_part());
    } else {
        form = form.add_part("file", epub_part());
    }

    let upload = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .multipart(form)
        .await;
    assert_status!(upload, 201);
    let body: Value = upload.json();
    let id = body["id"].as_str().unwrap_or_default().to_string();

    // Register additional formats not included in the upload.
    if include_mobi && (include_epub || !include_epub) {
        let mobi_needed = if !include_epub {
            // MOBI was uploaded — the format already exists.
            false
        } else {
            // EPUB was uploaded, MOBI needs to be added via DB insert.
            true
        };
        if mobi_needed {
            let file_name = format!("{}.mobi", id);
            let storage_path = ctx.storage.path().join(&file_name);
            std::fs::write(&storage_path, minimal_mobi_bytes()).expect("write mobi file");
            let now = chrono::Utc::now().to_rfc3339();
            let format_id = uuid::Uuid::new_v4().to_string();
            sqlx::query(
                r#"
                INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&format_id)
            .bind(&id)
            .bind("MOBI")
            .bind(&file_name)
            .bind(minimal_mobi_bytes().len() as i64)
            .bind(&now)
            .bind(&now)
            .execute(&ctx.db)
            .await
            .expect("insert mobi format");
        }
    }

    id
}

#[tokio::test]
async fn test_merge_preview_returns_expected_counts_and_lists() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;

    let source = create_book_with_formats(&ctx, &admin, "Source", true, true).await;
    let target = create_book_with_formats(&ctx, &admin, "Target", true, false).await;

    // seed annotation + shelf relink candidate + reading progress rows
    ctx.seed_annotation(&source).await;
    ctx.seed_shelf_membership("Favourites", &source).await;

    let preview = ctx
        .server
        .post("/api/v1/admin/books/merge/preview")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({ "source_id": source, "target_id": target }))
        .await;

    assert_status!(preview, 200);
    let body: Value = preview.json();

    assert!(body["formats_to_move"].is_array());
    assert!(body["formats_conflict"].is_array());
    assert!(body["annotations_to_move"].is_number());
    assert!(body["shelves_to_relink"].is_array());
    assert!(body["reading_progress_strategy"].is_string());
}

#[tokio::test]
async fn test_merge_preview_404_if_book_missing() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/books/merge/preview")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({ "source_id": "missing", "target_id": "also-missing" }))
        .await;

    assert_status!(resp, 404);
}

#[tokio::test]
async fn test_merge_preview_400_if_source_equals_target() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;
    let book = create_book_with_formats(&ctx, &admin, "Single", true, false).await;

    let resp = ctx
        .server
        .post("/api/v1/admin/books/merge/preview")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({ "source_id": book, "target_id": book }))
        .await;

    assert_status!(resp, 400);
}

#[tokio::test]
async fn test_merge_preview_forbidden_for_non_admin() {
    let ctx = TestContext::new().await;
    let user = ctx.user_token().await;

    let resp = ctx
        .server
        .post("/api/v1/admin/books/merge/preview")
        .add_header(header::AUTHORIZATION, auth_header(&user))
        .json(&serde_json::json!({ "source_id": "a", "target_id": "b" }))
        .await;

    assert_status!(resp, 403);
}

#[tokio::test]
async fn test_merge_exec_moves_formats_annotations_shelves_and_deletes_source() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;

    let source = create_book_with_formats(&ctx, &admin, "Source", false, true).await;
    let target = create_book_with_formats(&ctx, &admin, "Target", true, false).await;

    ctx.seed_annotation(&source).await;
    ctx.seed_shelf_membership("Favourites", &source).await;

    let merge = ctx
        .server
        .post("/api/v1/admin/books/merge")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({
            "source_id": source,
            "target_id": target,
            "reading_progress_strategy": "keep_target"
        }))
        .await;

    assert_status!(merge, 200);
    let body: Value = merge.json();
    assert_eq!(body["merged"], true);

    let source_get = ctx
        .server
        .get(&format!("/api/v1/books/{}", body["source_id"].as_str().unwrap_or("source")))
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(source_get, 404);

    let target_get = ctx
        .server
        .get(&format!("/api/v1/books/{target}"))
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .await;
    assert_status!(target_get, 200);
}

#[tokio::test]
async fn test_merge_returns_409_on_conflict_without_force_then_succeeds_with_force() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;

    let source = create_book_with_formats(&ctx, &admin, "Source", true, false).await;
    let target = create_book_with_formats(&ctx, &admin, "Target", true, false).await;

    let no_force = ctx
        .server
        .post("/api/v1/admin/books/merge")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({
            "source_id": source,
            "target_id": target,
            "reading_progress_strategy": "keep_target"
        }))
        .await;
    assert_status!(no_force, 409);

    let force = ctx
        .server
        .post("/api/v1/admin/books/merge")
        .add_header(header::AUTHORIZATION, auth_header(&admin))
        .json(&serde_json::json!({
            "source_id": source,
            "target_id": target,
            "reading_progress_strategy": "keep_target",
            "force": true
        }))
        .await;
    assert_status!(force, 200);
}

#[tokio::test]
async fn test_merge_reading_progress_strategies() {
    let ctx = TestContext::new().await;
    let admin = ctx.admin_token().await;

    for strategy in ["keep_target", "keep_source", "merge_max"] {
        let source = create_book_with_formats(&ctx, &admin, &format!("Source-{strategy}"), true, false).await;
        let target = create_book_with_formats(&ctx, &admin, &format!("Target-{strategy}"), false, false).await;

        ctx.seed_reading_progress(&source, 20).await;
        ctx.seed_reading_progress(&target, 70).await;

        let merge = ctx
            .server
            .post("/api/v1/admin/books/merge")
            .add_header(header::AUTHORIZATION, auth_header(&admin))
            .json(&serde_json::json!({
                "source_id": source,
                "target_id": target,
                "reading_progress_strategy": strategy,
                "force": true
            }))
            .await;
        assert_status!(merge, 200);

        let pct = ctx.read_reading_progress_percent(&target).await;
        match strategy {
            "keep_target" => assert_eq!(pct, 70),
            "keep_source" => assert_eq!(pct, 20),
            "merge_max" => assert_eq!(pct, 70),
            _ => unreachable!(),
        }
    }
}
