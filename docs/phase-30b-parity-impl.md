# Phase 30b — OPDS Parity II + Kobo Tag Sync + Shelf Edit: Implementation

## Context

Rust 2021, Axum 0.7.
Phase 30a complete: failing tests in `backend/tests/test_kobo_tags.rs`, `test_opds_parity.rs`, `test_shelf_edit.rs`.

Make all tests pass without modifying test files.

---

## 1. Test helpers — extend `backend/tests/common/mod.rs`

Add or update these helper methods on `TestContext`:

```rust
/// Seed a book and return its integer id.
pub async fn seed_book(&self, title: &str) -> i64 { /* existing or add */ }

/// Seed a book tagged with the given tag name. Returns book id.
pub async fn seed_book_with_tag(&self, title: &str, tag: &str) -> i64 {
    let book_id = self.seed_book(title).await;
    sqlx::query!(
        "INSERT OR IGNORE INTO tags (name) VALUES (?1);
         INSERT OR IGNORE INTO book_tags (book_id, tag_id)
           SELECT ?2, id FROM tags WHERE name = ?1",
        tag,
        book_id
    )
    .execute(&self.db)
    .await
    .expect("seed_book_with_tag");
    book_id
}

/// Seed a book with a format entry. Returns book id.
pub async fn seed_book_with_format(&self, title: &str, format: &str) -> i64 {
    let book_id = self.seed_book(title).await;
    sqlx::query!(
        "INSERT OR IGNORE INTO book_formats (book_id, format, path)
         VALUES (?1, UPPER(?2), ?3)",
        book_id,
        format,
        format!("{title}.{format}")
    )
    .execute(&self.db)
    .await
    .expect("seed_book_with_format");
    book_id
}

/// Mark a book read/unread for the default test user.
pub async fn mark_book_read(&self, book_id: i64, is_read: bool) {
    let user_id = &self.default_user_id;
    let flag: i64 = if is_read { 1 } else { 0 };
    sqlx::query!(
        "INSERT INTO book_user_state (user_id, book_id, is_read, is_archived)
         VALUES (?1, ?2, ?3, 0)
         ON CONFLICT (user_id, book_id) DO UPDATE SET is_read = excluded.is_read",
        user_id,
        book_id,
        flag
    )
    .execute(&self.db)
    .await
    .expect("mark_book_read");
}

/// Return the Kobo device book identifier (kobo_id or UUID) for a book.
pub async fn get_kobo_book_id(&self, book_id: i64) -> String {
    // Use book UUID from books table; Kobo sync uses this as RevisionId.
    sqlx::query_scalar!("SELECT uuid FROM books WHERE id = ?1", book_id)
        .fetch_one(&self.db)
        .await
        .expect("get_kobo_book_id")
}

/// Return book UUID.
pub async fn get_book_uuid(&self, book_id: i64) -> String {
    sqlx::query_scalar!("SELECT uuid FROM books WHERE id = ?1", book_id)
        .fetch_one(&self.db)
        .await
        .expect("get_book_uuid")
}

/// Create a public shelf via the REST API and return its id.
pub async fn seed_public_shelf(&self, name: &str, token: &str) -> String {
    let resp = self
        .server
        .post("/api/v1/shelves")
        .add_header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}"),
        )
        .json(&serde_json::json!({ "name": name, "public": true }))
        .await;
    resp.json::<serde_json::Value>()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Add a book to a shelf via the REST API.
pub async fn add_book_to_shelf(&self, book_id: i64, shelf_id: &str, token: &str) {
    let resp = self
        .server
        .post(&format!("/api/v1/shelves/{shelf_id}/books"))
        .add_header(
            axum::http::header::AUTHORIZATION,
            format!("Bearer {token}"),
        )
        .json(&serde_json::json!({ "book_id": book_id }))
        .await;
    assert_eq!(resp.status_code().as_u16(), 200);
}

/// Create a second user and return their JWT token.
pub async fn create_user_token(&self, email: &str) -> String {
    // Insert user directly, return a valid JWT for them.
    let user_id = uuid::Uuid::new_v4().to_string();
    let password_hash = "$argon2id$v=19$m=16,t=2,p=1$dGVzdA$testhashdoesnotmatter"; // not used
    sqlx::query!(
        "INSERT INTO users (id, email, username, password_hash, role)
         VALUES (?1, ?2, ?3, ?4, 'user')",
        user_id,
        email,
        email,
        password_hash
    )
    .execute(&self.db)
    .await
    .expect("create_user_token: insert user");
    crate::common::make_jwt(&user_id, "user", &self.jwt_secret)
}
```

