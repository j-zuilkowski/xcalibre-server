use crate::{config::AppConfig, llm::embeddings::EmbeddingClient};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub storage: Arc<dyn crate::storage::StorageBackend>,
    pub search: Arc<dyn crate::search::SearchBackend>,
    pub semantic_search: Option<Arc<crate::search::semantic::SemanticSearch>>,
}

impl AppState {
    pub async fn new(db: SqlitePool, config: AppConfig) -> Self {
        let storage = Arc::new(crate::storage::LocalFsStorage::new(
            &config.app.storage_path,
        ));
        let search = crate::search::build_search_backend(&config, db.clone()).await;
        let semantic_search = if config.llm.enabled {
            match EmbeddingClient::new(&config) {
                Ok(client) => Some(Arc::new(crate::search::semantic::SemanticSearch::new(
                    db.clone(),
                    client,
                ))),
                Err(err) => {
                    tracing::warn!(error = %err, "failed to initialize embedding client");
                    None
                }
            }
        } else {
            None
        };
        Self {
            db,
            config,
            storage,
            search,
            semantic_search,
        }
    }
}
