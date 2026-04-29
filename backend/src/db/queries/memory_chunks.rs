//! Merlin memory chunk storage and search queries.
//!
//! Touches: `memory_chunks`, `memory_chunks_fts`.
//!
//! The query patterns mirror `book_chunks`:
//! - insert and fetch a chunk record
//! - look up a chunk by id
//! - delete a chunk by id
//! - full-text search over the FTS5 virtual table
//! - semantic search over the embedding blob with `vec_distance_cosine`

use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use utoipa::ToSchema;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct MemoryChunk {
    pub id: String,
    pub session_id: Option<String>,
    pub project_path: Option<String>,
    pub chunk_type: String,
    pub text: String,
    pub tags: Option<String>,
    pub model_id: String,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct InsertMemoryChunkParams<'a> {
    pub id: &'a str,
    pub session_id: Option<&'a str>,
    pub project_path: Option<&'a str>,
    pub chunk_type: &'a str,
    pub text: &'a str,
    pub tags: Option<&'a str>,
    pub model_id: &'a str,
    pub embedding: Option<&'a [u8]>,
}

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct MemoryChunkSearchResult {
    pub id: String,
    pub session_id: Option<String>,
    pub project_path: Option<String>,
    pub chunk_type: String,
    pub text: String,
    pub tags: Option<String>,
    pub score: f32,
}

pub async fn insert_memory_chunk(
    pool: &SqlitePool,
    params: &InsertMemoryChunkParams<'_>,
) -> sqlx::Result<MemoryChunk> {
    let embedding = params.embedding.map(|value| value.to_vec());
    let row = sqlx::query(
        r#"
        INSERT INTO memory_chunks (
            id, session_id, project_path, chunk_type, text, tags, model_id, embedding
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        RETURNING id, session_id, project_path, chunk_type, text, tags, model_id, created_at
        "#,
    )
    .bind(params.id)
    .bind(params.session_id)
    .bind(params.project_path)
    .bind(params.chunk_type)
    .bind(params.text)
    .bind(params.tags)
    .bind(params.model_id)
    .bind(embedding)
    .fetch_one(pool)
    .await?;

    Ok(row_to_memory_chunk(row))
}

pub async fn get_memory_chunk(pool: &SqlitePool, id: &str) -> sqlx::Result<Option<MemoryChunk>> {
    let row = sqlx::query(
        r#"
        SELECT id, session_id, project_path, chunk_type, text, tags, model_id, created_at
        FROM memory_chunks
        WHERE id = ?
        LIMIT 1
        "#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(row_to_memory_chunk))
}

pub async fn delete_memory_chunk(pool: &SqlitePool, id: &str) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM memory_chunks WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn count_searchable_memory_chunks(
    pool: &SqlitePool,
    project_path: Option<&str>,
) -> sqlx::Result<i64> {
    let count = if let Some(project_path) = project_path {
        sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(1)
            FROM memory_chunks
            WHERE project_path = ?
            "#,
        )
        .bind(project_path)
        .fetch_one(pool)
        .await?
    } else {
        sqlx::query_scalar::<_, i64>("SELECT COUNT(1) FROM memory_chunks")
            .fetch_one(pool)
            .await?
    };

    Ok(count)
}

pub async fn search_memory_chunks_fts(
    pool: &SqlitePool,
    q: &str,
    limit: u32,
    project_path: Option<&str>,
) -> sqlx::Result<Vec<MemoryChunkSearchResult>> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            mc.id AS id,
            mc.session_id AS session_id,
            mc.project_path AS project_path,
            mc.chunk_type AS chunk_type,
            mc.text AS text,
            mc.tags AS tags,
            rank AS score
        FROM memory_chunks_fts
        INNER JOIN memory_chunks mc ON mc.rowid = memory_chunks_fts.rowid
        WHERE memory_chunks_fts MATCH 
        "#,
    );
    query.push_bind(q);
    query.push(" AND (");
    query.push_bind(project_path);
    query.push(" IS NULL OR mc.project_path = ");
    query.push_bind(project_path);
    query.push(")");
    query.push(" ORDER BY score ASC, mc.created_at DESC, mc.id ASC LIMIT ");
    query.push_bind(i64::from(limit.max(1)));

    let rows = query.build().fetch_all(pool).await?;
    Ok(rows.into_iter().map(row_to_search_result).collect())
}

pub async fn search_memory_chunks_semantic(
    pool: &SqlitePool,
    query_embedding: &[u8],
    limit: u32,
    model_id: &str,
    project_path: Option<&str>,
) -> sqlx::Result<Vec<MemoryChunkSearchResult>> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            id,
            session_id,
            project_path,
            chunk_type,
            text,
            tags,
            vec_distance_cosine(embedding, ?) AS score
        FROM memory_chunks
        WHERE model_id = ?
          AND embedding IS NOT NULL
          AND (
        "#,
    );
    query.push_bind(model_id);
    query.push_bind(query_embedding);
    query.push(" IS NULL OR project_path = ");
    query.push_bind(project_path);
    query.push(")");
    query.push(" ORDER BY score ASC, created_at DESC, id ASC LIMIT ");
    query.push_bind(i64::from(limit.max(1)));

    let rows = query.build().fetch_all(pool).await?;
    Ok(rows.into_iter().map(row_to_search_result).collect())
}

/// Serialize a cosine embedding as a little-endian `f32` BLOB for sqlite-vec.
pub fn serialize_embedding(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(vector));
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn row_to_memory_chunk(row: sqlx::sqlite::SqliteRow) -> MemoryChunk {
    MemoryChunk {
        id: row.get("id"),
        session_id: row.get("session_id"),
        project_path: row.get("project_path"),
        chunk_type: row.get("chunk_type"),
        text: row.get("text"),
        tags: row.get("tags"),
        model_id: row.get("model_id"),
        created_at: row.get("created_at"),
    }
}

fn row_to_search_result(row: sqlx::sqlite::SqliteRow) -> MemoryChunkSearchResult {
    MemoryChunkSearchResult {
        id: row.get("id"),
        session_id: row.get("session_id"),
        project_path: row.get("project_path"),
        chunk_type: row.get("chunk_type"),
        text: row.get("text"),
        tags: row.get("tags"),
        score: row.get::<f64, _>("score") as f32,
    }
}
