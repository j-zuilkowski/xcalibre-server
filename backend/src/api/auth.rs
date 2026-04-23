use crate::{
    auth::{ldap::authenticate_ldap, password::hash_password, totp as totp_auth},
    db::queries::{auth as auth_queries, oauth as oauth_queries, totp as totp_queries},
    middleware::auth::{
        issue_access_token, issue_totp_pending_token, AuthenticatedUser, TotpPendingUser,
    },
    AppError, AppState,
};
use argon2::password_hash::{PasswordHash, PasswordVerifier};
use argon2::Argon2;
use axum::{
    body::Body,
    extract::{Extension, Query, State},
    http::{
        header::{HeaderName, LOCATION, SET_COOKIE},
        HeaderMap, HeaderValue, StatusCode,
    },
    middleware,
    response::Response,
    routing::{get, patch, post},
    Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::time::Duration;
use utoipa::ToSchema;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    let totp_pending_layer = middleware::from_fn_with_state(
        state.clone(),
        crate::middleware::auth::require_totp_pending,
    );
    let public = Router::new()
        .route("/providers", get(auth_providers))
        .route("/oauth/:provider", get(oauth_start))
        .route("/oauth/:provider/callback", get(oauth_callback))
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .layer(crate::middleware::security_headers::auth_rate_limit_layer())
        .layer(middleware::from_fn_with_state(
            crate::middleware::security_headers::auth_rate_limit_headers_config(),
            crate::middleware::security_headers::apply_rate_limit_headers,
        ));
    let protected = Router::new()
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/me/password", patch(change_password))
        .route("/totp/setup", get(totp_setup))
        .route("/totp/confirm", post(totp_confirm))
        .route("/totp/disable", post(totp_disable))
        .route_layer(auth_layer);
    let totp_pending = Router::new()
        .route("/totp/verify", post(totp_verify))
        .route("/totp/verify-backup", post(totp_verify_backup))
        .layer(totp_pending_layer);

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(totp_pending)
}

