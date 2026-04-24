#![allow(dead_code, unused_imports)]

mod common;

use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    routing::post,
    Router,
};
use backend::{
    app, auth::totp as totp_auth, config::AppConfig, db::queries::webhooks as webhook_queries,
    webhooks as webhook_engine, AppState,
};
use common::{auth_header, test_db, TestContext, TEST_JWT_SECRET};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use sqlx::Row;
use tempfile::TempDir;
use tokio::sync::Mutex;
use uuid::Uuid;

#[derive(Clone, Debug)]
struct CapturedRequest {
    headers: HeaderMap,
    body: String,
}

#[derive(Clone)]
struct HookServerState {
    requests: Arc<Mutex<Vec<CapturedRequest>>>,
    calls: Arc<AtomicUsize>,
    fail_first: bool,
    always_fail: bool,
}

impl HookServerState {
    fn new(fail_first: bool, always_fail: bool) -> Self {
        Self {
            requests: Arc::new(Mutex::new(Vec::new())),
            calls: Arc::new(AtomicUsize::new(0)),
            fail_first,
            always_fail,
        }
    }
}

async fn hook_handler(
    State(state): State<HookServerState>,
    headers: HeaderMap,
    body: String,
) -> StatusCode {
    state.calls.fetch_add(1, Ordering::SeqCst);
    state
        .requests
        .lock()
        .await
        .push(CapturedRequest { headers, body });

    if state.always_fail {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    if state.fail_first && state.calls.load(Ordering::SeqCst) == 1 {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::OK
}

async fn start_hook_server(
    fail_first: bool,
    always_fail: bool,
) -> (reqwest::Client, Arc<Mutex<Vec<CapturedRequest>>>) {
    let state = HookServerState::new(fail_first, always_fail);
    let requests = state.requests.clone();
    let app = Router::new()
        .route("/hook", post(hook_handler))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind hook server");
    let addr = listener.local_addr().expect("hook server addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("hook server");
    });

    let client = reqwest::Client::builder()
        .resolve("example.com", addr)
        .build()
        .expect("build client");

    (client, requests)
}

async fn custom_context(http_client: reqwest::Client) -> TestContext {
    let storage = tempfile::tempdir().expect("tempdir");
    let db = test_db().await;
    std::env::set_var("AUTOLIBRE_DISABLE_METRICS", "1");

    let mut config = AppConfig::default();
    config.app.storage_path = storage.path().to_string_lossy().to_string();
    config.auth.jwt_secret = TEST_JWT_SECRET.to_string();

    let mut state = AppState::new(db.clone(), config)
        .await
        .expect("initialize app state");
    state.http_client = http_client;
    let server = axum_test::TestServer::new(app(state.clone())).expect("build test server");

    TestContext {
        db,
        storage,
        server,
        state,
    }
}

async fn insert_webhook(
    ctx: &TestContext,
    user_id: &str,
    url: &str,
    secret: &str,
    events: &[&str],
    enabled: bool,
) -> String {
    let encrypted_secret =
        totp_auth::encrypt_webhook_secret(secret, ctx.jwt_secret()).expect("encrypt secret");
    let events_json = serde_json::to_string(&events).expect("serialize events");
    let webhook = webhook_queries::create_webhook(
        &ctx.db,
        user_id,
        url,
        &encrypted_secret,
        &events_json,
        enabled,
    )
    .await
    .expect("create webhook");
    webhook.id
}

fn payload_for_event(event: &str) -> serde_json::Value {
    json!({
        "event": event,
        "timestamp": "2026-04-22T20:00:00Z",
        "library_name": "My Library",
        "data": {
            "id": "book-1",
            "title": "Webhook Book",
            "authors": ["Author One"],
            "formats": ["EPUB"],
            "cover_url": null
        }
    })
}

#[tokio::test]
async fn test_create_webhook_stores_encrypted_secret() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .post("/api/v1/users/me/webhooks")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "url": "https://example.com/hook",
            "secret": "my-secret",
            "events": ["book.added"]
        }))
        .await;

    assert_status!(response, 201);
    let body: serde_json::Value = response.json();
    let webhook_id = body["id"].as_str().expect("webhook id");

    let row = sqlx::query("SELECT secret FROM webhooks WHERE id = ?")
        .bind(webhook_id)
        .fetch_one(&ctx.db)
        .await
        .expect("fetch webhook");
    let stored_secret: String = row.get("secret");
    assert_ne!(stored_secret, "my-secret");

    let decrypted =
        totp_auth::decrypt_webhook_secret(&stored_secret, ctx.jwt_secret()).expect("decrypt");
    assert_eq!(decrypted, "my-secret");
}

