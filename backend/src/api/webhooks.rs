use crate::{
    auth::totp as totp_auth, db::queries::webhooks as webhook_queries,
    middleware::auth::AuthenticatedUser, webhooks as webhook_engine, AppError, AppState,
};
use axum::{
    extract::{Extension, Path, State},
    middleware,
    routing::{get, patch, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use utoipa::ToSchema;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route(
            "/api/v1/users/me/webhooks",
            get(list_webhooks).post(create_webhook),
        )
        .route(
            "/api/v1/users/me/webhooks/:id",
            patch(update_webhook).delete(delete_webhook),
        )
        .route("/api/v1/users/me/webhooks/:id/test", post(test_webhook))
        .route_layer(auth_layer)
}

const SUPPORTED_EVENTS: &[&str] = &[
    "book.added",
    "book.deleted",
    "import.completed",
    "llm_job.completed",
    "user.registered",
];

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateWebhookRequest {
    pub url: String,
    pub secret: String,
    pub events: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema, Default)]
pub struct UpdateWebhookRequest {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub events: Option<Vec<String>>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct WebhookResponse {
    pub id: String,
    pub url: String,
    pub events: Vec<String>,
    pub enabled: bool,
    pub last_delivery_at: Option<String>,
    pub last_error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeleteWebhookResponse {
    pub success: bool,
}

#[utoipa::path(
    get,
    path = "/api/v1/users/me/webhooks",
    tag = "users",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "Webhook list", body = [WebhookResponse]),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn list_webhooks(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<Vec<WebhookResponse>>, AppError> {
    let rows = webhook_queries::list_webhooks(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(Json(rows.into_iter().map(webhook_to_response).collect()))
}

#[utoipa::path(
    post,
    path = "/api/v1/users/me/webhooks",
    tag = "users",
    security(("bearer_auth" = [])),
    request_body = CreateWebhookRequest,
    responses(
        (status = 201, description = "Webhook created", body = WebhookResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn create_webhook(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<CreateWebhookRequest>,
) -> Result<(axum::http::StatusCode, Json<WebhookResponse>), AppError> {
    let url = payload.url.trim().to_string();
    let secret = payload.secret.trim().to_string();
    if url.is_empty() || secret.is_empty() {
        return Err(AppError::BadRequest);
    }

    webhook_engine::validate_webhook_target(&url, true).await?;
    let events = validate_events(&payload.events)?;
    let encrypted_secret =
        totp_auth::encrypt_webhook_secret(&secret, &state.config.auth.jwt_secret)?;
    let events_json = serde_json::to_string(&events).map_err(|_| AppError::Internal)?;

    let webhook = webhook_queries::create_webhook(
        &state.db,
        &auth_user.user.id,
        &url,
        &encrypted_secret,
        &events_json,
        true,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(webhook_to_response(webhook)),
    ))
}

#[utoipa::path(
    patch,
    path = "/api/v1/users/me/webhooks/{id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(("id" = String, Path, description = "Webhook id")),
    request_body = UpdateWebhookRequest,
    responses(
        (status = 200, description = "Webhook updated", body = WebhookResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn update_webhook(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(webhook_id): Path<String>,
    Json(payload): Json<UpdateWebhookRequest>,
) -> Result<Json<WebhookResponse>, AppError> {
    let Some(existing) =
        webhook_queries::get_webhook_by_id(&state.db, &auth_user.user.id, &webhook_id)
            .await
            .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let mut changed = false;
    let url = if let Some(url) = payload.url {
        let url = url.trim().to_string();
        if url.is_empty() {
            return Err(AppError::BadRequest);
        }
        webhook_engine::validate_webhook_target(&url, true).await?;
        changed = true;
        url
    } else {
        existing.url.clone()
    };

    let events = if let Some(events) = payload.events {
        changed = true;
        validate_events(&events)?
    } else {
        existing.events.clone()
    };

    let enabled = if let Some(enabled) = payload.enabled {
        changed = true;
        enabled
    } else {
        existing.enabled
    };

    if !changed {
        return Err(AppError::BadRequest);
    }

    let events_json = serde_json::to_string(&events).map_err(|_| AppError::Internal)?;
    let updated = webhook_queries::update_webhook(
        &state.db,
        &auth_user.user.id,
        &webhook_id,
        &url,
        &events_json,
        enabled,
    )
    .await
    .map_err(|_| AppError::Internal)?
    .ok_or(AppError::NotFound)?;

    Ok(Json(webhook_to_response(updated)))
}

#[utoipa::path(
    delete,
    path = "/api/v1/users/me/webhooks/{id}",
    tag = "users",
    security(("bearer_auth" = [])),
    params(("id" = String, Path, description = "Webhook id")),
    responses(
        (status = 200, description = "Webhook deleted", body = DeleteWebhookResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn delete_webhook(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(webhook_id): Path<String>,
) -> Result<Json<DeleteWebhookResponse>, AppError> {
    let deleted = webhook_queries::delete_webhook(&state.db, &auth_user.user.id, &webhook_id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }

    Ok(Json(DeleteWebhookResponse { success: true }))
}

#[utoipa::path(
    post,
    path = "/api/v1/users/me/webhooks/{id}/test",
    tag = "users",
    security(("bearer_auth" = [])),
    params(("id" = String, Path, description = "Webhook id")),
    responses(
        (status = 200, description = "Webhook test result", body = webhook_engine::DeliveryAttemptResult),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn test_webhook(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(webhook_id): Path<String>,
) -> Result<Json<webhook_engine::DeliveryAttemptResult>, AppError> {
    let Some(webhook) =
        webhook_queries::get_webhook_by_id(&state.db, &auth_user.user.id, &webhook_id)
            .await
            .map_err(|_| AppError::Internal)?
    else {
        return Err(AppError::NotFound);
    };

    let result = webhook_engine::send_webhook_test(&state.http_client, &webhook).await?;
    Ok(Json(result))
}

fn validate_events(events: &[String]) -> Result<Vec<String>, AppError> {
    if events.is_empty() {
        return Err(AppError::BadRequest);
    }

    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(events.len());
    for event in events {
        let event = event.trim();
        if event.is_empty() {
            return Err(AppError::BadRequest);
        }
        if !SUPPORTED_EVENTS.contains(&event) {
            return Err(AppError::Unprocessable);
        }
        if seen.insert(event.to_string()) {
            normalized.push(event.to_string());
        }
    }

    Ok(normalized)
}

fn webhook_to_response(webhook: webhook_queries::WebhookRecord) -> WebhookResponse {
    WebhookResponse {
        id: webhook.id,
        url: webhook.url,
        events: webhook.events,
        enabled: webhook.enabled,
        last_delivery_at: webhook.last_delivery_at,
        last_error: webhook.last_error,
        created_at: webhook.created_at,
    }
}
