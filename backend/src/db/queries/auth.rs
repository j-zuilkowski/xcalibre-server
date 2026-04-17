use crate::db::models::User;
use sqlx::SqlitePool;

pub async fn _placeholder(_db: &SqlitePool) -> anyhow::Result<()> {
    Ok(())
}

pub async fn _load_user(_db: &SqlitePool, _username: &str) -> anyhow::Result<Option<User>> {
    Ok(None)
}

