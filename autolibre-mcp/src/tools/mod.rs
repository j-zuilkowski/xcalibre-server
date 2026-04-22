use backend::{
    config::AppConfig,
    db::queries::books as book_queries,
    ingest::text as ingest_text,
    llm::embeddings::EmbeddingClient,
    search::semantic::SemanticSearch,
    storage::{LocalFsStorage, StorageBackend},
};
use rmcp::schemars::JsonSchema;
use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content},
    tool, tool_handler, tool_router, ErrorData, ServerHandler,
};
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::{collections::HashMap, sync::Arc};

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CalibreMcpServer {}

#[derive(Clone)]
pub struct CalibreMcpServer {
    db: SqlitePool,
    config: AppConfig,
    storage: LocalFsStorage,
    semantic_search: Option<Arc<SemanticSearch>>,
    tool_router: ToolRouter<Self>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct SearchBooksRequest {
    pub q: Option<String>,
    pub author: Option<String>,
    pub tags: Option<String>,
    pub document_type: Option<String>,
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchBooksResponse {
    pub results: Vec<book_queries::BookSummary>,
    pub total: i64,
    pub page: i64,
    pub page_size: i64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetBookMetadataRequest {
    pub book_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ListChaptersRequest {
    pub book_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListChaptersResponse {
    pub book_id: String,
    pub format: String,
    pub chapters: Vec<ingest_text::Chapter>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct GetBookTextRequest {
    pub book_id: String,
    pub chapter: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetBookTextResponse {
    pub book_id: String,
    pub format: String,
    pub chapter: Option<u32>,
    pub text: String,
    pub word_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SemanticSearchRequest {
    pub query: String,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SemanticSearchResult {
    pub book_id: String,
    pub title: String,
    pub authors: String,
    pub score: f32,
}

#[tool_router(router = tool_router)]
impl CalibreMcpServer {
    pub fn new(db: SqlitePool, config: AppConfig) -> anyhow::Result<Self> {
        let semantic_search = if config.llm.enabled {
            match EmbeddingClient::new(&config) {
                Ok(client) => Some(Arc::new(SemanticSearch::new(db.clone(), client))),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to initialize semantic search client");
                    None
                }
            }
        } else {
            None
        };

        Ok(Self {
            db,
            storage: LocalFsStorage::new(&config.app.storage_path),
            config,
            semantic_search,
            tool_router: Self::tool_router(),
        })
    }

    #[tool(
        name = "search_books",
        description = "Search the library by title, author, tag, series, or full-text query. Returns a paginated list of matching books with metadata."
    )]
    pub async fn search_books(
        &self,
        Parameters(params): Parameters<SearchBooksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let page = i64::from(params.page.unwrap_or(1).max(1));
        let page_size = i64::from(params.page_size.unwrap_or(20).clamp(1, 50));
        let tags = parse_comma_separated(params.tags.as_deref());
        let strict_author = params
            .author
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(|s| s.to_ascii_lowercase());
        let strict_document_type = validate_document_type(params.document_type.as_deref())?;

        let query_text = merge_query(params.q.as_deref(), params.author.as_deref());
        let page_data = book_queries::list_books(
            &self.db,
            &book_queries::ListBooksParams {
                q: query_text,
                tags,
                page,
                page_size,
                ..Default::default()
            },
        )
        .await
        .map_err(internal_error)?;

        let mut items = page_data.items;
        if let Some(author_filter) = strict_author.as_ref() {
            items.retain(|book| {
                book.authors.iter().any(|author| {
                    author
                        .name
                        .to_ascii_lowercase()
                        .contains(author_filter.as_str())
                })
            });
        }
        if let Some(doc_type) = strict_document_type.as_deref() {
            items.retain(|book| book.document_type.eq_ignore_ascii_case(doc_type));
        }

        let total = if strict_author.is_some() || strict_document_type.is_some() {
            items.len() as i64
        } else {
            page_data.total
        };

        let value = serde_json::to_value(SearchBooksResponse {
            results: items,
            total,
            page: page_data.page,
            page_size: page_data.page_size,
        })
        .map_err(internal_error)?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "get_book_metadata",
        description = "Get full metadata for a single book including authors, tags, series, formats, and identifiers."
    )]
    pub async fn get_book_metadata(
        &self,
        Parameters(params): Parameters<GetBookMetadataRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let book_id = params.book_id.trim();
        if book_id.is_empty() {
            return Err(ErrorData::invalid_params(
                "book_id is required",
                Some(serde_json::json!({ "field": "book_id" })),
            ));
        }

        let Some(book) = book_queries::get_book_by_id(&self.db, book_id, None, None)
            .await
            .map_err(internal_error)?
        else {
            return Err(ErrorData::resource_not_found(
                "book not found",
                Some(serde_json::json!({ "book_id": book_id })),
            ));
        };

        let value = serde_json::to_value(book).map_err(internal_error)?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "list_chapters",
        description = "List the chapters of a book with titles and word counts. Requires the book to have an EPUB or PDF format."
    )]
    pub async fn list_chapters(
        &self,
        Parameters(params): Parameters<ListChaptersRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let (book, format, full_path) = self.load_extractable_book(&params.book_id).await?;
        let chapters = ingest_text::list_chapters(&full_path, format).unwrap_or_default();

        let value = serde_json::to_value(ListChaptersResponse {
            book_id: book.id,
            format: format.to_string(),
            chapters,
        })
        .map_err(internal_error)?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "get_book_text",
        description = "Extract plain text from a book. Optionally request a single chapter by index (0-based, matching list_chapters output). Returns full book text when chapter is omitted. No LLM required."
    )]
    pub async fn get_book_text(
        &self,
        Parameters(params): Parameters<GetBookTextRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let (book, format, full_path) = self.load_extractable_book(&params.book_id).await?;
        let text =
            ingest_text::extract_text(&full_path, format, params.chapter).unwrap_or_default();
        let word_count = text.split_whitespace().count();

        let value = serde_json::to_value(GetBookTextResponse {
            book_id: book.id,
            format: format.to_string(),
            chapter: params.chapter,
            text,
            word_count,
        })
        .map_err(internal_error)?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "semantic_search",
        description = "Search the library by semantic meaning using vector embeddings. Requires LLM features to be enabled (llm.enabled = true in config)."
    )]
    pub async fn semantic_search(
        &self,
        Parameters(params): Parameters<SemanticSearchRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let query = params.query.trim();
        if query.is_empty() {
            return Err(ErrorData::invalid_params(
                "query is required",
                Some(serde_json::json!({ "field": "query" })),
            ));
        }

        if !self.config.llm.enabled {
            return Ok(CallToolResult::error(vec![Content::text(
                "semantic_search_unavailable: LLM features are disabled. Enable llm.enabled in config.toml.",
            )]));
        }

        let Some(semantic) = self.semantic_search.as_ref() else {
            return Ok(CallToolResult::error(vec![Content::text(
                "semantic_search_unavailable: semantic search is not configured.",
            )]));
        };
        if !semantic.is_configured() {
            return Ok(CallToolResult::error(vec![Content::text(
                "semantic_search_unavailable: semantic search is not configured.",
            )]));
        }

        let limit = params.limit.unwrap_or(10).clamp(1, 50);
        let search_page = semantic
            .search_semantic(query, 1, limit)
            .await
            .map_err(internal_error)?;

        let ordered_ids = search_page
            .hits
            .iter()
            .map(|hit| hit.book_id.clone())
            .collect::<Vec<_>>();
        let summaries = book_queries::list_book_summaries_by_ids(&self.db, &ordered_ids, None, None)
            .await
            .map_err(internal_error)?;
        let mut summary_by_id = HashMap::new();
        for summary in summaries {
            summary_by_id.insert(summary.id.clone(), summary);
        }

        let mut results = Vec::new();
        for hit in search_page.hits {
            if let Some(summary) = summary_by_id.remove(&hit.book_id) {
                results.push(SemanticSearchResult {
                    book_id: summary.id,
                    title: summary.title,
                    authors: summary
                        .authors
                        .iter()
                        .map(|author| author.name.clone())
                        .collect::<Vec<_>>()
                        .join(", "),
                    score: hit.score,
                });
            }
        }

        let value = serde_json::to_value(results).map_err(internal_error)?;
        Ok(CallToolResult::structured(value))
    }
}

impl CalibreMcpServer {
    async fn load_extractable_book(
        &self,
        book_id: &str,
    ) -> Result<(backend::db::models::Book, &'static str, std::path::PathBuf), ErrorData> {
        let normalized_id = book_id.trim();
        if normalized_id.is_empty() {
            return Err(ErrorData::invalid_params(
                "book_id is required",
                Some(serde_json::json!({ "field": "book_id" })),
            ));
        }

        let Some(book) = book_queries::get_book_by_id(&self.db, normalized_id, None, None)
            .await
            .map_err(internal_error)?
        else {
            return Err(ErrorData::resource_not_found(
                "book not found",
                Some(serde_json::json!({ "book_id": normalized_id })),
            ));
        };

        let Some(format) = preferred_extractable_format(&book) else {
            return Err(ErrorData::invalid_request(
                "no_extractable_format",
                Some(serde_json::json!({ "book_id": normalized_id })),
            ));
        };

        let Some(format_file) = book_queries::find_format_file(&self.db, &book.id, format)
            .await
            .map_err(internal_error)?
        else {
            return Err(ErrorData::invalid_request(
                "no_extractable_format",
                Some(serde_json::json!({ "book_id": normalized_id })),
            ));
        };

        let full_path = self
            .storage
            .resolve(&format_file.path)
            .map_err(internal_error)?;
        Ok((book, format, full_path))
    }
}

fn preferred_extractable_format(book: &backend::db::models::Book) -> Option<&'static str> {
    if book
        .formats
        .iter()
        .any(|format| format.format.eq_ignore_ascii_case("EPUB"))
    {
        return Some("EPUB");
    }
    if book
        .formats
        .iter()
        .any(|format| format.format.eq_ignore_ascii_case("PDF"))
    {
        return Some("PDF");
    }
    None
}

fn parse_comma_separated(input: Option<&str>) -> Vec<String> {
    input
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn merge_query(q: Option<&str>, author: Option<&str>) -> Option<String> {
    let q = q.unwrap_or_default().trim();
    let author = author.unwrap_or_default().trim();
    match (q.is_empty(), author.is_empty()) {
        (true, true) => None,
        (false, true) => Some(q.to_string()),
        (true, false) => Some(author.to_string()),
        (false, false) => Some(format!("{q} {author}")),
    }
}

fn validate_document_type(document_type: Option<&str>) -> Result<Option<String>, ErrorData> {
    let value = document_type
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| value.to_ascii_lowercase());

    let Some(value) = value else {
        return Ok(None);
    };

    if matches!(
        value.as_str(),
        "novel" | "textbook" | "reference" | "magazine" | "datasheet" | "comic"
    ) {
        Ok(Some(value))
    } else {
        Err(ErrorData::invalid_params(
            "document_type must be one of novel|textbook|reference|magazine|datasheet|comic",
            Some(serde_json::json!({ "field": "document_type", "value": value })),
        ))
    }
}

fn internal_error(err: impl std::fmt::Display) -> ErrorData {
    tracing::error!(error = %err, "mcp tool failure");
    ErrorData::internal_error(
        "internal_error",
        Some(serde_json::json!({ "details": err.to_string() })),
    )
}
