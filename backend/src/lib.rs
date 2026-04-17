pub mod api;
pub mod config;
pub mod db;
pub mod error;
pub mod middleware;
pub mod state;

pub use api::router as app;
pub use config::AppConfig;
pub use db::models::*;
pub use error::AppError;
pub use state::AppState;

pub type Result<T> = std::result::Result<T, AppError>;

pub async fn run() -> anyhow::Result<()> {
    Ok(())
}

