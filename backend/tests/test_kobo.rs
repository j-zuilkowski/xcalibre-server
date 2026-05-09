#![allow(dead_code, unused_imports)]

mod common;

use axum::http::header;
use chrono::Utc;
use common::{auth_header, TestContext};
use serde_json::Value;
use sqlx::Row;

async fn create_kobo_token(ctx: &TestContext) -> String {
    let admin_token = ctx.admin_token().await;
    let response = ctx
        .server
        .post("/api/v1/admin/tokens")
        .add_header(header::AUTHORIZATION, auth_header(&admin_token))
        .json(&serde_json::json!({ "name": "kobo-device" }))
        .await;
    assert_status!(response, 201);
    let body: Value = response.json();
    body["token"].as_str().unwrap_or_default().to_string()
}

#[tokio::test]
async fn test_kobo_initialization_registers_device() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    let path = format!("/kobo/{token}/v1/initialization");

    let response = ctx
        .server
        .get(path.as_str())
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-001"),
        )
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-devicename"),
            axum::http::HeaderValue::from_static("Kobo Clara"),
        )
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    assert_eq!(body["device_id"], "device-001");
    assert_eq!(body["device_name"], "Kobo Clara");

    let row = sqlx::query("SELECT device_id, device_name FROM kobo_devices WHERE device_id = ?")
        .bind("device-001")
        .fetch_one(&ctx.db)
        .await
        .expect("registered device");
    assert_eq!(row.get::<String, _>("device_id"), "device-001");
    assert_eq!(row.get::<String, _>("device_name"), "Kobo Clara");
}

#[tokio::test]
async fn test_kobo_sync_returns_book_list() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    let _ = ctx.create_book_with_file("Alpha", "EPUB").await;
    let _ = ctx.create_book_with_file("Beta", "PDF").await;
    let init_path = format!("/kobo/{token}/v1/initialization");
    let sync_path = format!("/kobo/{token}/v1/library/sync");

    let init = ctx
        .server
        .get(init_path.as_str())
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-002"),
        )
        .await;
    assert_status!(init, 200);

    let response = ctx
        .server
        .get(sync_path.as_str())
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-002"),
        )
        .await;

    assert_status!(response, 200);
    let body: Value = response.json();
    let changed = body["ChangedBooks"].as_array().cloned().unwrap_or_default();
    assert_eq!(changed.len(), 2);
    assert_eq!(changed[0]["BookMetadata"]["title"], "Alpha");
    assert!(!body["SyncToken"].as_str().unwrap_or_default().is_empty());
}

#[tokio::test]
async fn test_kobo_sync_delta_returns_only_changed() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    let _first = ctx.create_book_with_file("First", "EPUB").await;
    let second = ctx.create_book_with_file("Second", "PDF").await;
    let init_path = format!("/kobo/{token}/v1/initialization");
    let sync_path = format!("/kobo/{token}/v1/library/sync");

    let init = ctx
        .server
        .get(init_path.as_str())
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-003"),
        )
        .await;
    assert_status!(init, 200);

    let first_sync = ctx
        .server
        .get(sync_path.as_str())
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-003"),
        )
        .await;
    assert_status!(first_sync, 200);

    let later = Utc::now().to_rfc3339();
    sqlx::query("UPDATE books SET last_modified = ? WHERE id = ?")
        .bind(&later)
        .bind(&second.0.id)
        .execute(&ctx.db)
        .await
        .expect("update book timestamp");

    let delta = ctx
        .server
        .get(sync_path.as_str())
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-003"),
        )
        .await;
    assert_status!(delta, 200);
    let body: Value = delta.json();
    let changed = body["ChangedBooks"].as_array().cloned().unwrap_or_default();
    assert_eq!(changed.len(), 1);
    assert_eq!(changed[0]["BookMetadata"]["title"], "Second");
}

#[tokio::test]
async fn test_kobo_reading_state_syncs_to_progress_table() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    let (book, _) = ctx.create_book_with_file("Read Me", "EPUB").await;
    let init_path = format!("/kobo/{token}/v1/initialization");
    let state_path = format!("/kobo/{token}/v1/library/{}/state", book.id);

    let init = ctx
        .server
        .get(init_path.as_str())
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-004"),
        )
        .await;
    assert_status!(init, 200);

    let response = ctx
        .server
        .put(state_path.as_str())
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-004"),
        )
        .json(&serde_json::json!({
            "position": "epubcfi(/6/2[chapter1]!/4/2/2)",
            "percent_read": 0.42,
            "last_modified": Utc::now().to_rfc3339(),
        }))
        .await;
    assert_status!(response, 200);

    let row = sqlx::query(
        r#"
        SELECT rp.cfi, rp.percentage, rp.book_id, kp.kobo_position
        FROM reading_progress rp
        INNER JOIN kobo_reading_state kp ON kp.book_id = rp.book_id
        WHERE rp.book_id = ?
        "#,
    )
    .bind(&book.id)
    .fetch_one(&ctx.db)
    .await
    .expect("reading progress row");

    assert_eq!(row.get::<String, _>("book_id"), book.id);
    assert_eq!(
        row.get::<String, _>("cfi"),
        "epubcfi(/6/2[chapter1]!/4/2/2)"
    );
    assert_eq!(row.get::<f64, _>("percentage"), 0.42);
    assert_eq!(
        row.get::<String, _>("kobo_position"),
        "epubcfi(/6/2[chapter1]!/4/2/2)"
    );
}

