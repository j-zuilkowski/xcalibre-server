use crate::{
    config::AuthSection,
    db::queries::auth as auth_queries,
    middleware::auth::{issue_access_token, AuthenticatedUser},
    AppError, AppState,
};
use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use axum::{
    extract::{Extension, State},
    http::{
        header::{HeaderName, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    middleware,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    let protected = Router::new()
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/me/password", patch(change_password))
        .route_layer(auth_layer);

    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .merge(protected)
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    username: String,
    email: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize)]
struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    access_token: String,
    refresh_token: String,
    user: crate::db::models::User,
}

#[derive(Debug, Serialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Serialize)]
struct SuccessResponse {
    success: bool,
}

async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<crate::db::models::User>), AppError> {
    validate_registration(&payload)?;

    let user_count = auth_queries::count_users(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    if user_count > 0 {
        return Err(AppError::Conflict);
    }

    let password_hash = hash_password(&payload.password, &state.config.auth)?;
    let user = auth_queries::create_first_admin_user(
        &state.db,
        payload.username.trim(),
        payload.email.trim(),
        &password_hash,
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok((StatusCode::CREATED, Json(user)))
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<(HeaderMap, Json<LoginResponse>), AppError> {
    let username = payload.username.trim();
    let client_ip = client_ip_from_headers(&headers);

    if payload.username.trim().is_empty() || payload.password.is_empty() {
        return Err(AppError::BadRequest);
    }

    let mut user = auth_queries::find_user_auth_by_username(&state.db, username)
        .await
        .map_err(|_| AppError::Internal)?;
    let Some(mut user) = user.take() else {
        record_login_failure(&state, None, username, "invalid_credentials", client_ip.as_deref())
            .await;
        return Err(AppError::Unauthorized);
    };

    if !user.user.is_active {
        record_login_failure(
            &state,
            Some(&user.user.id),
            username,
            "inactive_user",
            client_ip.as_deref(),
        )
        .await;
        return Err(AppError::Unauthorized);
    }

    let now = Utc::now();
    if let Some(locked_until) = user.locked_until {
        if locked_until > now {
            record_login_failure(
                &state,
                Some(&user.user.id),
                username,
                "account_locked",
                client_ip.as_deref(),
            )
            .await;
            return Err(AppError::Unauthorized);
        }
        auth_queries::clear_login_lockout(&state.db, &user.user.id)
            .await
            .map_err(|_| AppError::Internal)?;
        user = auth_queries::find_user_auth_by_id(&state.db, &user.user.id)
            .await
            .map_err(|_| AppError::Internal)?
            .ok_or(AppError::Unauthorized)?;
    }

    if !verify_password(&user.password_hash, &payload.password) {
        auth_queries::mark_failed_login(
            &state.db,
            &user,
            state.config.auth.max_login_attempts,
            state.config.auth.lockout_duration_mins,
        )
        .await
        .map_err(|_| AppError::Internal)?;
        record_login_failure(
            &state,
            Some(&user.user.id),
            username,
            "invalid_credentials",
            client_ip.as_deref(),
        )
        .await;
        return Err(AppError::Unauthorized);
    }

    auth_queries::clear_login_lockout(&state.db, &user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;

    let response = create_login_response(&state, &user.user).await?;
    record_login_success(&state, &user.user.id, username, client_ip.as_deref()).await;

    Ok((
        refresh_cookie_headers(
            &state.config.app.base_url,
            &response.refresh_token,
            state.config.auth.refresh_token_ttl_days,
        )?,
        Json(response),
    ))
}

async fn logout(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<RefreshRequest>,
) -> Result<(HeaderMap, Json<SuccessResponse>), AppError> {
    if payload.refresh_token.trim().is_empty() {
        return Err(AppError::BadRequest);
    }

    let token = auth_queries::find_refresh_token(&state.db, &payload.refresh_token)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    if token.user_id != auth_user.user.id {
        return Err(AppError::Unauthorized);
    }

    auth_queries::revoke_refresh_token_by_id(&state.db, &token.id)
        .await
        .map_err(|_| AppError::Internal)?;

    Ok((
        clear_refresh_cookie_headers(&state.config.app.base_url)?,
        Json(SuccessResponse { success: true }),
    ))
}

async fn refresh(
    State(state): State<AppState>,
    Json(payload): Json<RefreshRequest>,
) -> Result<(HeaderMap, Json<RefreshResponse>), AppError> {
    if payload.refresh_token.trim().is_empty() {
        return Err(AppError::Unauthorized);
    }

    let token = auth_queries::find_refresh_token(&state.db, &payload.refresh_token)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    if token.revoked_at.is_some() || token.expires_at <= Utc::now() {
        return Err(AppError::Unauthorized);
    }

    let user = auth_queries::find_user_by_id(&state.db, &token.user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !user.is_active {
        return Err(AppError::Unauthorized);
    }

    auth_queries::revoke_refresh_token_by_id(&state.db, &token.id)
        .await
        .map_err(|_| AppError::Internal)?;

    let access_token = issue_access_token(
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

    let response = RefreshResponse {
        access_token,
        refresh_token,
    };

    Ok((
        refresh_cookie_headers(
            &state.config.app.base_url,
            &response.refresh_token,
            state.config.auth.refresh_token_ttl_days,
        )?,
        Json(response),
    ))
}

async fn me(
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<crate::db::models::User>, AppError> {
    Ok(Json(auth_user.user))
}

async fn change_password(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<ChangePasswordRequest>,
) -> Result<Json<SuccessResponse>, AppError> {
    if payload.new_password.trim().is_empty() {
        return Err(AppError::BadRequest);
    }

    let user = auth_queries::find_user_auth_by_id(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    if !verify_password(&user.password_hash, &payload.current_password) {
        return Err(AppError::BadRequest);
    }

    let new_hash = hash_password(&payload.new_password, &state.config.auth)?;
    auth_queries::update_password_hash(&state.db, &auth_user.user.id, &new_hash)
        .await
        .map_err(|_| AppError::Internal)?;
    if let Err(err) = auth_queries::audit_password_change(&state.db, &auth_user.user.id).await {
        tracing::warn!(error = %err, user_id = %auth_user.user.id, "failed to write password-change audit log");
    }

    Ok(Json(SuccessResponse { success: true }))
}

async fn create_login_response(
    state: &AppState,
    user: &crate::db::models::User,
) -> Result<LoginResponse, AppError> {
    let access_token = issue_access_token(
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

    Ok(LoginResponse {
        access_token,
        refresh_token,
        user: user.clone(),
    })
}

fn hash_password(password: &str, auth_config: &AuthSection) -> Result<String, AppError> {
    if password.trim().is_empty() {
        return Err(AppError::BadRequest);
    }
    let salt = SaltString::generate(&mut OsRng);
    let params = Params::new(
        auth_config.argon2_memory_kib,
        auth_config.argon2_iterations,
        auth_config.argon2_parallelism,
        None,
    )
    .map_err(|_| AppError::Internal)?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AppError::Internal)
}

fn verify_password(password_hash: &str, candidate: &str) -> bool {
    if candidate.is_empty() {
        return false;
    }

    let Ok(parsed_hash) = PasswordHash::new(password_hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(candidate.as_bytes(), &parsed_hash)
        .is_ok()
}

fn validate_registration(payload: &RegisterRequest) -> Result<(), AppError> {
    if payload.username.trim().is_empty()
        || payload.email.trim().is_empty()
        || payload.password.is_empty()
    {
        return Err(AppError::BadRequest);
    }
    if !payload.email.contains('@') {
        return Err(AppError::BadRequest);
    }
    Ok(())
}

fn refresh_cookie_headers(
    base_url: &str,
    refresh_token: &str,
    refresh_ttl_days: u64,
) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    let secure = base_url
        .trim()
        .to_ascii_lowercase()
        .starts_with("https://");
    let max_age = refresh_ttl_days.saturating_mul(24 * 60 * 60);

    let mut cookie = format!(
        "refresh_token={refresh_token}; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age={max_age}"
    );
    if secure {
        cookie.push_str("; Secure");
    }

    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| AppError::Internal)?,
    );
    Ok(headers)
}

fn clear_refresh_cookie_headers(base_url: &str) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    let secure = base_url
        .trim()
        .to_ascii_lowercase()
        .starts_with("https://");

    let mut cookie =
        "refresh_token=; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age=0".to_string();
    if secure {
        cookie.push_str("; Secure");
    }

    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| AppError::Internal)?,
    );
    Ok(headers)
}

fn client_ip_from_headers(headers: &HeaderMap) -> Option<String> {
    headers
        .get(HeaderName::from_static("x-forwarded-for"))
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(',').next())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| {
            headers
                .get(HeaderName::from_static("x-real-ip"))
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
}

async fn record_login_success(
    state: &AppState,
    user_id: &str,
    username: &str,
    client_ip: Option<&str>,
) {
    if let Err(err) = auth_queries::audit_login_success(&state.db, user_id, username, client_ip).await
    {
        tracing::warn!(error = %err, user_id = %user_id, "failed to write login-success audit log");
    }
}

async fn record_login_failure(
    state: &AppState,
    user_id: Option<&str>,
    username: &str,
    reason: &str,
    client_ip: Option<&str>,
) {
    if let Err(err) =
        auth_queries::audit_login_failure(&state.db, user_id, username, reason, client_ip).await
    {
        tracing::warn!(error = %err, username = %username, "failed to write login-failure audit log");
    }
}