Adjust field names (`uuid`, `default_user_id`, `jwt_secret`, DB query syntax) to match your actual `TestContext` struct. If `books` has no `uuid` column, use `id::TEXT` or add a UUID generation step.

---

## 2. Kobo tag sync — `backend/src/api/kobo.rs`

### 2.1 Router additions

```rust
// In pub fn router(state: AppState) -> Router<AppState>:
.route("/library/tags", post(create_kobo_tag))
.route("/library/tags/:tag_id", delete(delete_kobo_tag).put(rename_kobo_tag))
.route("/library/tags/:tag_id/items", post(add_kobo_tag_items))
.route("/library/tags/:tag_id/items/delete", delete(remove_kobo_tag_items))
```

Place these after the existing `/library/:kobo_book_id` routes.

### 2.2 Request/response types

```rust
#[derive(Deserialize)]
struct CreateTagBody {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Items", default)]
    items: Vec<KoboTagItem>,
}

#[derive(Deserialize)]
struct RenameTagBody {
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Deserialize)]
struct TagItemsBody {
    #[serde(rename = "Items")]
    items: Vec<KoboTagItem>,
}

#[derive(Deserialize)]
struct KoboTagItem {
    #[serde(rename = "RevisionId")]
    revision_id: String,
}
```

### 2.3 Handler — create tag

```rust
async fn create_kobo_tag(
    State(state): State<AppState>,
    Extension(context): Extension<KoboContext>,
    Json(body): Json<CreateTagBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Create a shelf via existing shelf_queries::create_shelf.
    let shelf_id = shelf_queries::create_shelf(
        &state.db,
        &context.user.id,
        &body.name,
        false,  // not public by default for Kobo-created shelves
    ).await?;

    // If items provided, add them to the shelf.
    for item in &body.items {
        if let Ok(book_id) = resolve_revision_id(&state, &item.revision_id).await {
            let _ = shelf_queries::add_book_to_shelf(&state.db, &shelf_id, book_id).await;
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({ "TagId": shelf_id })),
    ))
}
```

### 2.4 Handler — delete tag

```rust
async fn delete_kobo_tag(
    State(state): State<AppState>,
    Extension(context): Extension<KoboContext>,
    Path(tag_id): Path<String>,
) -> Result<StatusCode, AppError> {
    // Verify shelf belongs to this user.
    let exists = shelf_queries::get_shelf_owner(&state.db, &tag_id).await?;
    match exists {
        None => return Err(AppError::NotFound),
        Some(owner_id) if owner_id != context.user.id => return Err(AppError::Forbidden),
        _ => {}
    }
    shelf_queries::delete_shelf(&state.db, &tag_id).await?;
    Ok(StatusCode::OK)
}
```

### 2.5 Handler — rename tag

```rust
async fn rename_kobo_tag(
    State(state): State<AppState>,
    Extension(context): Extension<KoboContext>,
    Path(tag_id): Path<String>,
    Json(body): Json<RenameTagBody>,
) -> Result<StatusCode, AppError> {
    let exists = shelf_queries::get_shelf_owner(&state.db, &tag_id).await?;
    match exists {
        None => return Err(AppError::NotFound),
        Some(owner_id) if owner_id != context.user.id => return Err(AppError::Forbidden),
        _ => {}
    }
    shelf_queries::rename_shelf(&state.db, &tag_id, &body.name).await?;
    Ok(StatusCode::OK)
}
```

### 2.6 Handler — add items

```rust
async fn add_kobo_tag_items(
    State(state): State<AppState>,
    Extension(context): Extension<KoboContext>,
    Path(tag_id): Path<String>,
    Json(body): Json<TagItemsBody>,
) -> Result<StatusCode, AppError> {
    let exists = shelf_queries::get_shelf_owner(&state.db, &tag_id).await?;
    match exists {
        None => return Err(AppError::NotFound),
        Some(owner_id) if owner_id != context.user.id => return Err(AppError::Forbidden),
        _ => {}
    }
    for item in &body.items {
        if let Ok(book_id) = resolve_revision_id(&state, &item.revision_id).await {
            let _ = shelf_queries::add_book_to_shelf(&state.db, &tag_id, book_id).await;
        }
    }
    Ok(StatusCode::CREATED)
}
```

