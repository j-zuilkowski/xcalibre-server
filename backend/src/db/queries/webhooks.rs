use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WebhookRecord {
    pub id: String,
    pub user_id: String,
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
    pub enabled: bool,
    pub last_delivery_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Debug)]
pub struct PendingWebhookDeliveryRecord {
    pub id: String,
    pub webhook_id: String,
    pub webhook_url: String,
    pub webhook_secret: String,
    pub event: String,
    pub payload: String,
    pub attempts: i64,
    pub next_attempt_at: Option<String>,
}

pub async fn list_webhooks(db: &SqlitePool, user_id: &str) -> anyhow::Result<Vec<WebhookRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, url, secret, events, enabled, last_delivery_at, last_error, created_at
        FROM webhooks
        WHERE user_id = ?
        ORDER BY created_at DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    rows.into_iter().map(row_to_webhook).collect()
}

pub async fn get_webhook_by_id(
    db: &SqlitePool,
    user_id: &str,
    webhook_id: &str,
) -> anyhow::Result<Option<WebhookRecord>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, url, secret, events, enabled, last_delivery_at, last_error, created_at
        FROM webhooks
        WHERE id = ? AND user_id = ?
        LIMIT 1
        "#,
    )
    .bind(webhook_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    row.map(row_to_webhook).transpose()
}

pub async fn create_webhook(
    db: &SqlitePool,
    user_id: &str,
    url: &str,
    secret: &str,
    events_json: &str,
    enabled: bool,
) -> anyhow::Result<WebhookRecord> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO webhooks (
            id, user_id, url, secret, events, enabled, last_delivery_at, last_error, created_at
        )
        VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, ?)
        "#,
    )
    .bind(&id)
    .bind(user_id)
    .bind(url)
    .bind(secret)
    .bind(events_json)
    .bind(i64::from(enabled))
    .bind(&now)
    .execute(db)
    .await?;

    get_webhook_by_id(db, user_id, &id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("created webhook not found"))
}

pub async fn update_webhook(
    db: &SqlitePool,
    user_id: &str,
    webhook_id: &str,
    url: &str,
    events_json: &str,
    enabled: bool,
) -> anyhow::Result<Option<WebhookRecord>> {
    let result = sqlx::query(
        r#"
        UPDATE webhooks
        SET url = ?, events = ?, enabled = ?, last_error = NULL
        WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(url)
    .bind(events_json)
    .bind(i64::from(enabled))
    .bind(webhook_id)
    .bind(user_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    get_webhook_by_id(db, user_id, webhook_id).await
}

pub async fn delete_webhook(
    db: &SqlitePool,
    user_id: &str,
    webhook_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM webhooks
        WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(webhook_id)
    .bind(user_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn list_enabled_webhooks_for_event(
    db: &SqlitePool,
    event: &str,
) -> anyhow::Result<Vec<WebhookRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, url, secret, events, enabled, last_delivery_at, last_error, created_at
        FROM webhooks
        WHERE enabled = 1
          AND EXISTS (
              SELECT 1
              FROM json_each(webhooks.events)
              WHERE json_each.value = ?
          )
        ORDER BY created_at ASC
        "#,
    )
    .bind(event)
    .fetch_all(db)
    .await?;

    rows.into_iter().map(row_to_webhook).collect()
}

pub async fn list_enabled_admin_webhooks_for_event(
    db: &SqlitePool,
    event: &str,
) -> anyhow::Result<Vec<WebhookRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT w.id, w.user_id, w.url, w.secret, w.events, w.enabled, w.last_delivery_at, w.last_error, w.created_at
        FROM webhooks w
        INNER JOIN users u ON u.id = w.user_id
        INNER JOIN roles r ON r.id = u.role_id
        WHERE w.enabled = 1
          AND LOWER(r.name) = 'admin'
          AND EXISTS (
              SELECT 1
              FROM json_each(w.events)
              WHERE json_each.value = ?
          )
        ORDER BY w.created_at ASC
        "#,
    )
    .bind(event)
    .fetch_all(db)
    .await?;

    rows.into_iter().map(row_to_webhook).collect()
}