#[tokio::test]
async fn test_kobo_unknown_token_returns_401() {
    let ctx = TestContext::new().await;
    let path = "/kobo/not-a-token/v1/initialization";

    let response = ctx
        .server
        .get(path)
        .add_header(
            axum::http::HeaderName::from_static("x-kobo-deviceid"),
            axum::http::HeaderValue::from_static("device-005"),
        )
        .await;

    assert_status!(response, 401);
}

// ── Helper ────────────────────────────────────────────────────────────────────

/// Seeds a book with `has_cover = 1`, a cover JPEG on disk, and a UUID
/// identifier so `kobo_image` can look it up.
/// Returns `(book_id, uuid_str)`.
async fn seed_book_with_cover(ctx: &TestContext) -> (String, String) {
    let book_id = uuid::Uuid::new_v4().to_string();
    let book_uuid = uuid::Uuid::new_v4().to_string();
    let cover_file = format!("{}.jpg", book_id);
    let now = chrono::Utc::now().to_rfc3339();

    // Write a minimal valid JPEG to storage (not 1×1 white — use a 3-byte JFIF stub
    // that is recognisably different from WHITE_JPEG_1X1).
    let cover_bytes: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0xAB, 0xCD]; // distinct JFIF header
    let cover_path = ctx.storage.path().join(&cover_file);
    std::fs::write(&cover_path, &cover_bytes).expect("write cover");

    // Insert book row.
    sqlx::query(
        "INSERT INTO books (id, title, sort_title, has_cover, cover_path, flags, created_at, last_modified) \
         VALUES (?, 'Cover Book', 'Cover Book', 1, ?, NULL, ?, ?)"
    )
    .bind(&book_id)
    .bind(&cover_file)   // relative path — matches find_book_cover_path return value
    .bind(&now)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert book");

    // Insert UUID identifier.
    let ident_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO identifiers (id, book_id, id_type, value, last_modified) VALUES (?, ?, 'uuid', ?, ?)"
    )
    .bind(&ident_id)
    .bind(&book_id)
    .bind(&book_uuid)
    .bind(&now)
    .execute(&ctx.db)
    .await
    .expect("insert identifier");

    (book_id, book_uuid)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_kobo_image_resolves_cover_by_uuid() {
    let ctx = TestContext::new().await;
    let (_book_id, book_uuid) = seed_book_with_cover(&ctx).await;
    let token = create_kobo_token(&ctx).await;

    let path = format!("/kobo/{token}/v1/images/{book_uuid}/400/400/quality/false/image.jpg");
    let response = ctx.server.get(path.as_str()).await;

    assert_status!(response, 200);

    // The placeholder is 107 bytes (WHITE_JPEG_1X1 constant).
    // The seeded cover is 6 bytes but NOT the placeholder bytes.
    let body = response.as_bytes().to_vec();
    let white: &[u8] = &[
        0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46, 0x00, 0x01,
        0x01, 0x00, 0x00, 0x01,
    ];
    assert_ne!(
        &body[..body.len().min(16)],
        white,
        "response must be the seeded cover, not the 1×1 white placeholder"
    );
    // Verify JFIF SOI marker (0xFF 0xD8) — the seeded bytes start with this.
    assert_eq!(&body[..2], &[0xFF, 0xD8], "response must start with JFIF SOI marker");
}

#[tokio::test]
async fn test_kobo_image_falls_back_to_placeholder_for_unknown_uuid() {
    let ctx = TestContext::new().await;
    let token = create_kobo_token(&ctx).await;
    let unknown_uuid = uuid::Uuid::new_v4().to_string();

    let path = format!("/kobo/{token}/v1/images/{unknown_uuid}/400/400/quality/false/image.jpg");
    let response = ctx.server.get(path.as_str()).await;

    assert_status!(response, 200);
    // Any response is acceptable — just confirm we get a JPEG, not a 500.
    // The placeholder JFIF SOI is 0xFF 0xD8.
    let body = response.as_bytes().to_vec();
    assert!(!body.is_empty(), "placeholder response must not be empty");
    assert_eq!(&body[..2], &[0xFF, 0xD8], "placeholder must be a JPEG");
}