### 2.7 Handler — remove items

```rust
async fn remove_kobo_tag_items(
    State(state): State<AppState>,
    Extension(context): Extension<KoboContext>,
    Path(tag_id): Path<String>,
    Json(body): Json<TagItemsBody>,
) -> Result<StatusCode, AppError> {
    let exists = shelf_queries::get_shelf_owner(&state.db, &tag_id).await?;
    match exists {
        None => return Err(AppError::NotFound),
        Some(owner_id) if owner_id != context.user.id => return Err(AppError::Forbidden),
        _ => {}
    }
    for item in &body.items {
        if let Ok(book_id) = resolve_revision_id(&state, &item.revision_id).await {
            let _ = shelf_queries::remove_book_from_shelf(&state.db, &tag_id, book_id).await;
        }
    }
    Ok(StatusCode::OK)
}
```

### 2.8 Helper — resolve RevisionId to book_id

```rust
/// Resolve a Kobo RevisionId (UUID or kobo_id) to an integer book_id.
async fn resolve_revision_id(state: &AppState, revision_id: &str) -> Result<i64, AppError> {
    // Try matching book UUID first, then kobo_id in book_formats.
    let id = sqlx::query_scalar!(
        "SELECT id FROM books WHERE uuid = ?1
         UNION
         SELECT book_id FROM book_formats WHERE kobo_id = ?1
         LIMIT 1",
        revision_id
    )
    .fetch_optional(&state.db)
    .await?;
    id.ok_or(AppError::NotFound)
}
```

### 2.9 DB queries needed in `backend/src/db/queries/shelves.rs`

Add if missing:

```rust
pub async fn get_shelf_owner(
    db: &sqlx::SqlitePool,
    shelf_id: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar!("SELECT user_id FROM shelves WHERE id = ?1", shelf_id)
        .fetch_optional(db)
        .await
}

pub async fn rename_shelf(
    db: &sqlx::SqlitePool,
    shelf_id: &str,
    name: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE shelves SET name = ?1 WHERE id = ?2",
        name, shelf_id
    )
    .execute(db)
    .await?;
    Ok(())
}
```

---

## 3. OPDS parity feeds — `backend/src/api/opds.rs` (or `opds_enhancements.rs`)

### 3.1 Router additions

```rust
// Add to opds router:
.route("/category", get(category_feed))
.route("/category/letter/:ch", get(category_letter_feed))
.route("/category/:id", get(category_books_feed))
.route("/formats", get(formats_feed))
.route("/formats/:fmt", get(format_books_feed))
.route("/shelf", get(shelf_index_feed))
.route("/shelf/:id", get(shelf_books_feed))
.route("/readbooks", get(readbooks_feed))
.route("/unreadbooks", get(unreadbooks_feed))
.route("/ajax/book/:uuid", get(book_uuid_feed))
```

### 3.2 Category navigation feed

```rust
async fn category_feed(State(state): State<AppState>) -> Result<Response, AppError> {
    let tags = sqlx::query!(
        "SELECT t.id, t.name, COUNT(bt.book_id) AS cnt
         FROM tags t
         LEFT JOIN book_tags bt ON bt.tag_id = t.id
         GROUP BY t.id
         ORDER BY t.name"
    )
    .fetch_all(&state.db)
    .await?;

    let mut xml = String::new();
    push_feed_header(&mut xml, "Categories", "/opds/category", "navigation");
    for tag in &tags {
        push_navigation_entry(
            &mut xml,
            &tag.name,
            &format!("/opds/category/{}", tag.id),
            &format!("{} books", tag.cnt.unwrap_or(0)),
            "navigation",
        );
    }
    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}
```

### 3.3 Category letter feed

