use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Annotation {
    pub id: String,
    pub user_id: String,
    pub book_id: String,
    pub r#type: String,
    pub cfi_range: String,
    pub highlighted_text: Option<String>,
    pub note: Option<String>,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug)]
pub struct NewAnnotation {
    pub user_id: String,
    pub book_id: String,
    pub annotation_type: String,
    pub cfi_range: String,
    pub highlighted_text: Option<String>,
    pub note: Option<String>,
    pub color: String,
}

#[derive(Clone, Debug, Default)]
pub struct AnnotationPatch {
    pub note: Option<Option<String>>,
    pub color: Option<String>,
}

pub async fn list_annotations(
    db: &SqlitePool,
    user_id: &str,
    book_id: &str,
) -> anyhow::Result<Vec<Annotation>> {
    let rows = sqlx::query(
        r#"
        SELECT id, user_id, book_id, type, cfi_range, highlighted_text, note, color, created_at, updated_at
        FROM book_annotations
        WHERE user_id = ? AND book_id = ?
        ORDER BY cfi_range ASC, created_at ASC, id ASC
        "#,
    )
    .bind(user_id)
    .bind(book_id)
    .fetch_all(db)
    .await?;

    rows.into_iter()
        .map(row_to_annotation)
        .collect::<anyhow::Result<Vec<_>>>()
}

pub async fn create_annotation(
    db: &SqlitePool,
    input: NewAnnotation,
) -> anyhow::Result<Annotation> {
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO book_annotations (
            id, user_id, book_id, type, cfi_range, highlighted_text, note, color, created_at, updated_at
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&id)
    .bind(&input.user_id)
    .bind(&input.book_id)
    .bind(&input.annotation_type)
    .bind(&input.cfi_range)
    .bind(&input.highlighted_text)
    .bind(&input.note)
    .bind(&input.color)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?;

    get_annotation_by_id(db, &id)
        .await?
        .context("created annotation not found")
}

pub async fn update_annotation(
    db: &SqlitePool,
    ann_id: &str,
    user_id: &str,
    patch: AnnotationPatch,
) -> anyhow::Result<Option<Annotation>> {
    if patch.note.is_none() && patch.color.is_none() {
        return get_annotation_by_id_for_user(db, ann_id, user_id).await;
    }

    let note_set = patch.note.is_some();
    let note_value = patch.note.and_then(|value| value);
    let color_set = patch.color.is_some();
    let color_value = patch.color;
    let now = Utc::now().to_rfc3339();

    let result = sqlx::query(
        r#"
        UPDATE book_annotations
        SET
            note = CASE WHEN ? = 1 THEN ? ELSE note END,
            color = CASE WHEN ? = 1 THEN ? ELSE color END,
            updated_at = CASE WHEN ? = 1 OR ? = 1 THEN ? ELSE updated_at END
        WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(i64::from(note_set))
    .bind(note_value)
    .bind(i64::from(color_set))
    .bind(color_value)
    .bind(i64::from(note_set))
    .bind(i64::from(color_set))
    .bind(now)
    .bind(ann_id)
    .bind(user_id)
    .execute(db)
    .await?;

    if result.rows_affected() == 0 {
        return Ok(None);
    }

    get_annotation_by_id_for_user(db, ann_id, user_id).await
}

pub async fn delete_annotation(
    db: &SqlitePool,
    ann_id: &str,
    user_id: &str,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        r#"
        DELETE FROM book_annotations
        WHERE id = ? AND user_id = ?
        "#,
    )
    .bind(ann_id)
    .bind(user_id)
    .execute(db)
    .await?;

    Ok(result.rows_affected() > 0)
}

pub async fn get_annotation_by_id(
    db: &SqlitePool,
    ann_id: &str,
) -> anyhow::Result<Option<Annotation>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, book_id, type, cfi_range, highlighted_text, note, color, created_at, updated_at
        FROM book_annotations
        WHERE id = ?
        LIMIT 1
        "#,
    )
    .bind(ann_id)
    .fetch_optional(db)
    .await?;

    row.map(row_to_annotation).transpose()
}

async fn get_annotation_by_id_for_user(
    db: &SqlitePool,
    ann_id: &str,
    user_id: &str,
) -> anyhow::Result<Option<Annotation>> {
    let row = sqlx::query(
        r#"
        SELECT id, user_id, book_id, type, cfi_range, highlighted_text, note, color, created_at, updated_at
        FROM book_annotations
        WHERE id = ? AND user_id = ?
        LIMIT 1
        "#,
    )
    .bind(ann_id)
    .bind(user_id)
    .fetch_optional(db)
    .await?;

    row.map(row_to_annotation).transpose()
}

fn row_to_annotation(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<Annotation> {
    Ok(Annotation {
        id: row.get("id"),
        user_id: row.get("user_id"),
        book_id: row.get("book_id"),
        r#type: row.get("type"),
        cfi_range: row.get("cfi_range"),
        highlighted_text: row.get("highlighted_text"),
        note: row.get("note"),
        color: row.get("color"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}