#[tokio::test]
async fn test_create_webhook_rejects_http_url() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .post("/api/v1/users/me/webhooks")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "url": "http://example.com/hook",
            "secret": "my-secret",
            "events": ["book.added"]
        }))
        .await;

    assert_status!(response, 422);
}

#[tokio::test]
async fn test_create_webhook_rejects_unknown_events() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .post("/api/v1/users/me/webhooks")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "url": "https://example.com/hook",
            "secret": "my-secret",
            "events": ["unknown.event"]
        }))
        .await;

    assert_status!(response, 422);
}

#[tokio::test]
async fn test_create_webhook_rejects_private_ip_ssrf() {
    let ctx = TestContext::new().await;
    let token = ctx.user_token().await;

    let response = ctx
        .server
        .post("/api/v1/users/me/webhooks")
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .json(&json!({
            "url": "https://127.0.0.1/hook",
            "secret": "my-secret",
            "events": ["book.added"]
        }))
        .await;

    assert_status!(response, 422);
    let body: serde_json::Value = response.json();
    assert_eq!(body["error"], "ssrf_blocked");
}

#[tokio::test]
async fn test_enqueue_event_creates_delivery_for_subscribed_webhooks() {
    let ctx = TestContext::new().await;
    let (user, _) = ctx.create_user().await;
    let _enabled = insert_webhook(
        &ctx,
        &user.id,
        "https://example.com/one",
        "secret-a",
        &["book.added"],
        true,
    )
    .await;
    let _other = insert_webhook(
        &ctx,
        &user.id,
        "https://example.com/two",
        "secret-b",
        &["book.deleted"],
        true,
    )
    .await;

    webhook_engine::enqueue_event(&ctx.db, "book.added", payload_for_event("book.added"))
        .await
        .expect("enqueue event");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM webhook_deliveries")
        .fetch_one(&ctx.db)
        .await
        .expect("count deliveries");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn test_enqueue_event_skips_disabled_webhooks() {
    let ctx = TestContext::new().await;
    let (user, _) = ctx.create_user().await;
    let _disabled = insert_webhook(
        &ctx,
        &user.id,
        "https://example.com/one",
        "secret-a",
        &["book.added"],
        false,
    )
    .await;

    webhook_engine::enqueue_event(&ctx.db, "book.added", payload_for_event("book.added"))
        .await
        .expect("enqueue event");

    let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM webhook_deliveries")
        .fetch_one(&ctx.db)
        .await
        .expect("count deliveries");
    assert_eq!(count, 0);
}

#[tokio::test]
async fn test_delivery_sends_correct_hmac_signature() {
    let (http_client, requests) = start_hook_server(false, false).await;
    let ctx = custom_context(http_client).await;
    let (user, _) = ctx.create_user().await;
    let secret = "super-secret";
    let _webhook_id = insert_webhook(
        &ctx,
        &user.id,
        "http://example.com/hook",
        secret,
        &["book.added"],
        true,
    )
    .await;
    let payload = payload_for_event("book.added");

    webhook_engine::enqueue_event(&ctx.db, "book.added", payload.clone())
        .await
        .expect("enqueue event");
    webhook_engine::deliver_pending(&ctx.db, &ctx.state.http_client)
        .await
        .expect("deliver pending");

    let captured = requests.lock().await;
    assert_eq!(captured.len(), 1);
    let request = &captured[0];
    assert_eq!(
        request
            .headers
            .get("X-Autolibre-Event")
            .and_then(|value| value.to_str().ok()),
        Some("book.added")
    );
    let parsed_body: serde_json::Value = serde_json::from_str(&request.body).expect("parse body");
    assert_eq!(parsed_body, payload);

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("mac");
    mac.update(request.body.as_bytes());
    let expected = format!("sha256={}", hex::encode(mac.finalize().into_bytes()));

    assert_eq!(
        request
            .headers
            .get("X-Autolibre-Signature")
            .and_then(|value| value.to_str().ok()),
        Some(expected.as_str())
    );
}

#[tokio::test]
async fn test_delivery_retries_on_failure() {
    let (http_client, _requests) = start_hook_server(true, false).await;
    let ctx = custom_context(http_client).await;
    let (user, _) = ctx.create_user().await;
    let webhook_id = insert_webhook(
        &ctx,
        &user.id,
        "http://example.com/hook",
        "retry-secret",
        &["book.added"],
        true,
    )
    .await;

    webhook_engine::enqueue_event(&ctx.db, "book.added", payload_for_event("book.added"))
        .await
        .expect("enqueue event");

    webhook_engine::deliver_pending(&ctx.db, &ctx.state.http_client)
        .await
        .expect("first delivery");

    let row = sqlx::query(
        "SELECT status, attempts, next_attempt_at FROM webhook_deliveries WHERE webhook_id = ?",
    )
    .bind(&webhook_id)
    .fetch_one(&ctx.db)
    .await
    .expect("delivery row");
    assert_eq!(row.get::<String, _>("status"), "pending");
    assert_eq!(row.get::<i64, _>("attempts"), 1);

    sqlx::query("UPDATE webhook_deliveries SET next_attempt_at = ? WHERE webhook_id = ?")
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&webhook_id)
        .execute(&ctx.db)
        .await
        .expect("rewind next attempt");

    webhook_engine::deliver_pending(&ctx.db, &ctx.state.http_client)
        .await
        .expect("second delivery");

    let row = sqlx::query("SELECT status, attempts FROM webhook_deliveries WHERE webhook_id = ?")
        .bind(&webhook_id)
        .fetch_one(&ctx.db)
        .await
        .expect("delivery row");
    assert_eq!(row.get::<String, _>("status"), "delivered");
    assert_eq!(row.get::<i64, _>("attempts"), 2);
}

