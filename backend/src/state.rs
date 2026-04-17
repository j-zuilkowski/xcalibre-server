use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub storage: Arc<dyn crate::storage::StorageBackend>,
    pub llm_client: Option<LlmClient>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LlmClient;

impl AppState {
    pub fn new(db: SqlitePool, config: AppConfig) -> Self {
        let storage = Arc::new(crate::storage::LocalFsStorage::new(&config.app.storage_path));
        Self {
            db,
            config,
            storage,
            llm_client: None,
        }
    }
}
