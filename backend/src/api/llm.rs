use crate::{
    api::books::{accessible_library_id, load_book_or_not_found},
    db::queries::{books as book_queries, llm as llm_queries},
    llm::{
        classify::classify_book, derive::derive_book, quality::check_quality,
        validate::validate_book,
    },
    middleware::auth::AuthenticatedUser,
    AppError, AppState,
};
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::time::Duration;

const HEALTH_TIMEOUT_SECS: u64 = 3;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/llm/health", get(llm_health))
        .route("/api/v1/books/:id/classify", get(classify))
        .route("/api/v1/books/:id/validate", get(validate))
        .route("/api/v1/books/:id/quality", get(quality))
        .route("/api/v1/books/:id/derive", get(derive))
        .route("/api/v1/books/:id/tags/confirm", post(confirm_tags))
        .route("/api/v1/books/:id/tags/confirm-all", post(confirm_all_tags))
        .route("/api/v1/organize", post(organize))
        .route_layer(auth_layer)
}

#[derive(Debug, Serialize)]
struct LlmHealthResponse {
    enabled: bool,
    librarian: LibrarianHealth,
}

#[derive(Debug, Serialize)]
struct LibrarianHealth {
    available: bool,
    model_id: Option<String>,
    endpoint: String,
}

#[derive(Debug, Serialize)]
struct ClassifyResponse {
    book_id: String,
    suggestions: Vec<crate::llm::classify::TagSuggestion>,
    model_id: String,
    pending_count: usize,
}

#[derive(Debug, Serialize)]
struct ValidateResponse {
    book_id: String,
    severity: String,
    issues: Vec<crate::llm::validate::ValidationIssue>,
    model_id: String,
}

#[derive(Debug, Serialize)]
struct QualityResponse {
    book_id: String,
    score: f32,
    issues: Vec<crate::llm::quality::QualityIssue>,
    model_id: String,
}

#[derive(Debug, Serialize)]
struct DeriveResponse {
    book_id: String,
    summary: String,
    related_titles: Vec<String>,
    discussion_questions: Vec<String>,
    model_id: String,
}

#[derive(Debug, Deserialize, Default)]
struct ConfirmTagsRequest {
    #[serde(default)]
    confirm: Vec<String>,
    #[serde(default)]
    reject: Vec<String>,
}

#[derive(Debug, Serialize)]
struct OrganizeResponse {
    job_id: String,
}

async fn llm_health(State(state): State<AppState>) -> Result<Json<LlmHealthResponse>, AppError> {
    let endpoint = state.config.llm.librarian.endpoint.trim().to_string();
    let Some(chat_client) = state.chat_client.as_ref() else {
        return Ok(Json(LlmHealthResponse {
            enabled: false,
            librarian: LibrarianHealth {
                available: false,
                model_id: None,
                endpoint,
            },
        }));
    };

    let available = ping_models_endpoint(chat_client.endpoint()).await;

    Ok(Json(LlmHealthResponse {
        enabled: true,
        librarian: LibrarianHealth {
            available,
            model_id: Some(chat_client.model_id().to_string()),
            endpoint: chat_client.endpoint().to_string(),
        },
    }))
}

