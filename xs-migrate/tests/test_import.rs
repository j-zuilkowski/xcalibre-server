#![allow(dead_code, unused_imports)]

mod common;

use std::path::Path;

use xs_migrate::calibre::reader::CalibreReader;
use xs_migrate::import::pipeline::{ImportPipeline, LocalFs};
use rusqlite::Connection;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn test_import_creates_book_in_target_db() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        false,
        "default",
    );
    let report = pipeline.run(entries, &reader).await.expect("run import");

    assert_eq!(report.total, 3);
    assert_eq!(report.imported, 3);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.failed, 0);

    let books_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books")
        .fetch_one(&target_db)
        .await
        .expect("books count");
    assert_eq!(books_count, 3);
}

#[tokio::test]
async fn test_import_copies_book_file_to_storage() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        false,
        "default",
    );
    let _ = pipeline.run(entries, &reader).await.expect("run import");

    let path: String = sqlx::query_scalar("SELECT path FROM formats LIMIT 1")
        .fetch_one(&target_db)
        .await
        .expect("format path");
    assert!(path.starts_with("books/"));
    assert!(storage_dir.path().join(path).exists());
}

#[tokio::test]
async fn test_import_copies_cover_to_storage() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        false,
        "default",
    );
    let _ = pipeline.run(entries, &reader).await.expect("run import");

    let row =
        sqlx::query("SELECT has_cover, cover_path FROM books WHERE title = 'Cover Book' LIMIT 1")
            .fetch_one(&target_db)
            .await
            .expect("cover row");
    let has_cover: i64 = row.try_get("has_cover").expect("has_cover");
    let cover_path: String = row.try_get("cover_path").expect("cover_path");
    assert_eq!(has_cover, 1);

    let cover_abs = storage_dir.path().join(&cover_path);
    let thumb_abs = storage_dir
        .path()
        .join(cover_path.replace(".jpg", ".thumb.jpg"));
    assert!(cover_abs.exists());
    assert!(thumb_abs.exists());

    let cover_img = image::open(&cover_abs).expect("read cover image");
    let thumb_img = image::open(&thumb_abs).expect("read thumb image");
    assert!(cover_img.width() <= 400);
    assert!(cover_img.height() <= 600);
    assert!(thumb_img.width() <= 100);
    assert!(thumb_img.height() <= 150);
}

#[tokio::test]
async fn test_import_skips_duplicate_calibre_id() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    seed_existing_calibre_id(&target_db, "2").await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        false,
        "default",
    );
    let report = pipeline.run(entries, &reader).await.expect("run import");

    assert_eq!(report.imported, 2);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.failed, 0);

    let duplicate_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM identifiers WHERE id_type = 'calibre_id' AND value = '2'",
    )
    .fetch_one(&target_db)
    .await
    .expect("duplicate count");
    assert_eq!(duplicate_count, 1);
}

#[tokio::test]
async fn test_import_missing_file_is_skipped_not_fatal() {
    let library = common::calibre_fixture_library_dir();
    let missing_path = library
        .path()
        .join("Author Three")
        .join("Mobi Book (3)")
        .join("mobi-book.mobi");
    std::fs::remove_file(&missing_path).expect("remove mobi fixture");

    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        false,
        "default",
    );
    let report = pipeline.run(entries, &reader).await.expect("run import");

    assert_eq!(report.imported, 2);
    assert_eq!(report.failed, 1);
    assert_eq!(report.failures.len(), 1);
    assert!(report.failures[0].reason.contains("no format files found"));

    let books_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books")
        .fetch_one(&target_db)
        .await
        .expect("books count");
    assert_eq!(books_count, 2);
}

#[tokio::test]
async fn test_import_multiple_authors() {
    let library = common::calibre_fixture_library_dir();
    add_second_author_to_cover_book(library.path());

    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        false,
        "default",
    );
    let _ = pipeline.run(entries, &reader).await.expect("run import");

    let author_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM book_authors ba
        JOIN books b ON b.id = ba.book_id
        WHERE b.title = 'Cover Book'
        "#,
    )
    .fetch_one(&target_db)
    .await
    .expect("book author count");
    assert_eq!(author_count, 2);
}

#[tokio::test]
async fn test_import_multiple_formats() {
    let library = common::calibre_fixture_library_dir();
    add_second_format_to_cover_book(library.path());

    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        false,
        "default",
    );
    let _ = pipeline.run(entries, &reader).await.expect("run import");

    let format_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM formats f
        JOIN books b ON b.id = f.book_id
        WHERE b.title = 'Cover Book'
        "#,
    )
    .fetch_one(&target_db)
    .await
    .expect("format count");
    assert_eq!(format_count, 2);
}

#[tokio::test]
async fn test_import_identifiers_preserved() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        false,
        "default",
    );
    let _ = pipeline.run(entries, &reader).await.expect("run import");

    let rows = sqlx::query(
        r#"
        SELECT i.id_type, i.value
        FROM identifiers i
        JOIN books b ON b.id = i.book_id
        WHERE b.title = 'Isbn Book'
        ORDER BY i.id_type ASC
        "#,
    )
    .fetch_all(&target_db)
    .await
    .expect("identifier rows");

    let pairs: Vec<(String, String)> = rows
        .iter()
        .map(|row| {
            (
                row.try_get::<String, _>("id_type").expect("id_type"),
                row.try_get::<String, _>("value").expect("value"),
            )
        })
        .collect();

    assert!(pairs.contains(&(String::from("isbn"), String::from("9780000000002"))));
    assert!(pairs.contains(&(String::from("calibre_id"), String::from("2"))));
}

