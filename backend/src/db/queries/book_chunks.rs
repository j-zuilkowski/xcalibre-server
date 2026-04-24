use crate::ingest::chunker::ChunkType;
use chrono::Utc;
use serde::Serialize;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use utoipa::ToSchema;
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, ToSchema)]
pub struct BookChunkRecord {
    pub id: String,
    pub book_id: String,
    pub chunk_index: i64,
    pub chapter_index: i64,
    pub heading_path: Option<String>,
    pub chunk_type: ChunkType,
    pub text: String,
    pub word_count: i64,
    pub has_image: bool,
}

#[derive(Clone, Debug)]
pub struct BookChunkInsert {
    pub chunk_index: usize,
    pub chapter_index: usize,
    pub heading_path: Option<String>,
    pub chunk_type: ChunkType,
    pub text: String,
    pub word_count: usize,
    pub has_image: bool,
    pub embedding: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub struct ChunkSearchFilters<'a> {
    pub book_ids: &'a [String],
    pub collection_id: Option<&'a str>,
    pub chunk_type: Option<&'a str>,
}

#[derive(Clone, Debug)]
pub struct ChunkSearchRecord {
    pub id: String,
    pub book_id: String,
    pub chunk_index: i64,
    pub heading_path: Option<String>,
    pub chunk_type: ChunkType,
    pub text: String,
    pub word_count: i64,
    pub bm25_score: Option<f32>,
    pub cosine_score: Option<f32>,
}

pub async fn count_book_chunks(db: &SqlitePool, book_id: &str) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM book_chunks WHERE book_id = ?")
        .bind(book_id)
        .fetch_one(db)
        .await?;
    Ok(count)
}

pub async fn list_book_chunks(
    db: &SqlitePool,
    book_id: &str,
    chunk_type: Option<ChunkType>,
) -> anyhow::Result<Vec<BookChunkRecord>> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT id, book_id, chunk_index, chapter_index, heading_path, chunk_type, text, word_count, has_image
        FROM book_chunks
        WHERE book_id = 
        "#,
    );
    query.push_bind(book_id);

    if let Some(chunk_type) = chunk_type {
        query.push(" AND chunk_type = ");
        query.push_bind(chunk_type.as_str());
    }

    query.push(" ORDER BY chunk_index ASC, id ASC");

    let rows = query.build().fetch_all(db).await?;
    Ok(rows.into_iter().map(row_to_record).collect())
}

pub async fn replace_book_chunks(
    db: &SqlitePool,
    book_id: &str,
    chunks: &[BookChunkInsert],
) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM book_chunks WHERE book_id = ?")
        .bind(book_id)
        .execute(tx.as_mut())
        .await?;
    insert_book_chunks_in_tx(&mut tx, book_id, chunks).await?;
    tx.commit().await?;
    Ok(())
}

pub async fn insert_book_chunks_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    book_id: &str,
    chunks: &[BookChunkInsert],
) -> anyhow::Result<()> {
    if chunks.is_empty() {
        return Ok(());
    }

    let now = Utc::now().to_rfc3339();
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        INSERT INTO book_chunks (
            id, book_id, chunk_index, chapter_index, heading_path, chunk_type,
            text, word_count, has_image, embedding, created_at
        )
        "#,
    );
    query.push_values(chunks.iter(), |mut builder, chunk| {
        builder
            .push_bind(Uuid::new_v4().to_string())
            .push_bind(book_id)
            .push_bind(chunk.chunk_index as i64)
            .push_bind(chunk.chapter_index as i64)
            .push_bind(chunk.heading_path.clone())
            .push_bind(chunk.chunk_type.as_str())
            .push_bind(chunk.text.as_str())
            .push_bind(chunk.word_count as i64)
            .push_bind(i64::from(chunk.has_image))
            .push_bind(chunk.embedding.clone())
            .push_bind(now.clone());
    });

    query.build().execute(tx.as_mut()).await?;
    Ok(())
}

pub async fn count_searchable_book_chunks(
    db: &SqlitePool,
    filters: &ChunkSearchFilters<'_>,
) -> anyhow::Result<i64> {
    let mut query =
        QueryBuilder::<Sqlite>::new("SELECT COUNT(1) AS count FROM book_chunks bc");
    if let Some(collection_id) = filters.collection_id {
        query.push(" INNER JOIN shelf_books sb ON sb.book_id = bc.book_id AND sb.shelf_id = ");
        query.push_bind(collection_id);
    }
    query.push(" WHERE 1 = 1");
    apply_chunk_filters(&mut query, "bc", filters);

    let count: i64 = query.build_query_scalar().fetch_one(db).await?;
    Ok(count)
}

