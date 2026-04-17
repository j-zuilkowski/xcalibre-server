#![allow(dead_code, unused_imports)]

use axum_test::TestServer;
use backend::{
    app,
    config::AppConfig,
    db::models::{AuthorRef, Book, FormatRef, Identifier, RoleRef, SeriesRef, TagRef, User},
    AppState,
};
use chrono::Utc;
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use std::path::PathBuf;
use tempfile::TempDir;
use uuid::Uuid;

pub struct TestContext {
    pub db: SqlitePool,
    pub storage: TempDir,
    pub server: TestServer,
}

impl TestContext {
    pub async fn new() -> Self {
        let storage = tempfile::tempdir().expect("tempdir");
        let db = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect sqlite");
        let migrator = sqlx::migrate!("migrations/sqlite");
        migrator.run(&db).await.expect("run migrations");

        let state = AppState::new(db.clone(), AppConfig::default(), storage.path().to_path_buf());
        let server = TestServer::new(app(state)).expect("build test server");

        Self { db, storage, server }
    }

    pub async fn create_admin(&self) -> (User, String) {
        self.seed_role("admin").await;
        let password = "Test1234!".to_string();
        let user = self.insert_user("admin", "admin@example.com", "admin", &password).await;
        (user, password)
    }

    pub async fn create_user(&self) -> (User, String) {
        self.seed_role("user").await;
        let password = "Test1234!".to_string();
        let user = self.insert_user("user", "user@example.com", "user", &password).await;
        (user, password)
    }

    pub async fn login(&self, username: &str, password: &str) -> String {
        let response = self
            .server
            .post("/api/v1/auth/login")
            .json(&serde_json::json!({ "username": username, "password": password }))
            .await;
        let json: serde_json::Value = response.json();
        json["access_token"].as_str().unwrap_or_default().to_string()
    }

    pub async fn admin_token(&self) -> String {
        let (user, password) = self.create_admin().await;
        self.login(&user.username, &password).await
    }

    pub async fn user_token(&self) -> String {
        let (user, password) = self.create_user().await;
        self.login(&user.username, &password).await
    }

    pub async fn create_book(&self, title: &str, author: &str) -> Book {
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO books (id, title, sort_title, description, pubdate, language, rating, series_id, series_index, has_cover, cover_path, flags, indexed_at, created_at, last_modified)
            VALUES (?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL, NULL, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(title)
        .bind(title)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert book");

        let author_id = Uuid::new_v4().to_string();
        sqlx::query(
            "INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)",
        )
        .bind(&author_id)
        .bind(author)
        .bind(author)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert author");

        sqlx::query(
            "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, 0)",
        )
        .bind(&id)
        .bind(&author_id)
        .execute(&self.db)
        .await
        .expect("insert book author");

        Book {
            id,
            title: title.to_string(),
            sort_title: title.to_string(),
            description: None,
            pubdate: None,
            language: None,
            rating: None,
            series: None,
            series_index: None,
            authors: vec![AuthorRef {
                id: author_id,
                name: author.to_string(),
                sort_name: author.to_string(),
            }],
            tags: Vec::new(),
            formats: Vec::new(),
            cover_url: None,
            has_cover: false,
            identifiers: Vec::new(),
            created_at: now.clone(),
            last_modified: now.clone(),
            indexed_at: None,
        }
    }

    pub async fn create_book_with_file(&self, title: &str, format: &str) -> (Book, PathBuf) {
        let book = self.create_book(title, "Test Author").await;
        let file_name = format!("{}.{}", book.id, format.to_lowercase());
        let path = self.storage.path().join(&file_name);
        std::fs::write(&path, b"stage-1-placeholder").expect("write file");

        let now = Utc::now().to_rfc3339();
        let format_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&format_id)
        .bind(&book.id)
        .bind(format)
        .bind(&file_name)
        .bind(0_i64)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert format");

        let mut book = book;
        book.formats.push(FormatRef {
            id: format_id,
            format: format.to_string(),
            size_bytes: 0,
        });
        (book, path)
    }

    async fn seed_role(&self, role: &str) {
        let now = Utc::now().to_rfc3339();
        let _ = sqlx::query(
            r#"
            INSERT OR IGNORE INTO roles (id, name, can_upload, can_bulk, can_edit, can_download, created_at, last_modified)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(role)
        .bind(role)
        .bind(i64::from(role == "admin"))
        .bind(i64::from(role == "admin"))
        .bind(1_i64)
        .bind(1_i64)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await;
    }

    async fn insert_user(&self, username: &str, email: &str, role_id: &str, password: &str) -> User {
        let id = Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        let password_hash = hash_password(password);

        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, role_id, is_active, force_pw_reset, created_at, last_modified)
            VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(username)
        .bind(email)
        .bind(&password_hash)
        .bind(role_id)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert user");

        User {
            id,
            username: username.to_string(),
            email: email.to_string(),
            role: RoleRef {
                id: role_id.to_string(),
                name: role_id.to_string(),
            },
            is_active: true,
            force_pw_reset: false,
            created_at: now.clone(),
            last_modified: now,
        }
    }
}

pub fn minimal_epub_bytes() -> Vec<u8> {
    include_bytes!("../fixtures/minimal.epub").to_vec()
}

pub fn minimal_pdf_bytes() -> Vec<u8> {
    include_bytes!("../fixtures/minimal.pdf").to_vec()
}

#[macro_export]
macro_rules! assert_status {
    ($response:expr, $status:expr) => {{
        let status = $response.status_code();
        if format!("{:?}", status) != format!("{:?}", $status) {
            let body = $response.text();
            panic!("Expected status {} got {:?}: {}", $status, status, body);
        }
    }};
}

#[macro_export]
macro_rules! assert_json_field {
    ($response:expr, $field:expr, $value:expr) => {{
        let json: serde_json::Value = $response.json();
        assert_eq!(json[$field], $value, "Field '{}' mismatch", $field);
    }};
}

fn hash_password(password: &str) -> String {
    use argon2::{password_hash::{PasswordHasher, SaltString}, Argon2};
    use argon2::password_hash::rand_core::OsRng;

    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("hash password")
        .to_string()
}
