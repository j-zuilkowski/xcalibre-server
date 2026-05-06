use crate::{
    auth::magic_link::{generate_token, hash_token},
    config::AppConfig,
    db::queries::{auth as auth_queries, magic_link as ml_queries},
    error::AppError,
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

fn feature_enabled(cfg: &AppConfig) -> Result<(), AppError> {
    if cfg.auth.magic_link.enabled {
        Ok(())
    } else {
        Err(AppError::NotFound)
    }
}

// ── POST /api/v1/auth/magic-link/request ─────────────────────────────────────

#[derive(Deserialize)]
pub struct MagicLinkRequest {
    pub email: String,
}

pub async fn request_magic_link(
    State(state): State<AppState>,
    Json(body): Json<MagicLinkRequest>,
) -> Result<StatusCode, AppError> {
    feature_enabled(&state.config)?;

    // Silently succeed for unknown emails — no user enumeration
    if let Ok(Some(user)) = auth_queries::find_user_by_email(&state.db, &body.email).await {
        let raw = generate_token();
        let hash = hash_token(&raw);
        let ttl = state.config.auth.magic_link.token_ttl_minutes as i64;
        let expires_at = (Utc::now() + chrono::Duration::minutes(ttl)).timestamp();

        ml_queries::insert_token(&state.db, &user.id, &hash, expires_at).await?;

        let link = format!(
            "{}/auth/magic-link/verify?token={}",
            state.config.app.base_url.trim_end_matches('/'),
            raw
        );
        tracing::debug!(to = %body.email, link = %link, "magic link generated");
    }

    Ok(StatusCode::ACCEPTED)
}

// ── GET /api/v1/auth/magic-link/verify?token=<raw> ───────────────────────────

#[derive(Deserialize)]
pub struct VerifyQuery {
    pub token: String,
}

#[derive(Serialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
}

pub async fn verify_magic_link(
    State(state): State<AppState>,
    Query(q): Query<VerifyQuery>,
) -> Result<Json<TokenResponse>, AppError> {
    feature_enabled(&state.config)?;

    let hash = hash_token(&q.token);
    let row = ml_queries::find_token(&state.db, &hash)
        .await?
        .ok_or(AppError::Unauthorized)?;

    // Reject if already used
    if row.used_at.is_some() {
        return Err(AppError::Unauthorized);
    }

    // Reject if expired
    if Utc::now().timestamp() > row.expires_at {
        return Err(AppError::Unauthorized);
    }

    // Mark used atomically — if another request races, the UPDATE affects 0 rows
    let updated = sqlx::query(
        "UPDATE magic_link_tokens SET used_at = ? WHERE id = ? AND used_at IS NULL",
    )
    .bind(Utc::now().timestamp())
    .bind(&row.id)
    .execute(&state.db)
    .await?
    .rows_affected();

    if updated == 0 {
        return Err(AppError::Unauthorized);
    }

    let user = auth_queries::find_user_by_id(&state.db, &row.user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    let access_token = crate::middleware::auth::issue_access_token(
        &user.id,
        &state.config.auth.jwt_secret,
        state.config.auth.access_token_ttl_mins,
    )?;
    let refresh_token = auth_queries::generate_refresh_token();
    auth_queries::insert_refresh_token(
        &state.db,
        &user.id,
        &refresh_token,
        state.config.auth.refresh_token_ttl_days,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(Json(TokenResponse {
        access_token,
        refresh_token,
    }))
}

// ── DELETE /api/v1/admin/magic-link/revoke/:user_id ──────────────────────────

pub async fn revoke_user_magic_links(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
) -> Result<StatusCode, AppError> {
    feature_enabled(&state.config)?;
    ml_queries::revoke_user_tokens(&state.db, &user_id).await?;
    Ok(StatusCode::NO_CONTENT)
}
