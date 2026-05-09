# Phase 27a — Book Merge Tests

## Context
Rust 2021, Axum 0.7. TDD: write failing tests first.
Working dir: `~/Documents/localProject/xcalibre-server`

Add admin endpoints for merge preview and merge execution.

---

## Write to: `backend/tests/test_book_merge.rs`

```rust
#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use common::{auth_header, minimal_epub_bytes, minimal_mobi_bytes, TestContext};
use serde_json::Value;

async fn create_book_with_formats(
    ctx: &TestContext,
    token: &str,
    title: &str,
    include_epub: bool,
    include_mobi: bool,
) -> String {
    let mut form = axum_test::multipart::MultipartForm::new()
        .add_text("title", title)
        .add_text("authors", "Merge Tester");

    if include_epub {
        form = form.add_file("file", "book.epub", "application/epub+zip", minimal_epub_bytes());
    }

    let upload = ctx
        .server
        .post("/api/v1/books")
        .add_header(header::AUTHORIZATION, auth_header(token))
        .add_header(header::CONTENT_TYPE, "multipart/form-data")
        .multipart(form)
        .await;
    assert_status!(upload, 201);
    let id = upload.json::<Value>()["id"].as_str().unwrap_or_default().to_string();

    if include_mobi {
        let add_mobi = ctx
            .server
            .post(&format!("/api/v1/books/{id}/formats"))
            .add_header(header::AUTHORIZATION, auth_header(token))
            .add_header(header::CONTENT_TYPE, "multipart/form-data")
            .multipart(
                axum_test::multipart::MultipartForm::new().add_file(
                    "file",
                    "book.mobi",
                    "application/x-mobipocket-ebook",
                    minimal_mobi_bytes(),
                ),
            )
            .await;
        assert_status!(add_mobi, 201);
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
```

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_book_merge 2>&1 | tail -60
```
Expected: **BUILD FAILED** — admin merge preview/execute endpoints and transaction semantics not implemented yet.

## Commit
```bash
git add backend/tests/test_book_merge.rs
git commit -m "Phase 27a — book merge tests (failing)"
```

## Final Step
`Stop now. Do not run any more commands.`
