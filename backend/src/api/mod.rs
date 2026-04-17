use axum::Router;

pub mod auth;
pub mod books;

pub fn router(state: crate::AppState) -> Router {
    Router::new()
        .nest("/api/v1/auth", auth::router(state.clone()))
        .merge(books::router(state.clone()))
        .with_state(state)
}
