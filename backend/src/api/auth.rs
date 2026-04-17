use axum::{Json, Router};
use serde_json::json;

pub fn router() -> Router {
    Router::new()
        .route("/register", axum::routing::post(handler))
        .route("/login", axum::routing::post(handler))
        .route("/logout", axum::routing::post(handler))
        .route("/refresh", axum::routing::post(handler))
        .route("/me", axum::routing::get(handler))
        .route("/me/password", axum::routing::patch(handler))
}

async fn handler() -> Json<serde_json::Value> {
    Json(json!({
        "error": "not_implemented",
        "message": "stage 1 scaffold only"
    }))
}
