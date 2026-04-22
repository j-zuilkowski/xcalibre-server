use chrono::Utc;
use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug, Default)]
pub struct EmailSettings {
    pub id: String,
    pub smtp_host: String,
    pub smtp_port: i64,
    pub smtp_user: String,
    pub smtp_password: String,
    pub from_address: String,
    pub use_tls: bool,
    pub updated_at: String,
}

pub async fn get_email_settings(db: &SqlitePool) -> anyhow::Result<Option<EmailSettings>> {
    let row = sqlx::query(
        r#"
        SELECT id, smtp_host, smtp_port, smtp_user, smtp_password, from_address, use_tls, updated_at
        FROM email_settings
        WHERE id = 'singleton'
        "#,
    )
    .fetch_optional(db)
    .await?;

    Ok(row.map(row_to_email_settings))
}

pub async fn upsert_email_settings(
    db: &SqlitePool,
    settings: EmailSettings,
) -> anyhow::Result<EmailSettings> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO email_settings (
            id, smtp_host, smtp_port, smtp_user, smtp_password, from_address, use_tls, updated_at
        ) VALUES ('singleton', ?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            smtp_host = excluded.smtp_host,
            smtp_port = excluded.smtp_port,
            smtp_user = excluded.smtp_user,
            smtp_password = excluded.smtp_password,
            from_address = excluded.from_address,
            use_tls = excluded.use_tls,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(settings.smtp_host.trim())
    .bind(settings.smtp_port)
    .bind(settings.smtp_user.trim())
    .bind(settings.smtp_password)
    .bind(settings.from_address.trim())
    .bind(i64::from(settings.use_tls))
    .bind(&now)
    .execute(db)
    .await?;

    get_email_settings(db)
        .await?
        .map(|mut row| {
            row.updated_at = now;
            row
        })
        .ok_or_else(|| anyhow::anyhow!("email settings missing after upsert"))
}

fn row_to_email_settings(row: sqlx::sqlite::SqliteRow) -> EmailSettings {
    EmailSettings {
        id: row.get("id"),
        smtp_host: row.get("smtp_host"),
        smtp_port: row.get("smtp_port"),
        smtp_user: row.get("smtp_user"),
        smtp_password: row.get("smtp_password"),
        from_address: row.get("from_address"),
        use_tls: row.get::<i64, _>("use_tls") != 0,
        updated_at: row.get("updated_at"),
    }
}
