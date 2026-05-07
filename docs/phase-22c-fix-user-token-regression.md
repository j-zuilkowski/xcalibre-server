# Phase 22c — Fix `test_toggle_read_false` Regression

**Date:** 2026-05-07  
**Status:** Complete  
**Release:** v2.3.0 (no version bump — bug fix, no API change, no migration)

## Symptom

`test_toggle_read_false` panicked with:

```
load user id: RowNotFound
```

at `backend/tests/test_book_user_state.rs:42`.

## Root Cause

`user_token()` in `backend/tests/common/mod.rs:120` called
`create_user_with_email("user2@example.com")`, which created a user with
username `"user2"` (derived from the email prefix). The test then queried
for `WHERE username = ?` with bind value `"user"` — no match, hence
`RowNotFound`.

## Hidden Secondary Issue

Several other tests had the same hardcoded `WHERE username = "user"` pattern
but happened to work because they only created one user via `user_token()`
(the old `"user2"` username still existed, just not as `"user"`). Additionally,
`test_magic_link_revoke_requires_admin` called both `create_user()` (creates
`"user"/"user@example.com"`) and `user_token()`, which would have hit a
`UNIQUE constraint` on either `email` or `username` depending on the approach.

## Fix

A two-part fix was needed, not just the one-liner originally estimated:

### Part 1 — `user_token()` generates unique credentials

`backend/tests/common/mod.rs`: replaced `create_user_with_email("user2@example.com")`
with a direct `insert_user()` call using a random UUID-suffixed email, while
keeping `username = "user"` so that tests querying by that username still work:

```rust
pub async fn user_token(&self) -> String {
    self.seed_role("user").await;
    let password = "Test1234!".to_string();
    let unique = Uuid::new_v4().to_string().replace('-', "")[..12].to_string();
    let user = self
        .insert_user("user", &format!("user-{unique}@example.com"), "user", &password)
        .await;
    self.login(&user.username, &password).await.access_token
}
```

This ensures:
- **Username `"user"`** — so tests querying by username `"user"` find it.
- **Unique email** — so no conflict with `create_user()` (which uses `"user@example.com"`).

### Part 2 — Hardcoded `WHERE username = 'user'` queries updated

The first fix broke `test_magic_link_revoke_requires_admin` because that test
calls both `create_user()` (username `"user"`) and `user_token()` (also
username `"user"`), hitting a `UNIQUE constraint failure: users.username`.

Fix: Update all queries that hardcoded `username = "user"` to instead find
the non-admin user dynamically using `WHERE username != 'admin' LIMIT 1`:

| File | Change |
|---|---|
| `test_book_user_state.rs` | SELECT lookup `"user"` → `WHERE username != 'admin' LIMIT 1` |
| `test_download_history.rs` | SELECT lookup `"user"` → `ORDER BY created_at DESC LIMIT 1` |
| `test_tag_restrictions.rs` | 4× SELECT lookup `"user"` → `WHERE username != 'admin' LIMIT 1` |
| `test_goodreads_import.rs` | 3× SELECT lookup `'user'` → `WHERE username != 'admin' LIMIT 1` |
| `test_file_serving.rs` | 3× UPDATE `WHERE username = 'user'` → `WHERE id = (SELECT id FROM users WHERE username != 'admin' LIMIT 1)` |
| `test_mobi_reader.rs` | 1× UPDATE `WHERE username = 'user'` → `WHERE id = (SELECT id FROM users WHERE username != 'admin' LIMIT 1)` |

### Why `WHERE username != 'admin' LIMIT 1`?

This works because every affected test creates either:
1. **Only one user** (via `user_token()`) — no admin user exists, so the
   query returns the sole user.
2. **Admin + user** (via `admin_token()` + `user_token()`) — the query
   excludes the admin and returns the non-admin user.

No test in the suite creates multiple non-admin users alongside a call to
`user_token()`, so the pattern is safe.

### Why not `ORDER BY created_at DESC LIMIT 1`?

This was used for `test_download_history.rs` where only one user exists
(via `user_token()`). It wasn't used universally because the order of
`user_token()` vs `admin_token()` calls varies across tests, and
`ORDER BY DESC LIMIT 1` would return the wrong user in some cases.

### Files modified

```
backend/tests/common/mod.rs          # user_token() uses unique email
backend/tests/test_book_user_state.rs # user lookup no longer hardcoded
backend/tests/test_download_history.rs
backend/tests/test_tag_restrictions.rs
backend/tests/test_goodreads_import.rs
backend/tests/test_file_serving.rs
backend/tests/test_mobi_reader.rs
```

## Verification

- `cargo test` — **all tests pass** (integration test suite across all modules).
- `cargo clippy -- -D warnings` — zero warnings.
