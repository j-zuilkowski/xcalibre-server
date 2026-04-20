use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub storage: Arc<dyn crate::storage::StorageBackend>,
    pub search: Arc<dyn crate::search::SearchBackend>,
    pub llm: Option<LlmClient>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LlmClient;

impl AppState {
    pub async fn new(db: SqlitePool, config: AppConfig) -> Self {
        let storage = Arc::new(crate::storage::LocalFsStorage::new(
            &config.app.storage_path,
        ));
        let search = crate::search::build_search_backend(&config, db.clone()).await;
        Self {
            db,
            config,
            storage,
            search,
            llm: None,
        }
    }
}
