//! Prometheus metrics integration.
//!
//! Wraps `axum-prometheus` and the `metrics` crate to expose operational gauges,
//! counters, and histograms at the `/metrics` endpoint.
//!
//! # Metric names
//! All xcalibre-server-specific metrics are prefixed `xcalibre-server_`:
//! - `xcalibre_server_llm_jobs_queued` — pending LLM jobs gauge
//! - `xcalibre_server_llm_jobs_running` — in-flight LLM jobs gauge
//! - `xcalibre_server_llm_jobs_failed_total` — cumulative failure counter
//! - `xcalibre_server_import_jobs_active` — concurrent import operations gauge
//! - `xcalibre_server_search_unindexed_books` — books not yet in search index
//! - `xcalibre_server_db_pool_connections` — DB connection pool size
//! - `xcalibre_server_storage_bytes_total` — total bytes across all format files
//!
//! HTTP request metrics (latency, count, status) are automatically collected by
//! `axum-prometheus` via the Axum layer returned by [`prometheus_layer`].
//!
//! # Noop mode
//! In test builds and when `XCS_DISABLE_METRICS=1` is set, a noop backend
//! is used so tests don't pollute a global registry.  [`MetricsHandle::render`]
//! returns a minimal placeholder in noop mode.
//!
//! # Refresh cadence
//! [`refresh_database_size_metrics`] is called by the scheduler after each loop
//! iteration (~every 30–60 seconds) to sync gauges from DB counts.

use axum_prometheus::PrometheusMetricLayer;
use metrics_exporter_prometheus::PrometheusHandle;
use sqlx::SqlitePool;
use std::{env, sync::OnceLock, time::Instant};

pub const LLM_JOBS_QUEUED: &str = "xcalibre_server_llm_jobs_queued";
pub const LLM_JOBS_RUNNING: &str = "xcalibre_server_llm_jobs_running";
pub const LLM_JOBS_FAILED: &str = "xcalibre_server_llm_jobs_failed_total";
pub const IMPORT_JOBS_ACTIVE: &str = "xcalibre_server_import_jobs_active";
pub const SEARCH_INDEX_LAG: &str = "xcalibre_server_search_unindexed_books";
pub const DB_POOL_SIZE: &str = "xcalibre_server_db_pool_connections";
pub const STORAGE_BYTES: &str = "xcalibre_server_storage_bytes_total";

#[derive(Clone)]
pub struct MetricsBundle {
    layer: PrometheusMetricLayer<'static>,
    handle: MetricsHandle,
}

impl MetricsBundle {
    fn new() -> Self {
        if metrics_disabled() {
            Self {
                layer: PrometheusMetricLayer::new(),
                handle: MetricsHandle::noop(),
            }
        } else {
            let (layer, handle) = PrometheusMetricLayer::pair();
            Self {
                layer,
                handle: MetricsHandle::real(handle),
            }
        }
    }

    pub fn layer(&self) -> PrometheusMetricLayer<'static> {
        self.layer.clone()
    }

    pub fn handle(&self) -> MetricsHandle {
        self.handle.clone()
    }
}

static METRICS: OnceLock<MetricsBundle> = OnceLock::new();

#[derive(Clone)]
pub struct MetricsHandle {
    inner: MetricsHandleInner,
}

#[derive(Clone)]
enum MetricsHandleInner {
    Real(PrometheusHandle),
    Noop,
}

impl MetricsHandle {
    fn real(handle: PrometheusHandle) -> Self {
        Self {
            inner: MetricsHandleInner::Real(handle),
        }
    }

    fn noop() -> Self {
        Self {
            inner: MetricsHandleInner::Noop,
        }
    }

    pub fn render(&self) -> String {
        match &self.inner {
            MetricsHandleInner::Real(handle) => handle.render(),
            MetricsHandleInner::Noop => "# HELP axum_http_requests_total Total HTTP requests\n\
# TYPE axum_http_requests_total counter\n\
axum_http_requests_total 0\n"
                .to_string(),
        }
    }
}

pub fn metrics_bundle() -> &'static MetricsBundle {
    METRICS.get_or_init(MetricsBundle::new)
}

pub fn prometheus_layer() -> PrometheusMetricLayer<'static> {
    metrics_bundle().layer()
}

pub fn prometheus_handle() -> MetricsHandle {
    metrics_bundle().handle()
}

fn metrics_disabled() -> bool {
    cfg!(test)
        || matches!(
            env::var("XCS_DISABLE_METRICS").as_deref(),
            Ok("1") | Ok("true") | Ok("yes")
        )
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

/// RAII guard that increments the active-imports gauge on construction and
/// decrements it (and records duration) on drop.
///
/// Use by constructing at the start of an import handler; the gauge tracks
/// concurrent imports in flight across all requests.
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
        ::metrics::histogram!("xcalibre_server_import_duration_seconds").record(duration_secs);
    }
}

/// Refresh LLM job count and storage byte gauges from the database.
///
/// Queries `llm_jobs` for pending/running counts and `formats` for total storage.
/// Called periodically by the scheduler.  Errors are propagated to the caller
/// which logs them as warnings.
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
