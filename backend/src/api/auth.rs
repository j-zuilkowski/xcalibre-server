use crate::{
    auth::{ldap::authenticate_ldap, password::hash_password},
    db::queries::{auth as auth_queries, oauth as oauth_queries},
    middleware::auth::{issue_access_token, AuthenticatedUser},
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
use std::time::Duration;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);
    let public = Router::new()
        .route("/providers", get(auth_providers))
        .route("/oauth/:provider", get(oauth_start))
        .route("/oauth/:provider/callback", get(oauth_callback))
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .layer(crate::middleware::security_headers::auth_rate_limit_layer());
    let protected = Router::new()
        .route("/logout", post(logout))
        .route("/me", get(me))
        .route("/me/password", patch(change_password))
        .route_layer(auth_layer);

    Router::new().merge(public).merge(protected)
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
            auth_queries::clear_login_lockout(&state.db, &user.user.id)
                .await
                .map_err(|_| AppError::Internal)?;

            let response = create_login_response(&state, &user.user).await?;
            record_login_success(&state, &user.user.id, username, client_ip.as_deref()).await;

            return Ok((
                refresh_cookie_headers(
                    &state.config.app.base_url,
                    &response.refresh_token,
                    state.config.auth.refresh_token_ttl_days,
                )?,
                Json(response),
            ));
        }

        failed_local_user = Some(user.clone());
    }

    if state.config.ldap.enabled {
        match authenticate_ldap(&state.config, username, &payload.password).await {
            Ok(Some(ldap_user)) => {
                let user =
                    find_or_create_ldap_user(&state, &ldap_user.username, &ldap_user.email).await?;
                let response = create_login_response(&state, &user).await?;
                record_login_success(&state, &user.id, username, client_ip.as_deref()).await;
                return Ok((
                    refresh_cookie_headers(
                        &state.config.app.base_url,
                        &response.refresh_token,
                        state.config.auth.refresh_token_ttl_days,
                    )?,
                    Json(response),
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
    let (access_token, refresh_token) = issue_session_tokens(state, user).await?;

    Ok(LoginResponse {
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