async fn create_target_db() -> (SqlitePool, TempDir) {
    let dir = tempfile::tempdir().expect("target db temp dir");
    let db_path = dir.path().join("target.db");
    let options = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("connect target db");

    for sql in TARGET_SCHEMA {
        sqlx::query(sql)
            .execute(&pool)
            .await
            .expect("create target schema");
    }

    (pool, dir)
}

async fn seed_existing_calibre_id(pool: &SqlitePool, calibre_id: &str) {
    let now = "2025-01-01T00:00:00+00:00";
    let book_id = Uuid::new_v4().to_string();

    sqlx::query(
        r#"
        INSERT INTO books (
            id, library_id, title, sort_title, description, pubdate, language, rating,
            series_id, series_index, has_cover, cover_path, flags, indexed_at,
            created_at, last_modified
        )
        VALUES (?, 'default', 'Already Imported', 'Already Imported', NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL, NULL, ?, ?)
        "#,
    )
    .bind(&book_id)
    .bind(now)
    .bind(now)
    .execute(pool)
    .await
    .expect("insert existing book");

    sqlx::query(
        "INSERT INTO identifiers (id, book_id, id_type, value, last_modified) VALUES (?, ?, 'calibre_id', ?, ?)",
    )
    .bind(Uuid::new_v4().to_string())
    .bind(book_id)
    .bind(calibre_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("insert existing calibre id");
}

fn add_second_author_to_cover_book(library_path: &Path) {
    let metadata = library_path.join("metadata.db");
    let conn = Connection::open(metadata).expect("open metadata db");
    conn.execute(
        "INSERT INTO authors (id, name, sort) VALUES (4, 'Co Author', 'Author, Co')",
        [],
    )
    .expect("insert second author");
    conn.execute(
        "INSERT INTO books_authors_link (book, author) VALUES (1, 4)",
        [],
    )
    .expect("link second author");
}

fn add_second_format_to_cover_book(library_path: &Path) {
    let metadata = library_path.join("metadata.db");
    let conn = Connection::open(metadata).expect("open metadata db");
    conn.execute(
        "INSERT INTO data (id, book, format, name, uncompressed_size) VALUES (4, 1, 'PDF', 'cover-book-pdf', 444)",
        [],
    )
    .expect("insert second format");

    let path = library_path
        .join("Author One")
        .join("Cover Book (1)")
        .join("cover-book-pdf.pdf");
    std::fs::write(path, b"fixture-pdf").expect("write second format file");
}

const TARGET_SCHEMA: &[&str] = &[
    r#"
    CREATE TABLE authors (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        sort_name TEXT NOT NULL,
        last_modified TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE series (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        sort_name TEXT NOT NULL,
        last_modified TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE tags (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        source TEXT NOT NULL,
        last_modified TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE libraries (
        id TEXT PRIMARY KEY,
        name TEXT NOT NULL UNIQUE,
        calibre_db_path TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL
    )
    "#,
    "INSERT INTO libraries (id, name, calibre_db_path, created_at, updated_at) VALUES ('default', 'Default Library', '', '2025-01-01T00:00:00+00:00', '2025-01-01T00:00:00+00:00')",
    r#"
    CREATE TABLE books (
        id TEXT PRIMARY KEY,
        library_id TEXT NOT NULL DEFAULT 'default' REFERENCES libraries(id),
        title TEXT NOT NULL,
        sort_title TEXT NOT NULL,
        description TEXT,
        pubdate TEXT,
        language TEXT,
        rating INTEGER,
        series_id TEXT,
        series_index REAL,
        has_cover INTEGER NOT NULL DEFAULT 0,
        cover_path TEXT,
        flags TEXT,
        indexed_at TEXT,
        created_at TEXT NOT NULL,
        last_modified TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE book_authors (
        book_id TEXT NOT NULL,
        author_id TEXT NOT NULL,
        display_order INTEGER NOT NULL DEFAULT 0,
        PRIMARY KEY (book_id, author_id)
    )
    "#,
    r#"
    CREATE TABLE book_tags (
        book_id TEXT NOT NULL,
        tag_id TEXT NOT NULL,
        confirmed INTEGER NOT NULL DEFAULT 1,
        PRIMARY KEY (book_id, tag_id)
    )
    "#,
    r#"
    CREATE TABLE formats (
        id TEXT PRIMARY KEY,
        book_id TEXT NOT NULL,
        format TEXT NOT NULL,
        path TEXT NOT NULL,
        size_bytes INTEGER NOT NULL DEFAULT 0,
        created_at TEXT NOT NULL,
        last_modified TEXT NOT NULL
    )
    "#,
    r#"
    CREATE TABLE identifiers (
        id TEXT PRIMARY KEY,
        book_id TEXT NOT NULL,
        id_type TEXT NOT NULL,
        value TEXT NOT NULL,
        last_modified TEXT NOT NULL
    )
    "#,
];
