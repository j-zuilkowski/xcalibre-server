use crate::config::AppConfig;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub storage_path: PathBuf,
    pub llm_client: Option<LlmClient>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LlmClient;

impl AppState {
    pub fn new(db: SqlitePool, config: AppConfig, storage_path: PathBuf) -> Self {
        Self {
            db,
            config,
            storage_path,
            llm_client: None,
        }
    }
}

