//! Webhook delivery engine.
//!
//! Delivers signed HTTP POST callbacks to subscriber URLs when library events occur.
//!
//! # Delivery flow
//! 1. [`enqueue_event`] is called by event-generating code (book import, LLM job
//!    completion, etc.).  It serializes the payload, checks the size cap (1 MB),
//!    and inserts a `webhook_delivery` row for each active subscriber.
//! 2. The scheduler calls [`deliver_pending`] every 30 seconds.  It fetches up to
//!    50 pending deliveries (with `next_attempt_at <= now`), sends each via HTTP POST,
//!    and marks the delivery as delivered, failed, or scheduled for retry.
//!
//! # HMAC signing
//! Each delivery is signed with `HMAC-SHA256` over the raw JSON payload.  The
//! signature is sent as `X-Xcalibre-server-Signature: sha256=<hex>`.  The webhook secret
//! is stored encrypted (AES-256-GCM, key derived from `jwt_secret` via HKDF in
//! `auth::totp`); it is decrypted at delivery time.
//!
//! # Retry schedule with exponential backoff
//! | Attempt | Retry delay |
//! |---------|-------------|
//! | 1       | 30 seconds  |
//! | 2       | 5 minutes   |
//! | 3+      | Permanent failure (no more retries) |
//!
//! # SSRF protection
//! [`validate_webhook_target`] blocks private/loopback addresses.  Private endpoints
//! can be allowed per-webhook via a flag (for self-hosted integrations), but the
//! default is deny.
//!
//! # Payload size cap
//! Payloads larger than 1 MB are rejected at enqueue time.  The same check is
//! repeated at delivery time as a safety net.
//!
//! # `user.registered` events
//! These are delivered only to admin-owned webhooks (`admin_only = true`) to prevent
//! leaking PII to non-admin subscribers.

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
    net::IpAddr,
    sync::{Mutex, OnceLock},
    time::Duration,
};
use thiserror::Error;
use utoipa::ToSchema;

const WEBHOOK_DELIVERY_TIMEOUT: Duration = Duration::from_secs(10);
const WEBHOOK_TEST_TIMEOUT: Duration = Duration::from_secs(5);
const WEBHOOK_TEST_MESSAGE: &str = "Webhook test from xcalibre-server";
const MAX_WEBHOOK_PAYLOAD_BYTES: usize = 1_000_000;
static WEBHOOK_JWT_SECRET: OnceLock<Mutex<String>> = OnceLock::new();

/// The result of a single delivery attempt.
///
/// `should_retry = true` means a transient failure; the caller will schedule
/// another attempt according to the backoff table.
#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct DeliveryAttemptResult {
    pub delivered: bool,
    pub should_retry: bool,
    pub response_status: Option<u16>,
    pub error: Option<String>,
}

/// Parameters for a single webhook delivery HTTP request.
pub struct DeliveryRequest<'a> {
    http_client: &'a Client,
    jwt_secret: &'a str,
    timeout: Duration,
    require_https: bool,
}

/// Errors that can occur when validating a webhook target URL.
///
/// Mapped to [`AppError`] via the `From` impl: `PrivateOrLoopbackAddress` becomes
/// `AppError::SsrfBlocked`; all others become `AppError::Unprocessable`.
#[derive(Debug, Error)]
pub enum WebhookTargetError {
    #[error("invalid URL")]
    InvalidUrl,
    #[error("only http and https schemes are allowed")]
    UnsupportedScheme,
    #[error("URL must include a host")]
    MissingHost,
    #[error("private or loopback address")]
    PrivateOrLoopbackAddress,
}

impl From<WebhookTargetError> for AppError {
    fn from(err: WebhookTargetError) -> Self {
        match err {
            WebhookTargetError::PrivateOrLoopbackAddress => AppError::SsrfBlocked,
            _ => AppError::Unprocessable,
        }
    }
}

impl<'a> DeliveryRequest<'a> {
    pub fn new(
        http_client: &'a Client,
        jwt_secret: &'a str,
        timeout: Duration,
        require_https: bool,
    ) -> Self {
        Self {
            http_client,
            jwt_secret,
            timeout,
            require_https,
        }
    }
}

