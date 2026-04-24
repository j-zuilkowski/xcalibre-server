use crate::{
    db::queries::{api_tokens as api_token_queries, auth as auth_queries},
    AppError, AppState,
};
use axum::{
    extract::{ConnectInfo, Request, State},
    http::{header::AUTHORIZATION, HeaderName},
    middleware::Next,
    response::Response,
};
use chrono::{Duration, Utc};
use ipnet::IpNet;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::net::{IpAddr, SocketAddr};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: String,
    pub iat: usize,
    pub exp: usize,
    #[serde(default)]
    pub totp_pending: bool,
}

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
    pub user: crate::db::models::User,
}

#[derive(Clone, Debug)]
pub struct TotpPendingUser {
    pub user: crate::db::models::User,
}

pub fn issue_access_token(
    user_id: &str,
    jwt_secret: &str,
    ttl_mins: u64,
) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = AccessTokenClaims {
        sub: user_id.to_string(),
        iat: now.timestamp() as usize,
        exp: (now + Duration::minutes(ttl_mins as i64)).timestamp() as usize,
        totp_pending: false,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|_| AppError::Internal)
}

pub fn validate_access_token(token: &str, jwt_secret: &str) -> Result<AccessTokenClaims, AppError> {
    decode::<AccessTokenClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|token_data| token_data.claims)
    .map_err(|_| AppError::Unauthorized)
    .and_then(|claims| {
        if claims.totp_pending {
            Err(AppError::Forbidden)
        } else {
            Ok(claims)
        }
    })
}

pub fn issue_totp_pending_token(user_id: &str, jwt_secret: &str) -> Result<String, AppError> {
    let now = Utc::now();
    let claims = AccessTokenClaims {
        sub: user_id.to_string(),
        iat: now.timestamp() as usize,
        exp: (now + Duration::minutes(5)).timestamp() as usize,
        totp_pending: true,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(jwt_secret.as_bytes()),
    )
    .map_err(|_| AppError::Internal)
}

pub fn validate_totp_pending_token(
    token: &str,
    jwt_secret: &str,
) -> Result<AccessTokenClaims, AppError> {
    let claims = decode::<AccessTokenClaims>(
        token,
        &DecodingKey::from_secret(jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|token_data| token_data.claims)
    .map_err(|_| AppError::Unauthorized)?;

    if claims.totp_pending {
        Ok(claims)
    } else {
        Err(AppError::Forbidden)
    }
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let remote_ip = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|connect_info| connect_info.0.ip())
        .or_else(|| req.extensions().get::<SocketAddr>().map(SocketAddr::ip));

    if let Some(user) = authenticate_proxy_user(&state, req.headers(), remote_ip).await? {
        if !user.is_active {
            return Err(AppError::Unauthorized);
        }
        req.extensions_mut().insert(AuthenticatedUser { user });
        return Ok(next.run(req).await);
    }

    let token = bearer_token(req.headers()).ok_or(AppError::Unauthorized)?;
    let user = match validate_access_token(token, &state.config.auth.jwt_secret) {
        Ok(claims) => auth_queries::find_user_by_id(&state.db, &claims.sub)
            .await
            .map_err(|_| AppError::Internal)?,
        Err(AppError::Unauthorized) => authenticate_api_token(&state, token).await?,
        Err(err) => return Err(err),
    }
    .ok_or(AppError::Unauthorized)?;

    if !user.is_active {
        return Err(AppError::Unauthorized);
    }

    req.extensions_mut().insert(AuthenticatedUser { user });
    Ok(next.run(req).await)
}

pub async fn require_totp_pending(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = bearer_token(req.headers()).ok_or(AppError::Unauthorized)?;
    let claims = validate_totp_pending_token(token, &state.config.auth.jwt_secret)?;
    let user = auth_queries::find_user_by_id(&state.db, &claims.sub)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !user.is_active {
        return Err(AppError::Unauthorized);
    }

    req.extensions_mut().insert(TotpPendingUser { user });
    Ok(next.run(req).await)
}

async fn authenticate_proxy_user(
    state: &AppState,
    headers: &axum::http::HeaderMap,
    remote_ip: Option<IpAddr>,
) -> Result<Option<crate::db::models::User>, AppError> {
    if !state.config.auth.proxy.enabled {
        return Ok(None);
    }

    let trusted = remote_ip
        .map(|ip| is_trusted_proxy(ip, &state.config.auth.proxy.trusted_cidrs))
        .unwrap_or(false);
    if !trusted {
        return Ok(None);
    }

    let Some(header_name) = proxy_header_name(&state.config.auth.proxy.header) else {
        return Ok(None);
    };
    let Some(username_raw) = headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(None);
    };
    let username = username_raw.trim();
    if username.is_empty() {
        return Ok(None);
    }

    if let Some(user) = auth_queries::find_user_by_username(&state.db, username)
        .await
        .map_err(|_| AppError::Internal)?
    {
        return Ok(Some(user));
    }

    let email = proxy_email(headers, &state.config.auth.proxy.email_header);
    let password = generate_random_password();
    let password_hash = crate::auth::password::hash_password(&password, &state.config.auth)?;
    let user = auth_queries::create_user(&state.db, username, &email, "user", &password_hash)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(Some(user))
}

async fn authenticate_api_token(
    state: &AppState,
    token: &str,
) -> Result<Option<crate::db::models::User>, AppError> {
    let token_hash = hex_sha256(token);
    let Some(api_token) = api_token_queries::find_by_hash(&state.db, &token_hash)
        .await
        .map_err(|_| AppError::Internal)?
    else {
        return Ok(None);
    };

    api_token_queries::touch_last_used(&state.db, &api_token.id)
        .await
        .map_err(|_| AppError::Internal)?;

    let user = auth_queries::find_user_by_id(&state.db, &api_token.created_by)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok(user)
}

fn bearer_token(headers: &axum::http::HeaderMap) -> Option<&str> {
    let value = headers.get(AUTHORIZATION)?.to_str().ok()?;
    let (prefix, token) = value.split_once(' ')?;
    if prefix.eq_ignore_ascii_case("bearer") && !token.trim().is_empty() {
        Some(token.trim())
    } else {
        None
    }
}

fn proxy_header_name(header: &str) -> Option<HeaderName> {
    let trimmed = header.trim();
    if trimmed.is_empty() {
        return None;
    }

    match HeaderName::from_bytes(trimmed.as_bytes()) {
        Ok(name) => Some(name),
        Err(_) => {
            tracing::warn!(header = %trimmed, "invalid proxy auth header name");
            None
        }
    }
}

fn proxy_email(headers: &axum::http::HeaderMap, header_name: &str) -> String {
    let Some(header_name) = proxy_header_name(header_name) else {
        return String::new();
    };
    headers
        .get(header_name)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.trim().to_string())
        .unwrap_or_default()
}

pub fn is_trusted_proxy(remote_ip: IpAddr, trusted_cidrs: &[String]) -> bool {
    if trusted_cidrs.is_empty() {
        return true;
    }

    trusted_cidrs.iter().any(|cidr| {
        cidr.parse::<IpNet>()
            .map(|net| net.contains(&remote_ip))
            .unwrap_or(false)
    })
}

fn generate_random_password() -> String {
    let suffix = Uuid::new_v4().simple().to_string();
    format!("proxy-{suffix}")
}

fn hex_sha256(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    hex::encode(digest)
}
