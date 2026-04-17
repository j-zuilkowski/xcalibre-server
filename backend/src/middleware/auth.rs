use axum::{body::Body, http::Request, middleware::Next, response::Response};

pub async fn pass_through(req: Request<Body>, next: Next) -> Response {
    next.run(req).await
}
