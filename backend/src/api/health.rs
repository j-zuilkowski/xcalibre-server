use crate::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    version: &'static str,
    db: ComponentStatus,
    search: ComponentStatus,
}

#[derive(Serialize)]
pub struct ComponentStatus {
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

pub async fn health_handler(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
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
