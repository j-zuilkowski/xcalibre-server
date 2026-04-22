#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};
use serde_json::Value;
use sqlx::Row;

async fn create_library(ctx: &TestContext, token: &str, name: &str) -> Value {
    let response = ctx
        .server
        .post("/api/v1/admin/libraries")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(token))
        .json(&serde_json::json!({
            "name": name,
            "calibre_db_path": format!("/libraries/{name}/metadata.db")
        }))
        .await;
    assert_status!(response, 201);
    response.json()
}

#[tokio::test]
async fn test_admin_can_create_library() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let body = create_library(&ctx, &token, "Science Fiction").await;
    assert_eq!(body["name"], "Science Fiction");
    assert_eq!(
        body["calibre_db_path"],
        "/libraries/Science Fiction/metadata.db"
    );
    assert_eq!(body["book_count"], 0);

    let response = ctx
        .server
        .get("/api/v1/admin/libraries")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(response, 200);
    let libraries: Value = response.json();
    let libraries = libraries.as_array().cloned().unwrap_or_default();
    assert!(libraries
        .iter()
        .any(|library| library["name"] == "Science Fiction"));
}

#[tokio::test]
async fn test_admin_cannot_delete_library_with_books() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let body = create_library(&ctx, &token, "Archive").await;
    let library_id = body["id"].as_str().unwrap_or_default().to_string();
    let (book, _) = ctx.create_book_with_file("Archive Book", "EPUB").await;

    sqlx::query("UPDATE books SET library_id = ? WHERE id = ?")
        .bind(&library_id)
        .bind(&book.id)
        .execute(&ctx.db)
        .await
        .expect("move book to library");

    let response = ctx
        .server
        .delete(&format!("/api/v1/admin/libraries/{library_id}"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 409);
}

#[tokio::test]
async fn test_books_filtered_by_user_default_library() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;
    let user_token = ctx.user_token().await;

    let library = create_library(&ctx, &admin_token, "Comics").await;
    let library_id = library["id"].as_str().unwrap_or_default().to_string();

    let default_book = ctx
        .create_book_with_file("Default Library Book", "EPUB")
        .await;
    let library_book = ctx
        .create_book_with_file("Comics Library Book", "EPUB")
        .await;
    sqlx::query("UPDATE books SET library_id = ? WHERE id = ?")
        .bind(&library_id)
        .bind(&library_book.0.id)
        .execute(&ctx.db)
        .await
        .expect("move book to library");

    let switch = ctx
        .server
        .patch("/api/v1/users/me/library")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .json(&serde_json::json!({ "library_id": library_id }))
        .await;
    assert_status!(switch, 200);

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .await;
    assert_status!(response, 200);
    let body: Value = response.json();
    let items = body["items"].as_array().cloned().unwrap_or_default();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Comics Library Book");
    assert_ne!(
        items[0]["id"].as_str().unwrap_or_default(),
        default_book.0.id
    );
}

#[tokio::test]
async fn test_user_can_switch_library() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;
    let user_token = ctx.user_token().await;

    let library = create_library(&ctx, &admin_token, "History").await;
    let library_id = library["id"].as_str().unwrap_or_default().to_string();

    let response = ctx
        .server
        .patch("/api/v1/users/me/library")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .json(&serde_json::json!({ "library_id": library_id }))
        .await;
    assert_status!(response, 200);
    let body: Value = response.json();
    assert_eq!(body["default_library_id"], library_id);
}

#[tokio::test]
async fn test_admin_sees_all_libraries() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;
    let first = create_library(&ctx, &token, "Audio").await;
    let second = create_library(&ctx, &token, "Reference").await;

    let response = ctx
        .server
        .get("/api/v1/admin/libraries")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(response, 200);
    let body: Value = response.json();
    let libraries = body.as_array().cloned().unwrap_or_default();
    assert!(libraries.iter().any(|library| library["id"] == first["id"]));
    assert!(libraries
        .iter()
        .any(|library| library["id"] == second["id"]));
    assert!(libraries.iter().any(|library| library["id"] == "default"));
}
