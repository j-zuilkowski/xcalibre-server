//! Security headers, rate limiting, CORS, and upload-size enforcement.
//!
//! `apply_security_headers` injects five headers on every response:
//! - `X-Content-Type-Options: nosniff` — prevents MIME-type sniffing.
//! - `X-Frame-Options: DENY` — blocks framing by all origins.
//! - `Referrer-Policy: strict-origin-when-cross-origin` — limits referrer leakage.
//! - `Content-Security-Policy` — restricts resource loading to `'self'` plus
//!   `'unsafe-inline'` for script-src and style-src (required by epub.js and
//!   shadcn/ui CSS variables) and `data:/blob:` for cover images and epub.js
//!   web workers.
//! - `Permissions-Policy` — disables camera, microphone, and geolocation.
//!
//! Rate limiting uses `tower-governor` with a `ClientIpKeyExtractor` that
//! reads `X-Forwarded-For` (first address), then `X-Real-IP`, then the
//! direct socket address.  Separate layers are applied to auth endpoints
//! (`AUTH_RATE_LIMIT_PER_MINUTE = 10`) and the global API.
//!
//! `enforce_upload_size` checks `Content-Length` against `max_upload_bytes`
//! before the request body is read, avoiding unnecessary memory allocation.

use crate::AppError;
use axum::{
    extract::{Request as AxumRequest, State},
    http::{self, header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode},
    middleware::Next,
    response::Response,
};
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tower_governor::{
    governor::GovernorConfigBuilder, key_extractor::KeyExtractor, GovernorError, GovernorLayer,
};
use tower_http::cors::CorsLayer;

const X_CONTENT_TYPE_OPTIONS: &str = "x-content-type-options";
const X_FRAME_OPTIONS: &str = "x-frame-options";
const REFERRER_POLICY: &str = "referrer-policy";
const CONTENT_SECURITY_POLICY: &str = "content-security-policy";
const PERMISSIONS_POLICY: &str = "permissions-policy";

const X_CONTENT_TYPE_OPTIONS_VALUE: &str = "nosniff";
const X_FRAME_OPTIONS_VALUE: &str = "DENY";
const REFERRER_POLICY_VALUE: &str = "strict-origin-when-cross-origin";
const CONTENT_SECURITY_POLICY_VALUE: &str = concat!(
    "default-src 'self'; script-src 'self' 'unsafe-inline'; ",
    // shadcn/ui theme variables are applied through inline styles.
    "style-src 'self' 'unsafe-inline'; ",
    "img-src 'self' data: blob:; worker-src 'self' blob:"
);
const PERMISSIONS_POLICY_VALUE: &str = "camera=(), microphone=(), geolocation=()";

const AUTH_RATE_LIMIT_PER_MINUTE: u32 = 10;
const RATE_LIMIT_WINDOW_SECONDS: u64 = 60;
const UPLOAD_ROUTE: &str = "/api/v1/books";

#[derive(Clone, Copy, Debug)]
pub(crate) struct RateLimitHeaderConfig {
    limit: u32,
    window_seconds: u64,
}

impl RateLimitHeaderConfig {
    fn new(limit: u32, window_seconds: u64) -> Self {
        Self {
            limit: limit.max(1),
            window_seconds: window_seconds.max(1),
        }
    }
}

pub(crate) fn auth_rate_limit_layer(
) -> GovernorLayer<ClientIpKeyExtractor, governor::middleware::NoOpMiddleware> {
    governor_layer(AUTH_RATE_LIMIT_PER_MINUTE)
}

pub(crate) fn auth_rate_limit_headers_config() -> RateLimitHeaderConfig {
    RateLimitHeaderConfig::new(AUTH_RATE_LIMIT_PER_MINUTE, RATE_LIMIT_WINDOW_SECONDS)
}

pub(crate) fn global_rate_limit_layer(
    requests_per_minute: u32,
) -> GovernorLayer<ClientIpKeyExtractor, governor::middleware::NoOpMiddleware> {
    governor_layer(requests_per_minute.max(1))
}

