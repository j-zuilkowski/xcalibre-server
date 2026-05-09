# Phase 22b — KAG Knowledge Graph Layer: Implementation

## Context

Rust 2021, Axum 0.7, sqlx 0.7. TDD — phase 22a failing tests are in place.
Working dir: `~/Documents/localProject/xcalibre-server`

Phase 22a complete: 12 failing tests in `backend/tests/test_kag.rs`.
Phase 22b makes all 12 pass and bumps the version to v2.6.0.

Adds a knowledge graph layer to xcalibre-server so Merlin agents can write structured
entity-relationship triples from their sessions and retrieve them fused with book content.

Three new endpoints:
- `POST /api/v1/graph/triples` — session triple ingest (auth required; always accepted)
- `GET /api/v1/graph/traverse` — BFS traversal from an anchor entity with domain filter
- `GET /api/v1/search/enriched` — hybrid chunk search + graph traversal fused response

New module: `backend/src/graph/` with `mod.rs`, `traverse.rs`.
New MCP tool: `graph_traverse` in `xs-mcp/src/tools/mod.rs`.
New migration: `backend/migrations/sqlite/0034_knowledge_graph.sql` (and mariadb equivalent).

---

## Step 1 — Migration

### Write to: `backend/migrations/sqlite/0034_knowledge_graph.sql`

```sql
-- Knowledge graph: typed entity-relationship triples written by Merlin sessions
-- or extracted from book content by the ingest pipeline.
--
-- source: 'session' (Merlin agent write) | 'book' (ingest pipeline extraction)
-- source_id: session_id for session triples; book_id for book triples
-- chunk_index: NULL for session triples; the chunk ordinal for book triples

CREATE TABLE IF NOT EXISTS knowledge_graph (
    id          TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    subject     TEXT NOT NULL,
    predicate   TEXT NOT NULL,
    object      TEXT NOT NULL,
    domain_id   TEXT NOT NULL DEFAULT '',
    source      TEXT NOT NULL DEFAULT 'session' CHECK (source IN ('session', 'book')),
    source_id   TEXT NOT NULL DEFAULT '',
    chunk_index INTEGER,
    confidence  REAL NOT NULL DEFAULT 1.0,
    created_at  INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX IF NOT EXISTS idx_kg_subject   ON knowledge_graph (subject);
CREATE INDEX IF NOT EXISTS idx_kg_object    ON knowledge_graph (object);
CREATE INDEX IF NOT EXISTS idx_kg_domain    ON knowledge_graph (domain_id);
CREATE INDEX IF NOT EXISTS idx_kg_source_id ON knowledge_graph (source_id);

CREATE VIRTUAL TABLE IF NOT EXISTS knowledge_graph_fts
    USING fts5(subject, predicate, object, domain_id,
               content='knowledge_graph', content_rowid='rowid');

CREATE TRIGGER IF NOT EXISTS kg_ai AFTER INSERT ON knowledge_graph BEGIN
    INSERT INTO knowledge_graph_fts(rowid, subject, predicate, object, domain_id)
    VALUES (new.rowid, new.subject, new.predicate, new.object, new.domain_id);
END;

CREATE TRIGGER IF NOT EXISTS kg_ad AFTER DELETE ON knowledge_graph BEGIN
    INSERT INTO knowledge_graph_fts(knowledge_graph_fts, rowid, subject, predicate, object, domain_id)
    VALUES ('delete', old.rowid, old.subject, old.predicate, old.object, old.domain_id);
END;
```

### Write to: `backend/migrations/mariadb/0034_knowledge_graph.sql`

```sql
CREATE TABLE IF NOT EXISTS knowledge_graph (
    id          VARCHAR(32) PRIMARY KEY DEFAULT (LOWER(HEX(RANDOM_BYTES(16)))),
    subject     TEXT NOT NULL,
    predicate   TEXT NOT NULL,
    object      TEXT NOT NULL,
    domain_id   TEXT NOT NULL DEFAULT '',
    source      ENUM('session','book') NOT NULL DEFAULT 'session',
    source_id   TEXT NOT NULL DEFAULT '',
    chunk_index INT,
    confidence  DOUBLE NOT NULL DEFAULT 1.0,
    created_at  BIGINT NOT NULL DEFAULT (UNIX_TIMESTAMP())
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX idx_kg_subject   ON knowledge_graph (subject(191));
CREATE INDEX idx_kg_object    ON knowledge_graph (object(191));
CREATE INDEX idx_kg_domain    ON knowledge_graph (domain_id(191));
CREATE INDEX idx_kg_source_id ON knowledge_graph (source_id(191));

CREATE FULLTEXT INDEX idx_kg_fts
    ON knowledge_graph (subject, predicate, object, domain_id);
```

---

## Step 2 — Config

