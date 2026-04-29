//! Router assembly for xcalibre-server: merges all sub-routers into the main Axum router.
//!
//! The top-level `router` function wires together every API module, the Kobo sync
//! endpoint (nested under `/kobo/:kobo_token/v1/`), the OPDS catalog (nested under
//! `/opds/`), and static asset serving for the SPA frontend.
//!
//! Global middleware layers applied in order (outermost first):
//! 1. CORS — allows the configured `base_url` origin.
//! 2. Security headers — adds the 5 required headers (CSP, HSTS, X-Frame-Options, etc.).
//! 3. Upload size enforcement — rejects request bodies larger than `upload_max_bytes`.
//! 4. Rate limiting headers — attaches `X-RateLimit-*` headers.
//! 5. Global per-IP rate limiting — via `tower-governor`.
//!
//! The SPA fallback serves `apps/web/dist/index.html` for any GET/HEAD that is not an
//! `/api/` prefix and not a matched route, enabling client-side routing.

use axum::{
    extract::Request as AxumRequest, http::Method, middleware, response::Response, routing::get,
    Router,
};
use std::path::PathBuf;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

pub mod admin;
pub mod auth;
pub mod authors;
pub mod books;
pub mod collections;
pub mod docs;
pub mod health;
pub mod kobo;
pub mod llm;
pub mod memory;
pub mod opds;
pub mod search;
pub mod shelves;
pub mod users;
pub mod webhooks;

/// Assembles the complete application router with all sub-routers and global middleware.
pub fn router(state: crate::AppState) -> Router {
    let global_rate_limit_per_ip = state.config.limits.rate_limit_per_ip;
    let upload_max_bytes = state.config.limits.upload_max_bytes;
    let auth_router = auth::router(state.clone());
    let web_dist_dir = web_dist_dir();
    let assets_dir = web_dist_dir.join("assets");

    Router::new()
        .route("/health", get(health::health_handler))
        .merge(docs::openapi_routes(state.clone()))
        .nest("/api/v1/auth", auth_router)
        .merge(admin::router(state.clone()))
        .merge(collections::router(state.clone()))
        .merge(authors::router(state.clone()))
        .merge(books::router(state.clone()))
        .merge(users::router(state.clone()))
        .merge(webhooks::router(state.clone()))
        .nest("/kobo/:kobo_token/v1", kobo::router(state.clone()))
        .merge(llm::router(state.clone()))
        .merge(memory::router(state.clone()))
        .nest("/opds", opds::router(state.clone()))
        .merge(shelves::router(state.clone()))
        .merge(search::router(state.clone()))
        .nest_service("/assets", ServeDir::new(assets_dir))
        .fallback(spa_fallback)
        .layer(
            crate::middleware::security_headers::global_rate_limit_layer(global_rate_limit_per_ip),
        )
        .layer(middleware::from_fn_with_state(
            crate::middleware::security_headers::global_rate_limit_headers_config(
                global_rate_limit_per_ip,
            ),
            crate::middleware::security_headers::apply_rate_limit_headers,
        ))
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

/// Returns the SPA build output directory, overridable via the `WEB_DIST_DIR` environment variable.
fn web_dist_dir() -> PathBuf {
    std::env::var_os("WEB_DIST_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("apps/web/dist"))
}

/// Serves `index.html` for any GET/HEAD that is not an `/api/` path and has no matched route,
/// supporting client-side routing in the React SPA. Non-GET/HEAD methods return 404.
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
