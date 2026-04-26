//! Liveness and readiness health check for xcalibre-server.
//!
//! Exposes `GET /health` — no authentication required, intended for use by container
//! orchestrators and load balancers. Returns 200 OK when the DB is reachable and
//! 503 Service Unavailable when the DB query fails.
//!
//! Meilisearch status is reported as a sub-component: "disabled" when not configured,
//! "degraded" when configured but unavailable (falls back to FTS5 backend), "ok" otherwise.
//! Meilisearch degradation does not affect the top-level HTTP status.

use crate::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use utoipa::ToSchema;

/// Top-level health response including overall status, version, and per-component checks.
#[derive(Serialize, ToSchema)]
pub struct HealthResponse {
    status: &'static str,
    version: &'static str,
    db: ComponentStatus,
    search: ComponentStatus,
}

/// Status of a single infrastructure component, with an optional error message for degraded state.
#[derive(Serialize, ToSchema)]
pub struct ComponentStatus {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
/// Executes a `SELECT 1` DB probe and checks Meilisearch availability, then returns a
/// structured status payload; HTTP status is 503 only if the DB probe fails.
pub(crate) async fn health_handler(
    State(state): State<AppState>,
) -> (StatusCode, Json<HealthResponse>) {
    let db_status = match sqlx::query("SELECT 1").fetch_one(&state.db).await {
        Ok(_) => ComponentStatus {
            status: "ok",
            error: None,
        },
        Err(err) => ComponentStatus {
            status: "degraded",
            error: Some(err.to_string()),
        },
    };

    let search_status = if state.config.meilisearch.enabled {
        if state.search.backend_name() != "meilisearch" {
            ComponentStatus {
                status: "degraded",
                error: Some(format!(
                    "meilisearch unavailable; using {} backend",
                    state.search.backend_name()
                )),
            }
        } else if state.search.is_available().await {
            ComponentStatus {
                status: "ok",
                error: None,
            }
        } else {
            ComponentStatus {
                status: "degraded",
                error: Some("meilisearch health check failed".to_string()),
            }
        }
    } else {
        ComponentStatus {
            status: "disabled",
            error: None,
        }
    };

    let overall_status = if db_status.status == "ok" {
        "ok"
    } else {
        "degraded"
    };
    let http_status = if overall_status == "ok" {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        http_status,
        Json(HealthResponse {
            status: overall_status,
            version: env!("CARGO_PKG_VERSION"),
            db: db_status,
            search: search_status,
        }),
    )
}
