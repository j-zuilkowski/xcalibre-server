# Phase 19b — Magic Link Login Implementation

## Context
Rust 2021, Axum 0.7, sqlx 0.7, SQLite.
Phase 19a complete: failing tests in `backend/tests/test_magic_link.rs`.
`lettre` already in `Cargo.toml` (used by Send-to-Kindle). No new crate dependencies.

---

## 1. Migration — `backend/migrations/sqlite/0029_magic_link_tokens.sql`

```sql
CREATE TABLE IF NOT EXISTS magic_link_tokens (
    id          TEXT    PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id     TEXT    NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT    NOT NULL UNIQUE,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch()),
    expires_at  INTEGER NOT NULL,
    used_at     INTEGER
);

CREATE INDEX IF NOT EXISTS idx_magic_link_tokens_user_id   ON magic_link_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_magic_link_tokens_expires_at ON magic_link_tokens(expires_at);
```

Also create `backend/migrations/mariadb/0029_magic_link_tokens.sql`:

```sql
CREATE TABLE IF NOT EXISTS magic_link_tokens (
    id          VARCHAR(32)  NOT NULL PRIMARY KEY DEFAULT (LOWER(HEX(RANDOM_BYTES(16)))),
    user_id     VARCHAR(36)  NOT NULL,
    token_hash  VARCHAR(64)  NOT NULL UNIQUE,
    created_at  BIGINT       NOT NULL DEFAULT (UNIX_TIMESTAMP()),
    expires_at  BIGINT       NOT NULL,
    used_at     BIGINT,
    CONSTRAINT fk_mlt_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
);

CREATE INDEX idx_magic_link_tokens_user_id    ON magic_link_tokens(user_id);
CREATE INDEX idx_magic_link_tokens_expires_at ON magic_link_tokens(expires_at);
```

---

## 2. Config — add `MagicLinkSection` to `backend/src/config.rs`

Add after `ProxyAuthSection`:

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct MagicLinkSection {
    pub enabled: bool,
    pub token_ttl_minutes: u32,
}

impl Default for MagicLinkSection {
    fn default() -> Self {
        Self {
            enabled: false,
            token_ttl_minutes: 15,
        }
    }
}
```

Add `pub magic_link: MagicLinkSection` to `AuthSection`:

```rust
pub struct AuthSection {
    // ... existing fields ...
    pub magic_link: MagicLinkSection,
}
```

---

## 3. Token helper — `backend/src/auth/magic_link.rs`

```rust
use rand::Rng;
use sha2::{Digest, Sha256};

/// Generate a cryptographically random URL-safe token (32 bytes = 64 hex chars).
pub fn generate_token() -> String {
    let bytes: [u8; 32] = rand::thread_rng().gen();
    hex::encode(bytes)
}

/// SHA-256 hex digest of the raw token — stored in DB, never the raw value.
pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.as_bytes());
    hex::encode(hasher.finalize())
}
```

Add `pub mod magic_link;` to `backend/src/auth/mod.rs`.

---

## 4. DB queries — `backend/src/db/queries/magic_link.rs`

```rust
use crate::error::AppError;
use chrono::Utc;
use sqlx::SqlitePool;

pub struct MagicLinkToken {
    pub id: String,
    pub user_id: String,
    pub expires_at: i64,
    pub used_at: Option<i64>,
}

/// Insert a new magic-link token row. Returns the generated row id.
pub async fn insert_token(
    db: &SqlitePool,
    user_id: &str,
    token_hash: &str,
    expires_at: i64,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO magic_link_tokens (user_id, token_hash, expires_at) VALUES (?, ?, ?)",
    )
    .bind(user_id)
    .bind(token_hash)
    .bind(expires_at)
    .execute(db)
    .await?;
    Ok(())
}

