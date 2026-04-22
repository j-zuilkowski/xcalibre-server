use chrono::Utc;
use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug, Default)]
pub struct BookUserState {
    pub user_id: String,
    pub book_id: String,
    pub is_read: bool,
    pub is_archived: bool,
    pub updated_at: String,
}

pub async fn get_state(
    db: &SqlitePool,
    user_id: &str,
    book_id: &str,
) -> anyhow::Result<Option<BookUserState>> {
    let row = sqlx::query(
        r#"
        SELECT user_id, book_id, is_read, is_archived, updated_at
        FROM book_user_state
        WHERE user_id = ? AND book_id = ?
        LIMIT 1
        "#,
    )
    .bind(user_id)
    .bind(book_id)
    .fetch_optional(db)
    .await?;

    Ok(row.map(|row| BookUserState {
        user_id: row.get("user_id"),
        book_id: row.get("book_id"),
        is_read: row.get::<i64, _>("is_read") != 0,
        is_archived: row.get::<i64, _>("is_archived") != 0,
        updated_at: row.get("updated_at"),
    }))
}

pub async fn set_read(
    db: &SqlitePool,
    user_id: &str,
    book_id: &str,
    is_read: bool,
) -> anyhow::Result<()> {
    let current = get_state(db, user_id, book_id).await?;
    upsert_state(
        db,
        user_id,
        book_id,
        is_read,
        current.map(|state| state.is_archived).unwrap_or(false),
    )
    .await
}

pub async fn set_archived(
    db: &SqlitePool,
    user_id: &str,
    book_id: &str,
    is_archived: bool,
) -> anyhow::Result<()> {
    let current = get_state(db, user_id, book_id).await?;
    upsert_state(
        db,
        user_id,
        book_id,
        current.map(|state| state.is_read).unwrap_or(false),
        is_archived,
    )
    .await
}

async fn upsert_state(
    db: &SqlitePool,
    user_id: &str,
    book_id: &str,
    is_read: bool,
    is_archived: bool,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO book_user_state (user_id, book_id, is_read, is_archived, updated_at)
        VALUES (?, ?, ?, ?, ?)
        ON CONFLICT(user_id, book_id) DO UPDATE SET
            is_read = excluded.is_read,
            is_archived = excluded.is_archived,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(user_id)
    .bind(book_id)
    .bind(i64::from(is_read))
    .bind(i64::from(is_archived))
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}