#[derive(Debug, Deserialize)]
struct RegisterRequest {
    username: String,
    email: String,
    password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct RefreshRequest {
    refresh_token: String,
}

#[derive(Debug, Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

#[derive(Debug, Deserialize)]
struct OAuthCallbackQuery {
    code: String,
    state: String,
}

#[derive(Debug, Serialize)]
struct AuthProvidersResponse {
    google: bool,
    github: bool,
}

#[derive(Clone, Debug)]
struct ProviderSettings {
    client_id: String,
    client_secret: String,
    authorization_url: String,
    token_url: String,
    userinfo_url: String,
    email_url: String,
    scope: String,
}

#[derive(Debug, Deserialize)]
struct OAuthTokenResponse {
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct GoogleUserInfo {
    sub: String,
    email: String,
}

#[derive(Debug, Deserialize)]
struct GithubUserInfo {
    id: u64,
    login: String,
    email: Option<String>,
}

#[derive(Debug, Deserialize)]
struct GithubEmailRecord {
    email: String,
    primary: bool,
    verified: bool,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Serialize, ToSchema)]
#[serde(untagged)]
pub(crate) enum LoginResponse {
    Session(LoginSessionResponse),
    TotpRequired(LoginTotpRequiredResponse),
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct LoginSessionResponse {
    access_token: String,
    refresh_token: String,
    user: crate::db::models::User,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct LoginTotpRequiredResponse {
    totp_required: bool,
    totp_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RefreshResponse {
    access_token: String,
    refresh_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct SuccessResponse {
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

#[utoipa::path(
    post,
    path = "/api/v1/auth/login",
    tag = "auth",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login response", body = LoginResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn login(
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
    let local_user = user.take();
    let mut failed_local_user: Option<auth_queries::UserAuthRecord> = None;

    if let Some(mut user) = local_user {
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

        if verify_password(&user.password_hash, &payload.password) {
            if user.user.totp_enabled {
                let pending_token =
                    issue_totp_pending_token(&user.user.id, &state.config.auth.jwt_secret)?;
                return Ok((
                    HeaderMap::new(),
                    Json(LoginResponse::TotpRequired(LoginTotpRequiredResponse {
                        totp_required: true,
                        totp_token: pending_token,
                    })),
                ));
            }

            auth_queries::clear_login_lockout(&state.db, &user.user.id)
                .await
                .map_err(|_| AppError::Internal)?;

            let response = create_login_session_response(&state, &user.user).await?;
            record_login_success(&state, &user.user.id, username, client_ip.as_deref()).await;

            return Ok((
                refresh_cookie_headers(
                    &state.config.app.base_url,
                    &response.refresh_token,
                    state.config.auth.refresh_token_ttl_days,
                )?,
                Json(LoginResponse::Session(response)),
            ));
        }

        failed_local_user = Some(user.clone());
    }

    if state.config.ldap.enabled {
        match authenticate_ldap(&state.config, username, &payload.password).await {
            Ok(Some(ldap_user)) => {
                let user =
                    find_or_create_ldap_user(&state, &ldap_user.username, &ldap_user.email).await?;
                if user.totp_enabled {
                    let pending_token =
                        issue_totp_pending_token(&user.id, &state.config.auth.jwt_secret)?;
                    return Ok((
                        HeaderMap::new(),
                        Json(LoginResponse::TotpRequired(LoginTotpRequiredResponse {
                            totp_required: true,
                            totp_token: pending_token,
                        })),
                    ));
                }

                auth_queries::clear_login_lockout(&state.db, &user.id)
                    .await
                    .map_err(|_| AppError::Internal)?;
                let response = create_login_session_response(&state, &user).await?;
                record_login_success(&state, &user.id, username, client_ip.as_deref()).await;
                return Ok((
                    refresh_cookie_headers(
                        &state.config.app.base_url,
                        &response.refresh_token,
                        state.config.auth.refresh_token_ttl_days,
                    )?,
                    Json(LoginResponse::Session(response)),
                ));
            }
            Ok(None) => {
                let audit_user_id = failed_local_user.as_ref().map(|user| user.user.id.as_str());
                record_login_failure(
                    &state,
                    audit_user_id,
                    username,
                    "invalid_credentials",
                    client_ip.as_deref(),
                )
                .await;
                return Err(AppError::Unauthorized);
            }
            Err(err) => {
                tracing::warn!(error = %err, username = %username, "ldap authentication failed");
                let audit_user_id = failed_local_user.as_ref().map(|user| user.user.id.as_str());
                record_login_failure(
                    &state,
                    audit_user_id,
                    username,
                    "invalid_credentials",
                    client_ip.as_deref(),
                )
                .await;
                return Err(AppError::ServiceUnavailable);
            }
        }
    }

    if let Some(user) = failed_local_user.as_ref() {
        auth_queries::mark_failed_login(
            &state.db,
            user,
            state.config.auth.max_login_attempts,
            state.config.auth.lockout_duration_mins,
        )
        .await
        .map_err(|_| AppError::Internal)?;
    }

    record_login_failure(
        &state,
        failed_local_user.as_ref().map(|user| user.user.id.as_str()),
        username,
        "invalid_credentials",
        client_ip.as_deref(),
    )
    .await;
    Err(AppError::Unauthorized)
}

async fn auth_providers(
    State(state): State<AppState>,
) -> Result<Json<AuthProvidersResponse>, AppError> {
    Ok(Json(AuthProvidersResponse {
        google: provider_config(&state.config, "google").is_ok(),
        github: provider_config(&state.config, "github").is_ok(),
    }))
}

async fn oauth_start(
    State(state): State<AppState>,
    axum::extract::Path(provider): axum::extract::Path<String>,
) -> Result<Response, AppError> {
    let provider_config = provider_config(&state.config, &provider)?;
    let state_token = generate_oauth_state();
    let redirect_uri = oauth_redirect_uri(&state.config.app.base_url, &provider);
    let mut url =
        reqwest::Url::parse(&provider_config.authorization_url).map_err(|_| AppError::Internal)?;
    url.query_pairs_mut()
        .append_pair("client_id", &provider_config.client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("response_type", "code")
        .append_pair("scope", &provider_config.scope)
        .append_pair("state", &state_token);

    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    response.headers_mut().insert(
        LOCATION,
        HeaderValue::from_str(url.as_str()).map_err(|_| AppError::Internal)?,
    );
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&oauth_state_cookie(&provider, &state_token))
            .map_err(|_| AppError::Internal)?,
    );
    Ok(response)
}

async fn oauth_callback(
    State(state): State<AppState>,
    axum::extract::Path(provider): axum::extract::Path<String>,
    Query(query): Query<OAuthCallbackQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let provider_config = provider_config(&state.config, &provider)?;
    let cookie_state = read_cookie(&headers, "oauth_state").ok_or(AppError::BadRequest)?;
    if cookie_state != query.state {
        return Err(AppError::BadRequest);
    }

    let token = exchange_oauth_code(
        &provider_config,
        &state.config.app.base_url,
        &provider,
        &query.code,
    )
    .await?;
    let external_user = fetch_oauth_user(&provider_config, &token, &provider).await?;

    // Check for an existing OAuth account first. If found, use that user directly.
    // We never silently link a new OAuth login to an existing local account by email —
    // that would allow account takeover if an attacker controls an OAuth provider.
    let user = if let Some(existing_account) =
        oauth_queries::find_by_provider(&state.db, &provider, &external_user.provider_user_id)
            .await
            .map_err(|_| AppError::Internal)?
    {
        auth_queries::find_user_by_id(&state.db, &existing_account.user_id)
            .await
            .map_err(|_| AppError::Internal)?
            .ok_or(AppError::Internal)?
    } else {
        let new_user =
            create_oauth_user(&state, &external_user.username, &external_user.email).await?;
        oauth_queries::create_oauth_account(
            &state.db,
            &new_user.id,
            &provider,
            &external_user.provider_user_id,
            &external_user.email,
        )
        .await
        .map_err(|_| AppError::Internal)?;
        new_user
    };

    // Issue only a refresh token — access tokens are short-lived and the SPA
    // will call /auth/refresh after the redirect to obtain one.
    let refresh_token = auth_queries::generate_refresh_token();
    auth_queries::insert_refresh_token(
        &state.db,
        &user.id,
        &refresh_token,
        state.config.auth.refresh_token_ttl_days,
    )
    .await
    .map_err(|_| AppError::Internal)?;
    record_login_success(&state, &user.id, &user.username, None).await;
    let mut response = Response::new(Body::empty());
    *response.status_mut() = StatusCode::FOUND;
    response
        .headers_mut()
        .insert(LOCATION, HeaderValue::from_static("/"));
    let refresh_cookie = refresh_cookie_value(
        &state.config.app.base_url,
        &refresh_token,
        state.config.auth.refresh_token_ttl_days,
    );
    response.headers_mut().insert(
        SET_COOKIE,
        HeaderValue::from_str(&refresh_cookie).map_err(|_| AppError::Internal)?,
    );
    response.headers_mut().append(
        SET_COOKIE,
        HeaderValue::from_str(&clear_oauth_state_cookie(&provider))
            .map_err(|_| AppError::Internal)?,
    );
    Ok(response)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/logout",
    tag = "auth",
    security(("bearer_auth" = [])),
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Logout result", body = SuccessResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn logout(
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

#[utoipa::path(
    post,
    path = "/api/v1/auth/refresh",
    tag = "auth",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Refreshed tokens", body = RefreshResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn refresh(
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

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct TotpCodeRequest {
    code: String,
}

#[derive(Debug, Deserialize)]
struct TotpPasswordRequest {
    password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct TotpSetupResponse {
    secret_base32: String,
    otpauth_uri: String,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, ToSchema)]
struct TotpBackupCodesResponse {
    backup_codes: Vec<String>,
}

#[allow(dead_code)]
#[derive(Debug, Serialize, ToSchema)]
struct TotpVerifyErrorResponse {
    error: String,
}

#[utoipa::path(
    get,
    path = "/api/v1/auth/totp/setup",
    tag = "auth",
    security(("bearer_auth" = [])),
    responses(
        (status = 200, description = "TOTP setup payload", body = TotpSetupResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn totp_setup(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<Json<TotpSetupResponse>, AppError> {
    if auth_user.user.totp_enabled {
        return Err(AppError::Conflict);
    }

    let user = auth_queries::find_user_auth_by_id(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if user.totp_enabled {
        return Err(AppError::Conflict);
    }

    let secret_base32 = totp_auth::generate_secret_base32();
    let encrypted_secret =
        totp_auth::encrypt_secret(&secret_base32, &state.config.auth.jwt_secret)?;
    totp_queries::set_totp_setup_secret(&state.db, &auth_user.user.id, &encrypted_secret)
        .await
        .map_err(|_| AppError::Internal)?;

    let issuer = issuer_name(&state.config);
    let otpauth_uri = totp_auth::build_otpauth_uri(&issuer, &auth_user.user.email, &secret_base32);

    Ok(Json(TotpSetupResponse {
        secret_base32,
        otpauth_uri,
    }))
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/totp/confirm",
    tag = "auth",
    security(("bearer_auth" = [])),
    request_body = TotpCodeRequest,
    responses(
        (status = 200, description = "Generated backup codes", body = TotpBackupCodesResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = TotpVerifyErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn totp_confirm(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<TotpCodeRequest>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>), AppError> {
    let user = auth_queries::find_user_auth_by_id(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if user.totp_enabled {
        return Err(AppError::Conflict);
    }

    let Some(ciphertext) = user.totp_secret.as_deref() else {
        return Err(AppError::BadRequest);
    };
    let secret_base32 = totp_auth::decrypt_secret(ciphertext, &state.config.auth.jwt_secret)?;
    let issuer = issuer_name(&state.config);
    let code = payload.code.trim();
    if code.is_empty() {
        return Err(AppError::BadRequest);
    }

    if !totp_auth::validate_code(&issuer, &auth_user.user.email, &secret_base32, code)? {
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            HeaderMap::new(),
            Json(json!({
                "error": "invalid_totp",
                "message": "Invalid or expired code"
            })),
        ));
    }

    let now = Utc::now().to_rfc3339();
    let mut tx = state.db.begin().await.map_err(|_| AppError::Internal)?;
    sqlx::query(
        r#"
        UPDATE users
        SET totp_enabled = 1, login_attempts = 0, locked_until = NULL, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(&now)
    .bind(&auth_user.user.id)
    .execute(tx.as_mut())
    .await
    .map_err(|_| AppError::Internal)?;
    sqlx::query("DELETE FROM totp_backup_codes WHERE user_id = ?")
        .bind(&auth_user.user.id)
        .execute(tx.as_mut())
        .await
        .map_err(|_| AppError::Internal)?;

    let mut backup_codes = Vec::with_capacity(8);
    for _ in 0..8 {
        let code = totp_auth::generate_backup_code();
        let code_hash = hash_totp_backup_code(&code);
        let code_id = uuid::Uuid::new_v4().to_string();
        totp_queries::insert_totp_backup_code(
            &mut tx,
            &code_id,
            &auth_user.user.id,
            &code_hash,
            &now,
        )
        .await
        .map_err(|_| AppError::Internal)?;
        backup_codes.push(code);
    }

    tx.commit().await.map_err(|_| AppError::Internal)?;

    Ok((
        StatusCode::OK,
        HeaderMap::new(),
        Json(json!({
            "backup_codes": backup_codes
        })),
    ))
}

async fn totp_disable(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Json(payload): Json<TotpPasswordRequest>,
) -> Result<StatusCode, AppError> {
    let user = auth_queries::find_user_auth_by_id(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;

    if !verify_password(&user.password_hash, &payload.password) {
        return Err(AppError::BadRequest);
    }

    totp_queries::disable_totp(&state.db, &auth_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(
    post,
    path = "/api/v1/auth/totp/verify",
    tag = "auth",
    security(("bearer_auth" = [])),
    request_body = TotpCodeRequest,
    responses(
        (status = 200, description = "Login session", body = LoginSessionResponse),
        (status = 400, description = "Bad request", body = crate::error::AppErrorResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 403, description = "Forbidden", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = TotpVerifyErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn totp_verify(
    State(state): State<AppState>,
    Extension(totp_user): Extension<TotpPendingUser>,
    Json(payload): Json<TotpCodeRequest>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>), AppError> {
    let user = auth_queries::find_user_auth_by_id(&state.db, &totp_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !totp_user.user.is_active || !user.user.is_active {
        return Err(AppError::Unauthorized);
    }
    if !user.totp_enabled {
        return Err(AppError::Unauthorized);
    }

    let now = Utc::now();
    if let Some(locked_until) = user.locked_until {
        if locked_until > now {
            return Err(AppError::Unauthorized);
        }
    }

    let Some(ciphertext) = user.totp_secret.as_deref() else {
        return Err(AppError::Unauthorized);
    };
    let secret_base32 = totp_auth::decrypt_secret(ciphertext, &state.config.auth.jwt_secret)?;
    let code = payload.code.trim();
    if code.is_empty() {
        return Err(AppError::BadRequest);
    }

    let issuer = issuer_name(&state.config);
    let valid = totp_auth::validate_code(&issuer, &totp_user.user.email, &secret_base32, code)?;
    if !valid {
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
            Some(&totp_user.user.id),
            &totp_user.user.username,
            "invalid_totp",
            None,
        )
        .await;
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            HeaderMap::new(),
            Json(json!({
                "error": "invalid_totp"
            })),
        ));
    }

    auth_queries::clear_login_lockout(&state.db, &totp_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?;

    let response = create_login_session_response(&state, &user.user).await?;
    record_login_success(&state, &totp_user.user.id, &totp_user.user.username, None).await;

    Ok((
        StatusCode::OK,
        refresh_cookie_headers(
            &state.config.app.base_url,
            &response.refresh_token,
            state.config.auth.refresh_token_ttl_days,
        )?,
        Json(json!({
            "access_token": response.access_token,
            "refresh_token": response.refresh_token,
            "user": response.user,
        })),
    ))
}

async fn totp_verify_backup(
    State(state): State<AppState>,
    Extension(totp_user): Extension<TotpPendingUser>,
    Json(payload): Json<TotpCodeRequest>,
) -> Result<(StatusCode, HeaderMap, Json<serde_json::Value>), AppError> {
    let user = auth_queries::find_user_auth_by_id(&state.db, &totp_user.user.id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !totp_user.user.is_active || !user.user.is_active {
        return Err(AppError::Unauthorized);
    }
    if !user.totp_enabled {
        return Err(AppError::Unauthorized);
    }

    let code = payload.code.trim();
    if code.len() != 8 || !code.chars().all(|ch| ch.is_ascii_alphanumeric()) {
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
            Some(&totp_user.user.id),
            &totp_user.user.username,
            "invalid_backup_code",
            None,
        )
        .await;
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            HeaderMap::new(),
            Json(json!({
                "error": "invalid_backup_code"
            })),
        ));
    }

    let mut tx = state.db.begin().await.map_err(|_| AppError::Internal)?;
    let code_hash = hash_totp_backup_code(code);
    let Some(backup_code) =
        totp_queries::find_unused_backup_code_in_tx(&mut tx, &totp_user.user.id, &code_hash)
            .await
            .map_err(|_| AppError::Internal)?
    else {
        let _ = tx.rollback().await;
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
            Some(&totp_user.user.id),
            &totp_user.user.username,
            "invalid_backup_code",
            None,
        )
        .await;
        return Ok((
            StatusCode::UNPROCESSABLE_ENTITY,
            HeaderMap::new(),
            Json(json!({
                "error": "invalid_backup_code"
            })),
        ));
    };
    totp_queries::mark_backup_code_used(&mut tx, &backup_code.id)
        .await
        .map_err(|_| AppError::Internal)?;
    clear_login_lockout_in_tx(&mut tx, &totp_user.user.id).await?;
    let response = issue_session_tokens_in_transaction(&mut tx, &state, &user.user).await?;
    tx.commit().await.map_err(|_| AppError::Internal)?;

    record_login_success(&state, &totp_user.user.id, &totp_user.user.username, None).await;

    Ok((
        StatusCode::OK,
        refresh_cookie_headers(
            &state.config.app.base_url,
            &response.refresh_token,
            state.config.auth.refresh_token_ttl_days,
        )?,
        Json(json!({
            "access_token": response.access_token,
            "refresh_token": response.refresh_token,
            "user": response.user,
        })),
    ))
}

async fn create_login_session_response(
    state: &AppState,
    user: &crate::db::models::User,
) -> Result<LoginSessionResponse, AppError> {
    let (access_token, refresh_token) = issue_session_tokens(state, user).await?;

    Ok(LoginSessionResponse {
        access_token,
        refresh_token,
        user: user.clone(),
    })
}

async fn issue_session_tokens(
    state: &AppState,
    user: &crate::db::models::User,
) -> Result<(String, String), AppError> {
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

    Ok((access_token, refresh_token))
}

async fn issue_session_tokens_in_transaction(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    state: &AppState,
    user: &crate::db::models::User,
) -> Result<LoginSessionResponse, AppError> {
    let access_token = issue_access_token(
        &user.id,
        &state.config.auth.jwt_secret,
        state.config.auth.access_token_ttl_mins,
    )?;
    let refresh_token = auth_queries::generate_refresh_token();
    let now = Utc::now();
    let expires_at = now + chrono::Duration::days(state.config.auth.refresh_token_ttl_days as i64);
    let token_hash = auth_queries::hash_refresh_token(&refresh_token);
    let token_id = uuid::Uuid::new_v4().to_string();
    sqlx::query(
        r#"
        INSERT INTO refresh_tokens (id, user_id, token_hash, expires_at, created_at, revoked_at)
        VALUES (?, ?, ?, ?, ?, NULL)
        "#,
    )
    .bind(&token_id)
    .bind(&user.id)
    .bind(&token_hash)
    .bind(expires_at.to_rfc3339())
    .bind(now.to_rfc3339())
    .execute(tx.as_mut())
    .await
    .map_err(|_| AppError::Internal)?;

    Ok(LoginSessionResponse {
        access_token,
        refresh_token,
        user: user.clone(),
    })
}

async fn clear_login_lockout_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    user_id: &str,
) -> Result<(), AppError> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE users
        SET login_attempts = 0, locked_until = NULL, last_modified = ?
        WHERE id = ?
        "#,
    )
    .bind(now)
    .bind(user_id)
    .execute(tx.as_mut())
    .await
    .map_err(|_| AppError::Internal)?;
    Ok(())
}

// LDAP is a trusted enterprise directory — matching existing users by username or email is correct.
async fn find_or_create_ldap_user(
    state: &AppState,
    username: &str,
    email: &str,
) -> Result<crate::db::models::User, AppError> {
    if let Some(user) = auth_queries::find_user_by_username(&state.db, username)
        .await
        .map_err(|_| AppError::Internal)?
    {
        return Ok(user);
    }
    if let Some(user) = auth_queries::find_user_by_email(&state.db, email)
        .await
        .map_err(|_| AppError::Internal)?
    {
        return Ok(user);
    }
    let password = generate_random_password();
    let password_hash = hash_password(&password, &state.config.auth)?;
    auth_queries::create_user(&state.db, username, email, "user", &password_hash)
        .await
        .map_err(|_| AppError::Internal)
}

async fn create_oauth_user(
    state: &AppState,
    username: &str,
    email: &str,
) -> Result<crate::db::models::User, AppError> {
    let password = generate_random_password();
    let password_hash = hash_password(&password, &state.config.auth)?;
    auth_queries::create_user(&state.db, username, email, "user", &password_hash)
        .await
        .map_err(|_| AppError::Internal)
}

fn provider_config(
    config: &crate::config::AppConfig,
    provider: &str,
) -> Result<ProviderSettings, AppError> {
    let section = match provider {
        "google" => &config.oauth.google,
        "github" => &config.oauth.github,
        _ => return Err(AppError::BadRequest),
    };

    if section.client_id.trim().is_empty()
        || section.client_secret.trim().is_empty()
        || section.authorization_url.trim().is_empty()
        || section.token_url.trim().is_empty()
        || section.userinfo_url.trim().is_empty()
        || section.scope.trim().is_empty()
    {
        return Err(AppError::NotImplemented);
    }

    Ok(ProviderSettings {
        client_id: section.client_id.clone(),
        client_secret: section.client_secret.clone(),
        authorization_url: section.authorization_url.clone(),
        token_url: section.token_url.clone(),
        userinfo_url: section.userinfo_url.clone(),
        email_url: section.email_url.clone(),
        scope: section.scope.clone(),
    })
}

fn oauth_redirect_uri(base_url: &str, provider: &str) -> String {
    format!(
        "{}/api/v1/auth/oauth/{}/callback",
        base_url.trim_end_matches('/'),
        provider
    )
}

fn generate_oauth_state() -> String {
    use rand::{distributions::Alphanumeric, Rng};
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
}

fn oauth_state_cookie(_provider: &str, state: &str) -> String {
    format!("oauth_state={state}; Path=/api/v1/auth/oauth; HttpOnly; SameSite=Lax; Max-Age=600")
}

fn clear_oauth_state_cookie(_provider: &str) -> String {
    "oauth_state=; Path=/api/v1/auth/oauth; HttpOnly; SameSite=Lax; Max-Age=0".to_string()
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(axum::http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .and_then(|cookie_header| {
            cookie_header.split(';').find_map(|part| {
                let mut pieces = part.trim().splitn(2, '=');
                let key = pieces.next()?.trim();
                let value = pieces.next()?.trim();
                if key == name {
                    Some(value.to_string())
                } else {
                    None
                }
            })
        })
}

fn refresh_cookie_value(base_url: &str, refresh_token: &str, refresh_ttl_days: u64) -> String {
    let secure = base_url.trim().to_ascii_lowercase().starts_with("https://");
    let max_age = refresh_ttl_days.saturating_mul(24 * 60 * 60);
    let mut cookie = format!(
        "refresh_token={refresh_token}; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age={max_age}"
    );
    if secure {
        cookie.push_str("; Secure");
    }
    cookie
}

async fn exchange_oauth_code(
    provider: &ProviderSettings,
    base_url: &str,
    provider_name: &str,
    code: &str,
) -> Result<String, AppError> {
    let redirect_uri = oauth_redirect_uri(base_url, provider_name);
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()
        .map_err(|_| AppError::Internal)?;

    let response = client
        .post(&provider.token_url)
        .form(&[
            ("client_id", provider.client_id.as_str()),
            ("client_secret", provider.client_secret.as_str()),
            ("code", code),
            ("grant_type", "authorization_code"),
            ("redirect_uri", &redirect_uri),
        ])
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .map_err(|_| AppError::ServiceUnavailable)?;

    if !response.status().is_success() {
        return Err(AppError::ServiceUnavailable);
    }

    let token_response: OAuthTokenResponse = response
        .json()
        .await
        .map_err(|_| AppError::ServiceUnavailable)?;
    Ok(token_response.access_token)
}

async fn fetch_oauth_user(
    provider: &ProviderSettings,
    access_token: &str,
    provider_name: &str,
) -> Result<ExternalOAuthUser, AppError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent("autolibre")
        .build()
        .map_err(|_| AppError::Internal)?;

    if provider_name == "google" {
        let userinfo: GoogleUserInfo = client
            .get(&provider.userinfo_url)
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|_| AppError::ServiceUnavailable)?
            .error_for_status()
            .map_err(|_| AppError::ServiceUnavailable)?
            .json()
            .await
            .map_err(|_| AppError::ServiceUnavailable)?;
        return Ok(ExternalOAuthUser {
            provider_user_id: userinfo.sub,
            username: userinfo.email.clone(),
            email: userinfo.email,
        });
    }

    let userinfo: GithubUserInfo = client
        .get(&provider.userinfo_url)
        .header(reqwest::header::ACCEPT, "application/json")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|_| AppError::ServiceUnavailable)?
        .error_for_status()
        .map_err(|_| AppError::ServiceUnavailable)?
        .json()
        .await
        .map_err(|_| AppError::ServiceUnavailable)?;

    let email = if let Some(email) = userinfo.email.clone() {
        email
    } else if !provider.email_url.trim().is_empty() {
        let emails: Vec<GithubEmailRecord> = client
            .get(&provider.email_url)
            .header(reqwest::header::ACCEPT, "application/json")
            .bearer_auth(access_token)
            .send()
            .await
            .map_err(|_| AppError::ServiceUnavailable)?
            .error_for_status()
            .map_err(|_| AppError::ServiceUnavailable)?
            .json()
            .await
            .map_err(|_| AppError::ServiceUnavailable)?;
        let primary_verified = emails.iter().find(|entry| entry.primary && entry.verified);
        let verified = emails.iter().find(|entry| entry.verified);
        primary_verified
            .or(verified)
            .or_else(|| emails.first())
            .map(|entry| entry.email.clone())
            .ok_or(AppError::BadRequest)?
    } else {
        return Err(AppError::BadRequest);
    };

    Ok(ExternalOAuthUser {
        provider_user_id: userinfo.id.to_string(),
        username: userinfo.login,
        email,
    })
}

#[derive(Debug)]
struct ExternalOAuthUser {
    provider_user_id: String,
    username: String,
    email: String,
}

fn generate_random_password() -> String {
    use rand::{distributions::Alphanumeric, Rng};
    rand::thread_rng()
        .sample_iter(&Alphanumeric)
        .take(32)
        .map(char::from)
        .collect()
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
    let secure = base_url.trim().to_ascii_lowercase().starts_with("https://");
    let max_age = refresh_ttl_days.saturating_mul(24 * 60 * 60);

    let mut cookie = format!(
        "refresh_token={refresh_token}; Path=/api/v1/auth; HttpOnly; SameSite=Lax; Max-Age={max_age}"
    );
    if secure {
        cookie.push_str("; Secure");
    } else {
        tracing::warn!(
            "refresh token cookie issued without Secure flag — do not use in production without HTTPS"
        );
    }

    headers.insert(
        SET_COOKIE,
        HeaderValue::from_str(&cookie).map_err(|_| AppError::Internal)?,
    );
    Ok(headers)
}

fn clear_refresh_cookie_headers(base_url: &str) -> Result<HeaderMap, AppError> {
    let mut headers = HeaderMap::new();
    let secure = base_url.trim().to_ascii_lowercase().starts_with("https://");

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

fn issuer_name(config: &crate::config::AppConfig) -> String {
    let issuer = config.app.library_name.trim();
    if issuer.is_empty() {
        "autolibre".to_string()
    } else {
        issuer.to_string()
    }
}

fn hash_totp_backup_code(code: &str) -> String {
    let digest = Sha256::digest(code.as_bytes());
    hex::encode(digest)
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
    if let Err(err) =
        auth_queries::audit_login_success(&state.db, user_id, username, client_ip).await
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
