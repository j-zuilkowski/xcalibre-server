use crate::{
    db::queries::llm as llm_queries,
    llm::embeddings::EmbeddingClient,
    search::{SearchHit, SearchPage},
};
use chrono::Utc;
use sqlx::{Row, SqlitePool};

#[derive(Clone)]
pub struct SemanticSearch {
    db: SqlitePool,
    embeddings: EmbeddingClient,
}

#[derive(Clone, Debug)]
pub struct BookSemanticDocument {
    pub title: String,
    pub authors: String,
    pub description: String,
}

impl SemanticSearch {
    pub fn new(db: SqlitePool, embeddings: EmbeddingClient) -> Self {
        Self { db, embeddings }
    }

    pub fn is_configured(&self) -> bool {
        self.embeddings.is_configured()
    }

    pub fn model_id(&self) -> &str {
        self.embeddings.model_id()
    }

    pub async fn index_book(
        &self,
        book_id: &str,
        title: &str,
        authors: &str,
        description: &str,
    ) -> anyhow::Result<()> {
        let content = format!(
            "{} by {}. {}",
            title.trim(),
            authors.trim(),
            description.trim()
        );
        let result = self.index_book_embedding(book_id, &content).await;

        match result {
            Ok(()) => {
                llm_queries::mark_running_semantic_jobs_for_book_completed(&self.db, book_id)
                    .await?;
                Ok(())
            }
            Err(err) => {
                let message = format!("{err:#}");
                let _ = llm_queries::mark_running_semantic_jobs_for_book_failed(
                    &self.db, book_id, &message,
                )
                .await;
                Err(err)
            }
        }
    }

    pub async fn search_semantic(
        &self,
        query: &str,
        page: u32,
        page_size: u32,
    ) -> anyhow::Result<SearchPage> {
        let page = page.max(1);
        let page_size = clamp_page_size(page_size);
        let offset = i64::from(page.saturating_sub(1)) * i64::from(page_size);

        let vector = self.embeddings.embed(query).await?;
        let vector_blob = vec_to_blob(&vector);

        let rows = sqlx::query(
            r#"
            SELECT book_id, vec_distance_cosine(embedding, ?) AS distance
            FROM book_embeddings
            ORDER BY distance ASC
            LIMIT ? OFFSET ?
            "#,
        )
        .bind(vector_blob)
        .bind(i64::from(page_size))
        .bind(offset)
        .fetch_all(&self.db)
        .await?;

        let hits = rows
            .into_iter()
            .map(|row| {
                let distance: f64 = row.get("distance");
                SearchHit {
                    book_id: row.get("book_id"),
                    score: (1.0_f64 - distance) as f32,
                }
            })
            .collect::<Vec<_>>();

        let total: i64 = sqlx::query_scalar("SELECT COUNT(1) FROM book_embeddings")
            .fetch_one(&self.db)
            .await?;

        Ok(SearchPage {
            hits,
            total: total.max(0) as u64,
            page,
            page_size,
        })
    }

    pub async fn load_book_document(
        &self,
        book_id: &str,
    ) -> anyhow::Result<Option<BookSemanticDocument>> {
        load_book_semantic_document(&self.db, book_id).await
    }

    async fn index_book_embedding(&self, book_id: &str, content: &str) -> anyhow::Result<()> {
        let vector = self.embeddings.embed(content).await?;
        let vector_blob = vec_to_blob(&vector);
        let now = Utc::now().to_rfc3339();

        sqlx::query(
            r#"
            INSERT INTO book_embeddings (book_id, model_id, embedding, created_at)
            VALUES (?, ?, ?, ?)
            ON CONFLICT(book_id) DO UPDATE SET
                model_id = excluded.model_id,
                embedding = excluded.embedding,
                created_at = excluded.created_at
            "#,
        )
        .bind(book_id)
        .bind(self.model_id())
        .bind(vector_blob)
        .bind(now)
        .execute(&self.db)
        .await?;

        Ok(())
    }
}

pub async fn load_book_semantic_document(
    db: &SqlitePool,
    book_id: &str,
) -> anyhow::Result<Option<BookSemanticDocument>> {
    let row = sqlx::query(
        r#"
        SELECT
            b.title AS title,
            COALESCE((
                SELECT group_concat(author_name, ', ')
                FROM (
                    SELECT a.name AS author_name
                    FROM book_authors ba
                    INNER JOIN authors a ON a.id = ba.author_id
                    WHERE ba.book_id = b.id
                    ORDER BY ba.display_order ASC, a.sort_name ASC
                )
            ), 'Unknown Author') AS authors,
            COALESCE(b.description, '') AS description
        FROM books b
        WHERE b.id = ?
        "#,
    )
    .bind(book_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| BookSemanticDocument {
        title: row.get("title"),
        authors: row.get("authors"),
        description: row.get("description"),
    }))
}

fn vec_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(vector));
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn clamp_page_size(page_size: u32) -> u32 {
    match page_size {
        0 => 24,
        n if n > 50 => 50,
        n => n,
    }
}
