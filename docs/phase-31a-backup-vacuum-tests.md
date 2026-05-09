# Phase 31a — Backup VACUUM INTO Tests

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite/MariaDB.
`cargo clippy -- -D warnings` must pass at zero warnings.
Working dir: ~/Documents/localProject/xcalibre-server
Phases 1–30 + 22-KAG complete. Current release: v2.6.0.

Phase 28b implemented `POST /api/v1/admin/backup` but `do_backup()` writes an empty
file via `std::fs::write(&dest, b"")` instead of running `VACUUM INTO`. Any backup
restore would fail silently — the file is 0 bytes with no SQLite header.

New surface introduced in phase 31b:
  - `do_backup()` in `backend/src/api/admin/mod.rs` — replaces `fs::write` stub with
    `sqlx::query("VACUUM INTO ?").bind(dest_str).execute(&state.db).await`

TDD coverage:
  File 1 — `backend/tests/test_admin.rs`: new test `test_backup_creates_valid_sqlite_file`
  verifies that `POST /api/v1/admin/backup` returns 200, a filename in the response body,
  and that the created file starts with the 16-byte SQLite3 magic bytes
  (`b"SQLite format 3\0"`).

---

## Add to: backend/tests/test_admin.rs

Append after the existing tests:

```rust
#[tokio::test]
async fn test_backup_creates_valid_sqlite_file() {
    // Use a temp dir for the backup so the test is self-contained.
    let backup_dir = tempfile::tempdir().expect("tempdir");
    let mut config = xcalibre_server::config::AppConfig::default();
    config.backup.dir = backup_dir.path().to_string_lossy().to_string();
    let ctx = TestContext::new_with_config(config).await;

    let token = ctx.admin_token().await;
    let response = ctx
        .server
        .post("/api/v1/admin/backup")
        .add_header(
            header::AUTHORIZATION,
            format!("Bearer {token}").parse().unwrap(),
        )
        .await;

    assert_status!(response, 200);

    let body: serde_json::Value = response.json();
    let fname = body["path"]
        .as_str()
        .expect("response must contain 'path' field");
    assert!(fname.ends_with(".db"), "filename must end with .db, got {fname}");

    let dest = backup_dir.path().join(fname);
    assert!(dest.exists(), "backup file was not created at {dest:?}");

    // SQLite3 magic: first 16 bytes are "SQLite format 3\0"
    let bytes = std::fs::read(&dest).expect("read backup file");
    assert!(
        bytes.len() >= 16,
        "backup file too small ({} bytes) — expected a valid SQLite database",
        bytes.len()
    );
    assert_eq!(
        &bytes[..16],
        b"SQLite format 3\0",
        "backup file does not start with SQLite3 magic bytes"
    );
}
```

> **Note:** `xcalibre_server::config::AppConfig` must be `pub` — verify the `config`
> module is already re-exported via `lib.rs`. If `AppConfig` is not accessible from the
> integration test, add `pub use config::AppConfig;` to `lib.rs`.

---

## Verify

```bash
cd ~/Documents/localProject/xcalibre-server
cargo test test_backup_creates_valid_sqlite_file 2>&1 | tail -20
```

Expected: **FAILED** — `do_backup` writes an empty file, so the magic-bytes assertion
fires: `backup file does not start with SQLite3 magic bytes`.

## Commit

```bash
cd ~/Documents/localProject/xcalibre-server
git add backend/tests/test_admin.rs
git commit -m "Phase 31a — test_backup_creates_valid_sqlite_file (failing)"
```
