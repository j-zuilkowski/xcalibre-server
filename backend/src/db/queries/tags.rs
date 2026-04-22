use anyhow::Context;
use serde::Serialize;
use sqlx::{Row, SqlitePool};

#[derive(Clone, Debug, Serialize)]
pub struct TagLookupItem {
    pub id: String,
    pub name: String,
}

pub async fn search_tags(
    db: &SqlitePool,
    query: Option<&str>,
    limit: i64,
) -> anyhow::Result<Vec<TagLookupItem>> {
    let limit = limit.clamp(1, 50);
    let mut sql = String::from("SELECT id, name FROM tags");
    let trimmed = query.map(str::trim).filter(|value| !value.is_empty());
    if trimmed.is_some() {
        sql.push_str(" WHERE lower(name) LIKE lower(?)");
    }
    sql.push_str(" ORDER BY name ASC LIMIT ?");

    let mut statement = sqlx::query(&sql);
    if let Some(value) = trimmed {
        statement = statement.bind(format!("%{value}%"));
    }
    let rows = statement
        .bind(limit)
        .fetch_all(db)
        .await
        .context("search tags")?;

    Ok(rows
        .into_iter()
        .map(|row| TagLookupItem {
            id: row.get("id"),
            name: row.get("name"),
        })
        .collect())
}

pub async fn find_tag_by_id(
    db: &SqlitePool,
    tag_id: &str,
) -> anyhow::Result<Option<TagLookupItem>> {
    let row = sqlx::query("SELECT id, name FROM tags WHERE id = ?")
        .bind(tag_id)
        .fetch_optional(db)
        .await?;

    Ok(row.map(|row| TagLookupItem {
        id: row.get("id"),
        name: row.get("name"),
    }))
}
