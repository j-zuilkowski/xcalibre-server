# Phase 25b — Shelf Reordering + Inline Book Serve Implementation

## Context
Rust 2021, Axum 0.7.
Phase 25a complete: failing tests in:
- `backend/tests/test_shelf_reorder.rs`
- `backend/tests/test_inline_serve.rs`

Goal: implement shelf ordering and inline file serving while preserving existing authorization and file-serving safeguards.

---

## 1. Migration for shelf ordering column

### `backend/migrations/sqlite/0028_shelf_books_position.sql`

```sql
ALTER TABLE shelf_books
ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
```

### `backend/migrations/mariadb/0028_shelf_books_position.sql`

```sql
ALTER TABLE shelf_books
ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
```

If column already exists in one backend, make migration idempotent for that backend style.

---

## 2. Shelf reorder endpoint

### Add route in shelves router (`backend/src/api/shelves.rs`)

`PUT /api/v1/shelves/:shelf_id/order`

Request:

```json
{ "book_ids": ["id1", "id2", "id3"] }
```

Response on success:

```json
{}
```

### Handler behavior

- Require authenticated user.
- Load shelf by id; `404` if missing.
- Enforce ownership/admin; `403` if caller is neither shelf owner nor admin.
- Fetch current shelf members.
- Validate:
  - no unknown ids in request (`400`)
  - no missing shelf member ids (`400`)
- Run transaction:
  - for each input id with index `idx`, `UPDATE shelf_books SET position = ? WHERE shelf_id = ? AND book_id = ?`
- Return `200 {}`.

### Ensure shelf detail ordering

Update shelf details query for `GET /api/v1/shelves/:id` to:

```sql
ORDER BY shelf_books.position ASC, books.title ASC
```

---

## 3. Inline serving endpoint

### Extract shared file streaming helper in `backend/src/api/books.rs`

Create helper:

```rust
enum ContentDispositionMode {
    Attachment,
    Inline,
}

async fn stream_book_format(
    state: &AppState,
    auth: &AuthenticatedUser,
    book_id: &str,
    format: &str,
    disposition: ContentDispositionMode,
) -> Result<Response, AppError> {
    // existing download logic moved here
}
```

Existing route:
- `/api/v1/books/:book_id/download/:format` calls helper with `Attachment`.

New route:
- `/api/v1/books/:book_id/view/:format` calls helper with `Inline`.

Headers:
- `Content-Disposition: inline; filename="..."` for view route.
- Content-Type based on format (epub => `application/epub+zip`, etc.)

Errors:
- `404` for missing format.
- `403` for no read access.

---

## 4. Router and docs

### Router

Register `/view/:format` in books router.

### API docs

Update `docs/API.md`:
- `PUT /api/v1/shelves/:shelf_id/order`
- `GET /api/v1/books/:book_id/view/:format`

Include auth and error behavior.

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_shelf_reorder --test test_inline_serve 2>&1 | tail -60
cargo clippy -- -D warnings 2>&1 | tail -40
cargo audit 2>&1 | tail -40
```
Expected: **all shelf reorder and inline serve tests pass**, zero clippy warnings, zero audit vulnerabilities.

## Commit
```bash
git add backend/migrations/sqlite/0028_shelf_books_position.sql \
        backend/migrations/mariadb/0028_shelf_books_position.sql \
        backend/src/api/shelves.rs \
        backend/src/api/books.rs \
        docs/API.md
git commit -m "Phase 25b — shelf reorder and inline book serve implementation"
```
