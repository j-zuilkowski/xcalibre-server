//! OAuth account link queries.
//! Touches: `oauth_accounts`.
//!
//! Lookup is keyed on `(provider, provider_user_id)` — the stable identity
//! issued by the OAuth provider — never on email address.  Auto-linking by
//! email is intentionally absent to prevent account takeover when a user's
//! email is spoofed by a different provider.
//!
//! `create_oauth_account` is idempotent: if a record already exists for
//! `(provider, provider_user_id)` it updates `user_id` and `email` (handles
//! email changes and re-linking) rather than inserting a duplicate.

use crate::db::models::OauthAccount;
use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

/// Looks up an OAuth account by `(provider, provider_user_id)`.
/// Returns `None` if no account has been linked for this provider identity.
pub async fn find_by_provider(
    db: &SqlitePool,
    provider: &str,
    provider_user_id: &str,
) -> anyhow::Result<Option<OauthAccount>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, provider, provider_user_id, email, created_at
        FROM oauth_accounts
        WHERE provider = ? AND provider_user_id = ?
        "#,
    )
    .bind(provider)
    .bind(provider_user_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(row_to_oauth_account))
}

/// Returns all OAuth accounts linked to the given user.
pub async fn find_by_user_id(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<OauthAccount>> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, provider, provider_user_id, email, created_at
        FROM oauth_accounts
        WHERE user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    Ok(rows.into_iter().map(row_to_oauth_account).collect())
}

/// Deletes the OAuth account link for a given user and provider.
/// Returns the number of rows affected (0 if no matching link existed).
pub async fn delete_oauth_account(
    db: &SqlitePool,
    user_id: &str,
    provider: &str,
) -> anyhow::Result<u64> {
    let result = sqlx::query(
        r#"
        DELETE FROM oauth_accounts
        WHERE user_id = ? AND provider = ?
        "#,
    )
    .bind(user_id)
    .bind(provider)
    .execute(db)
    .await?;

    Ok(result.rows_affected())
}

/// Counts the number of OAuth accounts linked to the given user.
pub async fn count_oauth_accounts_by_user(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<i64> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(*) AS count
        FROM oauth_accounts
        WHERE user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    Ok(row.get("count"))
}

pub async fn create_oauth_account(
    db: &SqlitePool,
    user_id: &str,
    provider: &str,
    provider_user_id: &str,
    email: &str,
) -> anyhow::Result<OauthAccount> {
    let now = Utc::now().to_rfc3339();
    let mut tx = db.begin().await?;

    let existing = sqlx::query(
        r#"
        SELECT id, user_id, provider, provider_user_id, email, created_at
        FROM oauth_accounts
        WHERE provider = ? AND provider_user_id = ?
        "#,
    )
    .bind(provider)
    .bind(provider_user_id)
    .fetch_optional(&mut *tx)
    .await?;

    if let Some(row) = existing {
        let id: String = row.get("id");
        let created_at: String = row.get("created_at");
        sqlx::query(
            r#"
            UPDATE oauth_accounts
            SET user_id = ?, email = ?
            WHERE id = ?
            "#,
        )
        .bind(user_id)
        .bind(email)
        .bind(&id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(OauthAccount {
            id,
            user_id: user_id.to_string(),
            provider: provider.to_string(),
            provider_user_id: provider_user_id.to_string(),
            email: email.to_string(),
            created_at,
        });
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO oauth_accounts (id, user_id, provider, provider_user_id, email, created_at)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(provider)
    .bind(provider_user_id)
    .bind(email)
    .bind(&now)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    Ok(OauthAccount {
        id,
        user_id: user_id.to_string(),
        provider: provider.to_string(),
        provider_user_id: provider_user_id.to_string(),
        email: email.to_string(),
        created_at: now,
    })
}

fn row_to_oauth_account(row: sqlx::sqlite::SqliteRow) -> OauthAccount {
    OauthAccount {
        id: row.get("id"),
        user_id: row.get("user_id"),
        provider: row.get("provider"),
        provider_user_id: row.get("provider_user_id"),
        email: row.get("email"),
        created_at: row.get("created_at"),
    }
}
