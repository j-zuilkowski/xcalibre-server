use crate::db::models::{RoleRef, User};
use anyhow::Context;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct UserAuthRecord {
    pub user: User,
    pub password_hash: String,
    pub login_attempts: i64,
    pub locked_until: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug)]
pub struct RefreshTokenRecord {
    pub id: String,
    pub user_id: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

pub async fn count_users(db: &SqlitePool) -> anyhow::Result<i64> {
    let row = sqlx::query("SELECT COUNT(1) AS count FROM users")
        .fetch_one(db)
        .await?;
    Ok(row.get("count"))
}

pub async fn create_first_admin_user(
    db: &SqlitePool,
    username: &str,
    email: &str,
    password_hash: &str,
) -> anyhow::Result<User> {
    let now = Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO users (id, username, email, password_hash, role_id, is_active, force_pw_reset, login_attempts, locked_until, created_at, last_modified)
        VALUES (?, ?, ?, ?, 'admin', 1, 0, 0, NULL, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(username)
    .bind(email)
    .bind(password_hash)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;

    find_user_by_id(db, &id)
        .await?
        .context("created admin user not found")
}

pub async fn find_user_by_id(db: &SqlitePool, user_id: &str) -> anyhow::Result<Option<User>> {
    let auth = find_user_auth_by_id(db, user_id).await?;
    Ok(auth.map(|record| record.user))
}

pub async fn find_user_auth_by_id(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Option<UserAuthRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            u.id AS user_id,
            u.username AS username,
            u.email AS email,
            u.password_hash AS password_hash,
            u.role_id AS role_id,
            r.name AS role_name,
            u.is_active AS is_active,
            u.force_pw_reset AS force_pw_reset,
            u.login_attempts AS login_attempts,
            u.locked_until AS locked_until,
            u.created_at AS created_at,
            u.last_modified AS last_modified
        FROM users u
        INNER JOIN roles r ON r.id = u.role_id
        WHERE u.id = ?
        "#,
    )
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    row_to_user_auth(row).context("parse user auth by id")
}

pub async fn find_user_auth_by_username(
    db: &SqlitePool,
    username: &str,
) -> anyhow::Result<Option<UserAuthRecord>> {
    let row = sqlx::query(
        r#"
        SELECT
            u.id AS user_id,
            u.username AS username,
            u.email AS email,
            u.password_hash AS password_hash,
            u.role_id AS role_id,
            r.name AS role_name,
            u.is_active AS is_active,
            u.force_pw_reset AS force_pw_reset,
            u.login_attempts AS login_attempts,
            u.locked_until AS locked_until,
            u.created_at AS created_at,
            u.last_modified AS last_modified
        FROM users u
        INNER JOIN roles r ON r.id = u.role_id
        WHERE u.username = ?
        "#,
    )
    .bind(username)
    .fetch_optional(db)
    .await?;

    row_to_user_auth(row).context("parse user auth by username")
}

pub async fn mark_failed_login(
    db: &SqlitePool,
    user: &UserAuthRecord,
    max_login_attempts: u32,
    lockout_duration_mins: u64,
) -> anyhow::Result<()> {
    let attempts = user.login_attempts + 1;
    let now = Utc::now();
    let max_attempts = max_login_attempts.max(1) as i64;
    let lock_until = if attempts >= max_attempts {
        Some(now + Duration::minutes(lockout_duration_mins as i64))
    } else {
        user.locked_until
    };

    sqlx::query(
        r#"
        UPDATE users
        SET login_attempts = ?, locked_until = ?, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(attempts)
    .bind(lock_until.map(|dt| dt.to_rfc3339()))
    .bind(now.to_rfc3339())
    .bind(&user.user.id)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn clear_login_lockout(db: &SqlitePool, user_id: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE users
        SET login_attempts = 0, locked_until = NULL, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn update_password_hash(
    db: &SqlitePool,
    user_id: &str,
    password_hash: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE users
        SET password_hash = ?, force_pw_reset = 0, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(password_hash)
    .bind(now)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

pub fn generate_refresh_token() -> String {
    let token = format!("{}.{}", Uuid::new_v4(), Uuid::new_v4());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token)
}

pub fn hash_refresh_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

pub async fn insert_refresh_token(
    db: &SqlitePool,
    user_id: &str,
    refresh_token: &str,
    ttl_days: u64,
) -> anyhow::Result<RefreshTokenRecord> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now();
    let expires_at = now + Duration::days(ttl_days as i64);
    let token_hash = hash_refresh_token(refresh_token);

    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at, revoked_at)
        VALUES (?, ?, ?, ?, ?, NULL)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(&token_hash)
    .bind(expires_at.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(db)
    .await?;

    Ok(RefreshTokenRecord {
        id,
        user_id: user_id.to_string(),
        token_hash,
        expires_at,
        created_at: now,
        revoked_at: None,
    })
}

pub async fn find_refresh_token(
    db: &SqlitePool,
    refresh_token: &str,
) -> anyhow::Result<Option<RefreshTokenRecord>> {
    let token_hash = hash_refresh_token(refresh_token);
    let row = sqlx::query(
        r#"
        SELECT id, user_id, token_hash, expires_at, created_at, revoked_at
        FROM refresh_tokens
        WHERE token_hash = ?
        "#,
    )
    .bind(token_hash)
    .fetch_optional(db)
    .await?;

    row_to_refresh_token(row).context("parse refresh token")
}

pub async fn revoke_refresh_token_by_id(db: &SqlitePool, token_id: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE refresh_tokens
        SET revoked_at = COALESCE(revoked_at, ?)
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(token_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn audit_login_success(
    db: &SqlitePool,
    user_id: &str,
    username: &str,
    client_ip: Option<&str>,
) -> anyhow::Result<()> {
    write_user_audit_log(
        db,
        Some(user_id),
        user_id,
        json!({
            "event": "login_success",
            "username": username,
            "client_ip": client_ip,
        }),
    )
    .await
}

pub async fn audit_login_failure(
    db: &SqlitePool,
    user_id: Option<&str>,
    username: &str,
    reason: &str,
    client_ip: Option<&str>,
) -> anyhow::Result<()> {
    let entity_id = if let Some(user_id) = user_id {
        user_id.to_string()
    } else if username.trim().is_empty() {
        "unknown".to_string()
    } else {
        username.trim().to_string()
    };

    write_user_audit_log(
        db,
        user_id,
        &entity_id,
        json!({
            "event": "login_failure",
            "username": username,
            "reason": reason,
            "client_ip": client_ip,
        }),
    )
    .await
}

pub async fn audit_password_change(db: &SqlitePool, user_id: &str) -> anyhow::Result<()> {
    write_user_audit_log(
        db,
        Some(user_id),
        user_id,
        json!({
            "event": "password_change",
        }),
    )
    .await
}

async fn write_user_audit_log(
    db: &SqlitePool,
    user_id: Option<&str>,
    entity_id: &str,
    details: serde_json::Value,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO audit_log (id, user_id, action, entity, entity_id, diff_json, created_at)
        VALUES (?, ?, 'update', 'user', ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(entity_id)
    .bind(details.to_string())
    .bind(now)
    .execute(db)
    .await?;
    Ok(())
}

fn row_to_user_auth(
    row: Option<sqlx::sqlite::SqliteRow>,
) -> anyhow::Result<Option<UserAuthRecord>> {
    let Some(row) = row else {
        return Ok(None);
    };

    let locked_until = parse_optional_dt(row.get::<Option<String>, _>("locked_until"))?;
    Ok(Some(UserAuthRecord {
        user: User {
            id: row.get("user_id"),
            username: row.get("username"),
            email: row.get("email"),
            role: RoleRef {
                id: row.get("role_id"),
                name: row.get("role_name"),
            },
            is_active: row.get::<i64, _>("is_active") != 0,
            force_pw_reset: row.get::<i64, _>("force_pw_reset") != 0,
            created_at: row.get("created_at"),
            last_modified: row.get("last_modified"),
        },
        password_hash: row.get("password_hash"),
        login_attempts: row.get("login_attempts"),
        locked_until,
    }))
}

fn row_to_refresh_token(
    row: Option<sqlx::sqlite::SqliteRow>,
) -> anyhow::Result<Option<RefreshTokenRecord>> {
    let Some(row) = row else {
        return Ok(None);
    };

    Ok(Some(RefreshTokenRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        token_hash: row.get("token_hash"),
        expires_at: parse_dt(row.get("expires_at"))?,
        created_at: parse_dt(row.get("created_at"))?,
        revoked_at: parse_optional_dt(row.get("revoked_at"))?,
    }))
}

fn parse_dt(value: String) -> anyhow::Result<DateTime<Utc>> {
    let parsed = DateTime::parse_from_rfc3339(&value)
        .with_context(|| format!("invalid RFC3339 datetime: {value}"))?;
    Ok(parsed.with_timezone(&Utc))
}

fn parse_optional_dt(value: Option<String>) -> anyhow::Result<Option<DateTime<Utc>>> {
    value.map(parse_dt).transpose()
}
