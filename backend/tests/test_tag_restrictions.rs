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

async fn attach_tag(ctx: &TestContext, book_id: &str, tag_id: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1) ON CONFLICT(book_id, tag_id) DO UPDATE SET confirmed = 1",
    )
    .bind(book_id)
    .bind(tag_id)
    .execute(&ctx.db)
    .await
    .expect("attach tag");
    sqlx::query("UPDATE books SET last_modified = ? WHERE id = ?")
        .bind(&now)
        .bind(book_id)
        .execute(&ctx.db)
        .await
        .expect("touch book");
}

async fn set_restriction_via_db(ctx: &TestContext, user_id: &str, tag_id: &str, mode: &str) {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO user_tag_restrictions (user_id, tag_id, mode) VALUES (?, ?, ?) ON CONFLICT(user_id, tag_id) DO UPDATE SET mode = excluded.mode",
    )
    .bind(user_id)
    .bind(tag_id)
    .bind(mode)
    .execute(&ctx.db)
    .await
    .expect("insert restriction");
    sqlx::query("UPDATE users SET last_modified = ? WHERE id = ?")
        .bind(&now)
        .bind(user_id)
        .execute(&ctx.db)
        .await
        .expect("touch user");
}

#[tokio::test]
async fn test_blocked_tag_hides_book_from_list() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let book = ctx.create_book("Blocked Book", "Author A").await;
    let tag_id = add_tag(&ctx, "blocked-tag").await;
    attach_tag(&ctx, &book.id, &tag_id).await;

    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("user")
        .fetch_one(&ctx.db)
        .await
        .expect("load user");
    set_restriction_via_db(&ctx, &user_id, &tag_id, "block").await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 0);
}

#[tokio::test]
async fn test_allow_restriction_limits_visible_books() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let allowed = ctx.create_book("Allowed Book", "Author A").await;
    let hidden = ctx.create_book("Hidden Book", "Author A").await;
    let allow_tag_id = add_tag(&ctx, "allow-tag").await;
    let other_tag_id = add_tag(&ctx, "other-tag").await;
    attach_tag(&ctx, &allowed.id, &allow_tag_id).await;
    attach_tag(&ctx, &hidden.id, &other_tag_id).await;

    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("user")
        .fetch_one(&ctx.db)
        .await
        .expect("load user");
    set_restriction_via_db(&ctx, &user_id, &allow_tag_id, "allow").await;

    let response = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;
    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["total"], 1);
    assert_eq!(body["items"][0]["title"], "Allowed Book");
}

#[tokio::test]
async fn test_admin_can_set_restriction() {
    let ctx = TestContext::new().await;
    let admin_token = ctx.admin_token().await;
    let _user_token = ctx.user_token().await;
    let _book = ctx.create_book("Admin Book", "Author A").await;
    let tag_id = add_tag(&ctx, "admin-tag").await;
    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("user")
        .fetch_one(&ctx.db)
        .await
        .expect("load user");

    let response = ctx
        .server
        .post(&format!("/api/v1/admin/users/{user_id}/tag-restrictions"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&admin_token))
        .json(&serde_json::json!({
            "tag_id": tag_id,
            "mode": "block"
        }))
        .await;
    assert_status!(response, 204);

    let row =
        sqlx::query("SELECT mode FROM user_tag_restrictions WHERE user_id = ? AND tag_id = ?")
            .bind(&user_id)
            .bind(&tag_id)
            .fetch_one(&ctx.db)
            .await
            .expect("restriction row");
    assert_eq!(row.get::<String, _>("mode"), "block");
}

#[tokio::test]
async fn test_restriction_does_not_affect_other_users() {
    let ctx = TestContext::new().await;
    let user_token = ctx.user_token().await;
    let admin_token = ctx.admin_token().await;
    let book = ctx.create_book("Shared Book", "Author A").await;
    let tag_id = add_tag(&ctx, "shared-tag").await;
    attach_tag(&ctx, &book.id, &tag_id).await;

    let user_id: String = sqlx::query_scalar("SELECT id FROM users WHERE username = ?")
        .bind("user")
        .fetch_one(&ctx.db)
        .await
        .expect("load user");

    set_restriction_via_db(&ctx, &user_id, &tag_id, "block").await;

    let user_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .await;
    assert_status!(user_response, 200);
    let user_body: serde_json::Value = user_response.json();
    assert_eq!(user_body["total"], 0);

    let admin_response = ctx
        .server
        .get("/api/v1/books")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&admin_token))
        .await;
    assert_status!(admin_response, 200);
    let admin_body: serde_json::Value = admin_response.json();
    assert_eq!(admin_body["total"], 1);
    assert_eq!(admin_body["items"][0]["title"], "Shared Book");
}
