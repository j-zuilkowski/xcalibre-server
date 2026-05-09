//! KAG graph endpoints: triple ingest and BFS traversal.
//!
//! Routes:
//!   POST /api/v1/graph/triples  — ingest session triples (auth required)
//!   GET  /api/v1/graph/traverse — BFS traversal (auth required)

use crate::{graph::traverse, AppError, AppState};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    middleware,
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub fn router(state: AppState) -> Router<AppState> {
    let auth_layer =
        middleware::from_fn_with_state(state.clone(), crate::middleware::auth::require_auth);

    Router::new()
        .route("/api/v1/graph/triples", post(ingest_triples))
        .route("/api/v1/graph/traverse", get(traverse_graph))
        .route_layer(auth_layer)
}

// ── Ingest ──────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct IngestTripleItem {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    #[serde(default)]
    pub domain_id: String,
    #[serde(default)]
    pub session_id: String,
    #[serde(default = "default_confidence")]
    pub confidence: f64,
}

fn default_confidence() -> f64 {
    1.0
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct IngestTriplesRequest {
    pub triples: Vec<IngestTripleItem>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct IngestTriplesResponse {
    pub written: usize,
}

pub(crate) async fn ingest_triples(
    State(state): State<AppState>,
    Json(payload): Json<IngestTriplesRequest>,
) -> Result<(StatusCode, Json<IngestTriplesResponse>), AppError> {
    let mut written = 0usize;

    for t in &payload.triples {
        if t.subject.trim().is_empty() || t.predicate.trim().is_empty() || t.object.trim().is_empty() {
            return Err(AppError::UnprocessableMessage(
                "subject, predicate, and object are required".to_string(),
            ));
        }
        sqlx::query(
            r#"
            INSERT INTO knowledge_graph
                (subject, predicate, object, domain_id, source, source_id, confidence)
            VALUES (?, ?, ?, ?, 'session', ?, ?)
            "#,
        )
        .bind(t.subject.trim())
        .bind(t.predicate.trim())
        .bind(t.object.trim())
        .bind(t.domain_id.trim())
        .bind(t.session_id.trim())
        .bind(t.confidence.clamp(0.0, 1.0))
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, "failed to insert knowledge triple");
            AppError::Internal
        })?;
        written += 1;
    }

    Ok((StatusCode::CREATED, Json(IngestTriplesResponse { written })))
}

// ── Traverse ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct TraverseQuery {
    pub anchor: String,
    #[serde(default = "default_hops")]
    pub hops: u8,
    pub domain_id: Option<String>,
}

fn default_hops() -> u8 {
    2
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TraverseResponse {
    pub triples: Vec<crate::graph::KnowledgeTriple>,
}

pub(crate) async fn traverse_graph(
    State(state): State<AppState>,
    Query(q): Query<TraverseQuery>,
) -> Result<Json<TraverseResponse>, AppError> {
    let triples = traverse::bfs_traverse(
        &state.db,
        traverse::TraverseParams {
            anchor: q.anchor.trim(),
            hops: q.hops,
            max_hops: state.config.kag.max_hops,
            domain_id: q.domain_id.as_deref(),
        },
    )
    .await
    .map_err(|e| {
        tracing::error!(error = %e, "graph traversal failed");
        AppError::Internal
    })?;

    Ok(Json(TraverseResponse { triples }))
}