pub async fn search_chunks_bm25(
    db: &SqlitePool,
    query_text: &str,
    filters: &ChunkSearchFilters<'_>,
    limit: i64,
) -> anyhow::Result<Vec<ChunkSearchRecord>> {
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            bc.id AS id,
            bc.book_id AS book_id,
            bc.chunk_index AS chunk_index,
            bc.heading_path AS heading_path,
            bc.chunk_type AS chunk_type,
            bc.text AS text,
            bc.word_count AS word_count,
            bm25(book_chunks_fts) AS bm25_score,
            NULL AS cosine_distance
        FROM book_chunks_fts
        INNER JOIN book_chunks bc ON bc.rowid = book_chunks_fts.rowid
        "#,
    );
    if let Some(collection_id) = filters.collection_id {
        query.push(" INNER JOIN shelf_books sb ON sb.book_id = bc.book_id AND sb.shelf_id = ");
        query.push_bind(collection_id);
    }
    query.push(" WHERE book_chunks_fts MATCH ");
    query.push_bind(query_text);
    apply_chunk_filters(&mut query, "bc", filters);
    query.push(" ORDER BY bm25_score ASC, bc.book_id ASC, bc.chunk_index ASC, bc.id ASC LIMIT ");
    query.push_bind(limit.max(1));

    let rows = query.build().fetch_all(db).await?;
    Ok(rows.into_iter().map(row_to_search_record).collect())
}

pub async fn search_chunks_semantic(
    db: &SqlitePool,
    vector: &[f32],
    filters: &ChunkSearchFilters<'_>,
    limit: i64,
) -> anyhow::Result<Vec<ChunkSearchRecord>> {
    let vector_blob = vec_to_blob(vector);
    let mut query = QueryBuilder::<Sqlite>::new(
        r#"
        SELECT
            bc.id AS id,
            bc.book_id AS book_id,
            bc.chunk_index AS chunk_index,
            bc.heading_path AS heading_path,
            bc.chunk_type AS chunk_type,
            bc.text AS text,
            bc.word_count AS word_count,
            NULL AS bm25_score,
            vec_distance_cosine(bc.embedding, ?) AS cosine_distance
        FROM book_chunks bc
        "#,
    );
    query.push_bind(vector_blob);
    if let Some(collection_id) = filters.collection_id {
        query.push(" INNER JOIN shelf_books sb ON sb.book_id = bc.book_id AND sb.shelf_id = ");
        query.push_bind(collection_id);
    }
    query.push(" WHERE bc.embedding IS NOT NULL");
    apply_chunk_filters(&mut query, "bc", filters);
    query.push(" ORDER BY cosine_distance ASC, bc.book_id ASC, bc.chunk_index ASC, bc.id ASC LIMIT ");
    query.push_bind(limit.max(1));

    let rows = query.build().fetch_all(db).await?;
    Ok(rows.into_iter().map(row_to_search_record).collect())
}

fn apply_chunk_filters<'a>(
    query: &mut QueryBuilder<'a, Sqlite>,
    alias: &str,
    filters: &ChunkSearchFilters<'a>,
) {
    if !filters.book_ids.is_empty() {
        query.push(" AND ");
        query.push(alias);
        query.push(".book_id IN (");
        {
            let mut separated = query.separated(", ");
            for book_id in filters.book_ids {
                separated.push_bind(book_id);
            }
        }
        query.push(")");
    }

    if let Some(chunk_type) = filters.chunk_type {
        query.push(" AND ");
        query.push(alias);
        query.push(".chunk_type = ");
        query.push_bind(chunk_type);
    }
}

fn row_to_search_record(row: sqlx::sqlite::SqliteRow) -> ChunkSearchRecord {
    let chunk_type = row
        .get::<String, _>("chunk_type")
        .parse()
        .unwrap_or(ChunkType::Text);

    ChunkSearchRecord {
        id: row.get("id"),
        book_id: row.get("book_id"),
        chunk_index: row.get("chunk_index"),
        heading_path: row.get("heading_path"),
        chunk_type,
        text: row.get("text"),
        word_count: row.get("word_count"),
        bm25_score: row.get::<Option<f64>, _>("bm25_score").map(|score| score as f32),
        cosine_score: row
            .get::<Option<f64>, _>("cosine_distance")
            .map(|distance| (1.0_f64 - distance) as f32),
    }
}

fn vec_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(vector));
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn row_to_record(row: sqlx::sqlite::SqliteRow) -> BookChunkRecord {
    let chunk_type = row
        .get::<String, _>("chunk_type")
        .parse()
        .unwrap_or(ChunkType::Text);

    BookChunkRecord {
        id: row.get("id"),
        book_id: row.get("book_id"),
        chunk_index: row.get::<i64, _>("chunk_index"),
        chapter_index: row.get::<i64, _>("chapter_index"),
        heading_path: row.get("heading_path"),
        chunk_type,
        text: row.get("text"),
        word_count: row.get::<i64, _>("word_count"),
        has_image: row.get::<i64, _>("has_image") != 0,
    }
}
