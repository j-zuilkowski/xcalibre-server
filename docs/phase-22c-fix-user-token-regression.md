# Phase 22c — Fix: test_toggle_read_false regression

## Context
Rust, Axum, SQLx, Tokio. Zero warnings, zero errors. cargo test must be green.
Working dir: ~/Documents/localProject/xcalibre-server
Phase 22b complete: self-update implemented, all tests green at commit time.

## Root cause

`test_toggle_read_false` (backend/tests/test_book_user_state.rs:42) queries:

```sql
SELECT id FROM users WHERE username = 'user'
```

`TestContext::user_token()` (backend/tests/common/mod.rs:121) was changed to call
`create_user_with_email("user2@example.com")`, which derives username `"user2"` from
the email prefix — not `"user"`. The query returns `RowNotFound`.

`create_user()` (line 83) creates `username = "user"` and is the correct helper here.
The switch to `create_user_with_email` was unnecessary: each `TestContext::new()` uses a
fresh in-memory SQLite DB, so there is no cross-test username collision.

## Fix: backend/tests/common/mod.rs line 121

Change:
```rust
    pub async fn user_token(&self) -> String {
        let (user, password) = self.create_user_with_email("user2@example.com").await;
```

To:
```rust
    pub async fn user_token(&self) -> String {
        let (user, password) = self.create_user().await;
```

## Verify

```bash
cargo test 2>&1 | tail -5
```
Expected: `test result: ok. N passed; 0 failed`

## Commit

```bash
git add backend/tests/common/mod.rs docs/phase-22c-fix-user-token-regression.md
git commit -m "Phase 22c — fix test_toggle_read_false: user_token() must use create_user()"
```
