#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};
use sqlx::Row;
use uuid::Uuid;

async fn add_tag(ctx: &TestContext, name: &str) -> String {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("INSERT INTO tags (id, name, source, last_modified) VALUES (?, ?, 'manual', ?)")
        .bind(&id)
        .bind(name)
        .bind(&now)
        .execute(&ctx.db)
        .await
        .expect("insert tag");
    id
}

async fn attach_tag(ctx: &TestContext, book_id: &str, tag_id: &str, confirmed: bool) {
    sqlx::query(
        "INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, ?) ON CONFLICT(book_id, tag_id) DO UPDATE SET confirmed = excluded.confirmed",
    )
    .bind(book_id)
    .bind(tag_id)
    .bind(i64::from(confirmed))
    .execute(&ctx.db)
    .await
    .expect("attach tag");
}

#[tokio::test]
async fn test_list_tags_returns_book_counts() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let sci_fi_id = add_tag(&ctx, "Sci-Fi").await;
    let fantasy_id = add_tag(&ctx, "Fantasy").await;

    let first = ctx.create_book("Dune", "Frank Herbert").await;
    let second = ctx.create_book("Neuromancer", "William Gibson").await;

    attach_tag(&ctx, &first.id, &sci_fi_id, true).await;
    attach_tag(&ctx, &second.id, &sci_fi_id, false).await;
    attach_tag(&ctx, &first.id, &fantasy_id, true).await;

    let response = ctx
        .server
        .get("/api/v1/admin/tags")
        .add_query_param("page", "1")
        .add_query_param("page_size", "20")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().cloned().unwrap_or_default();

    let sci_fi = items
        .iter()
        .find(|item| item["id"] == sci_fi_id)
        .expect("sci-fi item present");
    assert_eq!(sci_fi["book_count"], 2);
    assert_eq!(sci_fi["confirmed_count"], 1);

    let fantasy = items
        .iter()
        .find(|item| item["id"] == fantasy_id)
        .expect("fantasy item present");
    assert_eq!(fantasy["book_count"], 1);
    assert_eq!(fantasy["confirmed_count"], 1);
}

