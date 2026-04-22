use anyhow::Context;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug, Serialize)]
pub struct UserTagRestriction {
    pub user_id: String,
    pub tag_id: String,
    pub tag_name: String,
    pub mode: String,
}

pub async fn get_restrictions(
    db: &SqlitePool,
    user_id: &str,
) -> anyhow::Result<Vec<UserTagRestriction>> {
    let rows = sqlx::query(
        r#"
        SELECT
            r.user_id AS user_id,
            r.tag_id AS tag_id,
            t.name AS tag_name,
            r.mode AS mode
        FROM user_tag_restrictions r
        INNER JOIN tags t ON t.id = r.tag_id
        WHERE r.user_id = ?
        ORDER BY t.name ASC, t.id ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await
    .context("get tag restrictions")?;

    Ok(rows
        .into_iter()
        .map(|row| UserTagRestriction {
            user_id: row.get("user_id"),
            tag_id: row.get("tag_id"),
            tag_name: row.get("tag_name"),
            mode: row.get("mode"),
        })
        .collect())
}

pub async fn set_restriction(
    db: &SqlitePool,
    user_id: &str,
    tag_id: &str,
    mode: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO user_tag_restrictions (user_id, tag_id, mode)
        VALUES (?, ?, ?)
        ON CONFLICT(user_id, tag_id) DO UPDATE SET mode = excluded.mode
        "#,
    )
    .bind(user_id)
    .bind(tag_id)
    .bind(mode)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn remove_restriction(
    db: &SqlitePool,
    user_id: &str,
    tag_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM user_tag_restrictions
        WHERE user_id = ? AND tag_id = ?
        "#,
    )
    .bind(user_id)
    .bind(tag_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}
