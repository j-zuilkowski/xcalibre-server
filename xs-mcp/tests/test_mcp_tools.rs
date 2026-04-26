use anyhow::Context;
use xs_mcp::tools::CalibreMcpServer;
use backend::config::AppConfig;
use chrono::Utc;
use rmcp::{model::CallToolRequestParams, ServiceExt};
use serde_json::Value;
use sqlx::SqlitePool;
use tempfile::TempDir;
use uuid::Uuid;

struct TestHarness {
    db: SqlitePool,
    storage: TempDir,
    llm_enabled: bool,
}

impl TestHarness {
    async fn new(llm_enabled: bool) -> anyhow::Result<Self> {
        let storage = tempfile::tempdir().context("create temp storage")?;
        let db = backend::db::connect_sqlite_pool("sqlite::memory:", 1)
            .await
            .context("connect sqlite")?;

        let migration_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../backend/migrations/sqlite");
        let migrator = sqlx::migrate::Migrator::new(migration_path.as_path())
            .await
            .context("load sqlite migrations")?;
        migrator.run(&db).await.context("run sqlite migrations")?;

        Ok(Self {
            db,
            storage,
            llm_enabled,
        })
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        arguments: Value,
    ) -> anyhow::Result<rmcp::model::CallToolResult> {
        let mut config = AppConfig::default();
        config.app.storage_path = self.storage.path().to_string_lossy().to_string();
        config.llm.enabled = self.llm_enabled;

        let server = CalibreMcpServer::new(self.db.clone(), config)?;
        let (server_transport, client_transport) = tokio::io::duplex(64 * 1024);
        let server_task = tokio::spawn(async move {
            server.serve(server_transport).await?.waiting().await?;
            anyhow::Ok(())
        });

        let client = ().serve(client_transport).await?;
        let args = arguments.as_object().cloned().unwrap_or_default();
        let result = client
            .call_tool(CallToolRequestParams::new(tool_name.to_string()).with_arguments(args))
            .await?;
        client.cancel().await?;
        server_task.await??;
        Ok(result)
    }

    async fn insert_book(
        &self,
        title: &str,
        author: &str,
        tag: Option<&str>,
    ) -> anyhow::Result<String> {
        let now = Utc::now().to_rfc3339();
        let book_id = Uuid::new_v4().to_string();
        sqlx::query(
            r#"
            INSERT INTO books (id, title, sort_title, description, pubdate, language, rating, series_id, series_index, has_cover, cover_path, flags, indexed_at, created_at, last_modified, document_type)
            VALUES (?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL, NULL, NULL, ?, ?, 'novel')
            "#,
        )
        .bind(&book_id)
        .bind(title)
        .bind(title)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .context("insert book")?;

        let author_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
            .bind(&author_id)
            .bind(author)
            .bind(author)
            .bind(&now)
            .execute(&self.db)
            .await
            .context("insert author")?;

        sqlx::query(
            "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, 0)",
        )
        .bind(&book_id)
        .bind(&author_id)
        .execute(&self.db)
        .await
        .context("insert book author")?;

        if let Some(tag_name) = tag {
            let tag_id = Uuid::new_v4().to_string();
            sqlx::query(
                "INSERT INTO tags (id, name, source, last_modified) VALUES (?, ?, 'manual', ?)",
            )
            .bind(&tag_id)
            .bind(tag_name)
            .bind(&now)
            .execute(&self.db)
            .await
            .context("insert tag")?;
            sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
                .bind(&book_id)
                .bind(&tag_id)
                .execute(&self.db)
                .await
                .context("insert book tag")?;
        }

        Ok(book_id)
    }

    async fn insert_epub_format(&self, book_id: &str) -> anyhow::Result<()> {
        let rel = format!("{book_id}.epub");
        let full = self.storage.path().join(&rel);
        std::fs::write(
            &full,
            include_bytes!("../../backend/tests/fixtures/minimal.epub"),
        )
        .context("write epub fixture")?;

        let now = Utc::now().to_rfc3339();
        sqlx::query(
            r#"
            INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
            VALUES (?, ?, 'EPUB', ?, ?, ?, ?)
            "#,
        )
        .bind(Uuid::new_v4().to_string())
        .bind(book_id)
        .bind(rel)
        .bind(0_i64)
        .bind(&now)
        .bind(&now)
        .execute(&self.db)
        .await
        .context("insert format")?;
        Ok(())
    }
}

