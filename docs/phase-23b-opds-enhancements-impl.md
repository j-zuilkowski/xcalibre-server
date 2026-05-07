# Phase 23b — OPDS Enhancements Implementation

## Context
Rust 2021, Axum 0.7.
Phase 23a complete: failing tests in `backend/tests/test_opds_enhancements.rs`.

Goal: implement all OPDS enhancements from Phase 23a with clean `clippy` and `audit`.

---

## 1. Add OPDS cover variants + in-memory LRU cache

### Create: `backend/src/api/opds_cover_cache.rs`

```rust
use std::num::NonZeroUsize;
use std::sync::Arc;

use lru::LruCache;
use tokio::sync::RwLock;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CoverVariant {
    Original,
    Thumb240,
    Large600,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct CoverCacheKey {
    pub book_id: String,
    pub variant: CoverVariant,
    pub webp: bool,
}

#[derive(Clone)]
pub struct OpdsCoverCache {
    inner: Arc<RwLock<LruCache<CoverCacheKey, Vec<u8>>>>,
}

impl OpdsCoverCache {
    pub fn new(max_entries: usize) -> Self {
        let cap = NonZeroUsize::new(max_entries.max(1)).expect("non-zero capacity guaranteed");
        Self {
            inner: Arc::new(RwLock::new(LruCache::new(cap))),
        }
    }

    pub async fn get(&self, key: &CoverCacheKey) -> Option<Vec<u8>> {
        self.inner.write().await.get(key).cloned()
    }

    pub async fn put(&self, key: CoverCacheKey, bytes: Vec<u8>) {
        self.inner.write().await.put(key, bytes);
    }
}
```

### Wire in app state (`backend/src/lib.rs` or state module)

```rust
pub struct AppState {
    // ... existing fields
    pub opds_cover_cache: OpdsCoverCache,
}

// in state construction
opds_cover_cache: OpdsCoverCache::new(200),
```

---

## 2. Extend OPDS handlers in `backend/src/api/opds.rs`

Add routes:
- `GET /opds/cover/:book_id`
- `GET /opds/cover/:book_id/thumb`
- `GET /opds/cover/:book_id/large`
- `GET /opds/osd`
- `GET /opds/search/:query` (path-based variant)
- `GET /opds/new`
- `GET /opds/hot`
- `GET /opds/stats`
- `GET /opds/discover`
- `GET /opds/authors/letter/:ch`
- `GET /opds/series/letter/:ch`

### Cover helpers

```rust
fn wants_webp(headers: &HeaderMap) -> bool {
    headers
        .get(axum::http::header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("image/webp"))
        .unwrap_or(false)
}

fn resize_dimensions(variant: CoverVariant) -> Option<(u32, u32)> {
    match variant {
        CoverVariant::Original => None,
        CoverVariant::Thumb240 => Some((240, 240)),
        CoverVariant::Large600 => Some((600, 600)),
    }
}
```

### Cover endpoint behavior

```rust
// shared handler logic
// 1) fetch cover_path from books
// 2) return 404 when cover_path is NULL or file missing
// 3) read bytes, optionally resize via image crate
// 4) encode webp when Accept contains image/webp; otherwise jpeg
// 5) cache by (book_id, variant, encoding)
// 6) return Content-Disposition: inline
```

Response headers:
- `Content-Type: image/webp` or `image/jpeg`
- `Content-Disposition: inline`

### OSD endpoint

Return static XML (no DB calls):

```xml
<?xml version="1.0" encoding="UTF-8"?>
<OpenSearchDescription xmlns="http://a9.com/-/spec/opensearch/1.1/">
  <ShortName>xcalibre OPDS</ShortName>
  <Description>Search xcalibre OPDS catalog</Description>
  <Url
    type="application/atom+xml;profile=opds-catalog"
    template="{base}/opds/search?q={searchTerms}" />
</OpenSearchDescription>
```

Content-Type:
`application/opensearchdescription+xml`

### `/opds/search/:query`

Normalize path segment and forward to existing query-based search handler.

### `/opds/new`

Single query sorted by `date_added DESC`, `LIMIT 30`, rendered as OPDS acquisition feed.

### `/opds/hot`

Single SQL query, no N+1:

```sql
SELECT b.id, b.title, b.created_at, COUNT(dh.book_id) AS downloads
FROM books b
JOIN download_history dh ON dh.book_id = b.id
GROUP BY b.id
ORDER BY downloads DESC, b.created_at DESC
LIMIT 30;
```

### `/opds/stats`

Single SQL query returning all counters:

```sql
SELECT
  (SELECT COUNT(*) FROM books) AS total_books,
  (SELECT COUNT(DISTINCT ba.author_id)
     FROM book_authors ba) AS total_authors,
  (SELECT COUNT(DISTINCT series) FROM books WHERE series IS NOT NULL AND series != '') AS total_series,
  (SELECT COUNT(DISTINCT bt.tag_id)
     FROM book_tags bt) AS total_tags,
  (SELECT COUNT(DISTINCT format) FROM book_formats) AS total_formats;
```

Respond JSON with the five required keys.

### `/opds/discover`

Load all shelf names and emit OPDS navigation entries.

### Letter feeds

Use NFKD normalization in Rust (`unicode-normalization` already present):

```rust
use unicode_normalization::UnicodeNormalization;

fn normalized_letter(input: &str) -> Option<String> {
    let ch = input.nfkd().next()?;
    Some(ch.to_ascii_uppercase().to_string())
}
```

Then SQL filter by first letter of sortable text:

```sql
WHERE UPPER(SUBSTR(author_sort, 1, 1)) = ?
```

and series equivalent.

---

## 3. Add index for hot feed query

### Migration: `backend/migrations/sqlite/0027_download_history_book_idx.sql`

```sql
CREATE INDEX IF NOT EXISTS idx_download_history_book_id
ON download_history(book_id);
```

### Migration: `backend/migrations/mariadb/0027_download_history_book_idx.sql`

```sql
CREATE INDEX IF NOT EXISTS idx_download_history_book_id
ON download_history(book_id);
```

---

## 4. Router registration and OpenAPI annotations

### `backend/src/api/opds.rs`

Add router paths for all new endpoints.

### `backend/src/api/openapi.rs` (or existing utoipa module)

Add `#[utoipa::path]` for:
- `opds_cover`
- `opds_cover_thumb`
- `opds_cover_large`
- `opds_osd`
- `opds_search_path`
- `opds_new`
- `opds_hot`
- `opds_stats`
- `opds_discover`
- `opds_authors_letter`
- `opds_series_letter`

Ensure these appear in `/api/docs` output.

---

## 5. Docs update

### `docs/API.md`

Update OPDS section with new endpoints, auth requirements, and response formats:
- image endpoints and content-negotiation behavior
- OSD descriptor endpoint
- path search compatibility
- new/hot/discover feeds
- stats JSON shape
- letter browsing paths

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_opds_enhancements 2>&1 | tail -60
cargo clippy -- -D warnings 2>&1 | tail -40
cargo audit 2>&1 | tail -40
```
Expected: **all `test_opds_enhancements` tests pass**, zero clippy warnings, zero audit vulnerabilities.

## Commit
```bash
git add backend/src/api/opds.rs \
        backend/src/api/opds_cover_cache.rs \
        backend/src/api/openapi.rs \
        backend/src/lib.rs \
        backend/migrations/sqlite/0027_download_history_book_idx.sql \
        backend/migrations/mariadb/0027_download_history_book_idx.sql \
        docs/API.md
git commit -m "Phase 23b — OPDS enhancements implementation"
```
