use crate::db::models::Book;
use sqlx::SqlitePool;

pub async fn _placeholder(_db: &SqlitePool) -> anyhow::Result<()> {
    Ok(())
}

pub async fn _load_book(_db: &SqlitePool, _id: &str) -> anyhow::Result<Option<Book>> {
    Ok(None)
}

