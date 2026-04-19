#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tempfile::TempDir;

const SCHEMA_SQL: &[&str] = &[
    r#"
    CREATE TABLE books (
        id INTEGER PRIMARY KEY,
        title TEXT NOT NULL,
        sort TEXT NOT NULL,
        author_sort TEXT NOT NULL,
        pubdate TEXT,
        series_index REAL,
        rating INTEGER,
        flags INTEGER,
        has_cover INTEGER NOT NULL DEFAULT 0,
        last_modified TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE authors (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        sort TEXT NOT NULL
    )
    "#,
    "CREATE TABLE books_authors_link (book INTEGER NOT NULL, author INTEGER NOT NULL)",
    r#"
    CREATE TABLE series (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL,
        sort TEXT NOT NULL
    )
    "#,
    "CREATE TABLE books_series_link (book INTEGER NOT NULL, series INTEGER NOT NULL)",
    r#"
    CREATE TABLE tags (
        id INTEGER PRIMARY KEY,
        name TEXT NOT NULL
    )
    "#,
    "CREATE TABLE books_tags_link (book INTEGER NOT NULL, tag INTEGER NOT NULL)",
    "CREATE TABLE ratings (id INTEGER PRIMARY KEY, rating INTEGER NOT NULL)",
    "CREATE TABLE books_ratings_link (book INTEGER NOT NULL, rating INTEGER NOT NULL)",
    r#"
    CREATE TABLE comments (
        id INTEGER PRIMARY KEY,
        book INTEGER NOT NULL,
        text TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE identifiers (
        id INTEGER PRIMARY KEY,
        book INTEGER NOT NULL,
        type TEXT NOT NULL,
        val TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE data (
        id INTEGER PRIMARY KEY,
        book INTEGER NOT NULL,
        format TEXT NOT NULL,
        name TEXT NOT NULL,
        uncompressed_size INTEGER
    )
    "#,
];

const FIXTURE_SQL: &[&str] = &[
    r#"
    INSERT INTO books (id, title, sort, author_sort, pubdate, series_index, rating, flags, has_cover, last_modified)
    VALUES
    (1, 'Cover Book', 'Cover Book', 'Author One', '2020-01-01T00:00:00+00:00', 1.0, NULL, 0, 1, '2024-01-01T10:00:00+00:00'),
    (2, 'Isbn Book', 'Isbn Book', 'Author Two', '2021-02-02T00:00:00+00:00', 2.0, NULL, 0, 0, '2024-01-02T10:00:00+00:00'),
    (3, 'Mobi Book', 'Mobi Book', 'Author Three', '2022-03-03T00:00:00+00:00', NULL, NULL, 0, 0, '2024-01-03T10:00:00+00:00')
    "#,
    r#"
    INSERT INTO authors (id, name, sort)
    VALUES
    (1, 'Author One', 'One, Author'),
    (2, 'Author Two', 'Two, Author'),
    (3, 'Author Three', 'Three, Author')
    "#,
    r#"
    INSERT INTO books_authors_link (book, author)
    VALUES (1, 1), (2, 2), (3, 3)
    "#,
    "INSERT INTO series (id, name, sort) VALUES (1, 'Series A', 'Series A')",
    "INSERT INTO books_series_link (book, series) VALUES (1, 1), (2, 1)",
    "INSERT INTO tags (id, name) VALUES (1, 'Fiction'), (2, 'Reference')",
    "INSERT INTO books_tags_link (book, tag) VALUES (1, 1), (2, 2), (3, 1)",
    "INSERT INTO ratings (id, rating) VALUES (1, 8), (2, 6)",
    "INSERT INTO books_ratings_link (book, rating) VALUES (1, 1), (2, 2)",
    r#"
    INSERT INTO comments (id, book, text)
    VALUES
    (1, 1, 'Cover book description'),
    (2, 2, 'ISBN book description'),
    (3, 3, 'MOBI book description')
    "#,
    r#"
    INSERT INTO identifiers (id, book, type, val)
    VALUES
    (1, 1, 'asin', 'B000000001'),
    (2, 2, 'isbn', '9780000000002')
    "#,
    r#"
    INSERT INTO data (id, book, format, name, uncompressed_size)
    VALUES
    (1, 1, 'EPUB', 'cover-book', 111),
    (2, 2, 'EPUB', 'isbn-book', 222),
    (3, 3, 'MOBI', 'mobi-book', 333)
    "#,
];

pub async fn create_calibre_fixture_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect sqlite fixture db");

    for sql in SCHEMA_SQL {
        sqlx::query(sql)
            .execute(&pool)
            .await
            .expect("create fixture schema");
    }
    for sql in FIXTURE_SQL {
        sqlx::query(sql)
            .execute(&pool)
            .await
            .expect("insert fixture rows");
    }
    pool
}

pub fn calibre_fixture_library_dir() -> TempDir {
    let temp_dir = tempfile::tempdir().expect("create fixture temp dir");
    let metadata_path = temp_dir.path().join("metadata.db");

    let conn = Connection::open(&metadata_path).expect("open fixture metadata.db");
    for sql in SCHEMA_SQL {
        conn.execute(sql, []).expect("create fixture schema");
    }
    for sql in FIXTURE_SQL {
        conn.execute(sql, []).expect("insert fixture rows");
    }

    create_book_fixture_file(temp_dir.path(), "Author One", "Cover Book", 1, "cover-book", "epub");
    create_book_fixture_file(temp_dir.path(), "Author Two", "Isbn Book", 2, "isbn-book", "epub");
    create_book_fixture_file(temp_dir.path(), "Author Three", "Mobi Book", 3, "mobi-book", "mobi");
    create_cover_fixture_file(temp_dir.path(), "Author One", "Cover Book", 1);

    temp_dir
}

fn create_book_fixture_file(
    library_path: &Path,
    author_sort: &str,
    title: &str,
    book_id: i64,
    name: &str,
    format: &str,
) {
    let book_dir = book_dir_path(library_path, author_sort, title, book_id);
    fs::create_dir_all(&book_dir).expect("create fixture book directory");
    fs::write(book_dir.join(format!("{name}.{format}")), b"fixture-book-data")
        .expect("write fixture book file");
}

fn create_cover_fixture_file(library_path: &Path, author_sort: &str, title: &str, book_id: i64) {
    let book_dir = book_dir_path(library_path, author_sort, title, book_id);
    fs::create_dir_all(&book_dir).expect("create fixture book directory");
    let image = image::RgbImage::from_pixel(640, 960, image::Rgb([10, 20, 200]));
    image
        .save_with_format(book_dir.join("cover.jpg"), image::ImageFormat::Jpeg)
        .expect("write fixture cover file");
}

fn book_dir_path(library_path: &Path, author_sort: &str, title: &str, book_id: i64) -> PathBuf {
    let author_dir = sanitize_component(author_sort);
    let title_dir = sanitize_component(&format!("{title} ({book_id})"));
    library_path.join(author_dir).join(title_dir)
}

fn sanitize_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ if ch.is_control() => '_',
            _ => ch,
        })
        .collect();

    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}
