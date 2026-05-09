# Phase 32b — Kobo Cover Resolution Implementation

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite/MariaDB.
`cargo clippy -- -D warnings` must pass at zero warnings.
Working dir: ~/Documents/localProject/xcalibre-server
Phase 32a complete: `test_kobo_image_resolves_cover_by_uuid` failing (stub returns placeholder).

---

## Edit: backend/src/api/kobo.rs

### 1. Replace the stub handler

Find:
```rust
async fn kobo_image(
    State(_state): State<AppState>,
    Path((_, _book_uuid, _width, _height, _quality, _greyscale)): Path<(
        String, // placeholder for kobo_token — captured by outer route
        String, // book_uuid
        String, // width
        String, // height
        String, // quality
        String, // greyscale
    )>,
) -> Result<Response<Body>, AppError> {
    // Try to resolve book_uuid to a book via identifier lookup.
    // If found and cover exists, serve the cover.
    // For now, return the 1×1 white JPEG placeholder.
    let response = Response::builder()
        .header(header::CONTENT_TYPE, "image/jpeg")
        .body(Body::from(WHITE_JPEG_1X1.to_vec()))
        .map_err(|_| AppError::Internal)?;
    Ok(response)
}
```

Replace with:
```rust
async fn kobo_image(
    State(state): State<AppState>,
    Path((_, book_uuid, _width, _height, _quality, _greyscale)): Path<(
        String, // kobo_token — consumed by outer route
        String, // book_uuid
        String, // width
        String, // height
        String, // quality
        String, // greyscale
    )>,
) -> Result<Response<Body>, AppError> {
    // 1. Resolve UUID → internal book_id via identifiers table.
    if let Some(book_id) = sqlx::query_scalar::<_, String>(
        "SELECT book_id FROM identifiers WHERE id_type = 'uuid' AND value = ? LIMIT 1",
    )
    .bind(&book_uuid)
    .fetch_optional(&state.db)
    .await
    .unwrap_or(None)
    {
        // 2. Look up the cover path (only returned when has_cover = 1).
        if let Ok(Some(cover_path)) =
            crate::db::queries::books::find_book_cover_path(&state.db, &book_id).await
        {
            // 3. Read cover bytes through the storage backend (works for local and S3).
            if let Ok(bytes) = state.storage.get_bytes(&cover_path).await {
                return Ok(Response::builder()
                    .header(header::CONTENT_TYPE, "image/jpeg")
                    .body(Body::from(bytes))
                    .map_err(|_| AppError::Internal)?);
            }
        }
    }

    // 4. Fallback: 1×1 white JPEG placeholder.
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "image/jpeg")
        .body(Body::from(WHITE_JPEG_1X1.to_vec()))
        .map_err(|_| AppError::Internal)?)
}
```

### 2. No new imports required

`crate::db::queries::books` is already re-exported via the `use crate::{...}` block at
the top of `kobo.rs`. Check what the existing import looks like:

```rust
use crate::{
    db::queries::{books as book_queries, kobo as kobo_queries, shelves as shelf_queries},
    ...
};
```

If `books` is already aliased as `book_queries`, use that alias instead:

```rust
if let Ok(Some(cover_path)) =
    book_queries::find_book_cover_path(&state.db, &book_id).await
```

Use whichever form matches the existing import — do not add a duplicate `use` statement.

### 3. `find_book_cover_path` visibility

`find_book_cover_path` is declared `pub` in `backend/src/db/queries/books.rs`. No
visibility change needed.

---

## Update: GAP.md

The "Remaining Open Gaps" table (near the bottom of `GAP.md`) still shows items 1–7 as
open (🔴 or 🟡). All of them were closed by Phase 30. Update both sub-tables:

```diff
-**High — False Positives (were incorrectly marked ✅)**
-
-| # | Gap | calibre-web Routes | Priority |
-|---|-----|--------------------|----------|
-| 1 | **Kobo tag sync** | ... | 🔴 High — Kobo shelves can't be created from device |
-| 2 | **OPDS category/tag feeds** | ... | 🔴 High — e-reader genre browse returns empty |
-| 3 | **OPDS read/unread feeds** | ... | 🔴 High — "Read" shortcut broken on OPDS clients |
-
-**Medium — Undocumented Gaps Found in Audit**
-
-| # | Gap | calibre-web Routes | Priority |
-|---|-----|--------------------|----------|
-| 4 | **Shelf edit** (rename / toggle public) | ... | 🟡 Medium — users can't rename shelves |
-| 5 | **OPDS shelf feed** | ... | 🟡 Medium — shelf browsing from e-reader impossible |
-| 6 | **OPDS formats feed** | ... | 🟡 Medium — format-based browsing not available |
-| 7 | **OPDS book UUID lookup** | ... | 🟡 Medium — some clients use UUID-based single-book fetch |
+**All previously open gaps closed by Phase 30 + Phase 32**
+
+| # | Gap | Closed |
+|---|-----|--------|
+| 1 | Kobo tag sync | ✅ Phase 30 |
+| 2 | OPDS category/tag feeds | ✅ Phase 30 |
+| 3 | OPDS read/unread feeds | ✅ Phase 30 |
+| 4 | Shelf edit (rename / toggle public) | ✅ Phase 30 |
+| 5 | OPDS shelf feed | ✅ Phase 30 |
+| 6 | OPDS formats feed | ✅ Phase 30 |
+| 7 | OPDS book UUID lookup | ✅ Phase 30 |
+| 8 | Kobo cover serving (UUID → book cover) | ✅ Phase 32 |
+| 9 | Backup produces valid SQLite file | ✅ Phase 31 |
```

---

## Verify

```bash
cd ~/Documents/localProject/xcalibre-server

# Target tests pass
cargo test test_kobo_image 2>&1 | tail -10

# Backup test still passes
cargo test test_backup_creates_valid_sqlite_file 2>&1 | tail -10

# Full suite
cargo test 2>&1 | tail -20

# Zero clippy warnings
cargo clippy -- -D warnings 2>&1 | tail -10
```

Expected: all tests pass, zero warnings.

---

## Commit, tag, push

```bash
cd ~/Documents/localProject/xcalibre-server
git add backend/src/api/kobo.rs \
        GAP.md \
        docs/phase-32a-kobo-cover-tests.md \
        docs/phase-32b-kobo-cover-impl.md
git commit -m "Phase 32b — Resolve Kobo cover by UUID; fix GAP.md; v2.7.0"
git tag v2.7.0
git push origin main --tags
gh release create v2.7.0 \
    --repo j-zuilkowski/xcalibre-server \
    --title "v2.7.0 — Real SQLite backup + Kobo cover resolution" \
    --notes "Phase 31: POST /api/v1/admin/backup now uses VACUUM INTO to produce a valid, restorable SQLite database instead of an empty placeholder file.\n\nPhase 32: GET /kobo/:token/v1/images/:uuid/... now resolves the Kobo book UUID via the identifiers table, fetches the cover path from the books table, and serves the actual cover image through the configured storage backend. Falls back to the 1×1 white JPEG placeholder when no cover is available." \
    --latest
```
