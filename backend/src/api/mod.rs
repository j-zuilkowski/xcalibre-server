use axum::Router;

pub mod auth;
pub mod books;

pub fn router(_state: crate::AppState) -> Router {
    Router::new()
        .nest("/api/v1/auth", auth::router())
        .nest("/api/v1/books", books::router())
}