/// Look up a token by its hash. Returns None if not found.
pub async fn find_token(
    db: &SqlitePool,
    token_hash: &str,
) -> Result<Option<MagicLinkToken>, AppError> {
    let row = sqlx::query_as!(
        MagicLinkToken,
        "SELECT id, user_id, expires_at, used_at \
         FROM magic_link_tokens WHERE token_hash = ?",
        token_hash
    )
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// Mark a token as used (sets used_at = now). Must be called inside a transaction
/// that also checks used_at IS NULL to prevent TOCTOU races.
pub async fn mark_token_used(db: &SqlitePool, token_id: &str) -> Result<(), AppError> {
    let now = Utc::now().timestamp();
    sqlx::query("UPDATE magic_link_tokens SET used_at = ? WHERE id = ?")
        .bind(now)
        .bind(token_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Delete all unused tokens for a user (admin revoke).
pub async fn revoke_user_tokens(db: &SqlitePool, user_id: &str) -> Result<u64, AppError> {
    let result =
        sqlx::query("DELETE FROM magic_link_tokens WHERE user_id = ? AND used_at IS NULL")
            .bind(user_id)
            .execute(db)
            .await?;
    Ok(result.rows_affected())
}
```

Add `pub mod magic_link;` to `backend/src/db/queries/mod.rs`.

---

## 5. Route handlers — `backend/src/api/magic_link.rs`

```rust
use crate::{
    auth::{
        jwt::issue_tokens,
        magic_link::{generate_token, hash_token},
    },
    config::AppConfig,
    db::queries::{auth as auth_queries, magic_link as ml_queries},
    error::AppError,
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

fn feature_enabled(cfg: &AppConfig) -> Result<(), AppError> {
    if cfg.auth.magic_link.enabled {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

// ── POST /api/v1/auth/magic-link/request ─────────────────────────────────────

#[derive(Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
}

pub async fn request_magic_link(
    State(state): State<AppState>,
    Json(body): Json<MagicLinkRequest>,
) -> Result<StatusCode, AppError> {
    feature_enabled(&state.config)?;

    // Silently succeed for unknown emails — no user enumeration
    if let Ok(Some(user)) = auth_queries::find_user_by_email(&state.db, &body.email).await {
        let raw = generate_token();
        let hash = hash_token(&raw);
        let ttl = state.config.auth.magic_link.token_ttl_minutes as i64;
        let expires_at = (Utc::now() + chrono::Duration::minutes(ttl)).timestamp();

        ml_queries::insert_token(&state.db, &user.id, &hash, expires_at).await?;

        // Send email — use existing email infrastructure; silently ignore send errors
        let link = format!(
            "{}/auth/magic-link/verify?token={}",
            state.config.app.base_url.trim_end_matches('/'),
            raw
        );
        let _ = crate::email::send_magic_link(&state, &body.email, &link).await;
    }

    Ok(StatusCode::ACCEPTED)
}

// ── GET /api/v1/auth/magic-link/verify?token=<raw> ───────────────────────────

#[derive(Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
}

pub async fn verify_magic_link(
    State(state): State<AppState>,
    Query(q): Query<VerifyQuery>,
) -> Result<Json<TokenResponse>, AppError> {
    feature_enabled(&state.config)?;

    let hash = hash_token(&q.token);
    let row = ml_queries::find_token(&state.db, &hash)
        .await?
        .ok_or(AppError::Unauthorized)?;

    // Reject if already used
    if row.used_at.is_some() {
        return Err(AppError::Unauthorized);
    }

    // Reject if expired
    if Utc::now().timestamp() > row.expires_at {
        return Err(AppError::Unauthorized);
    }

    // Mark used atomically — if another request races, the UPDATE affects 0 rows
    let updated = sqlx::query(
        "UPDATE magic_link_tokens SET used_at = ? WHERE id = ? AND used_at IS NULL",
    )
    .bind(Utc::now().timestamp())
    .bind(&row.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::Unauthorized);
    }

    let user = auth_queries::find_user_by_id(&state.db, &row.user_id)
        .await?
        .ok_or(AppError::Unauthorized)?;

    let (access_token, refresh_token) = issue_tokens(&state.config, &user)?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
    }))
}

// ── DELETE /api/v1/admin/magic-link/revoke/:user_id ──────────────────────────

pub async fn revoke_user_magic_links(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, AppError> {
    feature_enabled(&state.config)?;
    ml_queries::revoke_user_tokens(&state.db, &user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
```

---

## 6. Email helper — add to `backend/src/email.rs`

Add alongside the existing `send_book` function:

```rust
pub async fn send_magic_link(state: &AppState, to: &str, link: &str) -> Result<()> {
    // Re-use the existing `lettre` SMTP transport already set up for Send-to-Kindle.
    // If SMTP is not configured, return Ok silently — the admin sees the link in logs.
    let Some(mailer) = state.mailer.as_ref() else {
        tracing::debug!(to, link, "magic link (no SMTP configured — log only)");
        return Ok(());
    };
    let email = Message::builder()
        .to(to.parse()?)
        .from(state.config.email.from.parse()?)
        .subject("Your sign-in link")
        .body(format!("Click to sign in (valid 15 minutes):\n\n{link}"))?;
    mailer.send(&email)?;
    Ok(())
}
```

---

## 7. Router wiring — `backend/src/api/mod.rs`

```rust
// In the public auth router (no auth middleware):
.route("/auth/magic-link/request", post(magic_link::request_magic_link))
.route("/auth/magic-link/verify",  get(magic_link::verify_magic_link))

// In the admin router (require_admin middleware):
.route("/admin/magic-link/revoke/:user_id", delete(magic_link::revoke_user_magic_links))
```

---

## 8. Update `FEATURE_COMPARISON.md` and `docs/ARCHITECTURE.md`

In `~/Documents/localProject/FEATURE_COMPARISON.md`, change:

```
| Magic link (e-reader login) | ✗ | ✓ | ✗ | ✗ |
```
to:
```
| Magic link (e-reader login) | ✗ | ✓ | ✗ | ✓ configurable |
```

In `docs/ARCHITECTURE.md`, mark Phase 19 complete and update the gaps note.

---

## Verify
```bash
cd ~/Documents/localProject/xcalibre-server
cargo test --test test_magic_link 2>&1 | tail -20
cargo clippy -- -D warnings 2>&1 | grep "^error" | head -10
```
Expected: **all tests pass**, zero clippy errors.

## Commit
```bash
git add backend/src/auth/magic_link.rs \
        backend/src/api/magic_link.rs \
        backend/src/db/queries/magic_link.rs \
        backend/migrations/sqlite/0029_magic_link_tokens.sql \
        backend/migrations/mariadb/0029_magic_link_tokens.sql \
        backend/src/config.rs \
        backend/src/auth/mod.rs \
        backend/src/db/queries/mod.rs \
        backend/src/api/mod.rs \
        backend/src/email.rs
git commit -m "Phase 19b — magic link login (configurable enable/disable)"
```
