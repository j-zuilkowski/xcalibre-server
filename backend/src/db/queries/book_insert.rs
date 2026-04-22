use super::books::{
    get_book_by_id, get_or_create_author, normalize_author_names, optional_trimmed, UploadBookInput,
};
use crate::db::models::Book;
use chrono::Utc;
use sqlx::SqlitePool;
use uuid::Uuid;

pub async fn insert_uploaded_book_impl(
    db: &SqlitePool,
    input: UploadBookInput,
) -> anyhow::Result<Book> {
    let UploadBookInput {
        library_id,
        title,
        sort_title,
        description,
        pubdate,
        language,
        rating,
        document_type,
        series_id,
        series_index,
        author_names,
        identifiers,
        format,
        format_path,
        format_size_bytes,
    } = input;

    let now = Utc::now().to_rfc3339();
    let book_id = Uuid::new_v4().to_string();
    let mut tx = db.begin().await?;

    let mut insert_book = sqlx::query(
        r#"
        INSERT INTO books (
            id, library_id, title, sort_title, description, pubdate, language, rating, series_id, series_index,
            document_type, has_cover, cover_path, flags, indexed_at, created_at, last_modified
        ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0, NULL, NULL, NULL, ?, ?)
        "#,
    );
    insert_book = insert_book.bind(&book_id);
    insert_book = insert_book.bind(library_id.trim());
    insert_book = insert_book.bind(title.trim());
    insert_book = insert_book.bind(sort_title.trim());
    insert_book = insert_book.bind(optional_trimmed(description));
    insert_book = insert_book.bind(optional_trimmed(pubdate));
    insert_book = insert_book.bind(optional_trimmed(language));
    insert_book = insert_book.bind(rating);
    insert_book = insert_book.bind(optional_trimmed(series_id));
    insert_book = insert_book.bind(series_index);
    insert_book = insert_book.bind(document_type.trim().to_lowercase());
    insert_book = insert_book.bind(&now);
    insert_book = insert_book.bind(&now);
    insert_book.execute(&mut *tx).await?;

    let authors = normalize_author_names(author_names);
    for (display_order, author_name) in authors.into_iter().enumerate() {
        let author_id = get_or_create_author(&mut tx, &author_name, &now).await?;
        sqlx::query(
            "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, ?)",
        )
        .bind(&book_id)
        .bind(author_id)
        .bind(display_order as i64)
        .execute(&mut *tx)
        .await?;
    }

    let mut format_insert = sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    );
    format_insert = format_insert.bind(Uuid::new_v4().to_string());
    format_insert = format_insert.bind(&book_id);
    format_insert = format_insert.bind(format.trim().to_uppercase());
    format_insert = format_insert.bind(format_path.trim());
    format_insert = format_insert.bind(format_size_bytes);
    format_insert = format_insert.bind(&now);
    format_insert = format_insert.bind(&now);
    format_insert.execute(&mut *tx).await?;

    let mut seen_id_types = std::collections::BTreeSet::new();
    for id in identifiers {
        let id_type = id.id_type.trim().to_lowercase();
        let value = id.value.trim().to_string();
        if id_type.is_empty() || value.is_empty() || !seen_id_types.insert(id_type.clone()) {
            continue;
        }
        sqlx::query(
            r#"
            INSERT INTO identifiers (id, book_id, id_type, value, last_modified)
            VALUES (?, ?, ?, ?, ?)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(&book_id)
        .bind(id_type)
        .bind(value)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    let book = get_book_by_id(db, &book_id, None, None)
        .await?
        .ok_or_else(|| anyhow::anyhow!("uploaded book missing after commit"))?;
    Ok(book)
}
