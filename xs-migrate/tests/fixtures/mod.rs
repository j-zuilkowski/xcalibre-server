#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;
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

pub fn calibre_import_fixture_library_dir(include_missing_file: bool) -> TempDir {
    let temp_dir = tempfile::tempdir().expect("create fixture temp dir");
    let metadata_path = temp_dir.path().join("metadata.db");

    let conn = Connection::open(&metadata_path).expect("open fixture metadata.db");
    for sql in SCHEMA_SQL {
        conn.execute(sql, []).expect("create fixture schema");
    }

    conn.execute(
        r#"
        INSERT INTO books (id, title, sort, author_sort, pubdate, series_index, rating, flags, has_cover, last_modified)
        VALUES
        (1, 'Alpha Book', 'Alpha Book', 'Author One', '2024-01-01T00:00:00+00:00', 1.0, NULL, 0, 0, '2024-01-01T10:00:00+00:00'),
        (2, 'Beta Book', 'Beta Book', 'Author Two', '2024-01-02T00:00:00+00:00', 2.0, NULL, 0, 0, '2024-01-02T10:00:00+00:00'),
        (3, 'Gamma Book', 'Gamma Book', 'Author One', '2024-01-03T00:00:00+00:00', 3.0, NULL, 0, 0, '2024-01-03T10:00:00+00:00')
        "#,
        [],
    )
    .expect("insert books");

    conn.execute(
        r#"
        INSERT INTO authors (id, name, sort)
        VALUES
        (1, 'Author One', 'One, Author'),
        (2, 'Author Two', 'Two, Author')
        "#,
        [],
    )
    .expect("insert authors");

    conn.execute(
        r#"
        INSERT INTO books_authors_link (book, author)
        VALUES
        (1, 1),
        (2, 2),
        (3, 1)
        "#,
        [],
    )
    .expect("link authors");

    conn.execute(
        "INSERT INTO tags (id, name) VALUES (1, 'Fiction'), (2, 'Reference'), (3, 'Mystery'), (4, 'Sci-Fi'), (5, 'History')",
        [],
    )
    .expect("insert tags");

    conn.execute(
        r#"
        INSERT INTO books_tags_link (book, tag)
        VALUES
        (1, 1),
        (1, 2),
        (2, 3),
        (2, 4),
        (3, 5)
        "#,
        [],
    )
    .expect("link tags");

    conn.execute(
        r#"
        INSERT INTO data (id, book, format, name, uncompressed_size)
        VALUES
        (1, 1, 'EPUB', 'alpha-book', 111),
        (2, 2, 'EPUB', 'beta-book', 222),
        (3, 3, 'EPUB', 'gamma-book', 333)
        "#,
        [],
    )
    .expect("insert formats");

    create_book_file(temp_dir.path(), "Author One", "Alpha Book", 1, "alpha-book", "epub");
    create_book_file(temp_dir.path(), "Author Two", "Beta Book", 2, "beta-book", "epub");
    if !include_missing_file {
        create_book_file(temp_dir.path(), "Author One", "Gamma Book", 3, "gamma-book", "epub");
    }

    temp_dir
}

fn create_book_file(
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