```rust
async fn category_letter_feed(
    State(state): State<AppState>,
    Path(ch): Path<String>,
) -> Result<Response, AppError> {
    use unicode_normalization::UnicodeNormalization;
    let prefix = ch.nfkd().collect::<String>().to_lowercase();

    let tags = sqlx::query!(
        "SELECT t.id, t.name, COUNT(bt.book_id) AS cnt
         FROM tags t
         LEFT JOIN book_tags bt ON bt.tag_id = t.id
         GROUP BY t.id"
    )
    .fetch_all(&state.db)
    .await?;

    let mut xml = String::new();
    push_feed_header(
        &mut xml,
        &format!("Categories — {}", ch.to_uppercase()),
        &format!("/opds/category/letter/{ch}"),
        "navigation",
    );
    for tag in tags.iter().filter(|t| {
        t.name
            .nfkd()
            .collect::<String>()
            .to_lowercase()
            .starts_with(&prefix)
    }) {
        push_navigation_entry(
            &mut xml,
            &tag.name,
            &format!("/opds/category/{}", tag.id),
            &format!("{} books", tag.cnt.unwrap_or(0)),
            "navigation",
        );
    }
    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}
```

### 3.4 Category books feed

```rust
async fn category_books_feed(
    State(state): State<AppState>,
    Path(tag_id): Path<i64>,
    Query(q): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = q.page.unwrap_or(0).max(0);
    let page_size = q.page_size.unwrap_or(50).clamp(1, 200);

    let books = sqlx::query_as!(
        Book,
        "SELECT b.* FROM books b
         JOIN book_tags bt ON bt.book_id = b.id
         WHERE bt.tag_id = ?1
         ORDER BY b.title
         LIMIT ?2 OFFSET ?3",
        tag_id,
        page_size,
        page * page_size
    )
    .fetch_all(&state.db)
    .await?;

    let tag_name = sqlx::query_scalar!("SELECT name FROM tags WHERE id = ?1", tag_id)
        .fetch_optional(&state.db)
        .await?
        .unwrap_or_default();

    build_book_feed(
        &state,
        &books,
        &format!("Category: {tag_name}"),
        &format!("/opds/category/{tag_id}"),
        page,
        page_size,
    )
    .await
}
```

### 3.5 Formats navigation feed

```rust
async fn formats_feed(State(state): State<AppState>) -> Result<Response, AppError> {
    let formats = sqlx::query!(
        "SELECT UPPER(format) AS fmt, COUNT(*) AS cnt
         FROM book_formats
         GROUP BY UPPER(format)
         ORDER BY fmt"
    )
    .fetch_all(&state.db)
    .await?;

    let mut xml = String::new();
    push_feed_header(&mut xml, "Formats", "/opds/formats", "navigation");
    for f in &formats {
        let fmt = f.fmt.as_deref().unwrap_or("UNKNOWN");
        push_navigation_entry(
            &mut xml,
            fmt,
            &format!("/opds/formats/{}", fmt.to_lowercase()),
            &format!("{} books", f.cnt),
            "navigation",
        );
    }
    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}
```

### 3.6 Per-format books feed

```rust
async fn format_books_feed(
    State(state): State<AppState>,
    Path(fmt): Path<String>,
    Query(q): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let page = q.page.unwrap_or(0).max(0);
    let page_size = q.page_size.unwrap_or(50).clamp(1, 200);
    let fmt_upper = fmt.to_uppercase();

    let books = sqlx::query_as!(
        Book,
        "SELECT DISTINCT b.* FROM books b
         JOIN book_formats bf ON bf.book_id = b.id
         WHERE UPPER(bf.format) = ?1
         ORDER BY b.title
         LIMIT ?2 OFFSET ?3",
        fmt_upper,
        page_size,
        page * page_size
    )
    .fetch_all(&state.db)
    .await?;

    build_book_feed(
        &state,
        &books,
        &format!("Format: {}", fmt.to_uppercase()),
        &format!("/opds/formats/{fmt}"),
        page,
        page_size,
    )
    .await
}
```

### 3.7 Shelf index navigation feed

```rust
async fn shelf_index_feed(State(state): State<AppState>) -> Result<Response, AppError> {
    let shelves = sqlx::query!(
        "SELECT id, name FROM shelves WHERE public = 1 ORDER BY name"
    )
    .fetch_all(&state.db)
    .await?;

    let mut xml = String::new();
    push_feed_header(&mut xml, "Shelves", "/opds/shelf", "navigation");
    for shelf in &shelves {
        push_navigation_entry(
            &mut xml,
            &shelf.name,
            &format!("/opds/shelf/{}", shelf.id),
            "",
            "navigation",
        );
    }
    push_feed_footer(&mut xml);
    Ok(xml_response(xml))
}
```

