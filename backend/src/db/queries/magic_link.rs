use crate::error::AppError;
use chrono::Utc;
use sqlx::{Row, SqlitePool};

pub struct MagicLinkToken {
    pub id: String,
    pub user_id: String,
    pub expires_at: i64,
    pub used_at: Option<i64>,
}

/// Insert a new magic-link token row.
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
    let row = sqlx::query(
        "SELECT id, user_id, expires_at, used_at \
         FROM magic_link_tokens WHERE token_hash = ?",
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await?;

    let token = row.map(|r| MagicLinkToken {
        id: r.get("id"),
        user_id: r.get("user_id"),
        expires_at: r.get("expires_at"),
        used_at: r.get("used_at"),
    });
    Ok(token)
}

/// Mark a token as used (sets used_at = now).
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
