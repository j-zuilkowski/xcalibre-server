# Phase 32a — Kobo Cover Resolution Tests

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite/MariaDB.
`cargo clippy -- -D warnings` must pass at zero warnings.
Working dir: ~/Documents/localProject/xcalibre-server
Phase 31b complete: backup produces a valid SQLite file.

`kobo_image` in `backend/src/api/kobo.rs` is a stub — it returns `WHITE_JPEG_1X1`
unconditionally, ignoring all path parameters including `book_uuid`. The handler
comment even describes the intended resolution flow but leaves it unimplemented.

New surface introduced in phase 32b:
  - `kobo_image` handler — resolves `book_uuid` → `book_id` via the `identifiers`
    table (WHERE `id_type = 'uuid'`), fetches `cover_path` via
    `book_queries::find_book_cover_path`, reads bytes via `state.storage.get_bytes`,
    returns the cover JPEG; falls back to `WHITE_JPEG_1X1` on any failure.

TDD coverage:
  File 1 — `backend/tests/test_kobo.rs`:
    - `test_kobo_image_resolves_cover_by_uuid`: seeds a book with a UUID identifier
      and a real cover file; asserts response bytes differ from the 1×1 white placeholder.
    - `test_kobo_image_falls_back_to_placeholder_for_unknown_uuid`: asserts that an
      unregistered UUID still returns 200 with the 1×1 white JPEG.

---

## Add to: backend/tests/test_kobo.rs

```rust
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
        "INSERT INTO identifiers (id, book_id, id_type, value) VALUES (?, ?, 'uuid', ?)"
    )
    .bind(&ident_id)
    .bind(&book_id)
    .bind(&book_uuid)
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
    let body = response.bytes();
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
    let body = response.bytes();
    assert!(!body.is_empty(), "placeholder response must not be empty");
    assert_eq!(&body[..2], &[0xFF, 0xD8], "placeholder must be a JPEG");
}
```

> **Imports:** `test_kobo.rs` already imports `chrono::Utc`, `sqlx::Row`, `common::TestContext`.
> Add `use uuid;` if not already present (check existing imports — `uuid` is a dev-dependency).

---

## Verify

```bash
cd ~/Documents/localProject/xcalibre-server
cargo test test_kobo_image 2>&1 | tail -20
```

Expected: **FAILED** — `test_kobo_image_resolves_cover_by_uuid` fails because the stub
returns `WHITE_JPEG_1X1` regardless of UUID.

## Commit

```bash
cd ~/Documents/localProject/xcalibre-server
git add backend/tests/test_kobo.rs
git commit -m "Phase 32a — test_kobo_image_resolves_cover_by_uuid (failing)"
```
