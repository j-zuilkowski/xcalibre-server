//! Merlin memory chunk ingest and deletion endpoints.
//!
//! Routes under `/api/v1/memory/`. All routes require authentication.

use crate::{
    db::queries::memory_chunks as memory_queries, AppError, AppState,
};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    middleware,
    routing::{delete, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

const MAX_MEMORY_TEXT_CHARS: usize = 32_768;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/memory", post(ingest_memory_chunk))
        .route("/api/v1/memory/:id", delete(delete_memory_chunk))
        .route_layer(auth_layer)
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct IngestMemoryChunkRequest {
    pub text: String,
    pub session_id: Option<String>,
    pub project_path: Option<String>,
    #[serde(default = "default_chunk_type")]
    pub chunk_type: String,
    pub tags: Option<Vec<String>>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IngestMemoryChunkResponse {
    pub id: String,
    pub created_at: i64,
}

#[utoipa::path(
    post,
    path = "/api/v1/memory",
    tag = "memory",
    security(("bearer_auth" = [])),
    request_body = IngestMemoryChunkRequest,
    responses(
        (status = 201, description = "Memory chunk created", body = IngestMemoryChunkResponse),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 422, description = "Unprocessable", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn ingest_memory_chunk(
    State(state): State<AppState>,
    Json(payload): Json<IngestMemoryChunkRequest>,
) -> Result<(StatusCode, Json<IngestMemoryChunkResponse>), AppError> {
    let text = payload.text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::UnprocessableMessage(
            "text must not be blank".to_string(),
        ));
    }
    if text.chars().count() > MAX_MEMORY_TEXT_CHARS {
        return Err(AppError::UnprocessableMessage(format!(
            "text must not exceed {MAX_MEMORY_TEXT_CHARS} characters"
        )));
    }
    if !matches!(payload.chunk_type.as_str(), "episodic" | "factual") {
        return Err(AppError::UnprocessableMessage(
            "chunk_type must be either 'episodic' or 'factual'".to_string(),
        ));
    }

    let session_id = normalize_optional_string(payload.session_id);
    let project_path = normalize_optional_string(payload.project_path);
    let tags = normalize_tags(payload.tags);
    let id = Uuid::new_v4().to_string();

    let (embedding, model_id) = if state.config.llm.enabled {
        if let Some(semantic) = state.semantic_search.as_ref() {
            if semantic.is_configured() {
                match semantic.embed_text(&text).await {
                    Ok(vector) => (Some(memory_queries::serialize_embedding(&vector)), semantic.model_id().to_string()),
                    Err(err) => {
                        tracing::warn!(error = %err, "memory embedding failed");
                        (None, String::new())
                    }
                }
            } else {
                (None, String::new())
            }
        } else {
            (None, String::new())
        }
    } else {
        (None, String::new())
    };

    let inserted = memory_queries::insert_memory_chunk(
        &state.db,
        &memory_queries::InsertMemoryChunkParams {
            id: &id,
            session_id: session_id.as_deref(),
            project_path: project_path.as_deref(),
            chunk_type: &payload.chunk_type,
            text: &text,
            tags: tags.as_deref(),
            model_id: &model_id,
            embedding: embedding.as_deref(),
        },
    )
    .await
    .map_err(|_| AppError::Internal)?;

    Ok((
        StatusCode::CREATED,
        Json(IngestMemoryChunkResponse {
            id: inserted.id,
            created_at: inserted.created_at,
        }),
    ))
}

#[utoipa::path(
    delete,
    path = "/api/v1/memory/{id}",
    tag = "memory",
    security(("bearer_auth" = [])),
    params(
        ("id" = String, Path, description = "Memory chunk id")
    ),
    responses(
        (status = 204, description = "Memory chunk deleted"),
        (status = 401, description = "Unauthorized", body = crate::error::AppErrorResponse),
        (status = 404, description = "Not found", body = crate::error::AppErrorResponse),
        (status = 429, description = "Rate limited", body = crate::error::AppErrorResponse)
    )
)]
pub(crate) async fn delete_memory_chunk(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    if memory_queries::get_memory_chunk(&state.db, &id)
        .await
        .map_err(|_| AppError::Internal)?
        .is_none()
    {
        return Err(AppError::NotFound);
    }

    let deleted = memory_queries::delete_memory_chunk(&state.db, &id)
        .await
        .map_err(|_| AppError::Internal)?;
    if !deleted {
        return Err(AppError::NotFound);
    }

    Ok(StatusCode::NO_CONTENT)
}

fn default_chunk_type() -> String {
    "episodic".to_string()
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(|candidate| {
        let trimmed = candidate.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_tags(tags: Option<Vec<String>>) -> Option<String> {
    let values = tags?
        .into_iter()
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        None
    } else {
        serde_json::to_string(&values).ok()
    }
}