### Edit: `backend/src/config.rs`

Add `KagSection` after the `MeilisearchSection` block and add `kag: KagSection` to `AppConfig`.

**Add after `MeilisearchSection` impl block:**

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct KagSection {
    /// Maximum BFS hops for graph traversal; values above this are clamped.
    pub max_hops: u8,
}

impl Default for KagSection {
    fn default() -> Self {
        Self { max_hops: 3 }
    }
}
```

**In `AppConfig` struct, add:**
```rust
pub kag: KagSection,
```

**In `AppConfig::default()`, add:**
```rust
kag: KagSection::default(),
```

---

## Step 3 — Graph module

### Write to: `backend/src/graph/mod.rs`

```rust
//! Knowledge graph: typed entity-relationship triples + BFS traversal.

pub mod traverse;

use serde::{Deserialize, Serialize};

/// A single entity-relationship triple stored in `knowledge_graph`.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
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
```

### Write to: `backend/src/graph/traverse.rs`

```rust
//! BFS traversal over the `knowledge_graph` table.
//!
//! Iterative SQL loop: each hop expands the frontier of known entity names by
//! fetching all triples where `subject` OR `object` is in the current frontier,
//! then adding newly discovered nodes to the frontier for the next hop.
//!
//! Capped at `max_hops` (config value, default 3). Domain filter is applied at
//! every hop so edges from other domains never expand the frontier.

use super::KnowledgeTriple;
use sqlx::SqlitePool;

pub struct TraverseParams<'a> {
    pub anchor: &'a str,
    pub hops: u8,
    pub max_hops: u8,
    pub domain_id: Option<&'a str>,
}

pub async fn bfs_traverse(
    db: &SqlitePool,
    params: TraverseParams<'_>,
) -> Result<Vec<KnowledgeTriple>, sqlx::Error> {
    let hops = params.hops.min(params.max_hops);
    if hops == 0 || params.anchor.is_empty() {
        return Ok(vec![]);
    }

    let mut visited: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut frontier: Vec<String> = vec![params.anchor.to_string()];
    let mut results: Vec<KnowledgeTriple> = Vec::new();

    for _ in 0..hops {
        if frontier.is_empty() {
            break;
        }

        // Build a parameterised IN clause for the current frontier.
        // SQLite does not support array binding, so we use a JSON workaround with json_each.
        let frontier_json = serde_json::to_string(&frontier).unwrap_or_else(|_| "[]".into());

        let rows: Vec<KnowledgeTriple> = if let Some(domain) = params.domain_id {
            sqlx::query_as::<_, KnowledgeTriple>(
                r#"
                SELECT id, subject, predicate, object, domain_id, source, source_id,
                       chunk_index, confidence, created_at
                FROM knowledge_graph
                WHERE domain_id = ?
                  AND (
                    subject IN (SELECT value FROM json_each(?))
                    OR object IN (SELECT value FROM json_each(?))
                  )
                "#,
            )
            .bind(domain)
            .bind(&frontier_json)
            .bind(&frontier_json)
            .fetch_all(db)
            .await?
        } else {
            sqlx::query_as::<_, KnowledgeTriple>(
                r#"
                SELECT id, subject, predicate, object, domain_id, source, source_id,
                       chunk_index, confidence, created_at
                FROM knowledge_graph
                WHERE subject IN (SELECT value FROM json_each(?))
                   OR object IN (SELECT value FROM json_each(?))
                "#,
            )
            .bind(&frontier_json)
            .bind(&frontier_json)
            .fetch_all(db)
            .await?
        };

        let mut next_frontier: Vec<String> = Vec::new();
        for triple in rows {
            if !visited.contains(&triple.id) {
                visited.insert(triple.id.clone());
                // Add newly discovered nodes to the next frontier.
                if !frontier.contains(&triple.subject) && !visited.contains(&triple.subject) {
                    next_frontier.push(triple.subject.clone());
                }
                if !frontier.contains(&triple.object) && !visited.contains(&triple.object) {
                    next_frontier.push(triple.object.clone());
                }
                results.push(triple);
            }
        }
        frontier = next_frontier;
    }

    Ok(results)
}
```

---

## Step 4 — API handler

### Write to: `backend/src/api/graph.rs`

```rust
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

#[utoipa::path(
    post,
    path = "/api/v1/graph/triples",
    tag = "graph",
    security(("bearer_auth" = [])),
    request_body = IngestTriplesRequest,
    responses(
        (status = 201, description = "Triples written", body = IngestTriplesResponse),
        (status = 401, description = "Unauthorized"),
        (status = 422, description = "Unprocessable")
    )
)]
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

