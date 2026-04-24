use crate::{
    config::AppConfig,
    db::queries::libraries as library_queries,
    llm::{chat::ChatClient, embeddings::EmbeddingClient},
    metrics,
    storage::StorageBackend,
    storage_s3::S3Storage,
};
use sqlx::SqlitePool;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: AppConfig,
    pub storage: Arc<dyn crate::storage::StorageBackend>,
    pub search: Arc<dyn crate::search::SearchBackend>,
    pub semantic_search: Option<Arc<crate::search::semantic::SemanticSearch>>,
    pub chat_client: Option<ChatClient>,
}

impl AppState {
    pub async fn new(db: SqlitePool, config: AppConfig) -> anyhow::Result<Self> {
        let _ = metrics::metrics_bundle();

        if let Err(err) =
            library_queries::sync_default_library_path(&db, &config.app.calibre_db_path).await
        {
            tracing::warn!(error = %err, "failed to sync default library path");
        }
        if config.auth.proxy.enabled {
            tracing::warn!(
                "proxy auth is enabled — ensure this server is behind a trusted reverse proxy"
            );
        }

        let storage_kind = config.storage.backend.trim().to_ascii_lowercase();
        let storage: Arc<dyn StorageBackend> = match storage_kind.as_str() {
            "local" => Arc::new(crate::storage::LocalFsStorage::new(
                &config.app.storage_path,
            )),
            "s3" => Arc::new(S3Storage::new(&config.storage.s3).await?),
            other => anyhow::bail!("Unknown storage backend: {other}"),
        };
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
        let chat_client = ChatClient::new(&config);

        metrics::set_db_pool_size(db.size() as u64);
        if let Err(err) = metrics::refresh_database_size_metrics(&db).await {
            tracing::warn!(error = %err, "failed to refresh startup metrics");
        }
        Ok(Self {
            db,
            config,
            storage,
            search,
            semantic_search,
            chat_client,
        })
    }
}
