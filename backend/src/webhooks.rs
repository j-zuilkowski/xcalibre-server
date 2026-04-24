use crate::{
    auth::totp as totp_auth, config::is_private_or_loopback,
    db::queries::webhooks as webhook_queries, AppError,
};
use chrono::Utc;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::Serialize;
use serde_json::Value;
use sha2::Sha256;
use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};
use utoipa::ToSchema;

const WEBHOOK_DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
const WEBHOOK_TEST_TIMEOUT: Duration = Duration::from_secs(5);
const WEBHOOK_TEST_MESSAGE: &str = "Webhook test from autolibre";
static WEBHOOK_JWT_SECRET: OnceLock<Mutex<String>> = OnceLock::new();

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct DeliveryAttemptResult {
    pub delivered: bool,
    pub response_status: Option<u16>,
    pub error: Option<String>,
}

struct DeliveryRequest<'a> {
    http_client: &'a Client,
    jwt_secret: &'a str,
    timeout: Duration,
    require_https: bool,
}

pub async fn enqueue_event(
    db: &sqlx::SqlitePool,
    event: &str,
    payload: Value,
) -> anyhow::Result<usize> {
    let payload_json = serde_json::to_string(&payload)?;
    let now = Utc::now().to_rfc3339();

    let webhooks = if event == "user.registered" {
        webhook_queries::list_enabled_admin_webhooks_for_event(db, event).await?
    } else {
        webhook_queries::list_enabled_webhooks_for_event(db, event).await?
    };

    for webhook in &webhooks {
        webhook_queries::insert_delivery(db, &webhook.id, event, &payload_json, &now).await?;
    }

    Ok(webhooks.len())
}

pub async fn deliver_pending(db: &sqlx::SqlitePool, http_client: &Client) -> anyhow::Result<usize> {
    let now = Utc::now().to_rfc3339();
    let pending = webhook_queries::list_pending_deliveries(db, &now, 50).await?;
    let jwt_secret = webhook_jwt_secret()?;
    let delivery_request = DeliveryRequest {
        http_client,
        jwt_secret: &jwt_secret,
        timeout: WEBHOOK_DELIVERY_TIMEOUT,
        require_https: false,
    };
    let mut processed = 0usize;

    for delivery in pending {
        processed += 1;
        match deliver_single_delivery(
            &delivery_request,
            &delivery.webhook_url,
            &delivery.webhook_secret,
            &delivery.event,
            &delivery.payload,
        )
        .await
        {
            Ok(result) => {
                if result.delivered {
                    webhook_queries::mark_delivery_delivered(
                        db,
                        &delivery.id,
                        i64::from(result.response_status.unwrap_or(200)),
                    )
                    .await?;
                    webhook_queries::mark_webhook_delivery_success(db, &delivery.webhook_id)
                        .await?;
                } else {
                    let attempts = delivery.attempts + 1;
                    let error = result
                        .error
                        .clone()
                        .unwrap_or_else(|| "webhook_delivery_failed".to_string());
                    if attempts >= 3 {
                        webhook_queries::mark_delivery_failed(db, &delivery.id, &error).await?;
                    } else {
                        let next_attempt_at = retry_deadline_for_attempt(attempts);
                        webhook_queries::mark_delivery_retry(
                            db,
                            &delivery.id,
                            &error,
                            &next_attempt_at,
                        )
                        .await?;
                    }
                }
            }
            Err(err) => {
                let attempts = delivery.attempts + 1;
                let error = err.to_string();
                if attempts >= 3 {
                    webhook_queries::mark_delivery_failed(db, &delivery.id, &error).await?;
                } else {
                    let next_attempt_at = retry_deadline_for_attempt(attempts);
                    webhook_queries::mark_delivery_retry(
                        db,
                        &delivery.id,
                        &error,
                        &next_attempt_at,
                    )
                    .await?;
                }
            }
        }
    }

    Ok(processed)
}