#[utoipa::path(
    get,
    path = "/api/v1/graph/traverse",
    tag = "graph",
    security(("bearer_auth" = [])),
    params(
        ("anchor" = String, Query, description = "Starting entity name"),
        ("hops"   = u8,     Query, description = "BFS depth (clamped to server max)"),
        ("domain_id" = Option<String>, Query, description = "Restrict to this domain")
    ),
    responses(
        (status = 200, description = "BFS result", body = TraverseResponse),
        (status = 401, description = "Unauthorized")
    )
)]
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
```

---

## Step 5 — Enriched search endpoint

### Edit: `backend/src/api/search.rs`

Add the `enriched_search` handler. Append after the existing `run_chunk_search` function.

**Add at bottom of search.rs:**

```rust
// ── Enriched search ──────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct EnrichedSearchQuery {
    pub q: Option<String>,
    #[serde(default = "default_enriched_hops")]
    pub hops: u8,
    pub domain_id: Option<String>,
    pub limit: Option<u32>,
}

fn default_enriched_hops() -> u8 {
    1
}

#[derive(Debug, Serialize, ToSchema)]
pub struct EnrichedSearchResponse {
    pub chunks: Vec<ChunkSearchItem>,
    pub graph: crate::api::graph::TraverseResponse,
}

#[utoipa::path(
    get,
    path = "/api/v1/search/enriched",
    tag = "search",
    security(("bearer_auth" = [])),
    params(
        ("q"         = String,         Query, description = "Search query (required)"),
        ("hops"      = u8,             Query, description = "Graph traversal depth"),
        ("domain_id" = Option<String>, Query, description = "Domain filter for graph"),
        ("limit"     = Option<u32>,    Query, description = "Max chunks returned")
    ),
    responses(
        (status = 200, description = "Fused chunk + graph response", body = EnrichedSearchResponse),
        (status = 400, description = "Missing q parameter"),
        (status = 401, description = "Unauthorized")
    )
)]
pub(crate) async fn enriched_search(
    State(state): State<AppState>,
    Query(q): Query<EnrichedSearchQuery>,
) -> Result<Json<EnrichedSearchResponse>, AppError> {
    let query_text = q.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| {
        AppError::BadRequest("q parameter is required".to_string())
    })?;

    // Run chunk search and graph traversal concurrently.
    let (chunks_result, graph_result) = tokio::join!(
        run_chunk_search(&state, query_text, q.limit),
        crate::graph::traverse::bfs_traverse(
            &state.db,
            crate::graph::traverse::TraverseParams {
                anchor: query_text,
                hops: q.hops,
                max_hops: state.config.kag.max_hops,
                domain_id: q.domain_id.as_deref(),
            },
        )
    );

    let chunks = chunks_result.unwrap_or_default();
    let triples = graph_result.unwrap_or_default();

    Ok(Json(EnrichedSearchResponse {
        chunks,
        graph: crate::api::graph::TraverseResponse { triples },
    }))
}
```

**Also add `ChunkSearchItem` and `run_chunk_search` return type** — check the existing `search.rs` to see if `ChunkSearchItem` is already exported; if not, make it `pub`. The enriched endpoint re-uses whatever item type `run_chunk_search` already returns. The `run_chunk_search` function signature should become:

```rust
pub async fn run_chunk_search(
    state: &AppState,
    query: &str,
    limit: Option<u32>,
) -> Result<Vec<ChunkSearchItem>, AppError>
```

where `ChunkSearchItem` is the same struct already returned by `GET /api/v1/search/chunks`. Make both `pub` if they aren't already.

---

## Step 6 — Wire router

### Edit: `backend/src/api/mod.rs`

**Add module declaration** (with existing modules):
```rust
pub mod graph;
```

**In `router()` function**, add after `.merge(memory::router(state.clone()))`:
```rust
.merge(graph::router(state.clone()))
```

**Add enriched search route** — in `search::router()`, add:
```rust
.route("/api/v1/search/enriched", get(search::enriched_search))
```

Or wire it directly in `api/mod.rs` via `search::router` expansion — follow the pattern in `search.rs` where search routes are assembled. The enriched route must sit inside the same auth layer as the existing search routes.

---

## Step 7 — Wire AppState

### Edit: `backend/src/lib.rs` or `state.rs`

The `AppState` already holds `config: AppConfig` which now has `kag: KagSection`. No new fields needed on `AppState` — the graph handlers read `state.config.kag.max_hops` directly.

---

## Step 8 — MCP tool

### Edit: `xs-mcp/src/tools/mod.rs`

**Add request/response types** (alongside existing types):

```rust
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GraphTraverseRequest {
    /// Starting entity name (e.g. "U4", "FnA", "turmeric")
    pub anchor: String,
    /// BFS depth — server clamps to its configured max (default 3)
    #[serde(default = "default_graph_hops")]
    pub hops: u8,
    /// Optional domain filter (e.g. "electronics", "software")
    pub domain_id: Option<String>,
}

