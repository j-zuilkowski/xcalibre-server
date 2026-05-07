# Phase 26b — OAuth Account Linking Implementation

## Context
Rust 2021, Axum 0.7.
Phase 26a complete: failing tests in `backend/tests/test_oauth_linking.rs`.

Goal: add post-login OAuth provider linking and unlinking with conflict checks and lockout guard.

---

## 1. Schema check and migration (only if missing)

Check whether `oauth_accounts` already exists with required columns:
- `user_id`
- `provider` (`github` or `google`)
- `provider_account_id`
- `linked_at`

If missing, add migrations:
- `backend/migrations/sqlite/0029_oauth_accounts.sql`
- `backend/migrations/mariadb/0029_oauth_accounts.sql`

Schema shape:

```sql
CREATE TABLE oauth_accounts (
  id TEXT PRIMARY KEY,
  user_id TEXT NOT NULL,
  provider TEXT NOT NULL,
  provider_account_id TEXT NOT NULL,
  linked_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
  UNIQUE(provider, provider_account_id),
  UNIQUE(user_id, provider),
  FOREIGN KEY(user_id) REFERENCES users(id) ON DELETE CASCADE
);
```

---

## 2. New API endpoints

### `GET /api/v1/me/oauth/providers`

Authenticated route returning:

```json
{ "linked": ["github"], "available": ["google"] }
```

Implementation:
- Read linked providers for current user.
- `available = configured providers - linked`.

### `GET /auth/oauth/:provider/link`

- Requires bearer token or authenticated session cookie.
- If provider already linked to current user, return `400`.
- Build signed state: `user_id + nonce + timestamp` with existing HMAC helper.
- Redirect to provider authorize URL.

### `GET /auth/oauth/:provider/link/callback`

- Validate state signature and timestamp.
- Exchange code for access token using existing OAuth client path.
- Fetch provider account id from provider profile.
- If provider account already linked to a different user: `409`.
- Else insert link row for current user.
- Return `200 { "linked": true }`.

### `DELETE /api/v1/me/oauth/:provider`

- Remove provider link for current user.
- Lockout guard before delete:
  - If `users.password_hash` is null and this is the only linked provider, return `400`.
- Success returns `200`.

---

## 3. Reuse existing OAuth client configuration

Do not create a parallel OAuth stack. Reuse existing provider config and token/userinfo exchange helpers used by login flow.

Only behavioral split:
- login flow: can sign in / create account
- link flow: must attach provider account to already-authenticated user from signed state

---

## 4. Security details

- State payload for link flow must be HMAC signed.
- Include nonce and timestamp to prevent replay.
- Apply expiration window equivalent to existing OAuth state lifetime.
- Use constant-time signature comparison from existing helper utilities.

---

## 5. Docs update

Update `docs/API.md` Auth section with:
- provider list route
- link start/callback routes
- unlink route
- conflict and lockout errors

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_oauth_linking 2>&1 | tail -60
cargo clippy -- -D warnings 2>&1 | tail -40
cargo audit 2>&1 | tail -40
```
Expected: **all `test_oauth_linking` tests pass**, zero clippy warnings, zero audit vulnerabilities.

## Commit
```bash
git add backend/src/api/auth.rs \
        backend/src/db/queries/auth.rs \
        backend/src/db/queries/oauth.rs \
        backend/src/api/mod.rs \
        docs/API.md
# include migration files only if table/columns are missing in current schema
git commit -m "Phase 26b — OAuth account linking implementation"
```
