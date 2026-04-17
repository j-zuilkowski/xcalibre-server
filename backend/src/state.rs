use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub llm_client: Option<LlmClient>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LlmClient;

impl AppState {
    pub fn new(db: SqlitePool, config: AppConfig) -> Self {
        Self {
            db,
            config,
            llm_client: None,
        }
    }
}