pub(crate) fn global_rate_limit_headers_config(requests_per_minute: u32) -> RateLimitHeaderConfig {
    RateLimitHeaderConfig::new(requests_per_minute, RATE_LIMIT_WINDOW_SECONDS)
}

/// Builds a `CorsLayer` that allows the origin derived from `base_url`
/// (scheme + host + optional port).  Logs a warning and returns a
/// deny-all CORS layer when `base_url` cannot be parsed.
pub(crate) fn cors_layer(base_url: &str) -> CorsLayer {
    let base = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PATCH,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .max_age(Duration::from_secs(3_600));

    match cors_origin_from_base_url(base_url) {
        Ok(origin) => base.allow_origin(origin),
        Err(err) => {
            tracing::warn!(
                base_url = %base_url,
                error = %err,
                "invalid APP_BASE_URL for CORS origin; no origins will be allowed"
            );
            base
        }
    }
}

/// Appends all five mandatory security headers to every response.  This
/// middleware must be registered at the outermost router layer so it fires
/// for all routes, including error responses from inner middleware.
pub(crate) async fn apply_security_headers(request: AxumRequest, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    put_static_header(
        headers,
        X_CONTENT_TYPE_OPTIONS,
        X_CONTENT_TYPE_OPTIONS_VALUE,
    );
    put_static_header(headers, X_FRAME_OPTIONS, X_FRAME_OPTIONS_VALUE);
    put_static_header(headers, REFERRER_POLICY, REFERRER_POLICY_VALUE);
    put_static_header(
        headers,
        CONTENT_SECURITY_POLICY,
        CONTENT_SECURITY_POLICY_VALUE,
    );
    put_static_header(headers, PERMISSIONS_POLICY, PERMISSIONS_POLICY_VALUE);

    response
}

/// Rejects book upload requests whose `Content-Length` exceeds
/// `max_upload_bytes` before the body is streamed.  Non-upload routes and
/// requests without a `Content-Length` header pass through unconditionally.
pub(crate) async fn enforce_upload_size(
    State(max_upload_bytes): State<u64>,
    request: AxumRequest,
    next: Next,
) -> Result<Response, AppError> {
    if is_upload_request(&request) {
        if let Some(raw_content_length) = request.headers().get(header::CONTENT_LENGTH) {
            let content_length = raw_content_length
                .to_str()
                .ok()
                .and_then(|value| value.parse::<u64>().ok())
                .ok_or(AppError::BadRequest)?;

            if content_length > max_upload_bytes {
                return Err(AppError::PayloadTooLarge);
            }
        }
    }

    Ok(next.run(request).await)
}

/// Adds standard rate-limit response headers (`X-RateLimit-Limit`,
/// `X-RateLimit-Remaining`, `X-RateLimit-Reset`, `Retry-After` on 429).
/// Values are derived from `tower-governor` headers when present; defaults
/// fall back to the configured `limit` and `window_seconds`.
pub(crate) async fn apply_rate_limit_headers(
    State(config): State<RateLimitHeaderConfig>,
    request: AxumRequest,
    next: Next,
) -> Response {
    let mut response = next.run(request).await;
    let status = response.status();
    let headers = response.headers_mut();

    insert_header_if_absent(
        headers,
        "x-ratelimit-limit",
        HeaderValue::from(config.limit.max(1)),
    );

    if let Some(remaining) = headers.get("x-ratelimit-remaining").cloned() {
        insert_header_if_absent(headers, "x-ratelimit-remaining", remaining);
    }

    let retry_after_seconds = if status == StatusCode::TOO_MANY_REQUESTS {
        retry_after_from_headers(headers)
            .unwrap_or(config.window_seconds)
            .max(1)
    } else {
        0
    };

    if status == StatusCode::TOO_MANY_REQUESTS {
        insert_header_if_absent(
            headers,
            header::RETRY_AFTER.as_str(),
            HeaderValue::from(retry_after_seconds),
        );
    }

    let reset_in = if status == StatusCode::TOO_MANY_REQUESTS {
        retry_after_seconds
    } else {
        config.window_seconds
    };
    let reset_timestamp = unix_now().saturating_add(reset_in);
    insert_header_if_absent(
        headers,
        "x-ratelimit-reset",
        HeaderValue::from(reset_timestamp),
    );

    response
}