#[tokio::test]
async fn test_delivery_marks_failed_after_3_attempts() {
    let (http_client, _requests) = start_hook_server(false, true).await;
    let ctx = custom_context(http_client).await;
    let (user, _) = ctx.create_user().await;
    let webhook_id = insert_webhook(
        &ctx,
        &user.id,
        "http://example.com/hook",
        "fail-secret",
        &["book.added"],
        true,
    )
    .await;

    webhook_engine::enqueue_event(&ctx.db, "book.added", payload_for_event("book.added"))
        .await
        .expect("enqueue event");

    for _ in 0..3 {
        webhook_engine::deliver_pending(&ctx.db, &ctx.state.http_client)
            .await
            .expect("delivery attempt");

        sqlx::query(
            "UPDATE webhook_deliveries SET next_attempt_at = ? WHERE webhook_id = ? AND status = 'pending'",
        )
        .bind(chrono::Utc::now().to_rfc3339())
        .bind(&webhook_id)
        .execute(&ctx.db)
        .await
        .expect("rewind next attempt");
    }

    let row = sqlx::query("SELECT status, attempts FROM webhook_deliveries WHERE webhook_id = ?")
        .bind(&webhook_id)
        .fetch_one(&ctx.db)
        .await
        .expect("delivery row");
    assert_eq!(row.get::<String, _>("status"), "failed");
    assert_eq!(row.get::<i64, _>("attempts"), 3);
}

#[tokio::test]
async fn test_test_endpoint_fires_ping_synchronously() {
    let (http_client, requests) = start_hook_server(false, false).await;
    let ctx = custom_context(http_client).await;
    let (user, password) = ctx.create_user().await;
    let webhook_id = insert_webhook(
        &ctx,
        &user.id,
        "http://example.com/hook",
        "ping-secret",
        &["book.added"],
        true,
    )
    .await;
    let token = ctx.login(&user.username, &password).await.access_token;

    let response = ctx
        .server
        .post(&format!("/api/v1/users/me/webhooks/{webhook_id}/test"))
        .add_header(axum::http::header::AUTHORIZATION, auth_header(&token))
        .await;

    assert_status!(response, 200);
    let body: serde_json::Value = response.json();
    assert_eq!(body["delivered"], true);
    assert_eq!(body["response_status"], 200);

    let captured = requests.lock().await;
    assert_eq!(captured.len(), 1);
    assert_eq!(
        captured[0]
            .headers
            .get("X-Autolibre-Event")
            .and_then(|value| value.to_str().ok()),
        Some("ping")
    );
    let parsed_body: serde_json::Value =
        serde_json::from_str(&captured[0].body).expect("parse body");
    assert_eq!(
        parsed_body,
        json!({ "message": "Webhook test from autolibre" })
    );
}
