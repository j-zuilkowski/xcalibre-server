#![allow(dead_code, unused_imports)]

use axum::http::HeaderValue;
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
    cell::RefCell,
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
    /// Tracks the most recently created user ID for provider-linking helpers.
    /// Uses `RefCell` for interior mutability since helper methods take `&self`.
    pub last_user_id: RefCell<Option<String>>,
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
        std::env::set_var("XCS_DISABLE_METRICS", "1");
        config.app.storage_path = storage.path().to_string_lossy().to_string();
        if config.auth.jwt_secret.trim().is_empty() {
            config.auth.jwt_secret = TEST_JWT_SECRET.to_string();
        }
        // Bump rate limits for test scenarios that create many books/logins
        config.limits.auth_rate_limit_per_minute = 1000;
        config.limits.rate_limit_per_ip = 2000;
        let state = AppState::new(db.clone(), config)
            .await
            .expect("initialize app state");
        let server = TestServer::new(app(state.clone())).expect("build test server");

        Self {
            db,
            storage,
            server,
            state,
            last_user_id: RefCell::new(None),
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

    pub async fn create_user_with_email(&self, email: &str) -> (User, String) {
        self.seed_role("user").await;
        let password = "Test1234!".to_string();
        let user = self
            .insert_user(
                email.split('@').next().unwrap_or("user"),
                email,
                "user",
                &password,
            )
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

    fn set_last_user_id(&self, id: String) {
        *self.last_user_id.borrow_mut() = Some(id);
    }

    pub async fn admin_token(&self) -> String {
        let user_id = Uuid::new_v4().to_string();
        let password = "Test1234!".to_string();
        self.seed_role("admin").await;
        let user = self.insert_user(&format!("admin-{user_id}"), &format!("admin-{user_id}@example.com"), "admin", &password).await;
        self.set_last_user_id(user.id.clone());
        self.login(&user.username, &password).await.access_token
    }

    pub async fn user_token(&self) -> String {
        self.seed_role("user").await;
        let password = "Test1234!".to_string();
        let unique = Uuid::new_v4().to_string().replace('-', "")[..12].to_string();
        let user = self.insert_user(&format!("user-{unique}"), &format!("user-{unique}@example.com"), "user", &password).await;
        self.set_last_user_id(user.id.clone());
        self.login(&user.username, &password).await.access_token
    }

    /// Creates a new user with the given email and returns just the access token.
    pub async fn create_user_and_token(&self, email: &str) -> String {
        let (user, password) = self.create_user_with_email(email).await;
        self.set_last_user_id(user.id.clone());
        let username = email.split('@').next().unwrap_or("user").to_string();
        self.login(&username, &password).await.access_token
    }

    /// Returns a JWT access token as a `HeaderValue` for OPDS Basic Auth.
    pub async fn opds_basic_auth_header(&self) -> HeaderValue {
        let token = self.admin_token().await;
        HeaderValue::from_str(&format!("Bearer {token}")).expect("valid bearer header")
    }

    /// Returns the user ID of a freshly-created admin user.
    pub async fn admin_user_id(&self) -> String {
        let id = Uuid::new_v4().to_string();
        let password = "Test1234!".to_string();
        self.seed_role("admin").await;
        let user = self.insert_user(&format!("admin-{id}"), &format!("admin-{id}@example.com"), "admin", &password).await;
        self.set_last_user_id(user.id.clone());
        user.id
    }

    pub fn jwt_secret(&self) -> &'static str {
        TEST_JWT_SECRET
    }

    /// Inserts an `oauth_accounts` row for the most recently created user.
    pub async fn link_oauth_for_current_user(&self, provider: &str, provider_account_id: &str) {
        let user_id = self.last_user_id.borrow().clone().expect("no user created yet — call user_token() or create_user first");
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO oauth_accounts (id, user_id, provider, provider_user_id, email, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&user_id)
        .bind(provider)
        .bind(provider_account_id)
        .bind("test@example.com")
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert oauth account");
    }

    /// Inserts an `oauth_accounts` row for a different user (not the current one).
    pub async fn link_oauth_for_other_user(&self, provider: &str, provider_account_id: &str) {
        let now = Utc::now().to_rfc3339();
        let other_id = Uuid::new_v4().to_string();
        let other_user_id = Uuid::new_v4().to_string();
        let password_hash = hash_password("Other1234!");
        self.seed_role("user").await;

        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, role_id, is_active, force_pw_reset, created_at, last_modified)
            VALUES (?, ?, ?, ?, ?, 1, 0, ?, ?)
            "#,
        )
        .bind(&other_user_id)
        .bind("other-user")
        .bind("other@example.com")
        .bind(&password_hash)
        .bind("user")
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert other user");

        sqlx::query(
            r#"
            INSERT INTO oauth_accounts (id, user_id, provider, provider_user_id, email, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&other_id)
        .bind(&other_user_id)
        .bind(provider)
        .bind(provider_account_id)
        .bind("other@example.com")
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert oauth account for other user");
    }

    /// Creates a user with an empty password hash (oauth-only) and links a provider.
    /// Returns the access token for this user.
    pub async fn create_oauth_only_user_and_token(
        &self,
        email: &str,
        provider: &str,
        provider_account_id: &str,
    ) -> String {
        let now = Utc::now().to_rfc3339();
        let user_id = Uuid::new_v4().to_string();
        let oauth_id = Uuid::new_v4().to_string();
        let username = email.split('@').next().unwrap_or("oauth-user");
        self.seed_role("user").await;

        // Insert user with empty password hash — indicates oauth-only user.
        sqlx::query(
            r#"
            INSERT INTO users (id, username, email, password_hash, role_id, is_active, force_pw_reset, created_at, last_modified)
            VALUES (?, ?, ?, '', ?, 1, 0, ?, ?)
            "#,
        )
        .bind(&user_id)
        .bind(username)
        .bind(email)
        .bind("user")
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert oauth-only user");

        // Link the provider account.
        sqlx::query(
            r#"
            INSERT INTO oauth_accounts (id, user_id, provider, provider_user_id, email, created_at)
            VALUES (?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&oauth_id)
        .bind(&user_id)
        .bind(provider)
        .bind(provider_account_id)
        .bind(email)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert oauth account");

        // Track this as the last user for subsequent helpers.
        self.set_last_user_id(user_id.clone());

        // Issue a JWT directly using the test secret.
        backend::middleware::auth::issue_access_token(&user_id, TEST_JWT_SECRET, 15)
            .expect("issue test token")
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
            totp_enabled: false,
            created_at: now.clone(),
            last_modified: now,
        }
    }

    /// Seeds a `book_annotations` row for the most recently created user and book.
    pub async fn seed_annotation(&self, book_id: &str) {
        let user_id = self
            .last_user_id
            .borrow()
            .clone()
            .expect("no user created yet");
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO book_annotations (id, user_id, book_id, type, cfi_range, highlighted_text, note, color, created_at, updated_at)
            VALUES (?, ?, ?, 'highlight', 'epubcfi(/6/2[chap01]!/4/1:0)', 'some highlighted text', 'a test note', 'yellow', ?, ?)
            "#,
        )
        .bind(&id)
        .bind(&user_id)
        .bind(book_id)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert annotation");
    }

    /// Seeds a shelf named `shelf_name` (owned by the most recently created user)
    /// with `book_id` as a member.
    pub async fn seed_shelf_membership(&self, shelf_name: &str, book_id: &str) {
        let user_id = self
            .last_user_id
            .borrow()
            .clone()
            .expect("no user created yet");
        let now = Utc::now().to_rfc3339();
        let shelf_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO shelves (id, user_id, name, is_public, created_at, last_modified)
            VALUES (?, ?, ?, 0, ?, ?)
            "#,
        )
        .bind(&shelf_id)
        .bind(&user_id)
        .bind(shelf_name)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert shelf");

        sqlx::query(
            r#"
            INSERT INTO shelf_books (shelf_id, book_id, display_order, added_at)
            VALUES (?, ?, 0, ?)
            "#,
        )
        .bind(&shelf_id)
        .bind(book_id)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert shelf book");
    }

    /// Seeds a `reading_progress` row for the most recently created user and book.
    /// Uses the book's first format row as the `format_id`.
    pub async fn seed_reading_progress(&self, book_id: &str, percentage: i64) {
        let user_id = self
            .last_user_id
            .borrow()
            .clone()
            .expect("no user created yet");
        let now = Utc::now().to_rfc3339();
        let id = Uuid::new_v4().to_string();

        let format_id: String = sqlx::query_scalar(
            "SELECT id FROM formats WHERE book_id = ? LIMIT 1",
        )
        .bind(book_id)
        .fetch_one(&self.db)
        .await
        .expect("book has at least one format");

        sqlx::query(
            r#"
            INSERT INTO reading_progress (id, user_id, book_id, format_id, cfi, page, percentage, updated_at, last_modified)
            VALUES (?, ?, ?, ?, NULL, NULL, ?, ?, ?)
            ON CONFLICT(user_id, book_id) DO UPDATE SET
                percentage = excluded.percentage,
                format_id = excluded.format_id,
                updated_at = excluded.updated_at,
                last_modified = excluded.last_modified
            "#,
        )
        .bind(&id)
        .bind(&user_id)
        .bind(book_id)
        .bind(&format_id)
        .bind(percentage as f64)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .expect("insert reading progress");
    }

    /// Reads the current `percentage` from `reading_progress` for the most
    /// recently created user and the given book.
    pub async fn read_reading_progress_percent(&self, book_id: &str) -> i64 {
        let user_id = self
            .last_user_id
            .borrow()
            .clone()
            .expect("no user created yet");
        let pct: f64 = sqlx::query_scalar(
            "SELECT percentage FROM reading_progress WHERE user_id = ? AND book_id = ?",
        )
        .bind(&user_id)
        .bind(book_id)
        .fetch_one(&self.db)
        .await
        .expect("read reading progress");
        pct as i64
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

pub fn minimal_azw3_bytes() -> Vec<u8> {
    include_bytes!("../fixtures/minimal.azw3").to_vec()
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