/// Enqueue a webhook event for all enabled subscribers of this event type.
///
/// The payload is serialized to JSON and its size is checked against the 1 MB cap.
/// Oversized payloads are dropped with a warning — no error is returned to the caller.
/// Returns the number of webhook delivery rows inserted (one per subscriber).
pub async fn enqueue_event(
    db: &sqlx::SqlitePool,
    event: &str,
    payload: Value,
) -> anyhow::Result<usize> {
    let payload_bytes = serde_json::to_vec(&payload)?;
    if payload_bytes.len() > MAX_WEBHOOK_PAYLOAD_BYTES {
        tracing::warn!(
            size = payload_bytes.len(),
            "webhook payload exceeds size limit; skipping enqueue"
        );
        return Ok(0);
    }
    let payload_json = String::from_utf8(payload_bytes)?;
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

/// Fetch and deliver up to 50 pending webhook deliveries whose retry time has passed.
///
/// For each delivery, calls [`deliver_single_delivery`] and updates the DB row:
/// - Success (2xx) → `delivered`
/// - Transient failure + attempts < 3 → retry with backoff
/// - Permanent failure or attempts ≥ 3 → `failed`
///
/// Returns the count of deliveries processed (attempted, regardless of outcome).
pub async fn deliver_pending(db: &sqlx::SqlitePool, http_client: &Client) -> anyhow::Result<usize> {
    let now = Utc::now().to_rfc3339();
    let pending = webhook_queries::list_pending_deliveries(db, &now, 50).await?;
    let jwt_secret = webhook_jwt_secret()?;
    let delivery_request =
        DeliveryRequest::new(http_client, &jwt_secret, WEBHOOK_DELIVERY_TIMEOUT, false);
    let mut processed = 0usize;

    for delivery in pending {
        processed += 1;
        match deliver_single_delivery(
            &delivery_request,
            &delivery.webhook_id,
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
                    let error = result
                        .error
                        .clone()
                        .unwrap_or_else(|| "webhook_delivery_failed".to_string());
                    if !result.should_retry {
                        webhook_queries::mark_delivery_failed(db, &delivery.id, &error).await?;
                    } else {
                        let attempts = delivery.attempts + 1;
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
    let delivery_request =
        DeliveryRequest::new(http_client, &jwt_secret, WEBHOOK_TEST_TIMEOUT, false);
    deliver_single_delivery(
        &delivery_request,
        &webhook.id,
        &webhook.url,
        &webhook.secret,
        "ping",
        &payload_json,
    )
    .await
}

pub async fn deliver_single_delivery(
    request: &DeliveryRequest<'_>,
    webhook_id: &str,
    url: &str,
    encrypted_secret: &str,
    event: &str,
    payload_json: &str,
) -> Result<DeliveryAttemptResult, AppError> {
    if payload_json.len() > MAX_WEBHOOK_PAYLOAD_BYTES {
        tracing::warn!(
            webhook_id = %webhook_id,
            payload_bytes = payload_json.len(),
            "webhook payload exceeds 1 MB limit - delivery skipped"
        );
        return Ok(DeliveryAttemptResult {
            delivered: false,
            should_retry: false,
            response_status: None,
            error: Some(format!(
                "payload_too_large: {} bytes exceeds 1 MB limit",
                payload_json.len()
            )),
        });
    }

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
        .header("X-Xcalibre-server-Signature", format!("sha256={signature}"))
        .header("X-Xcalibre-server-Event", event)
        .body(payload_json.to_string())
        .send()
        .await;

    match response {
        Ok(response) => {
            let status = response.status().as_u16();
            if response.status().is_success() {
                Ok(DeliveryAttemptResult {
                    delivered: true,
                    should_retry: false,
                    response_status: Some(status),
                    error: None,
                })
            } else {
                Ok(DeliveryAttemptResult {
                    delivered: false,
                    should_retry: true,
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
                should_retry: true,
                response_status: None,
                error: Some(message),
            })
        }
    }
}

/// Validate a webhook target URL for scheme, host presence, and SSRF safety.
///
/// Blocks private/loopback IP addresses and `localhost` by default.
/// Set `allow_private_endpoints = true` only for explicitly trusted self-hosted targets.
pub async fn validate_webhook_target(
    url: &str,
    allow_private_endpoints: bool,
) -> Result<(), WebhookTargetError> {
    let parsed = reqwest::Url::parse(url).map_err(|_| WebhookTargetError::InvalidUrl)?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(WebhookTargetError::UnsupportedScheme),
    }

    let host = parsed.host_str().ok_or(WebhookTargetError::MissingHost)?;
    if allow_private_endpoints {
        return Ok(());
    }

    if host.eq_ignore_ascii_case("localhost") {
        return Err(WebhookTargetError::PrivateOrLoopbackAddress);
    }

    if let Ok(ip) = host.parse::<IpAddr>() {
        if is_private_or_loopback(ip) {
            return Err(WebhookTargetError::PrivateOrLoopbackAddress);
        }
    }

    Ok(())
}

/// Compute the next retry timestamp using exponential backoff.
///
/// Attempt 1 → 30s, attempt 2 → 5 min, attempt 3+ → 30 min.
/// After 3 total attempts the delivery is marked permanently failed; this function
/// is therefore only called when `attempts < 3`.
fn retry_deadline_for_attempt(attempts_after_increment: i64) -> String {
    let delay = match attempts_after_increment {
        1 => Duration::from_secs(30),
        2 => Duration::from_secs(5 * 60),
        _ => Duration::from_secs(30 * 60),
    };
    (Utc::now() + chrono::Duration::from_std(delay).expect("valid delay")).to_rfc3339()
}

/// Initialize the module-level JWT secret used for HMAC-signing webhook payloads.
///
/// Called once at app startup with the value from `AppConfig.auth.jwt_secret`.
/// Subsequent calls update the secret (for key rotation), protected by a `Mutex`.
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