fn result_to_json(result: &rmcp::model::CallToolResult) -> Value {
    if let Some(value) = result.structured_content.clone() {
        return value;
    }

    let text = result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_default();
    serde_json::from_str(&text).unwrap_or(Value::String(text))
}

#[tokio::test]
async fn test_search_books_returns_results() {
    let harness = TestHarness::new(false).await.expect("harness");
    let _ = harness
        .insert_book("Fiction Starter", "Author One", Some("fiction"))
        .await
        .expect("seed book1");
    let _ = harness
        .insert_book("Another Fiction Story", "Author Two", None)
        .await
        .expect("seed book2");
    let _ = harness
        .insert_book("Reference Manual", "Author Three", None)
        .await
        .expect("seed book3");

    let result = harness
        .call_tool("search_books", serde_json::json!({ "q": "fiction" }))
        .await
        .expect("search_books call");

    let json = result_to_json(&result);
    assert!(json["results"].is_array());
}

#[tokio::test]
async fn test_get_book_metadata_returns_full_record() {
    let harness = TestHarness::new(false).await.expect("harness");
    let book_id = harness
        .insert_book("Metadata Book", "Tagged Author", Some("metadata"))
        .await
        .expect("seed book");

    let result = harness
        .call_tool(
            "get_book_metadata",
            serde_json::json!({ "book_id": book_id }),
        )
        .await
        .expect("get_book_metadata call");

    let json = result_to_json(&result);
    let authors = json["authors"].as_array().cloned().unwrap_or_default();
    assert!(!authors.is_empty());
}

#[tokio::test]
async fn test_get_book_text_no_llm_required() {
    let harness = TestHarness::new(false).await.expect("harness");
    let book_id = harness
        .insert_book("Text Book", "Reader Author", None)
        .await
        .expect("seed book");
    harness
        .insert_epub_format(&book_id)
        .await
        .expect("seed epub format");

    let result = harness
        .call_tool("get_book_text", serde_json::json!({ "book_id": book_id }))
        .await
        .expect("get_book_text call");
    assert_ne!(result.is_error, Some(true));

    let json = result_to_json(&result);
    assert!(!json["text"].as_str().unwrap_or_default().is_empty());
}

#[tokio::test]
async fn test_semantic_search_error_when_disabled() {
    let harness = TestHarness::new(false).await.expect("harness");
    let result = harness
        .call_tool(
            "semantic_search",
            serde_json::json!({ "query": "systems programming" }),
        )
        .await
        .expect("semantic_search call");

    assert_eq!(result.is_error, Some(true));
    let text = result
        .content
        .first()
        .and_then(|content| content.raw.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_default();
    assert!(text.contains("semantic_search_unavailable"));
}

#[tokio::test]
async fn test_list_chapters_returns_spine() {
    let harness = TestHarness::new(false).await.expect("harness");
    let book_id = harness
        .insert_book("Chaptered Book", "Chapter Author", None)
        .await
        .expect("seed book");
    harness
        .insert_epub_format(&book_id)
        .await
        .expect("seed epub format");

    let result = harness
        .call_tool("list_chapters", serde_json::json!({ "book_id": book_id }))
        .await
        .expect("list_chapters call");

    let json = result_to_json(&result);
    let chapters = json["chapters"].as_array().cloned().unwrap_or_default();
    assert!(!chapters.is_empty());
}

#[tokio::test]
async fn test_synthesize_requires_authentication() {
    let harness = TestHarness::new(false).await.expect("harness");

    let error = harness
        .call_tool(
            "synthesize",
            serde_json::json!({
                "query": "Summarize the procedure",
                "format": "runsheet"
            }),
        )
        .await
        .expect_err("synthesize should fail without an API token");

    let text = error.to_string();
    assert!(text.contains("search_chunks_unavailable"));
}
