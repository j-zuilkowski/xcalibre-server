# Phase 27b — Book Merge Implementation

## Context
Rust 2021, Axum 0.7.
Phase 27a complete: failing tests in `backend/tests/test_book_merge.rs`.

Goal: implement admin merge preview + merge execution with strict transactional behavior and format conflict controls.

---

## 1. Add admin endpoints

In `backend/src/api/books_admin.rs` (or existing admin books module), add:
- `POST /api/v1/admin/books/merge/preview`
- `POST /api/v1/admin/books/merge`

Both routes require admin guard.

Request model:

```json
{
  "source_id": "...",
  "target_id": "...",
  "reading_progress_strategy": "keep_target",
  "force": false
}
```

`reading_progress_strategy` allowed values:
- `keep_target`
- `keep_source`
- `merge_max`

---

## 2. Preview endpoint behavior

- `400` if `source_id == target_id`.
- `404` if either book does not exist.
- Detect format overlaps and non-overlaps.
- Count source annotations.
- List source shelf names where relink is needed.
- Return preview JSON:

```json
{
  "formats_to_move": ["epub", "mobi"],
  "formats_conflict": ["epub"],
  "annotations_to_move": 3,
  "shelves_to_relink": ["Favourites"],
  "reading_progress_strategy": "keep_target"
}
```

---

## 3. Merge endpoint behavior in one transaction

Use one `sqlx::Transaction` for all DB writes.

Sequence:
1. Validate source/target and strategy.
2. Detect format conflicts:

```sql
SELECT format
FROM book_formats
WHERE book_id IN (?, ?)
GROUP BY format
HAVING COUNT(*) > 1;
```

3. If conflicts exist and `force != true`, return `409`.
4. Move `book_formats` rows from source to target:
   - non-conflicting formats: reassign `book_id`.
   - conflicting formats with `force=true`: overwrite target ownership according to policy (target replaced by source).
5. Move annotations:

```sql
UPDATE annotations SET book_id = ? WHERE book_id = ?;
```

6. Relink shelves:

```sql
UPDATE OR IGNORE shelf_books
SET book_id = ?
WHERE book_id = ?;
```

(Use backend-equivalent conflict-safe update for MariaDB.)

7. Merge reading progress:
- `keep_target`: no change.
- `keep_source`: move source rows onto target.
- `merge_max`: target percent = max(source, target).

8. Delete source book row.
9. Commit transaction.

Return:

```json
{ "merged": true, "target_id": "..." }
```

If any step fails, rollback and return `500` with error detail payload.

---

## 4. File moves for format assets

When moving formats between books, move file paths on disk using `std::fs::rename` before transaction commit.
If any rename fails:
- abort merge
- rollback DB transaction
- return `500`

Keep path traversal defenses from existing file-serving helpers.

---

## 5. Docs update

Update `docs/API.md` Admin section:
- preview endpoint schema
- merge endpoint schema
- `force` conflict behavior
- strategies for reading progress

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_book_merge 2>&1 | tail -80
cargo clippy -- -D warnings 2>&1 | tail -40
cargo audit 2>&1 | tail -40
```
Expected: **all `test_book_merge` tests pass**, zero clippy warnings, zero audit vulnerabilities.

## Commit
```bash
git add backend/src/api/books_admin.rs \
        backend/src/db/queries/books.rs \
        backend/src/db/queries/annotations.rs \
        backend/src/db/queries/shelves.rs \
        backend/src/api/mod.rs \
        docs/API.md
git commit -m "Phase 27b — book merge implementation"
```

## Final Step
`Stop now. Do not run any more commands.`
