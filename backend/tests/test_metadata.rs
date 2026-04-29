#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};

#[tokio::test]
async fn test_metadata_search_returns_200_for_existing_book() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Dune", "Frank Herbert").await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/metadata/search", book.id))
        .add_query_param("q", "Dune")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert!(body.is_array());
    assert!(body.as_array().map(|items| items.len() <= 20).unwrap_or(false));
}

#[tokio::test]
async fn test_metadata_search_returns_404_for_unknown_book() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .get("/api/v1/books/nonexistent-id/metadata/search")
        .add_query_param("q", "anything")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 404);
}

#[tokio::test]
async fn test_metadata_apply_updates_title_and_description() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Old Title", "Old Author").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/metadata/apply", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "source": "google_books",
            "external_id": "abc",
            "title": "New Title",
            "authors": null,
            "description": "A great book.",
            "publisher": null,
            "published_date": null,
            "isbn_13": null,
            "isbn_10": null,
            "cover_url": null
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["title"], "New Title");
    assert_eq!(body["description"], "A great book.");
}

#[tokio::test]
async fn test_metadata_apply_stores_external_id_as_identifier() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let book = ctx.create_book("Title", "Author").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/metadata/apply", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "source": "google_books",
            "external_id": "vol123",
            "title": null,
            "authors": null,
            "description": null,
            "publisher": null,
            "published_date": null,
            "isbn_13": null,
            "isbn_10": null,
            "cover_url": null
        }))
        .await;
    assert_status!(response, 200);

    let fetched = ctx
        .server
        .get(&format!("/api/v1/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(fetched, 200);
    let body: serde_json::Value = fetched.json();
    let identifiers = body["identifiers"].as_array().expect("identifiers array");
    assert!(identifiers.iter().any(|identifier| {
        identifier["id_type"] == "google_books" && identifier["value"] == "vol123"
    }));
}

#[tokio::test]
async fn test_metadata_apply_requires_can_edit_permission() {
    let ctx = TestContext::new().await;
    let (user, password) = ctx.create_user().await;
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("no_edit")
    .bind("no_edit")
    .bind(0_i64)
    .bind(0_i64)
    .bind(0_i64)
    .bind(1_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert role");
    sqlx::query("UPDATE users SET role_id = ? WHERE id = ?")
        .bind("no_edit")
        .bind(&user.id)
        .execute(&ctx.db)
        .await
        .expect("set role");
    let token = ctx.login(&user.username, &password).await.access_token;
    let book = ctx.create_book("Title", "Author").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/metadata/apply", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "source": "google_books",
            "external_id": "vol123",
            "title": "Denied",
            "authors": null,
            "description": null,
            "publisher": null,
            "published_date": null,
            "isbn_13": null,
            "isbn_10": null,
            "cover_url": null
        }))
        .await;

    assert_status!(response, 403);
}
