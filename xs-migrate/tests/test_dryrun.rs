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
async fn test_dryrun_writes_nothing_to_db() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline = ImportPipeline::new(
        target_db.clone(),
        LocalFs::new(storage_dir.path()),
        true,
        "default",
    );
    let report = pipeline.run(entries, &reader).await.expect("run import");

    assert_eq!(report.imported, 3);
    let books_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM books")
        .fetch_one(&target_db)
        .await
        .expect("books count");
    let formats_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM formats")
        .fetch_one(&target_db)
        .await
        .expect("formats count");
    let identifiers_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM identifiers")
        .fetch_one(&target_db)
        .await
        .expect("identifiers count");

    assert_eq!(books_count, 0);
    assert_eq!(formats_count, 0);
    assert_eq!(identifiers_count, 0);
}

#[tokio::test]
async fn test_dryrun_writes_nothing_to_storage() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline =
        ImportPipeline::new(target_db, LocalFs::new(storage_dir.path()), true, "default");
    let report = pipeline.run(entries, &reader).await.expect("run import");

    assert_eq!(report.imported, 3);

    let mut created_files = Vec::new();
    collect_files(storage_dir.path(), &mut created_files);
    assert!(created_files.is_empty());
}

#[tokio::test]
async fn test_dryrun_report_shows_expected_counts() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline =
        ImportPipeline::new(target_db, LocalFs::new(storage_dir.path()), true, "default");
    let report = pipeline.run(entries, &reader).await.expect("run import");

    assert_eq!(report.total, 3);
    assert_eq!(report.imported, 3);
    assert_eq!(report.skipped, 0);
    assert_eq!(report.failed, 0);
    assert!(report.failures.is_empty());

    let json = report.to_json();
    assert!(json.contains("\"total\": 3"));
    assert!(json.contains("\"imported\": 3"));
}

#[tokio::test]
async fn test_report_counts_skipped_books() {
    let library = common::calibre_fixture_library_dir();
    let reader = CalibreReader::open(library.path()).expect("open fixture reader");
    let entries = reader.read_all_entries().expect("read entries");
    let (target_db, _target_db_dir) = create_target_db().await;
    seed_existing_calibre_id(&target_db, "1").await;
    let storage_dir = tempfile::tempdir().expect("storage dir");

    let pipeline =
        ImportPipeline::new(target_db, LocalFs::new(storage_dir.path()), true, "default");
    let report = pipeline.run(entries, &reader).await.expect("run import");

    assert_eq!(report.total, 3);
    assert_eq!(report.imported, 2);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.failed, 0);
}

#[tokio::test]
async fn test_report_counts_failed_books() {
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

    let pipeline =
        ImportPipeline::new(target_db, LocalFs::new(storage_dir.path()), true, "default");
    let report = pipeline.run(entries, &reader).await.expect("run import");

    assert_eq!(report.total, 3);
    assert_eq!(report.imported, 2);
    assert_eq!(report.skipped, 1);
    assert_eq!(report.failed, 0);
    assert!(report.failures.is_empty());
}

fn collect_files(root: &Path, out: &mut Vec<String>) {
    let entries = std::fs::read_dir(root).expect("read dir");
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, out);
        } else {
            out.push(path.to_string_lossy().to_string());
        }
    }
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
            id, title, sort_title, description, pubdate, language, rating,
            series_id, series_index, has_cover, cover_path, flags, indexed_at,
            created_at, last_modified
        )
        VALUES (?, 'Already Imported', 'Already Imported', NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL, NULL, ?, ?)
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
    CREATE TABLE books (
        id TEXT PRIMARY KEY,
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
