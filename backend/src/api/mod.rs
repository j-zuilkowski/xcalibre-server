use axum::{middleware, Router};

pub mod auth;
pub mod books;

pub fn router(state: crate::AppState) -> Router {
    let global_rate_limit_per_ip = state.config.limits.rate_limit_per_ip;
    let upload_max_bytes = state.config.limits.upload_max_bytes;
    let auth_router = auth::router(state.clone())
        .layer(crate::middleware::security_headers::auth_rate_limit_layer());

    Router::new()
        .nest("/api/v1/auth", auth_router)
        .merge(books::router(state.clone()))
        .layer(crate::middleware::security_headers::global_rate_limit_layer(
            global_rate_limit_per_ip,
        ))
        .layer(middleware::from_fn_with_state(
            upload_max_bytes,
            crate::middleware::security_headers::enforce_upload_size,
        ))
        .layer(middleware::from_fn(
            crate::middleware::security_headers::apply_security_headers,
        ))
        .with_state(state)
}