pub async fn send_webhook_test(
    http_client: &Client,
    webhook: &webhook_queries::WebhookRecord,
) -> Result<DeliveryAttemptResult, AppError> {
    let jwt_secret = webhook_jwt_secret()?;
    let payload_json = serde_json::json!({ "message": WEBHOOK_TEST_MESSAGE }).to_string();
    let delivery_request = DeliveryRequest {
        http_client,
        jwt_secret: &jwt_secret,
        timeout: WEBHOOK_TEST_TIMEOUT,
        require_https: false,
    };
    deliver_single_delivery(
        &delivery_request,
        &webhook.url,
        &webhook.secret,
        "ping",
        &payload_json,
    )
    .await
}

async fn deliver_single_delivery(
    request: &DeliveryRequest<'_>,
    url: &str,
    encrypted_secret: &str,
    event: &str,
    payload_json: &str,
) -> Result<DeliveryAttemptResult, AppError> {
    validate_webhook_target(url, request.require_https).await?;

    let secret = totp_auth::decrypt_webhook_secret(encrypted_secret, request.jwt_secret)?;
    let mut mac =
        Hmac::<Sha256>::new_from_slice(secret.as_bytes()).map_err(|_| AppError::Internal)?;
    mac.update(payload_json.as_bytes());
    let signature = hex::encode(mac.finalize().into_bytes());

    let response = request
        .http_client
        .post(url)
        .timeout(request.timeout)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .header("X-Autolibre-Signature", format!("sha256={signature}"))
        .header("X-Autolibre-Event", event)
        .body(payload_json.to_string())
        .send()
        .await;

    match response {
        Ok(response) => {
            let status = response.status().as_u16();
            if response.status().is_success() {
                Ok(DeliveryAttemptResult {
                    delivered: true,
                    response_status: Some(status),
                    error: None,
                })
            } else {
                Ok(DeliveryAttemptResult {
                    delivered: false,
                    response_status: Some(status),
                    error: Some(format!("http_status_{status}")),
                })
            }
        }
        Err(err) => {
            let message = if err.is_timeout() {
                "timeout".to_string()
            } else {
                err.to_string()
            };
            Ok(DeliveryAttemptResult {
                delivered: false,
                response_status: None,
                error: Some(message),
            })
        }
    }
}

pub async fn validate_webhook_target(url: &str, require_https: bool) -> Result<(), AppError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| AppError::Unprocessable)?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(AppError::Unprocessable),
    }
    if require_https && parsed.scheme() != "https" {
        return Err(AppError::Unprocessable);
    }

    let host = parsed.host_str().ok_or(AppError::Unprocessable)?;
    let port = parsed
        .port_or_known_default()
        .ok_or(AppError::Unprocessable)?;
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|_| AppError::Unprocessable)?;

    for addr in resolved {
        if is_private_or_loopback(addr.ip()) {
            return Err(AppError::SsrfBlocked);
        }
    }

    Ok(())
}

fn retry_deadline_for_attempt(attempts_after_increment: i64) -> String {
    let delay = match attempts_after_increment {
        1 => Duration::from_secs(30),
        2 => Duration::from_secs(5 * 60),
        _ => Duration::from_secs(30 * 60),
    };
    (Utc::now() + chrono::Duration::from_std(delay).expect("valid delay")).to_rfc3339()
}

pub fn set_webhook_jwt_secret(secret: String) {
    let lock = WEBHOOK_JWT_SECRET.get_or_init(|| Mutex::new(String::new()));
    if let Ok(mut current) = lock.lock() {
        *current = secret;
    }
}

fn webhook_jwt_secret() -> Result<String, AppError> {
    let lock = WEBHOOK_JWT_SECRET.get().ok_or(AppError::Internal)?;
    let current = lock.lock().map_err(|_| AppError::Internal)?;
    Ok(current.clone())
}
