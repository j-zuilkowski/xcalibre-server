use axum::{
    extract::Request as AxumRequest, http::Method, middleware, response::Response, routing::get,
    Router,
};
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

pub mod admin;
pub mod auth;
pub mod books;
pub mod health;
pub mod kobo;
pub mod llm;
pub mod opds;
pub mod search;
pub mod shelves;
pub mod users;

pub fn router(state: crate::AppState) -> Router {
    let global_rate_limit_per_ip = state.config.limits.rate_limit_per_ip;
    let upload_max_bytes = state.config.limits.upload_max_bytes;
    let auth_router = auth::router(state.clone());
    let web_dist_dir = web_dist_dir();
    let assets_dir = web_dist_dir.join("assets");

    Router::new()
        .route("/health", get(health::health_handler))
        .nest("/api/v1/auth", auth_router)
        .merge(admin::router(state.clone()))
        .merge(books::router(state.clone()))
        .merge(users::router(state.clone()))
        .nest("/kobo/:kobo_token/v1", kobo::router(state.clone()))
        .merge(llm::router(state.clone()))
        .nest("/opds", opds::router(state.clone()))
        .merge(shelves::router(state.clone()))
        .merge(search::router(state.clone()))
        .nest_service("/assets", ServeDir::new(assets_dir))
        .fallback(spa_fallback)
        .layer(
            crate::middleware::security_headers::global_rate_limit_layer(global_rate_limit_per_ip),
        )
        .layer(middleware::from_fn_with_state(
            upload_max_bytes,
            crate::middleware::security_headers::enforce_upload_size,
        ))
        .layer(middleware::from_fn(
            crate::middleware::security_headers::apply_security_headers,
        ))
        .layer(crate::middleware::security_headers::cors_layer(
            &state.config.app.base_url,
        ))
        .with_state(state)
}

fn web_dist_dir() -> PathBuf {
    std::env::var_os("WEB_DIST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps/web/dist"))
}

async fn spa_fallback(request: AxumRequest) -> Result<Response, crate::AppError> {
    if !matches!(request.method(), &Method::GET | &Method::HEAD) {
        return Err(crate::AppError::NotFound);
    }

    if request.uri().path().starts_with("/api/") {
        return Err(crate::AppError::NotFound);
    }

    let index_path = web_dist_dir().join("index.html");
    let response = ServeFile::new(index_path)
        .oneshot(request)
        .await
        .map_err(|_| crate::AppError::Internal)?
        .map(axum::body::Body::new);

    if response.status() == axum::http::StatusCode::NOT_FOUND {
        return Err(crate::AppError::NotFound);
    }

    Ok(response)
}
