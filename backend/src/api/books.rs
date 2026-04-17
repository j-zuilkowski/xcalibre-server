use axum::{Json, Router};
use serde_json::json;

pub fn router() -> Router {
    Router::new()
        .route("/", axum::routing::get(handler).post(handler))
        .route("/:id", axum::routing::get(handler).patch(handler).delete(handler))
        .route("/:id/cover", axum::routing::get(handler))
        .route("/:id/formats/:format/download", axum::routing::get(handler))
        .route("/:id/formats/:format/stream", axum::routing::get(handler))
}

async fn handler() -> Json<serde_json::Value> {
    Json(json!({
        "error": "not_implemented",
        "message": "stage 1 scaffold only"
    }))
}