fn is_upload_request(request: &AxumRequest) -> bool {
    request.method() == Method::POST && request.uri().path() == UPLOAD_ROUTE
}

fn put_static_header(headers: &mut HeaderMap, name: &'static str, value: &'static str) {
    headers.insert(
        HeaderName::from_static(name),
        HeaderValue::from_static(value),
    );
}

fn insert_header_if_absent(headers: &mut HeaderMap, name: &'static str, value: HeaderValue) {
    let header_name = HeaderName::from_static(name);
    if !headers.contains_key(&header_name) {
        headers.insert(header_name, value);
    }
}

fn retry_after_from_headers(headers: &HeaderMap) -> Option<u64> {
    headers
        .get("x-ratelimit-after")
        .or_else(|| headers.get(header::RETRY_AFTER))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
}

fn unix_now() -> u64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs(),
        Err(_) => 0,
    }
}

fn cors_origin_from_base_url(base_url: &str) -> Result<HeaderValue, String> {
    let parsed = reqwest::Url::parse(base_url).map_err(|err| err.to_string())?;
    let scheme = parsed.scheme();
    if scheme != "http" && scheme != "https" {
        return Err(format!("unsupported scheme: {scheme}"));
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| "APP_BASE_URL has no host".to_string())?;
    let origin = if let Some(port) = parsed.port() {
        format!("{scheme}://{host}:{port}")
    } else {
        format!("{scheme}://{host}")
    };
    HeaderValue::from_str(&origin).map_err(|err| err.to_string())
}

fn governor_layer(
    requests_per_minute: u32,
) -> GovernorLayer<ClientIpKeyExtractor, governor::middleware::NoOpMiddleware> {
    let mut builder = GovernorConfigBuilder::default().key_extractor(ClientIpKeyExtractor);
    builder
        .per_millisecond(refill_period_millis(requests_per_minute))
        .burst_size(requests_per_minute);
    let config = builder
        .finish()
        .expect("governor config must use a non-zero period and burst size");

    GovernorLayer {
        config: Arc::new(config),
    }
}

fn refill_period_millis(requests_per_minute: u32) -> u64 {
    let rate = u64::from(requests_per_minute.max(1));
    60_000_u64.saturating_add(rate - 1) / rate
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ClientIpKeyExtractor;

impl KeyExtractor for ClientIpKeyExtractor {
    type Key = IpAddr;

    fn extract<T>(&self, request: &http::Request<T>) -> Result<Self::Key, GovernorError> {
        x_forwarded_for_ip(request.headers())
            .or_else(|| {
                if request.headers().contains_key("x-forwarded-for") {
                    direct_connection_ip(request)
                } else {
                    None
                }
            })
            .or_else(|| x_real_ip(request.headers()))
            .or_else(|| direct_connection_ip(request))
            .or(Some(IpAddr::V4(Ipv4Addr::LOCALHOST)))
            .ok_or(GovernorError::UnableToExtractKey)
    }
}

fn x_forwarded_for_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-forwarded-for")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
}

fn x_real_ip(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get("x-real-ip")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<IpAddr>().ok())
}

fn connect_info_ip<T>(request: &http::Request<T>) -> Option<IpAddr> {
    request
        .extensions()
        .get::<axum::extract::ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.ip())
}

fn socket_addr_ip<T>(request: &http::Request<T>) -> Option<IpAddr> {
    request
        .extensions()
        .get::<SocketAddr>()
        .map(std::net::SocketAddr::ip)
}

fn direct_connection_ip<T>(request: &http::Request<T>) -> Option<IpAddr> {
    connect_info_ip(request).or_else(|| socket_addr_ip(request))
}