#[tokio::test]
async fn test_rename_tag_updates_name() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let tag_id = add_tag(&ctx, "sci-fi").await;

    let response = ctx
        .server
        .patch(&format!("/api/v1/admin/tags/{tag_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "name": "Science Fiction" }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["name"], "Science Fiction");

    let row = sqlx::query("SELECT name FROM tags WHERE id = ?")
        .bind(&tag_id)
        .fetch_one(&ctx.db)
        .await
        .expect("load renamed tag");
    assert_eq!(row.get::<String, _>("name"), "Science Fiction");
}

#[tokio::test]
async fn test_rename_tag_conflicts_with_existing_name_returns_409() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let source_id = add_tag(&ctx, "sci-fi").await;
    let _target_id = add_tag(&ctx, "Science Fiction").await;

    let response = ctx
        .server
        .patch(&format!("/api/v1/admin/tags/{source_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "name": "science fiction" }))
        .await;

    assert_status!(response, 409);
    let body: serde_json::Value = response.json();
    assert_eq!(body["error"], "tag_name_conflict");
}

#[tokio::test]
async fn test_delete_tag_removes_from_all_books() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let tag_id = add_tag(&ctx, "to-delete").await;
    let first = ctx.create_book("Delete One", "A").await;
    let second = ctx.create_book("Delete Two", "B").await;

    attach_tag(&ctx, &first.id, &tag_id, true).await;
    attach_tag(&ctx, &second.id, &tag_id, true).await;

    let response = ctx
        .server
        .delete(&format!("/api/v1/admin/tags/{tag_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 204);

    let tag_exists: Option<String> = sqlx::query_scalar("SELECT id FROM tags WHERE id = ?")
        .bind(&tag_id)
        .fetch_optional(&ctx.db)
        .await
        .expect("check deleted tag");
    assert!(tag_exists.is_none());

    let remaining_links: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM book_tags WHERE tag_id = ?")
            .bind(&tag_id)
            .fetch_one(&ctx.db)
            .await
            .expect("count remaining links");
    assert_eq!(remaining_links, 0);
}

#[tokio::test]
async fn test_delete_nonexistent_tag_returns_404() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let missing = Uuid::new_v4().to_string();

    let response = ctx
        .server
        .delete(&format!("/api/v1/admin/tags/{missing}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_merge_tag_moves_books_to_target() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let source_id = add_tag(&ctx, "sci-fi").await;
    let target_id = add_tag(&ctx, "Science Fiction").await;
    let first = ctx.create_book("Book One", "A").await;
    let second = ctx.create_book("Book Two", "B").await;

    attach_tag(&ctx, &first.id, &source_id, true).await;
    attach_tag(&ctx, &second.id, &source_id, false).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/admin/tags/{source_id}/merge"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "into_tag_id": target_id }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["merged_book_count"], 2);
    assert_eq!(body["target_tag"]["id"], target_id);

    let source_exists: Option<String> = sqlx::query_scalar("SELECT id FROM tags WHERE id = ?")
        .bind(&source_id)
        .fetch_optional(&ctx.db)
        .await
        .expect("check source deleted");
    assert!(source_exists.is_none());

    let target_links: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM book_tags WHERE tag_id = ?")
        .bind(&target_id)
        .fetch_one(&ctx.db)
        .await
        .expect("count target links");
    assert_eq!(target_links, 2);
}

#[tokio::test]
async fn test_merge_tag_does_not_duplicate_on_books_that_already_have_target() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let source_id = add_tag(&ctx, "sci-fi").await;
    let target_id = add_tag(&ctx, "Science Fiction").await;
    let first = ctx.create_book("Overlap", "A").await;
    let second = ctx.create_book("Source Only", "B").await;

    attach_tag(&ctx, &first.id, &source_id, true).await;
    attach_tag(&ctx, &first.id, &target_id, true).await;
    attach_tag(&ctx, &second.id, &source_id, true).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/admin/tags/{source_id}/merge"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "into_tag_id": target_id }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["merged_book_count"], 1);

    let first_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM book_tags WHERE book_id = ? AND tag_id = ?")
            .bind(&first.id)
            .bind(&target_id)
            .fetch_one(&ctx.db)
            .await
            .expect("count first tag rows");
    assert_eq!(first_count, 1);

    let second_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM book_tags WHERE book_id = ? AND tag_id = ?")
            .bind(&second.id)
            .bind(&target_id)
            .fetch_one(&ctx.db)
            .await
            .expect("count second tag rows");
    assert_eq!(second_count, 1);
}

#[tokio::test]
async fn test_merge_is_atomic_source_deleted_after_merge() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let source_id = add_tag(&ctx, "old-name").await;
    let target_id = add_tag(&ctx, "new-name").await;
    let book = ctx.create_book("Atomic Merge", "A").await;
    attach_tag(&ctx, &book.id, &source_id, true).await;

    let response = ctx
        .server
        .post(&format!("/api/v1/admin/tags/{source_id}/merge"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({ "into_tag_id": target_id }))
        .await;

    assert_status!(response, 200);

    let source_tag_exists: Option<String> = sqlx::query_scalar("SELECT id FROM tags WHERE id = ?")
        .bind(&source_id)
        .fetch_optional(&ctx.db)
        .await
        .expect("check source tag");
    assert!(source_tag_exists.is_none());

    let source_link_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM book_tags WHERE tag_id = ?")
            .bind(&source_id)
            .fetch_one(&ctx.db)
            .await
            .expect("count source links");
    assert_eq!(source_link_count, 0);

    let target_link_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM book_tags WHERE tag_id = ?")
            .bind(&target_id)
            .fetch_one(&ctx.db)
            .await
            .expect("count target links");
    assert_eq!(target_link_count, 1);
}
