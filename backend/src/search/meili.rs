use crate::{
    db::models::Book,
    search::{SearchBackend, SearchHit, SearchPage, SearchQuery},
};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const AVAILABILITY_CACHE_TTL: Duration = Duration::from_secs(30);

pub struct MeilisearchBackend {
    base_url: String,
    api_key: Option<String>,
    http: reqwest::Client,
    availability: RwLock<Option<AvailabilityCache>>,
}

#[derive(Clone, Copy)]
struct AvailabilityCache {
    checked_at: Instant,
    available: bool,
}

#[derive(Debug, Deserialize)]
struct MeiliSearchResponse {
    #[serde(default)]
    hits: Vec<MeiliHit>,
    #[serde(rename = "estimatedTotalHits")]
    estimated_total_hits: Option<u64>,
    #[serde(rename = "totalHits")]
    total_hits: Option<u64>,
    page: Option<u32>,
    #[serde(rename = "hitsPerPage")]
    hits_per_page: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct MeiliHit {
    id: Value,
    title: Option<String>,
    #[serde(rename = "_rankingScore")]
    ranking_score: Option<f64>,
}

#[derive(Debug, Serialize)]
struct BookIndexDocument {
    id: String,
    title: String,
    authors: Vec<String>,
    tags: Vec<String>,
    series: Option<String>,
    language: Option<String>,
    description: Option<String>,
}

impl MeilisearchBackend {
    pub async fn new(base_url: String, api_key: Option<String>) -> Option<Self> {
        let base_url = normalize_base_url(base_url);
        let api_key = normalize_api_key(api_key);

        let candidate = Self {
            base_url,
            api_key,
            http: reqwest::Client::new(),
            availability: RwLock::new(None),
        };

        if !candidate.ping_health().await {
            tracing::warn!(
                base_url = %candidate.base_url,
                "meilisearch health check failed at startup; falling back to fts5"
            );
            return None;
        }

        Some(candidate)
    }

    async fn ping_health(&self) -> bool {
        let response = self
            .request(Method::GET, "/health", None)
            .await
            .and_then(require_success_status);

        response.is_ok()
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<reqwest::Response> {
        let url = format!("{}{}", self.base_url, path);
        let mut req = self.http.request(method, url);

        if let Some(api_key) = self.api_key.as_deref() {
            req = req.bearer_auth(api_key);
        }

        if let Some(payload) = body {
            req = req.json(&payload);
        }

        Ok(req.send().await?)
    }

    async fn search_meili(&self, query: &SearchQuery) -> Result<SearchPage> {
        let page = if query.page < 1 { 1 } else { query.page };
        let page_size = clamp_page_size(query.page_size);

        let mut body = json!({
            "q": query.q.trim(),
            "page": page,
            "hitsPerPage": page_size,
            "showRankingScore": true,
        });

        if let Some(filters) = meili_filters(query) {
            body["filter"] = filters;
        }

        let response = self
            .request(Method::POST, "/indexes/books/search", Some(body))
            .await
            .and_then(require_success_status)?;

        let payload: MeiliSearchResponse = response.json().await?;

        let hits = payload
            .hits
            .into_iter()
            .filter_map(|hit| {
                meili_id_to_string(hit.id).map(|book_id| SearchHit {
                    book_id,
                    score: hit.ranking_score.unwrap_or_default() as f32,
                })
            })
            .collect::<Vec<_>>();

        let total = payload
            .estimated_total_hits
            .or(payload.total_hits)
            .unwrap_or(hits.len() as u64);

        Ok(SearchPage {
            hits,
            total,
            page: payload.page.unwrap_or(page),
            page_size: payload.hits_per_page.unwrap_or(page_size),
        })
    }

    async fn suggest_meili(&self, q: &str, limit: u8) -> Result<Vec<String>> {
        let body = json!({
            "q": q.trim(),
            "limit": limit.clamp(1, 10),
            "attributesToRetrieve": ["title"],
        });

        let response = self
            .request(Method::POST, "/indexes/books/search", Some(body))
            .await
            .and_then(require_success_status)?;

        let payload: MeiliSearchResponse = response.json().await?;

        Ok(payload
            .hits
            .into_iter()
            .filter_map(|hit| hit.title)
            .collect::<Vec<_>>())
    }

    async fn upsert_document(&self, book: &Book) -> Result<()> {
        let doc = BookIndexDocument {
            id: book.id.clone(),
            title: book.title.clone(),
            authors: book
                .authors
                .iter()
                .map(|author| author.name.clone())
                .collect(),
            tags: book.tags.iter().map(|tag| tag.name.clone()).collect(),
            series: book.series.as_ref().map(|series| series.name.clone()),
            language: book.language.clone(),
            description: book.description.clone(),
        };

        self.request(Method::POST, "/indexes/books/documents", Some(json!([doc])))
            .await
            .and_then(require_success_status)?;

        Ok(())
    }

    async fn remove_document(&self, book_id: &str) -> Result<()> {
        let path = format!("/indexes/books/documents/{book_id}");
        self.request(Method::DELETE, &path, None)
            .await
            .and_then(require_success_status)?;
        Ok(())
    }
}

#[async_trait]
impl SearchBackend for MeilisearchBackend {
    async fn search(&self, query: &SearchQuery) -> Result<SearchPage> {
        if query.q.trim().is_empty() {
            return Ok(SearchPage {
                hits: Vec::new(),
                total: 0,
                page: query.page.max(1),
                page_size: clamp_page_size(query.page_size),
            });
        }

        self.search_meili(query).await
    }

