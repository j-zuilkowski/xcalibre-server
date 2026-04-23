use chrono::Utc;
use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug)]
pub struct TotpBackupCodeRecord {
    pub id: String,
    pub user_id: String,
    pub code_hash: String,
    pub used_at: Option<String>,
    pub created_at: String,
}

pub async fn set_totp_setup_secret(
    db: &SqlitePool,
    user_id: &str,
    encrypted_secret: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE users
        SET totp_secret = ?, totp_enabled = 0, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(encrypted_secret)
    .bind(now)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn enable_totp(db: &SqlitePool, user_id: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE users
        SET totp_enabled = 1, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(user_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn disable_totp(db: &SqlitePool, user_id: &str) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = db.begin().await?;
    sqlx::query(
        r#"
        UPDATE users
        SET totp_enabled = 0, totp_secret = NULL, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(&now)
    .bind(user_id)
    .execute(tx.as_mut())
    .await?;
    sqlx::query("DELETE FROM totp_backup_codes WHERE user_id = ?")
        .bind(user_id)
        .execute(tx.as_mut())
        .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn insert_totp_backup_code(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    code_id: &str,
    user_id: &str,
    code_hash: &str,
    created_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO totp_backup_codes (id, user_id, code_hash, used_at, created_at)
        VALUES (?, ?, ?, NULL, ?)
        "#,
    )
    .bind(code_id)
    .bind(user_id)
    .bind(code_hash)
    .bind(created_at)
    .execute(tx.as_mut())
    .await?;
    Ok(())
}

pub async fn find_unused_backup_code(
    db: &SqlitePool,
    user_id: &str,
    code_hash: &str,
) -> anyhow::Result<Option<TotpBackupCodeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, code_hash, used_at, created_at
        FROM totp_backup_codes
        WHERE user_id = ? AND code_hash = ? AND used_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(code_hash)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| TotpBackupCodeRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        code_hash: row.get("code_hash"),
        used_at: row.get("used_at"),
        created_at: row.get("created_at"),
    }))
}

pub async fn find_unused_backup_code_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
    code_hash: &str,
) -> anyhow::Result<Option<TotpBackupCodeRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, code_hash, used_at, created_at
        FROM totp_backup_codes
        WHERE user_id = ? AND code_hash = ? AND used_at IS NULL
        "#,
    )
    .bind(user_id)
    .bind(code_hash)
    .fetch_optional(tx.as_mut())
    .await?;

    Ok(row.map(|row| TotpBackupCodeRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        code_hash: row.get("code_hash"),
        used_at: row.get("used_at"),
        created_at: row.get("created_at"),
    }))
}

pub async fn mark_backup_code_used(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    code_id: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE totp_backup_codes
        SET used_at = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(code_id)
    .execute(tx.as_mut())
    .await?;
    Ok(())
}

pub async fn clear_totp_backup_codes(db: &SqlitePool, user_id: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM totp_backup_codes WHERE user_id = ?")
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(())
}
