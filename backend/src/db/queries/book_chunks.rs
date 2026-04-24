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
