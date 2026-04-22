use chrono::Utc;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug, Default)]
pub struct DownloadHistoryEntry {
    pub id: String,
    pub book_id: String,
    pub title: String,
    pub format: String,
    pub downloaded_at: String,
}

#[derive(Clone, Debug, Default)]
pub struct DownloadHistoryPage {
    pub items: Vec<DownloadHistoryEntry>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

pub async fn insert_download_history(
    db: &SqlitePool,
    user_id: &str,
    book_id: &str,
    format: &str,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO download_history (id, user_id, book_id, format, downloaded_at)
        VALUES (?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(user_id)
    .bind(book_id)
    .bind(format.trim().to_string())
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn list_download_history(
    db: &SqlitePool,
    user_id: &str,
    page: i64,
    page_size: i64,
) -> anyhow::Result<DownloadHistoryPage> {
    let page_size = page_size.clamp(1, 100);
    let page = page.max(1);
    let offset = (page - 1) * page_size;

    let total: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM download_history
        WHERE user_id = ?
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let rows = sqlx::query(
        r#"
        SELECT dh.id, dh.book_id, b.title, dh.format, dh.downloaded_at
        FROM download_history dh
        INNER JOIN books b ON b.id = dh.book_id
        WHERE dh.user_id = ?
        ORDER BY dh.downloaded_at DESC, dh.id DESC
        LIMIT ? OFFSET ?
        "#,
    )
    .bind(user_id)
    .bind(page_size)
    .bind(offset)
    .fetch_all(db)
    .await?;

    let items = rows
        .into_iter()
        .map(|row| DownloadHistoryEntry {
            id: row.get("id"),
            book_id: row.get("book_id"),
            title: row.get("title"),
            format: row.get("format"),
            downloaded_at: row.get("downloaded_at"),
        })
        .collect::<Vec<_>>();

    Ok(DownloadHistoryPage {
        items,
        total,
        page,
        page_size,
    })
}