    async fn suggest(&self, q: &str, limit: u8) -> Result<Vec<String>> {
        if q.trim().is_empty() {
            return Ok(Vec::new());
        }

        self.suggest_meili(q, limit).await
    }

    async fn index_book(&self, book: &Book) -> Result<()> {
        self.upsert_document(book).await
    }

    async fn remove_book(&self, book_id: &str) -> Result<()> {
        self.remove_document(book_id).await
    }

    async fn is_available(&self) -> bool {
        if let Some(availability) = *self.availability.read().await {
            if availability.checked_at.elapsed() <= AVAILABILITY_CACHE_TTL {
                return availability.available;
            }
        }

        let available = self.ping_health().await;
        let mut lock = self.availability.write().await;
        *lock = Some(AvailabilityCache {
            checked_at: Instant::now(),
            available,
        });
        available
    }

    fn backend_name(&self) -> &'static str {
        "meilisearch"
    }
}

fn meili_id_to_string(id: Value) -> Option<String> {
    match id {
        Value::String(value) => Some(value),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn normalize_base_url(base_url: String) -> String {
    base_url.trim_end_matches('/').to_string()
}

fn normalize_api_key(api_key: Option<String>) -> Option<String> {
    api_key.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn require_success_status(response: reqwest::Response) -> Result<reqwest::Response> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(anyhow!(
            "meilisearch request failed with status {}",
            response.status()
        ))
    }
}

fn meili_filters(query: &SearchQuery) -> Option<Value> {
    let mut filters = Vec::new();

    if let Some(author) = query
        .author_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        filters.push(format!("authors = \"{author}\""));
    }

    if let Some(tag) = query
        .tag
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        filters.push(format!("tags = \"{tag}\""));
    }

    if let Some(language) = query
        .language
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        filters.push(format!("language = \"{language}\""));
    }

    if query
        .format
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .is_some()
    {
        tracing::debug!("format filter is not supported by the meilisearch index document shape");
    }

    if filters.is_empty() {
        None
    } else {
        Some(json!(filters))
    }
}

fn clamp_page_size(page_size: u32) -> u32 {
    match page_size {
        0 => 24,
        n if n > 100 => 100,
        n => n,
    }
}
