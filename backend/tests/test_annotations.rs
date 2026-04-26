#![allow(dead_code, unused_imports)]

mod common;

use backend::{
    auth::password::hash_password,
    db::models::{RoleRef, User},
};
use chrono::Utc;
use common::{auth_header, TestContext};
use uuid::Uuid;

async fn create_annotation(
    ctx: &TestContext,
    token: &str,
    book_id: &str,
    payload: serde_json::Value,
) -> serde_json::Value {
    let response = ctx
        .server
        .post(&format!("/api/v1/books/{book_id}/annotations"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(token))
        .json(&payload)
        .await;
    assert_status!(response, 201);
    response.json()
}

async fn create_user_with_token(ctx: &TestContext, username: &str) -> (User, String) {
    let password = "Test1234!".to_string();
    let email = format!("{username}@example.com");
    let now = Utc::now().to_rfc3339();
    let password_hash = hash_password(&password, &ctx.state.config.auth).expect("hash password");

    let _ = sqlx::query(
        r#"
        INSERT OR IGNORE INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind("user")
    .bind("user")
    .bind(0_i64)
    .bind(0_i64)
    .bind(1_i64)
    .bind(1_i64)
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("seed role");

    let user_id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, password_hash, role_id, is_active, force_pw_reset, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?)
        "#,
    )
    .bind(&user_id)
    .bind(username)
    .bind(&email)
    .bind(password_hash)
    .bind("user")
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert user");

    let user = User {
        id: user_id,
        username: username.to_string(),
        email,
        role: RoleRef {
            id: "user".to_string(),
            name: "user".to_string(),
        },
        is_active: true,
        force_pw_reset: false,
        default_library_id: "default".to_string(),
        totp_enabled: false,
        created_at: now.clone(),
        last_modified: now,
    };

    let token = ctx.login(username, &password).await.access_token;
    (user, token)
}

#[tokio::test]
async fn test_create_highlight_returns_201() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/annotations", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "type": "highlight",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:128)",
            "highlighted_text": "The text the user selected",
            "note": null,
            "color": "yellow"
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["type"], "highlight");
    assert_eq!(body["color"], "yellow");
    assert_eq!(body["highlighted_text"], "The text the user selected");
}

#[tokio::test]
async fn test_create_note_requires_note_text() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/annotations", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "type": "note",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:128)",
            "highlighted_text": "Selected text",
            "color": "yellow"
        }))
        .await;

    assert_status!(response, 400);
}

#[tokio::test]
async fn test_create_bookmark_accepts_null_highlighted_text() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/annotations", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "type": "bookmark",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:0)",
            "highlighted_text": null,
            "note": null
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    assert_eq!(body["type"], "bookmark");
    assert!(body["highlighted_text"].is_null());
    assert_eq!(body["color"], "yellow");
}

#[tokio::test]
async fn test_list_annotations_excludes_other_users() {
    let ctx = TestContext::new().await;
    let (_user_a, user_a_token) = create_user_with_token(&ctx, "user_a").await;
    let (_user_b, user_b_token) = create_user_with_token(&ctx, "user_b").await;
    let book = ctx.create_book("Annotation Book", "Reader").await;

    let user_annotation = create_annotation(
        &ctx,
        &user_a_token,
        &book.id,
        serde_json::json!({
            "type": "highlight",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:32)",
            "highlighted_text": "User text",
            "note": null,
            "color": "yellow"
        }),
    )
    .await;

    let _other_user_annotation = create_annotation(
        &ctx,
        &user_b_token,
        &book.id,
        serde_json::json!({
            "type": "highlight",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:33,/1:64)",
            "highlighted_text": "Other user text",
            "note": null,
            "color": "blue"
        }),
    )
    .await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/annotations", book.id))
        .add_header(
            axum::http::header::AUTHORIZATION,
            auth_header(&user_a_token),
        )
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    let items = body.as_array().expect("annotations array");
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], user_annotation["id"]);
}

