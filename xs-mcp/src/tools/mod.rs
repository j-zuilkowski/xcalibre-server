use backend::{
    config::AppConfig,
    db::queries::{books as book_queries, collections as collection_queries},
    ingest::text as ingest_text,
    llm::synthesize as synthesis,
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

#[tool_handler(router = self.tool_router)]
impl ServerHandler for CalibreMcpServer {}

#[derive(Clone)]
pub struct CalibreMcpServer {
    db: SqlitePool,
    storage: LocalFsStorage,
    llm_enabled: bool,
    chat_client: Option<backend::llm::chat::ChatClient>,
    api_client: reqwest::Client,
    api_base_url: String,
    api_token: Option<String>,
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

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct SearchChunksRequest {
    pub query: String,
    pub book_ids: Option<Vec<String>>,
    pub collection_id: Option<String>,
    pub chunk_type: Option<String>,
    pub limit: Option<u32>,
    pub rerank: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchChunksResponseItem {
    pub chunk_id: String,
    pub book_id: String,
    pub book_title: String,
    pub heading_path: Option<String>,
    pub chunk_type: String,
    pub text: String,
    pub word_count: i64,
    pub bm25_score: Option<f32>,
    pub cosine_score: Option<f32>,
    pub rrf_score: f32,
    pub rerank_score: Option<f32>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct SearchChunksResponse {
    pub query: String,
    pub chunks: Vec<SearchChunksResponseItem>,
    pub total_searched: u64,
    pub retrieval_ms: u64,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct SynthesizeRequest {
    pub query: String,
    pub format: String,
    pub collection_id: Option<String>,
    pub book_ids: Option<Vec<String>>,
    pub chunk_type: Option<String>,
    pub rerank: Option<bool>,
    pub limit: Option<u32>,
    pub custom_prompt: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Default)]
pub struct GetCollectionRequest {
    pub collection_id: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ListCollectionsResponse {
    pub collections: Vec<collection_queries::CollectionSummary>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GetCollectionResponse {
    #[serde(flatten)]
    pub collection: collection_queries::CollectionDetail,
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
        let api_client = reqwest::Client::builder().build()?;
        let api_base_url = config.app.base_url.trim_end_matches('/').to_string();
        let api_token = std::env::var("XCS_API_TOKEN")
            .or_else(|_| std::env::var("APP_API_TOKEN"))
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        Ok(Self {
            db,
            storage: LocalFsStorage::new(&config.app.storage_path),
            llm_enabled: config.llm.enabled,
            chat_client: backend::llm::chat::ChatClient::new(&config),
            api_client,
            api_base_url,
            api_token,
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
                publisher: None,
                rating_bucket: None,
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
        name = "list_collections",
        description = "List accessible collections owned by the current user or marked public."
    )]
    pub async fn list_collections(&self) -> Result<CallToolResult, ErrorData> {
        let Some(token) = self.api_token.as_deref() else {
            return Ok(CallToolResult::error(vec![Content::text(
                "collections_unavailable: configure XCS_API_TOKEN or APP_API_TOKEN.",
            )]));
        };

        let url = format!("{}/api/v1/collections", self.api_base_url);
        let response = self
            .api_client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(internal_error)?
            .error_for_status()
            .map_err(internal_error)?;

        let collections: Vec<collection_queries::CollectionSummary> =
            response.json().await.map_err(internal_error)?;
        let value = serde_json::to_value(ListCollectionsResponse { collections })
            .map_err(internal_error)?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "get_collection",
        description = "Get a collection with its book list."
    )]
    pub async fn get_collection(
        &self,
        Parameters(params): Parameters<GetCollectionRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let collection_id = params.collection_id.trim();
        if collection_id.is_empty() {
            return Err(ErrorData::invalid_params(
                "collection_id is required",
                Some(serde_json::json!({ "field": "collection_id" })),
            ));
        }

        let Some(token) = self.api_token.as_deref() else {
            return Ok(CallToolResult::error(vec![Content::text(
                "collections_unavailable: configure XCS_API_TOKEN or APP_API_TOKEN.",
            )]));
        };

        let url = format!("{}/api/v1/collections/{}", self.api_base_url, collection_id);
        let response = self
            .api_client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(internal_error)?
            .error_for_status()
            .map_err(internal_error)?;

        let collection: collection_queries::CollectionDetail =
            response.json().await.map_err(internal_error)?;
        let value =
            serde_json::to_value(GetCollectionResponse { collection }).map_err(internal_error)?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(
        name = "search_chunks",
        description = "Search the library with hybrid BM25 plus semantic chunk retrieval. Returns chunk-level passages with provenance fields."
    )]
    pub async fn search_chunks(
        &self,
        Parameters(params): Parameters<SearchChunksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        self.search_chunks_internal(params).await
    }

    #[tool(
        name = "semantic_search",
        description = "Deprecated alias for search_chunks."
    )]
    pub async fn semantic_search(
        &self,
        Parameters(params): Parameters<SearchChunksRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        if !self.llm_enabled {
            return Ok(CallToolResult::error(vec![Content::text(
                "semantic_search_unavailable: LLM features are disabled. Enable llm.enabled in config.toml.",
            )]));
        }

        self.search_chunks_internal(params).await
    }

    #[tool(
        name = "synthesize",
        description = "Retrieve relevant passages from the library and synthesize a grounded derivative work in the specified format."
    )]
    pub async fn synthesize(
        &self,
        Parameters(params): Parameters<SynthesizeRequest>,
    ) -> Result<CallToolResult, ErrorData> {
        let chunks = self
            .retrieve_chunks(&SearchChunksRequest {
                query: params.query.clone(),
                book_ids: params.book_ids.clone(),
                collection_id: params.collection_id.clone(),
                chunk_type: params.chunk_type.clone(),
                limit: params.limit,
                rerank: params.rerank,
            })
            .await?;

        let synthesis_chunks = chunks
            .chunks
            .iter()
            .map(|chunk| synthesis::SynthesisChunk {
                chunk_id: chunk.chunk_id.clone(),
                book_id: chunk.book_id.clone(),
                book_title: chunk.book_title.clone(),
                heading_path: chunk.heading_path.clone(),
                chunk_type: chunk.chunk_type.clone(),
                text: chunk.text.clone(),
                word_count: chunk.word_count,
                bm25_score: chunk.bm25_score,
                cosine_score: chunk.cosine_score,
                rrf_score: chunk.rrf_score,
                rerank_score: chunk.rerank_score,
            })
            .collect::<Vec<_>>();

        let result = synthesis::synthesize(
            self.chat_client.as_ref(),
            self.llm_enabled,
            &params.query,
            &params.format,
            params.custom_prompt.as_deref(),
            synthesis_chunks,
            chunks.retrieval_ms,
        )
        .await
        .map_err(internal_error)?;

        let value = serde_json::to_value(result).map_err(internal_error)?;
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

    async fn search_chunks_internal(
        &self,
        params: SearchChunksRequest,
    ) -> Result<CallToolResult, ErrorData> {
        let payload = self.retrieve_chunks(&params).await?;
        let value = serde_json::to_value(payload.chunks).map_err(internal_error)?;
        Ok(CallToolResult::structured(value))
    }

    async fn retrieve_chunks(
        &self,
        params: &SearchChunksRequest,
    ) -> Result<SearchChunksResponse, ErrorData> {
        let query = params.query.trim();
        if query.is_empty() {
            return Err(ErrorData::invalid_params(
                "query is required",
                Some(serde_json::json!({ "field": "query" })),
            ));
        }

        let Some(token) = self.api_token.as_deref() else {
            return Err(ErrorData::internal_error(
                "search_chunks_unavailable",
                Some(
                    serde_json::json!({ "details": "configure XCS_API_TOKEN or APP_API_TOKEN" }),
                ),
            ));
        };

        let limit = params.limit.unwrap_or(10).clamp(1, 50);
        let mut query_params = vec![
            ("q".to_string(), query.to_string()),
            ("limit".to_string(), limit.to_string()),
        ];

        if params.rerank.unwrap_or(false) {
            query_params.push(("rerank".to_string(), "true".to_string()));
        }

        if let Some(chunk_type) = params
            .chunk_type
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            query_params.push(("type".to_string(), chunk_type.to_string()));
        }

        if let Some(book_ids) = params.book_ids.as_ref() {
            for book_id in book_ids {
                let value = book_id.trim();
                if !value.is_empty() {
                    query_params.push(("book_ids[]".to_string(), value.to_string()));
                }
            }
        }

        let url = if let Some(collection_id) = params
            .collection_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            format!(
                "{}/api/v1/collections/{}/search/chunks",
                self.api_base_url, collection_id
            )
        } else {
            format!("{}/api/v1/search/chunks", self.api_base_url)
        };

        let response = self
            .api_client
            .get(url)
            .bearer_auth(token)
            .query(&query_params)
            .send()
            .await
            .map_err(internal_error)?
            .error_for_status()
            .map_err(internal_error)?;

        response.json().await.map_err(internal_error)
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