async fn classify(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<ClassifyResponse>, AppError> {
    let Some(chat_client) = state.chat_client.as_ref() else {
        return Err(AppError::ServiceUnavailable);
    };

    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let authors = book
        .authors
        .iter()
        .map(|author| author.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let description = book.description.as_deref().unwrap_or_default();

    let result = classify_book(chat_client, &book.title, &authors, description).await;

    llm_queries::insert_tag_suggestions(&state.db, &book_id, &result.suggestions)
        .await
        .map_err(|_| AppError::Internal)?;

    let pending_count = llm_queries::list_pending_tags(&state.db, &book_id)
        .await
        .map_err(|_| AppError::Internal)?
        .len();

    Ok(Json(ClassifyResponse {
        book_id,
        suggestions: result.suggestions,
        model_id: result.model_id,
        pending_count,
    }))
}

async fn confirm_tags(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
    Json(payload): Json<ConfirmTagsRequest>,
) -> Result<Json<crate::db::models::Book>, AppError> {
    ensure_can_edit(&state, &auth_user.user.id).await?;
    load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    llm_queries::confirm_tags(&state.db, &book_id, &payload.confirm, &payload.reject)
        .await
        .map_err(|_| AppError::Internal)?;

    let updated = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    Ok(Json(updated))
}

async fn confirm_all_tags(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<crate::db::models::Book>, AppError> {
    ensure_can_edit(&state, &auth_user.user.id).await?;
    load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;

    llm_queries::confirm_all_pending_tags(&state.db, &book_id)
        .await
        .map_err(|_| AppError::Internal)?;

    let updated = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    Ok(Json(updated))
}

async fn validate(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<ValidateResponse>, AppError> {
    let Some(chat_client) = state.chat_client.as_ref() else {
        return Err(AppError::ServiceUnavailable);
    };

    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let authors = book
        .authors
        .iter()
        .map(|author| author.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let description = book.description.as_deref().unwrap_or_default();

    let result = validate_book(
        chat_client,
        &book.title,
        &authors,
        description,
        book.language.as_deref(),
    )
    .await;

    Ok(Json(ValidateResponse {
        book_id,
        severity: result.severity,
        issues: result.issues,
        model_id: result.model_id,
    }))
}

async fn quality(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<QualityResponse>, AppError> {
    let Some(chat_client) = state.chat_client.as_ref() else {
        return Err(AppError::ServiceUnavailable);
    };

    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let description = book.description.as_deref().unwrap_or_default();
    let result = check_quality(chat_client, &book.title, description).await;

    Ok(Json(QualityResponse {
        book_id,
        score: result.score,
        issues: result.issues,
        model_id: result.model_id,
    }))
}

async fn derive(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
    Path(book_id): Path<String>,
) -> Result<Json<DeriveResponse>, AppError> {
    let Some(chat_client) = state.chat_client.as_ref() else {
        return Err(AppError::ServiceUnavailable);
    };

    let book = load_book_or_not_found(
        &state.db,
        &book_id,
        accessible_library_id(&auth_user.user),
        Some(auth_user.user.id.as_str()),
    )
    .await?;
    let authors = book
        .authors
        .iter()
        .map(|author| author.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let description = book.description.as_deref().unwrap_or_default();
    let result = derive_book(chat_client, &book.title, &authors, description).await;

    Ok(Json(DeriveResponse {
        book_id,
        summary: result.summary,
        related_titles: result.related_titles,
        discussion_questions: result.discussion_questions,
        model_id: result.model_id,
    }))
}

async fn organize(
    State(state): State<AppState>,
    Extension(auth_user): Extension<AuthenticatedUser>,
) -> Result<(StatusCode, Json<OrganizeResponse>), AppError> {
    ensure_admin(&state, &auth_user.user.id).await?;
    let job_id = llm_queries::enqueue_organize_job(&state.db)
        .await
        .map_err(|_| AppError::Internal)?;
    Ok((StatusCode::ACCEPTED, Json(OrganizeResponse { job_id })))
}

async fn ensure_can_edit(state: &AppState, user_id: &str) -> Result<(), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.can_edit {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

async fn ensure_admin(state: &AppState, user_id: &str) -> Result<(), AppError> {
    let perms = book_queries::role_permissions_for_user(&state.db, user_id)
        .await
        .map_err(|_| AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if !perms.is_admin() {
        return Err(AppError::Forbidden);
    }
    Ok(())
}

async fn ping_models_endpoint(endpoint: &str) -> bool {
    if endpoint.trim().is_empty() {
        return false;
    }

    let Ok(http) = reqwest::Client::builder()
        .timeout(Duration::from_secs(HEALTH_TIMEOUT_SECS))
        .build()
    else {
        return false;
    };

    let Ok(response) = http.get(models_url(endpoint)).send().await else {
        return false;
    };

    response.status().is_success()
}

fn models_url(endpoint: &str) -> String {
    let trimmed = endpoint.trim_end_matches('/');
    if trimmed.ends_with("/v1/models") {
        trimmed.to_string()
    } else if trimmed.ends_with("/v1") {
        format!("{trimmed}/models")
    } else {
        format!("{trimmed}/v1/models")
    }
}
