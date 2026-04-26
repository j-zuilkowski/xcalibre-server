//! API token CRUD and lookup queries.
//! Touches: `api_tokens`.
//!
//! Token values are never stored in plaintext; `find_by_hash` looks up by the
//! hex-encoded SHA-256 of the bearer string.  The caller (middleware) is
//! responsible for verifying `expires_at` and scope after retrieval.
//!
//! `touch_last_used` updates `last_used_at` on every authenticated request.
//! This is best-effort: failures are suppressed by the middleware caller so a
//! broken write does not interrupt the request.

use anyhow::Context;
use chrono::Utc;
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::auth::TokenScope;

#[derive(Clone, Debug, Serialize)]
pub struct ApiToken {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub created_at: String,
    pub last_used_at: Option<String>,
    pub expires_at: Option<i64>,
    pub scope: TokenScope,
}

pub async fn create_token(
    db: &SqlitePool,
    name: &str,
    token_hash: &str,
    created_by: &str,
    expires_at: Option<i64>,
    scope: TokenScope,
) -> anyhow::Result<ApiToken> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO api_tokens (id, name, token_hash, created_by, created_at, last_used_at, expires_at, scope)
        VALUES (?, ?, ?, ?, ?, NULL, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(name)
    .bind(token_hash)
    .bind(created_by)
    .bind(&now)
    .bind(expires_at)
    .bind(scope.as_str())
    .execute(db)
    .await?;

    find_by_id(db, &id)
        .await?
        .context("created api token not found")
}

/// Looks up an API token by its SHA-256 hash.  The caller must check
/// `expires_at` and token scope before granting access.
pub async fn find_by_hash(db: &SqlitePool, token_hash: &str) -> anyhow::Result<Option<ApiToken>> {
    let row = sqlx::query(
        r#"
        SELECT id, name, created_by, created_at, last_used_at, expires_at, scope
        FROM api_tokens
        WHERE token_hash = ?
        "#,
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await?;

    row_to_api_token(row).context("parse api token by hash")
}

pub async fn touch_last_used(db: &SqlitePool, id: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE api_tokens
        SET last_used_at = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn list_tokens(db: &SqlitePool, created_by: &str) -> anyhow::Result<Vec<ApiToken>> {
    let rows = sqlx::query(
        r#"
        SELECT id, name, created_by, created_at, last_used_at, expires_at, scope
        FROM api_tokens
        WHERE created_by = ?
        ORDER BY created_at DESC, id DESC
        "#,
    )
    .bind(created_by)
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(|row| row_to_api_token(Some(row)).map(|token| token.expect("row was provided")))
        .collect::<anyhow::Result<Vec<_>>>()
        .context("parse api token list")
}

pub async fn delete_token(db: &SqlitePool, id: &str, created_by: &str) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM api_tokens
        WHERE id = ? AND created_by = ?
        "#,
    )
    .bind(id)
    .bind(created_by)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

async fn find_by_id(db: &SqlitePool, id: &str) -> anyhow::Result<Option<ApiToken>> {
    let row = sqlx::query(
        r#"
        SELECT id, name, created_by, created_at, last_used_at, expires_at, scope
        FROM api_tokens
        WHERE id = ?
        "#,
    )
    .bind(id)
    .fetch_optional(db)
    .await?;

    row_to_api_token(row).context("parse api token by id")
}

fn row_to_api_token(row: Option<sqlx::sqlite::SqliteRow>) -> anyhow::Result<Option<ApiToken>> {
    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(ApiToken {
        id: row.get("id"),
        name: row.get("name"),
        created_by: row.get("created_by"),
        created_at: row.get("created_at"),
        last_used_at: row.get("last_used_at"),
        expires_at: row.get("expires_at"),
        scope: parse_scope(&row.get::<String, _>("scope"))?,
    }))
}

fn parse_scope(scope: &str) -> anyhow::Result<TokenScope> {
    match scope {
        "read" => Ok(TokenScope::Read),
        "write" => Ok(TokenScope::Write),
        "admin" => Ok(TokenScope::Admin),
        other => anyhow::bail!("invalid api token scope: {other}"),
    }
}
