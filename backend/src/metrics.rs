use axum_prometheus::PrometheusMetricLayer;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use sqlx::SqlitePool;
use std::{sync::OnceLock, time::Instant};

pub const LLM_JOBS_QUEUED: &str = "autolibre_llm_jobs_queued";
pub const LLM_JOBS_RUNNING: &str = "autolibre_llm_jobs_running";
pub const LLM_JOBS_FAILED: &str = "autolibre_llm_jobs_failed_total";
pub const IMPORT_JOBS_ACTIVE: &str = "autolibre_import_jobs_active";
pub const SEARCH_INDEX_LAG: &str = "autolibre_search_unindexed_books";
pub const DB_POOL_SIZE: &str = "autolibre_db_pool_connections";
pub const STORAGE_BYTES: &str = "autolibre_storage_bytes_total";

#[derive(Clone)]
pub struct MetricsBundle {
    layer: PrometheusMetricLayer<'static>,
    handle: PrometheusHandle,
}

impl MetricsBundle {
    fn new() -> Self {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        metrics::set_global_recorder(recorder).expect("Failed to set global recorder");
        let layer = PrometheusMetricLayer::new();
        Self { layer, handle }
    }

    pub fn layer(&self) -> PrometheusMetricLayer<'static> {
        self.layer.clone()
    }

    pub fn handle(&self) -> PrometheusHandle {
        self.handle.clone()
    }
}

static METRICS: OnceLock<MetricsBundle> = OnceLock::new();

pub fn metrics_bundle() -> &'static MetricsBundle {
    METRICS.get_or_init(MetricsBundle::new)
}

pub fn prometheus_layer() -> PrometheusMetricLayer<'static> {
    metrics_bundle().layer()
}

pub fn prometheus_handle() -> PrometheusHandle {
    metrics_bundle().handle()
}

pub fn set_db_pool_size(size: u64) {
    ::metrics::gauge!(DB_POOL_SIZE).set(size as f64);
}

pub fn set_llm_jobs_queued(count: u64) {
    ::metrics::gauge!(LLM_JOBS_QUEUED).set(count as f64);
}

pub fn set_llm_jobs_running(count: u64) {
    ::metrics::gauge!(LLM_JOBS_RUNNING).set(count as f64);
}

pub fn increment_llm_jobs_queued() {
    ::metrics::gauge!(LLM_JOBS_QUEUED).increment(1.0);
}

pub fn increment_llm_jobs_queued_by(count: u64) {
    if count > 0 {
        ::metrics::gauge!(LLM_JOBS_QUEUED).increment(count as f64);
    }
}

pub fn decrement_llm_jobs_queued(count: u64) {
    if count > 0 {
        ::metrics::gauge!(LLM_JOBS_QUEUED).decrement(count as f64);
    }
}

pub fn increment_llm_jobs_running() {
    ::metrics::gauge!(LLM_JOBS_RUNNING).increment(1.0);
}

pub fn increment_llm_jobs_running_by(count: u64) {
    if count > 0 {
        ::metrics::gauge!(LLM_JOBS_RUNNING).increment(count as f64);
    }
}

pub fn decrement_llm_jobs_running(count: u64) {
    if count > 0 {
        ::metrics::gauge!(LLM_JOBS_RUNNING).decrement(count as f64);
    }
}

pub fn increment_llm_jobs_failed(count: u64) {
    if count > 0 {
        ::metrics::counter!(LLM_JOBS_FAILED).increment(count);
    }
}

pub fn set_search_index_lag(count: u64) {
    ::metrics::gauge!(SEARCH_INDEX_LAG).set(count as f64);
}

pub fn set_storage_bytes(bytes: u64) {
    ::metrics::gauge!(STORAGE_BYTES).set(bytes as f64);
}

pub struct ImportMetricsGuard {
    started_at: Instant,
}

impl ImportMetricsGuard {
    pub fn new() -> Self {
        ::metrics::gauge!(IMPORT_JOBS_ACTIVE).increment(1.0);
        Self {
            started_at: Instant::now(),
        }
    }
}

impl Default for ImportMetricsGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for ImportMetricsGuard {
    fn drop(&mut self) {
        ::metrics::gauge!(IMPORT_JOBS_ACTIVE).decrement(1.0);
        let duration_secs = self.started_at.elapsed().as_secs_f64();
        ::metrics::histogram!("autolibre_import_duration_seconds").record(duration_secs);
    }
}

pub async fn refresh_database_size_metrics(db: &SqlitePool) -> anyhow::Result<()> {
    let queued: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(1)
        FROM llm_jobs
        WHERE status = 'pending'
        "#,
    )
    .fetch_one(db)
    .await?;
    let running: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(1)
        FROM llm_jobs
        WHERE status = 'running'
        "#,
    )
    .fetch_one(db)
    .await?;
    let storage_bytes: i64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(SUM(size_bytes), 0)
        FROM formats
        "#,
    )
    .fetch_one(db)
    .await?;

    set_llm_jobs_queued(queued.max(0) as u64);
    set_llm_jobs_running(running.max(0) as u64);
    set_storage_bytes(storage_bytes.max(0) as u64);

    Ok(())
}
