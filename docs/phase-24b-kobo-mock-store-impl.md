# Phase 24b — Kobo Mock Store Endpoints Implementation

## Context
Rust 2021, Axum 0.7.
Phase 24a complete: failing tests in `backend/tests/test_kobo_mock_store.rs`.

Goal: implement the calibre-web-compatible Kobo mock store routes with safe default responses and stable firmware handshake behavior.

---

## 1. Implement mock store handlers in `backend/src/api/kobo.rs`

Add handlers under `/kobo/:token/v1/`:
- `GET  products/books/prices`
- `GET  products/books/recommendations`
- `GET  products/dailydeal`
- `POST analytics/gettests`
- `GET  deals`
- `POST affiliate`
- `GET  user/loyalty/benefits`
- `GET  user/recommendations`
- `GET  user/wishlist`
- `POST user/wishlist/items`
- `DELETE user/wishlist/items/:item_id`
- `GET  user/profile`
- `GET  products/books/:product_id`

Each endpoint returns minimal valid JSON with Kobo-like shape, for example:

```json
{
  "Result": "Success",
  "Data": []
}
```

Use more specific shapes where firmware expects named fields (derive from calibre-web `kobo.py` response structures) while keeping responses static and DB-free.

---

## 2. Implement Kobo image route

Add:
`GET /kobo/:token/v1/images/:book_uuid/:width/:height/:quality/:greyscale/image.jpg`

Behavior:
1. Parse `book_uuid`.
2. Resolve to book using `books.identifier` first; if schema has `kobo_book_uuid`, check that as fallback.
3. If cover exists, read/resize to requested width and height, encode JPEG, return 200.
4. If cover missing or UUID unknown, return 1x1 white JPEG bytes with `Content-Type: image/jpeg`.

Implementation notes:
- Accept `width`/`height` as bounded integers.
- Ignore `quality` and `greyscale` for now unless already supported by existing image utilities.
- Keep behavior deterministic and no panic on malformed UUID.

---

## 3. Router wiring

In `backend/src/api/kobo.rs` or module router builder:
- Add a sub-router for `/kobo/:token/v1/images/*`.
- Add a sub-router for `/kobo/:token/v1/*` store endpoints.
- Keep existing Kobo sync endpoints unchanged.

No new migrations required.

---

## 4. API docs

Update `docs/API.md` Kobo section:
- Document mock store endpoint set and intent (firmware compatibility).
- Document image route and fallback JPEG behavior.

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_kobo_mock_store 2>&1 | tail -60
cargo clippy -- -D warnings 2>&1 | tail -40
cargo audit 2>&1 | tail -40
```
Expected: **all `test_kobo_mock_store` tests pass**, zero clippy warnings, zero audit vulnerabilities.

## Commit
```bash
git add backend/src/api/kobo.rs \
        docs/API.md
git commit -m "Phase 24b — Kobo mock store endpoint implementation"
```
