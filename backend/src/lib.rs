pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod llm;
pub mod middleware;
pub mod search;
pub mod state;
pub mod storage;

pub use api::router as app;
pub use config::AppConfig;
pub use db::models::*;
pub use error::AppError;
pub use state::AppState;

pub type Result<T> = std::result::Result<T, AppError>;

pub async fn bootstrap() -> anyhow::Result<(AppState, tokio::net::TcpListener)> {
    init_tracing();

    let config = config::load_config().await?;
    let db = db::connect_sqlite_pool(&config.database.url, 5).await?;
    let migration_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/sqlite");
    let migrator = sqlx::migrate::Migrator::new(migration_path.as_path()).await?;
    migrator.run(&db).await?;

    let state = AppState::new(db, config).await;
    let bind_addr = std::env::var("APP_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8083".to_string());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;

    Ok((state, listener))
}

pub async fn run() -> anyhow::Result<()> {
    let (state, listener) = bootstrap().await?;
    if state.config.llm.enabled {
        tokio::spawn(crate::llm::job_runner::run_semantic_job_runner(
            state.clone(),
        ));
    }
    axum::serve(listener, app(state)).await?;
    Ok(())
}

fn init_tracing() {
    static TRACING_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    let _ = TRACING_INIT.get_or_init(|| {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(
                tracing_subscriber::EnvFilter::try_from_default_env()
                    .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
            )
            .try_init();
    });
}
