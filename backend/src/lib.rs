pub mod api;
pub mod auth;
pub mod config;
pub mod db;
pub mod error;
pub mod ingest;
pub mod llm;
pub mod metrics;
pub mod middleware;
pub mod scheduler;
pub mod search;
pub mod webhooks;
pub mod state;
pub mod storage;
pub mod storage_s3;

use axum::{routing::get, Router};

pub use config::AppConfig;
pub use db::models::*;
pub use error::AppError;
pub use state::AppState;

pub type Result<T> = std::result::Result<T, AppError>;

pub fn app(state: AppState) -> Router {
    let (prometheus_layer, metrics_handle) = {
        let bundle = crate::metrics::metrics_bundle();
        (bundle.layer(), bundle.handle())
    };
    let api_router = api::router(state.clone());

    Router::new()
        .route(
            "/metrics",
            get(move || async move { metrics_handle.render() }),
        )
        .merge(api_router)
        .layer(prometheus_layer)
}

pub async fn bootstrap() -> anyhow::Result<(AppState, tokio::net::TcpListener)> {
    init_tracing();
    let _ = crate::metrics::metrics_bundle();

    let config = config::load_config().await?;
    let db = db::connect_sqlite_pool(&config.database.url, 5).await?;
    let migration_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/sqlite");
    let migrator = sqlx::migrate::Migrator::new(migration_path.as_path()).await?;
    migrator.run(&db).await?;

    let state = AppState::new(db, config).await?;
    let bind_addr = std::env::var("APP_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8083".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    Ok((state, listener))
}

pub async fn run() -> anyhow::Result<()> {
    let (state, listener) = bootstrap().await?;
    if state.config.llm.enabled {
        crate::llm::job_runner::reset_orphaned_semantic_jobs(&state).await?;
        tokio::spawn(crate::llm::job_runner::run_semantic_job_runner(
            state.clone(),
        ));
    }
    tokio::spawn(crate::scheduler::run_scheduler(state.clone()));
    axum::serve(listener, app(state)).await?;
    Ok(())
}

fn init_tracing() {
    static TRACING_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    let _ = TRACING_INIT.get_or_init(|| {
        use tracing_subscriber::EnvFilter;

        let log_format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "json".to_string());
        match log_format.as_str() {
            "text" => {
                tracing_subscriber::fmt()
                    .with_env_filter(EnvFilter::from_default_env())
                    .init();
            }
            _ => {
                tracing_subscriber::fmt()
                    .json()
                    .with_env_filter(EnvFilter::from_default_env())
                    .with_current_span(true)
                    .with_span_list(false)
                    .init();
            }
        }
    });
}
