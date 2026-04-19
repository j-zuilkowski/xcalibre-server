#![allow(dead_code)]

pub async fn create_calibre_fixture_db() -> sqlx::SqlitePool {
    todo!(
        "phase 2 scaffold: build in-memory metadata.db with 3 books (cover/isbn/mobi fixtures)"
    )
}

pub fn calibre_fixture_library_dir() -> tempfile::TempDir {
    todo!("phase 2 scaffold: create fake calibre library files and cover.jpg")
}
