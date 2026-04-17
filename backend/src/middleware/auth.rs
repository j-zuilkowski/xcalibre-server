use crate::{db::queries::auth as auth_queries, AppError, AppState};
use axum::{
    extract::{Request, State},
    http::header::AUTHORIZATION,
    middleware::Next,
    response::Response,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    pub sub: String,
    pub iat: usize,
    pub exp: usize,
}

#[derive(Clone, Debug)]
pub struct AuthenticatedUser {
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
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = bearer_token(req.headers()).ok_or(AppError::Unauthorized)?;
    let claims = validate_access_token(token, &state.config.auth.jwt_secret)?;

    let user = auth_queries::find_user_by_id(&state.db, &claims.sub)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    if !user.is_active {
        return Err(AppError::Unauthorized);
    }

    req.extensions_mut().insert(AuthenticatedUser { user });
    Ok(next.run(req).await)
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