#[tokio::test]
async fn test_update_annotation_changes_color() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;
    let annotation = create_annotation(
        &ctx,
        &token,
        &book.id,
        serde_json::json!({
            "type": "highlight",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:32)",
            "highlighted_text": "Color me",
            "note": null,
            "color": "yellow"
        }),
    )
    .await;
    let annotation_id = annotation["id"]
        .as_str()
        .expect("annotation id")
        .to_string();

    let response = ctx
        .server
        .patch(&format!(
            "/api/v1/books/{}/annotations/{annotation_id}",
            book.id
        ))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "color": "green"
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["color"], "green");
}

#[tokio::test]
async fn test_patch_annotation_by_non_owner_returns_403() {
    let ctx = TestContext::new().await;
    let (_user_a, token_a) = create_user_with_token(&ctx, "user_a").await;
    let (_user_b, token_b) = create_user_with_token(&ctx, "user_b").await;
    let book = ctx.create_book("Annotation Book", "Reader").await;
    let annotation = create_annotation(
        &ctx,
        &token_b,
        &book.id,
        serde_json::json!({
            "type": "highlight",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:32)",
            "highlighted_text": "Owner text",
            "note": null,
            "color": "yellow"
        }),
    )
    .await;
    let annotation_id = annotation["id"]
        .as_str()
        .expect("annotation id")
        .to_string();

    let response = ctx
        .server
        .patch(&format!(
            "/api/v1/books/{}/annotations/{annotation_id}",
            book.id
        ))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token_a))
        .json(&serde_json::json!({
            "color": "green"
        }))
        .await;

    assert!(
        response.status_code() == 403 || response.status_code() == 404,
        "Expected 403 or 404, got {}",
        response.status_code()
    );
}

#[tokio::test]
async fn test_delete_annotation_returns_204() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;
    let annotation = create_annotation(
        &ctx,
        &token,
        &book.id,
        serde_json::json!({
            "type": "bookmark",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:0)",
            "highlighted_text": null,
            "note": null
        }),
    )
    .await;
    let annotation_id = annotation["id"]
        .as_str()
        .expect("annotation id")
        .to_string();

    let response = ctx
        .server
        .delete(&format!(
            "/api/v1/books/{}/annotations/{annotation_id}",
            book.id
        ))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 204);

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM book_annotations WHERE id = ?")
        .bind(annotation_id)
        .fetch_one(&ctx.db)
        .await
        .expect("query annotation count");
    assert_eq!(remaining, 0);
}

#[tokio::test]
async fn test_delete_annotation_by_non_owner_returns_403() {
    let ctx = TestContext::new().await;
    let (_user_a, token_a) = create_user_with_token(&ctx, "user_a").await;
    let (_user_b, token_b) = create_user_with_token(&ctx, "user_b").await;
    let book = ctx.create_book("Annotation Book", "Reader").await;
    let annotation = create_annotation(
        &ctx,
        &token_b,
        &book.id,
        serde_json::json!({
            "type": "bookmark",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:0)",
            "highlighted_text": null,
            "note": null
        }),
    )
    .await;
    let annotation_id = annotation["id"]
        .as_str()
        .expect("annotation id")
        .to_string();

    let response = ctx
        .server
        .delete(&format!(
            "/api/v1/books/{}/annotations/{annotation_id}",
            book.id
        ))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token_a))
        .await;

    assert!(
        response.status_code() == 403 || response.status_code() == 404,
        "Expected 403 or 404, got {}",
        response.status_code()
    );
}

#[tokio::test]
async fn test_annotations_cascade_delete_on_book_delete() {
    let ctx = TestContext::new().await;
    let user_token = ctx.user_token().await;
    let admin_token = ctx.admin_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;
    let annotation = create_annotation(
        &ctx,
        &user_token,
        &book.id,
        serde_json::json!({
            "type": "highlight",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:0,/1:32)",
            "highlighted_text": "Cascade text",
            "note": null,
            "color": "yellow"
        }),
    )
    .await;
    let annotation_id = annotation["id"]
        .as_str()
        .expect("annotation id")
        .to_string();

    let response = ctx
        .server
        .delete(&format!("/api/v1/books/{}", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&admin_token))
        .await;
    assert_status!(response, 200);

    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM book_annotations WHERE id = ?")
        .bind(annotation_id)
        .fetch_one(&ctx.db)
        .await
        .expect("query annotation count");
    assert_eq!(remaining, 0);
}