pub async fn insert_delivery(
    db: &SqlitePool,
    webhook_id: &str,
    event: &str,
    payload: &str,
    next_attempt_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO webhook_deliveries (
            id, webhook_id, event, payload, status, attempts, next_attempt_at,
            response_status, created_at, delivered_at
        )
        VALUES (?, ?, ?, ?, 'pending', 0, ?, NULL, ?, NULL)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(webhook_id)
    .bind(event)
    .bind(payload)
    .bind(next_attempt_at)
    .bind(Utc::now().to_rfc3339())
    .execute(db)
    .await?;
    Ok(())
}

pub async fn list_pending_deliveries(
    db: &SqlitePool,
    now: &str,
    limit: i64,
) -> anyhow::Result<Vec<PendingWebhookDeliveryRecord>> {
    let rows = sqlx::query(
        r#"
        SELECT
            d.id AS delivery_id,
            d.webhook_id,
            d.event,
            d.payload,
            d.attempts,
            d.next_attempt_at,
            w.url AS webhook_url,
            w.secret AS webhook_secret
        FROM webhook_deliveries d
        INNER JOIN webhooks w ON w.id = d.webhook_id
        WHERE d.status = 'pending'
          AND (d.next_attempt_at IS NULL OR d.next_attempt_at <= ?)
        ORDER BY d.created_at ASC
        LIMIT ?
        "#,
    )
    .bind(now)
    .bind(limit)
    .fetch_all(db)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PendingWebhookDeliveryRecord {
            id: row.get("delivery_id"),
            webhook_id: row.get("webhook_id"),
            webhook_url: row.get("webhook_url"),
            webhook_secret: row.get("webhook_secret"),
            event: row.get("event"),
            payload: row.get("payload"),
            attempts: row.get("attempts"),
            next_attempt_at: row.get("next_attempt_at"),
        })
        .collect())
}

pub async fn mark_delivery_delivered(
    db: &SqlitePool,
    delivery_id: &str,
    response_status: i64,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE webhook_deliveries
        SET status = 'delivered',
            attempts = attempts + 1,
            response_status = ?,
            delivered_at = ?,
            next_attempt_at = NULL
        WHERE id = ?
        "#,
    )
    .bind(response_status)
    .bind(now)
    .bind(delivery_id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn mark_delivery_retry(
    db: &SqlitePool,
    delivery_id: &str,
    error_message: &str,
    next_attempt_at: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE webhook_deliveries
        SET attempts = attempts + 1,
            status = 'pending',
            next_attempt_at = ?,
            response_status = NULL
        WHERE id = ?
        "#,
    )
    .bind(next_attempt_at)
    .bind(delivery_id)
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        UPDATE webhooks
        SET last_error = ?
        WHERE id = (
            SELECT webhook_id
            FROM webhook_deliveries
            WHERE id = ?
        )
        "#,
    )
    .bind(error_message)
    .bind(delivery_id)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn mark_delivery_failed(
    db: &SqlitePool,
    delivery_id: &str,
    error_message: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE webhook_deliveries
        SET attempts = attempts + 1,
            status = 'failed',
            response_status = NULL
        WHERE id = ?
        "#,
    )
    .bind(delivery_id)
    .execute(db)
    .await?;

    sqlx::query(
        r#"
        UPDATE webhooks
        SET last_error = ?
        WHERE id = (
            SELECT webhook_id
            FROM webhook_deliveries
            WHERE id = ?
        )
        "#,
    )
    .bind(error_message)
    .bind(delivery_id)
    .execute(db)
    .await?;

    Ok(())
}

pub async fn mark_webhook_delivery_success(
    db: &SqlitePool,
    webhook_id: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE webhooks
        SET last_delivery_at = ?,
            last_error = NULL
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(webhook_id)
    .execute(db)
    .await?;
    Ok(())
}

fn row_to_webhook(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<WebhookRecord> {
    let events_json: String = row.get("events");
    let events = serde_json::from_str::<Vec<String>>(&events_json)?;

    Ok(WebhookRecord {
        id: row.get("id"),
        user_id: row.get("user_id"),
        url: row.get("url"),
        secret: row.get("secret"),
        events,
        enabled: row.get::<i64, _>("enabled") != 0,
        last_delivery_at: row.get("last_delivery_at"),
        last_error: row.get("last_error"),
        created_at: row.get("created_at"),
    })
}
