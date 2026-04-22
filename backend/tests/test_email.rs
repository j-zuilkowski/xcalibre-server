#![allow(dead_code, unused_imports)]

mod common;

use backend::{
    api::books::{build_book_email_message, send_message_via_transport},
    db::queries::email_settings::EmailSettings,
};
use common::{auth_header, TestContext};
use lettre::transport::stub::AsyncStubTransport;
use lettre::AsyncTransport;

#[tokio::test]
async fn test_send_returns_503_when_not_configured() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;
    let (book, _path) = ctx.create_book_with_file("Send Me", "EPUB").await;

    let response = ctx
        .server
        .post(&format!("/api/v1/books/{}/send", book.id))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "to": "user@kindle.com",
            "format": "EPUB"
        }))
        .await;

    assert_status!(response, 503);
}

#[tokio::test]
async fn test_admin_can_update_email_settings() {
    let ctx = TestContext::new().await;
    let token = ctx.admin_token().await;

    let response = ctx
        .server
        .put("/api/v1/admin/email-settings")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&serde_json::json!({
            "smtp_host": "smtp.example.com",
            "smtp_port": 587,
            "smtp_user": "mailer",
            "smtp_password": "secret",
            "from_address": "noreply@example.com",
            "use_tls": true
        }))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["smtp_host"], "smtp.example.com");
    assert_eq!(body["smtp_password"], "");

    let fetched = ctx
        .server
        .get("/api/v1/admin/email-settings")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(fetched, 200);
    let fetched_body: serde_json::Value = fetched.json();
    assert_eq!(fetched_body["smtp_user"], "mailer");
    assert_eq!(fetched_body["smtp_password"], "");
}

#[tokio::test]
async fn test_send_book_by_email() {
    let ctx = TestContext::new().await;
    let (book, path) = ctx.create_book_with_file("Kindle Me", "EPUB").await;
    let bytes = std::fs::read(path).expect("read fixture");

    let email_settings = EmailSettings {
        id: "singleton".to_string(),
        smtp_host: "smtp.example.com".to_string(),
        smtp_port: 587,
        smtp_user: "mailer".to_string(),
        smtp_password: "secret".to_string(),
        from_address: "noreply@example.com".to_string(),
        use_tls: true,
        updated_at: "2026-04-20T00:00:00Z".to_string(),
    };

    let message =
        build_book_email_message(&email_settings, &book, "user@kindle.com", "EPUB", &bytes)
            .expect("build message");

    let transport = AsyncStubTransport::new_ok();
    send_message_via_transport(&transport, message)
        .await
        .expect("send message");

    assert_eq!(transport.messages().await.len(), 1);
}
