use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::json;
use thiserror::Error;
use utoipa::ToSchema;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("bad request")]
    BadRequest,
    #[error("{0}")]
    BadRequestMessage(String),
    #[error("unauthorized")]
    Unauthorized,
    #[error("{0}")]
    UnauthorizedMessage(String),
    #[error("{0}")]
    Forbidden(String),
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("{0}")]
    ConflictMessage(String),
    #[error("payload too large")]
    PayloadTooLarge,
    #[error("unprocessable")]
    Unprocessable,
    #[error("{0}")]
    UnprocessableMessage(String),
    #[error("{0}")]
    UnprocessableEntity(String),
    #[error("no extractable format")]
    NoExtractableFormat,
    #[error("not implemented")]
    NotImplemented,
    #[error("service unavailable")]
    ServiceUnavailable,
    #[error("webhook url must be a public endpoint")]
    SsrfBlocked,
    #[error("too many requests")]
    TooManyRequests,
    #[error("internal error")]
    Internal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AppErrorResponse {
    pub error: String,
    pub message: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::BadRequest => (StatusCode::BAD_REQUEST, Json(json!({"error": "bad_request"}))).into_response(),
            AppError::BadRequestMessage(msg) => (StatusCode::BAD_REQUEST, Json(json!({"error": msg}))).into_response(),
            AppError::Unauthorized | AppError::UnauthorizedMessage(_) => {
                (StatusCode::UNAUTHORIZED, Json(json!({"error": "unauthorized"}))).into_response()
            }
            AppError::Forbidden(msg) => {
                (StatusCode::FORBIDDEN, Json(json!({"error": "forbidden", "message": msg}))).into_response()
            }
            AppError::NotFound => (StatusCode::NOT_FOUND, Json(json!({"error": "not_found"}))).into_response(),
            AppError::Conflict => (StatusCode::CONFLICT, Json(json!({"error": "conflict"}))).into_response(),
            AppError::ConflictMessage(msg) => {
                (StatusCode::CONFLICT, Json(json!({"error": msg}))).into_response()
            }
            AppError::PayloadTooLarge => {
                (StatusCode::PAYLOAD_TOO_LARGE, Json(json!({"error": "payload_too_large"}))).into_response()
            }
            AppError::Unprocessable | AppError::UnprocessableMessage(_) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error": "unprocessable"}))).into_response()
            }
            AppError::UnprocessableEntity(msg) => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error": msg, "checksum_ok": false}))).into_response()
            }
            AppError::NoExtractableFormat => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error": "no_extractable_format"}))).into_response()
            }
            AppError::NotImplemented => {
                (StatusCode::NOT_IMPLEMENTED, Json(json!({"error": "not_implemented"}))).into_response()
            }
            AppError::ServiceUnavailable => {
                (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"error": "llm_unavailable"}))).into_response()
            }
            AppError::SsrfBlocked => {
                (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({"error": "ssrf_blocked"}))).into_response()
            }
            AppError::TooManyRequests => {
                (StatusCode::TOO_MANY_REQUESTS, Json(json!({"error": "too_many_requests"}))).into_response()
            }
            AppError::Internal => {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal_error"}))).into_response()
            }
        }
    }
}

impl From<sqlx::Error> for AppError {
    fn from(_: sqlx::Error) -> Self {
        AppError::Internal
    }
}