fn default_graph_hops() -> u8 {
    2
}
```

**Add tool method** inside the `#[tool_router]` impl block:

```rust
#[tool(
    name = "graph_traverse",
    description = "BFS-traverse the knowledge graph from an anchor entity. Returns typed entity-relationship triples reachable within the specified hop count. Filter by domain_id to restrict results to a subject area."
)]
pub async fn graph_traverse(
    &self,
    Parameters(params): Parameters<GraphTraverseRequest>,
) -> Result<CallToolResult, ErrorData> {
    let anchor = params.anchor.trim();
    if anchor.is_empty() {
        return Err(ErrorData::invalid_params(
            "anchor is required",
            Some(serde_json::json!({ "field": "anchor" })),
        ));
    }

    let Some(token) = self.api_token.as_deref() else {
        return Ok(CallToolResult::error(vec![Content::text(
            "graph_traverse_unavailable: configure XCS_API_TOKEN or APP_API_TOKEN.",
        )]));
    };

    let mut query = vec![
        ("anchor".to_string(), anchor.to_string()),
        ("hops".to_string(), params.hops.to_string()),
    ];
    if let Some(domain) = params.domain_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        query.push(("domain_id".to_string(), domain.to_string()));
    }

    let url = format!("{}/api/v1/graph/traverse", self.api_base_url);
    let response = self
        .api_client
        .get(url)
        .bearer_auth(token)
        .query(&query)
        .send()
        .await
        .map_err(internal_error)?
        .error_for_status()
        .map_err(internal_error)?;

    let body: serde_json::Value = response.json().await.map_err(internal_error)?;
    Ok(CallToolResult::structured(body))
}
```

---

## Step 9 — AppError variants

### Edit: `backend/src/error.rs`

Ensure `AppError::BadRequest(String)` exists. Check current variants — if missing, add:

```rust
BadRequest(String),
```

And the `IntoResponse` arm:
```rust
AppError::BadRequest(msg) => (
    StatusCode::BAD_REQUEST,
    Json(AppErrorResponse { error: msg }),
)
    .into_response(),
```

---

## Step 10 — Quality gates

```bash
cd ~/Documents/localProject/xcalibre-server

# Apply migration
cargo sqlx migrate run --database-url sqlite:./library.db

# Run KAG tests (must all pass)
cargo test -p backend test_kag 2>&1 | tail -30

# Full test suite
cargo test --workspace 2>&1 | tail -20

# Zero warnings
cargo clippy -- -D warnings 2>&1 | tail -20

# Zero CVEs
cargo audit
```

Expected: 12/12 tests pass, zero clippy warnings, zero audit CVEs.

---

## Step 11 — Docs updates

### Edit: `docs/STATE.md`

Update the Phase 22-KAG row:
```
| Phase 22-KAG | KAG graph layer (knowledge_graph table, graph/ module, 3 endpoints, MCP tool) | ✅ Complete — v2.6.0 |
```

Update migration table — add:
```
| `0034_knowledge_graph.sql` | `knowledge_graph` table + 4 indexes + FTS5 + triggers | ✅ Applied |
```

Update total count line:
```
Total: **45 tables, 29 migrations** across SQLite and MariaDB migration sets.
```

Update Overall Status:
```
## Overall Status: Phase 22-KAG Complete — v2.6.0
```

### Edit: `CLAUDE.md`

Update `**Current version: 2.4.0**` → `**Current version: 2.6.0**`

Update `Phases 1–28 complete. 44 tables, 28 migrations.` → `Phases 1–28 + 22-KAG complete. 45 tables, 29 migrations.`

---

## Step 12 — Version bump and tag

### Edit: `backend/Cargo.toml`

```toml
version = "2.6.0"
```

### Edit: `xs-mcp/Cargo.toml`

```toml
version = "2.6.0"
```

### Edit: `xs-migrate/Cargo.toml`

```toml
version = "2.6.0"
```

---

## Commit and tag

```bash
cd ~/Documents/localProject/xcalibre-server

git add \
  backend/migrations/sqlite/0034_knowledge_graph.sql \
  backend/migrations/mariadb/0034_knowledge_graph.sql \
  backend/src/config.rs \
  backend/src/graph/mod.rs \
  backend/src/graph/traverse.rs \
  backend/src/api/graph.rs \
  backend/src/api/search.rs \
  backend/src/api/mod.rs \
  xs-mcp/src/tools/mod.rs \
  backend/Cargo.toml \
  xs-mcp/Cargo.toml \
  xs-migrate/Cargo.toml \
  docs/STATE.md \
  CLAUDE.md

git commit -m "Phase 22-KAG — KAG knowledge graph layer (v2.6.0)"
git tag v2.6.0
git push && git push --tags
```