### 3.8 Per-shelf books feed

```rust
async fn shelf_books_feed(
    State(state): State<AppState>,
    Path(shelf_id): Path<String>,
    Query(q): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let shelf = sqlx::query!(
        "SELECT id, name, public FROM shelves WHERE id = ?1",
        shelf_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    // Only expose public shelves via unauthenticated OPDS.
    if shelf.public == 0 {
        return Err(AppError::NotFound);
    }

    let page = q.page.unwrap_or(0).max(0);
    let page_size = q.page_size.unwrap_or(50).clamp(1, 200);

    let books = sqlx::query_as!(
        Book,
        "SELECT b.* FROM books b
         JOIN shelf_books sb ON sb.book_id = b.id
         WHERE sb.shelf_id = ?1
         ORDER BY sb.position, b.title
         LIMIT ?2 OFFSET ?3",
        shelf_id,
        page_size,
        page * page_size
    )
    .fetch_all(&state.db)
    .await?;

    build_book_feed(
        &state,
        &books,
        &shelf.name,
        &format!("/opds/shelf/{shelf_id}"),
        page,
        page_size,
    )
    .await
}
```

### 3.9 Read / unread feeds

Both feeds resolve the user via `?token=<api_token>` query param — same mechanism as download links. Extract the user from the token; 401 if absent.

```rust
#[derive(Deserialize)]
struct TokenQuery {
    token: Option<String>,
}

async fn readbooks_feed(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> Result<Response, AppError> {
    let user_id = resolve_opds_user(&state, &q.token).await?;
    let books = sqlx::query_as!(
        Book,
        "SELECT b.* FROM books b
         JOIN book_user_state bus ON bus.book_id = b.id
         WHERE bus.user_id = ?1 AND bus.is_read = 1
         ORDER BY b.title",
        user_id
    )
    .fetch_all(&state.db)
    .await?;

    build_book_feed(&state, &books, "Read Books", "/opds/readbooks", 0, 200).await
}

async fn unreadbooks_feed(
    State(state): State<AppState>,
    Query(q): Query<TokenQuery>,
) -> Result<Response, AppError> {
    let user_id = resolve_opds_user(&state, &q.token).await?;
    let books = sqlx::query_as!(
        Book,
        "SELECT b.* FROM books b
         LEFT JOIN book_user_state bus
           ON bus.book_id = b.id AND bus.user_id = ?1
         WHERE bus.is_read IS NULL OR bus.is_read = 0
         ORDER BY b.title",
        user_id
    )
    .fetch_all(&state.db)
    .await?;

    build_book_feed(&state, &books, "Unread Books", "/opds/unreadbooks", 0, 200).await
}

/// Resolve `?token=<api_token>` to a user_id. Returns 401 if absent or invalid.
async fn resolve_opds_user(
    state: &AppState,
    token_opt: &Option<String>,
) -> Result<String, AppError> {
    let token = token_opt.as_deref().ok_or(AppError::Unauthorized)?;
    let token_hash = hex::encode(sha2::Sha256::digest(token.as_bytes()));
    let user_id = sqlx::query_scalar!(
        "SELECT user_id FROM api_tokens
         WHERE token_hash = ?1
           AND (expires_at IS NULL OR expires_at > strftime('%s','now'))",
        token_hash
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::Unauthorized)?;
    Ok(user_id)
}
```

### 3.10 Book UUID lookup

```rust
async fn book_uuid_feed(
    State(state): State<AppState>,
    Path(uuid): Path<String>,
) -> Result<Response, AppError> {
    let book = sqlx::query_as!(
        Book,
        "SELECT * FROM books WHERE uuid = ?1",
        uuid
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    build_book_feed(&state, &[book], "Book", &format!("/opds/ajax/book/{uuid}"), 0, 1).await
}
```

---

## 4. Shelf edit — `backend/src/api/shelves.rs`

### 4.1 Router addition

