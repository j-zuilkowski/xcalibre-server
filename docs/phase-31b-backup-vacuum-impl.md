# Phase 31b — Backup VACUUM INTO Implementation

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite/MariaDB.
`cargo clippy -- -D warnings` must pass at zero warnings.
Working dir: ~/Documents/localProject/xcalibre-server
Phase 31a complete: `test_backup_creates_valid_sqlite_file` failing (empty file).

---

## Edit: backend/src/api/admin/mod.rs

Locate `do_backup()` (the private async function, not the public handler `admin_backup`).
The current stub body is three lines:

```rust
async fn do_backup(state: &AppState) -> Result<Json<serde_json::Value>, AppError> {
    let d = std::path::Path::new(&state.config.backup.dir);
    std::fs::create_dir_all(d).map_err(|_| AppError::Internal)?;
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let fname = format!("xcalibre-{}.db", ts);
    let dest = d.join(&fname);
    std::fs::write(&dest, b"").map_err(|_| AppError::Internal)?;  // ← STUB
    Ok(Json(json!({"path": fname})))
}
```

Replace the stub line with the real VACUUM INTO call:

```rust
async fn do_backup(state: &AppState) -> Result<Json<serde_json::Value>, AppError> {
    let d = std::path::Path::new(&state.config.backup.dir);
    std::fs::create_dir_all(d).map_err(|_| AppError::Internal)?;
    let ts = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    let fname = format!("xcalibre-{}.db", ts);
    let dest = d.join(&fname);
    let dest_str = dest.to_str().ok_or(AppError::Internal)?;
    sqlx::query("VACUUM INTO ?")
        .bind(dest_str)
        .execute(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(json!({"path": fname})))
}
```

No other changes to this file.

### Why VACUUM INTO?

`VACUUM INTO '<path>'` is the official SQLite hot-backup mechanism. It:
- Creates a new, compacted copy of the database at the given path
- Runs safely while the database is open and under write load
- Produces a valid SQLite file with correct magic bytes and page structure
- Does not require WAL checkpoint management

`sqlx::query("VACUUM INTO ?").execute(&state.db)` works with the existing `SqlitePool`
because sqlx routes `VACUUM` to a single connection internally.

---

## Version bump: 2.6.0 → 2.7.0

Edit `backend/Cargo.toml`:

```diff
-version = "2.6.0"
+version = "2.7.0"
```

Edit `backend/src/api/version.rs` (or wherever the version string constant lives) if a
hardcoded `VERSION` const exists:

```diff
-pub const VERSION: &str = "2.6.0";
+pub const VERSION: &str = "2.7.0";
```

---

## Verify

```bash
cd ~/Documents/localProject/xcalibre-server

# Target test passes
cargo test test_backup_creates_valid_sqlite_file 2>&1 | tail -10

# Full suite still green
cargo test 2>&1 | tail -20

# Zero clippy warnings
cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: all tests pass, zero warnings.

---

## Commit, tag, push

```bash
cd ~/Documents/localProject/xcalibre-server
git add backend/src/api/admin/mod.rs \
        backend/Cargo.toml \
        docs/phase-31a-backup-vacuum-tests.md \
        docs/phase-31b-backup-vacuum-impl.md
git commit -m "Phase 31b — Replace backup stub with VACUUM INTO; v2.7.0"
git tag v2.7.0
git push origin main --tags
gh release create v2.7.0 \
    --repo j-zuilkowski/xcalibre-server \
    --title "v2.7.0 — Real SQLite backup + Kobo cover resolution" \
    --notes "Phase 31: POST /api/v1/admin/backup now produces a valid SQLite database via VACUUM INTO instead of an empty placeholder file. Phase 32: Kobo /images/:uuid/... now resolves the book UUID via the identifiers table and serves the actual cover; falls back to 1×1 white JPEG only when no cover exists." \
    --latest
```

> **Note:** Tag and release after **both** phase 31b and 32b are committed so the
> release notes describe the complete v2.7.0 change set. Commit phase 31b first, then
> continue to phase 32 before tagging.
