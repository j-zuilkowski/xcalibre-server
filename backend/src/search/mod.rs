use anyhow::Result;
use async_trait::async_trait;

pub mod fts5;

#[derive(Clone, Debug)]
pub struct SearchQuery {
    pub q: String,
    pub author_id: Option<String>,
    pub tag: Option<String>,
    pub language: Option<String>,
    pub format: Option<String>,
    pub page: u32,
    pub page_size: u32,
}

#[derive(Clone, Debug)]
pub struct SearchHit {
    pub book_id: String,
    pub score: f32,
}

#[derive(Clone, Debug)]
pub struct SearchPage {
    pub hits: Vec<SearchHit>,
    pub total: u64,
    pub page: u32,
    pub page_size: u32,
}

#[async_trait]
pub trait SearchBackend: Send + Sync {
    async fn search(&self, query: &SearchQuery) -> Result<SearchPage>;
    async fn suggest(&self, q: &str, limit: u8) -> Result<Vec<String>>;
    async fn is_available(&self) -> bool;
    fn backend_name(&self) -> &'static str;
}
