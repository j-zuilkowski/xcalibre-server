use crate::{
    db::queries::{api_tokens as api_token_queries, auth as auth_queries, kobo as kobo_queries},
    AppError, AppState,
};
use axum::{
    extract::{OriginalUri, Request, State},
    http::HeaderName,
    middleware::Next,
    response::Response,
};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug)]
pub struct KoboAuthContext {
    pub user: crate::db::models::User,
    pub api_token: crate::db::queries::api_tokens::ApiToken,
    pub kobo_token: String,
    pub device: Option<crate::db::models::KoboDevice>,
}

pub async fn kobo_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let original_path = req
        .extensions()
        .get::<OriginalUri>()
        .map(|uri| uri.0.path().to_string())
        .unwrap_or_else(|| req.uri().path().to_string());
    let token = extract_kobo_token(original_path.as_str())
        .ok_or(AppError::Unauthorized)?
        .to_string();
    let token_hash = hash_token(&token);
    let api_token = api_token_queries::find_by_hash(&state.db, &token_hash)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    let user = auth_queries::find_user_by_id(&state.db, &api_token.created_by)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !user.is_active {
        return Err(AppError::Unauthorized);
    }

    let device = if let Some(device_id) = req
        .headers()
        .get(HeaderName::from_static("x-kobo-deviceid"))
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.trim().is_empty())
    {
        let device = kobo_queries::find_device_by_device_id(&state.db, device_id)
            .await
            .map_err(|_| AppError::Internal)?;
        if let Some(device) = device.as_ref() {
            if device.user_id != user.id {
                return Err(AppError::Unauthorized);
            }
        }
        device
    } else {
        None
    };

    req.extensions_mut().insert(KoboAuthContext {
        user,
        api_token,
        kobo_token: token,
        device,
    });

    Ok(next.run(req).await)
}

fn extract_kobo_token(path: &str) -> Option<&str> {
    let mut segments = path.trim_start_matches('/').split('/');
    match (segments.next(), segments.next(), segments.next()) {
        (Some("kobo"), Some(token), Some("v1")) if !token.is_empty() => Some(token),
        _ => None,
    }
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    hex::encode(digest)
}