```rust
// In pub fn router(state: AppState) -> Router<AppState>:
.route("/api/v1/shelves/:id", get(get_shelf).patch(patch_shelf).delete(delete_shelf))
```

### 4.2 Request type

```rust
#[derive(Deserialize)]
struct PatchShelfBody {
    name: Option<String>,
    public: Option<bool>,
}
```

### 4.3 Handler

```rust
pub(crate) async fn patch_shelf(
    State(state): State<AppState>,
    Extension(current_user): Extension<AuthUser>,
    Path(shelf_id): Path<String>,
    Json(body): Json<PatchShelfBody>,
) -> Result<Json<Shelf>, AppError> {
    ensure_owner(&state, &current_user.id, &shelf_id).await?;

    if let Some(ref name) = body.name {
        sqlx::query!(
            "UPDATE shelves SET name = ?1 WHERE id = ?2",
            name,
            shelf_id
        )
        .execute(&state.db)
        .await?;
    }

    if let Some(public) = body.public {
        let flag: i64 = if public { 1 } else { 0 };
        sqlx::query!(
            "UPDATE shelves SET public = ?1 WHERE id = ?2",
            flag,
            shelf_id
        )
        .execute(&state.db)
        .await?;
    }

    let shelf = shelf_queries::get_shelf_by_id(&state.db, &shelf_id)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(shelf))
}
```

Add `get_shelf_by_id` to `db/queries/shelves.rs` if it does not already exist.

---

## 5. Quality gates

```bash
cd ~/Documents/localProject/xcalibre-server

cargo test -p backend test_kobo_tags 2>&1 | tail -30
cargo test -p backend test_opds_parity 2>&1 | tail -30
cargo test -p backend test_shelf_edit 2>&1 | tail -30

# All workspace tests must still pass.
cargo test --workspace 2>&1 | tail -20

# Zero clippy warnings.
cargo clippy -- -D warnings 2>&1 | tail -20

# Zero audit CVEs.
cargo audit 2>&1 | tail -10
```

All 18 new tests must pass. No regressions.

---

## 6. Documentation updates

### `docs/API.md`

Add to Shelves table:
```
| PATCH | `/shelves/:id` | Yes | Owner/Admin | Rename shelf or toggle public. Body: `{ "name"?: string, "public"?: bool }` |
```

Add to OPDS table all new routes (category, formats, shelf, readbooks, unreadbooks, ajax/book).

### `GAP.md`

- Mark Kobo tag sync ✅ (was false positive ❌)
- Mark OPDS category, read/unread, shelf, formats feeds ✅
- Mark shelf edit ✅

### `docs/STATE.md`

- Add Phase 30 row to phase completion table
- Bump "Overall Status" to Phase 30 Complete — v2.5.0

### `CLAUDE.md`

- Update status line: "Phases 1–30 complete. Current release: v2.5.0."

---

## 7. Version bump → 2.5.0

```bash
# backend/Cargo.toml
sed -i '' 's/^version = "2.4.0"/version = "2.5.0"/' backend/Cargo.toml

# xs-mcp/Cargo.toml
sed -i '' 's/^version = "2.4.0"/version = "2.5.0"/' xs-mcp/Cargo.toml

# xs-migrate/Cargo.toml
sed -i '' 's/^version = "2.4.0"/version = "2.5.0"/' xs-migrate/Cargo.toml

# package.json (root)
sed -i '' 's/"version": "2.4.0"/"version": "2.5.0"/' package.json

# packages/shared/package.json
sed -i '' 's/"version": "2.4.0"/"version": "2.5.0"/' packages/shared/package.json

cargo check --workspace   # refresh Cargo.lock
```

---

## 8. Commit

```bash
git add \
  backend/src/api/kobo.rs \
  backend/src/api/opds.rs \
  backend/src/api/shelves.rs \
  backend/src/db/queries/shelves.rs \
  backend/tests/common/mod.rs \
  docs/API.md \
  GAP.md \
  docs/STATE.md \
  CLAUDE.md \
  backend/Cargo.toml xs-mcp/Cargo.toml xs-migrate/Cargo.toml \
  package.json packages/shared/package.json Cargo.lock

git commit -m "Phase 30 — OPDS parity II, Kobo tag sync, shelf edit (v2.5.0)"
git tag v2.5.0
git push && git push --tags
```
