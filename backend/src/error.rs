use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("bad request")]
    BadRequest,
    #[error("unauthorized")]
    Unauthorized,
    #[error("forbidden")]
    Forbidden,
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("unprocessable")]
    Unprocessable,
    #[error("no extractable format")]
    NoExtractableFormat,
    #[error("service unavailable")]
    ServiceUnavailable,
    #[error("internal error")]
    Internal,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, error) = match self {
            AppError::BadRequest => (StatusCode::BAD_REQUEST, "bad_request"),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            AppError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            AppError::Conflict => (StatusCode::CONFLICT, "conflict"),
            AppError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, "payload_too_large"),
            AppError::Unprocessable => (StatusCode::UNPROCESSABLE_ENTITY, "unprocessable"),
            AppError::NoExtractableFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, "no_extractable_format")
            }
            AppError::ServiceUnavailable => (StatusCode::SERVICE_UNAVAILABLE, "llm_unavailable"),
            AppError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        let body = Json(json!({
            "error": error,
            "message": self.to_string(),
        }));
        (status, body).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(_: sqlx::Error) -> Self {
        AppError::Internal
    }
}
