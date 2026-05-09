//! Knowledge graph: typed entity-relationship triples + BFS traversal.

pub mod traverse;

use serde::{Deserialize, Serialize};

/// A single entity-relationship triple stored in `knowledge_graph`.
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, ToSchema)]
pub struct KnowledgeTriple {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub domain_id: String,
    pub source: String,
    pub source_id: String,
    pub chunk_index: Option<i64>,
    pub confidence: f64,
    pub created_at: i64,
}
