#![allow(dead_code, unused_imports)]

mod common;

use common::{auth_header, TestContext};

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
async fn test_list_annotations_only_returns_own() {
    let ctx = TestContext::new().await;
    let user_token = ctx.user_token().await;
    let admin_token = ctx.admin_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;

    let user_annotation = create_annotation(
        &ctx,
        &user_token,
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

    let _admin_annotation = create_annotation(
        &ctx,
        &admin_token,
        &book.id,
        serde_json::json!({
            "type": "highlight",
            "cfi_range": "epubcfi(/6/4[chap01]!/4/2/1:33,/1:64)",
            "highlighted_text": "Admin text",
            "note": null,
            "color": "blue"
        }),
    )
    .await;

    let response = ctx
        .server
        .get(&format!("/api/v1/books/{}/annotations", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
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
async fn test_update_annotation_owned_by_other_user_returns_403() {
    let ctx = TestContext::new().await;
    let user_token = ctx.user_token().await;
    let admin_token = ctx.admin_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;
    let annotation = create_annotation(
        &ctx,
        &admin_token,
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
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .json(&serde_json::json!({
            "color": "green"
        }))
        .await;

    assert_status!(response, 403);
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
async fn test_delete_annotation_owned_by_other_user_returns_403() {
    let ctx = TestContext::new().await;
    let user_token = ctx.user_token().await;
    let admin_token = ctx.admin_token().await;
    let book = ctx.create_book("Annotation Book", "Reader").await;
    let annotation = create_annotation(
        &ctx,
        &admin_token,
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
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&user_token))
        .await;

    assert_status!(response, 403);
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
