#![allow(dead_code, unused_imports)]

use axum_test::TestServer;
use backend::{
    app,
    config::AppConfig,
    db::models::{AuthorRef, Book, FormatRef, Identifier, RoleRef, SeriesRef, TagRef, User},
    AppState,
};
use chrono::Utc;
use serde::Deserialize;
use sqlx::SqlitePool;
use std::{
    io::Write,
    path::{Path, PathBuf},
};
use tempfile::TempDir;
use uuid::Uuid;

pub const TEST_JWT_SECRET: &str = "MDEyMzQ1Njc4OWFiY2RlZjAxMjM0NTY3ODlhYmNkZWY";

#[derive(Clone, Debug, Deserialize)]
pub struct LoginResult {
    pub access_token: String,
    pub refresh_token: String,
    pub user: User,
}

pub struct TestContext {
    pub db: SqlitePool,
    pub storage: TempDir,
    pub server: TestServer,
    pub state: AppState,
}

pub async fn test_db() -> SqlitePool {
    let db = backend::db::connect_sqlite_pool("sqlite::memory:", 1)
        .await
        .expect("connect sqlite");
    let migration_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/sqlite");
    let migrator = sqlx::migrate::Migrator::new(migration_path.as_path())
        .await
        .expect("load migrations");
    migrator.run(&db).await.expect("run migrations");
    db
}

impl TestContext {
    pub async fn new() -> Self {
        Self::new_with_config(AppConfig::default()).await
    }

    pub async fn new_with_config(mut config: AppConfig) -> Self {
        let storage = tempfile::tempdir().expect("tempdir");
        let db = test_db().await;
        config.app.storage_path = storage.path().to_string_lossy().to_string();
        if config.auth.jwt_secret.trim().is_empty() {
            config.auth.jwt_secret = TEST_JWT_SECRET.to_string();
        }
        let state = AppState::new(db.clone(), config).await;
        let server = TestServer::new(app(state.clone())).expect("build test server");

        Self {
            db,
            storage,
            server,
            state,
        }
    }

    pub async fn create_admin(&self) -> (User, String) {
        self.seed_role("admin").await;
        let password = "Test1234!".to_string();
        let user = self
            .insert_user("admin", "admin@example.com", "admin", &password)
            .await;
        (user, password)
    }

    pub async fn create_user(&self) -> (User, String) {
        self.seed_role("user").await;
        let password = "Test1234!".to_string();
        let user = self
            .insert_user("user", "user@example.com", "user", &password)
            .await;
        (user, password)
    }

    pub async fn login(&self, username: &str, password: &str) -> LoginResult {
        let response = self
            .server
            .post("/api/v1/auth/login")
            .json(&serde_json::json!({ "username": username, "password": password }))
            .await;
        response.json::<LoginResult>()
    }

    pub async fn admin_token(&self) -> String {
        let (user, password) = self.create_admin().await;
        self.login(&user.username, &password).await.access_token
    }

    pub async fn user_token(&self) -> String {
        let (user, password) = self.create_user().await;
        self.login(&user.username, &password).await.access_token
    }

    pub fn jwt_secret(&self) -> &'static str {
        TEST_JWT_SECRET
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
        sqlx::query("INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
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
            document_type: "unknown".to_string(),
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
            is_read: false,
            is_archived: false,
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

    async fn insert_user(
        &self,
        username: &str,
        email: &str,
        role_id: &str,
        password: &str,
    ) -> User {
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
            default_library_id: "default".to_string(),
            created_at: now.clone(),
            last_modified: now,
        }
    }
}

pub fn auth_header(access_token: &str) -> axum::http::HeaderValue {
    let value = format!("Bearer {access_token}");
    axum::http::HeaderValue::from_str(&value).expect("valid auth header")
}

pub fn minimal_epub_bytes() -> Vec<u8> {
    include_bytes!("../fixtures/minimal.epub").to_vec()
}

pub fn minimal_pdf_bytes() -> Vec<u8> {
    include_bytes!("../fixtures/minimal.pdf").to_vec()
}

pub fn minimal_mobi_bytes() -> Vec<u8> {
    include_bytes!("../fixtures/minimal.mobi").to_vec()
}

pub fn epub_with_cover_bytes() -> Vec<u8> {
    use zip::write::FileOptions;

    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options = FileOptions::default();

    zip.start_file("mimetype", options).expect("start mimetype");
    zip.write_all(b"application/epub+zip")
        .expect("write mimetype");

    zip.start_file("META-INF/container.xml", options)
        .expect("start container.xml");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
    )
    .expect("write container.xml");

    zip.start_file("OEBPS/content.opf", options)
        .expect("start content.opf");
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<package version="3.0" xmlns="http://www.idpf.org/2007/opf" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Cover Test Book</dc:title>
    <dc:creator>Cover Test Author</dc:creator>
  </metadata>
  <manifest>
    <item id="cover" href="images/cover.jpg" media-type="image/jpeg" properties="cover-image"/>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  </manifest>
  <spine>
    <itemref idref="nav"/>
  </spine>
</package>"#,
    )
    .expect("write content.opf");

    zip.start_file("OEBPS/nav.xhtml", options)
        .expect("start nav.xhtml");
    zip.write_all(
        br#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>Nav</body></html>"#,
    )
    .expect("write nav.xhtml");

    zip.start_file("OEBPS/images/cover.jpg", options)
        .expect("start cover image");
    let cover_image = image::DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
        200,
        300,
        image::Rgb([0x52, 0xA3, 0xD9]),
    ));
    let mut cover_cursor = std::io::Cursor::new(Vec::new());
    cover_image
        .write_to(&mut cover_cursor, image::ImageFormat::Jpeg)
        .expect("encode cover jpeg");
    zip.write_all(&cover_cursor.into_inner())
        .expect("write cover image");

    let cursor = zip.finish().expect("finish zip");
    cursor.into_inner()
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
    use argon2::password_hash::rand_core::OsRng;
    use argon2::{
        password_hash::{PasswordHasher, SaltString},
        Argon2,
    };

    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .expect("hash password")
        .to_string()
}
