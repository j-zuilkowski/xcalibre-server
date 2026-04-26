//! Background job loop for asynchronous LLM processing.
//!
//! Polls the `llm_jobs` table every 30 seconds and dispatches pending jobs to Tokio
//! tasks.  At most [`MAX_CONCURRENT_LLM_JOBS`] jobs run in parallel (controlled by a
//! Tokio [`Semaphore`]).
//!
//! # Job types
//! | `job_type`       | Handler                       | Description                         |
//! |------------------|-------------------------------|-------------------------------------|
//! | `semantic_index` | [`process_semantic_job`]      | Embed book text into `book_embeddings` |
//! | `classify`       | [`process_classify_job`]      | Tag suggestions via LLM             |
//! | `organize`       | [`process_organize_job`]      | Enqueue `classify` for unprocessed books (up to 50 at a time) |
//! | `backup`         | [`process_backup_job`]        | No-op placeholder; marks completed  |
//!
//! # Restart safety
//! [`reset_orphaned_semantic_jobs`] is called at startup to reset any jobs that were
//! `running` when the server was killed, moving them back to `pending`.
//!
//! # Single-process design
//! There is no distributed lock.  This runner is intended to run in one process.
//! If horizontal scaling is needed later, a DB-level advisory lock or a separate queue
//! service would be required.
//!
//! # Metrics
//! Running/queued counts are tracked via [`metrics::decrement_llm_jobs_running`] /
//! [`metrics::increment_llm_jobs_queued_by`] for Prometheus export.

use crate::{
    db::queries::{books as book_queries, llm as llm_queries},
    llm::classify::classify_book,
    metrics,
    state::AppState,
    webhooks as webhook_engine,
};
use chrono::Utc;
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

const LLM_JOB_POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_LLM_JOBS: usize = 3;

/// Spawn the background LLM job runner as a detached Tokio task.
///
/// The returned [`JoinHandle`] can be used to abort the task on shutdown,
/// but is typically dropped (the task runs for the lifetime of the process).
pub fn spawn_semantic_job_runner(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run_semantic_job_runner(state).await;
    })
}

/// Reset any jobs stuck in `running` state back to `pending`.
///
/// Called once at server startup to recover from unclean shutdowns.
/// Also updates Prometheus metrics to reflect the corrected counts.
pub async fn reset_orphaned_semantic_jobs(state: &AppState) -> anyhow::Result<()> {
    let reset = sqlx::query(
        r#"
        UPDATE llm_jobs
        SET status = 'pending', started_at = NULL, error_text = 'reset after server restart'
        WHERE status = 'running'
        "#,
    )
    .execute(&state.db)
    .await?;
    let rows = reset.rows_affected();
    metrics::decrement_llm_jobs_running(rows);
    metrics::increment_llm_jobs_queued_by(rows);

    Ok(())
}

pub async fn run_semantic_job_runner(state: AppState) {
    let mut interval = tokio::time::interval(LLM_JOB_POLL_INTERVAL);
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_LLM_JOBS));

    loop {
        interval.tick().await;
        if let Err(err) = process_pending_jobs_once_with_semaphore(&state, semaphore.clone()).await
        {
            tracing::error!(error = %err, "llm job runner iteration failed");
        }
    }
}

/// Claim and process all currently-claimable pending jobs, returning the count processed.
///
/// Exposed for integration tests that need to drive the job loop synchronously.
pub async fn process_pending_jobs_once(state: &AppState) -> anyhow::Result<usize> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_LLM_JOBS));
    process_pending_jobs_once_with_semaphore(state, semaphore).await
}

async fn process_pending_jobs_once_with_semaphore(
    state: &AppState,
    semaphore: Arc<Semaphore>,
) -> anyhow::Result<usize> {
    let mut handles = Vec::new();

    loop {
        let permit = match semaphore.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => break,
        };

        let Some(job) = llm_queries::claim_next_pending_job(&state.db).await? else {
            drop(permit);
            break;
        };

        let state = state.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            process_job(state, job).await;
        }));
    }

    let processed = handles.len();
    for handle in handles {
        if let Err(err) = handle.await {
            tracing::error!(error = %err, "llm job task join error");
        }
    }

    Ok(processed)
}

async fn process_job(state: AppState, job: llm_queries::SemanticIndexJob) {
    match job.job_type.as_str() {
        "semantic_index" => process_semantic_job(state, job).await,
        "classify" => process_classify_job(state, job).await,
        "organize" => process_organize_job(state, job).await,
        "backup" => process_backup_job(state, job).await,
        other => {
            tracing::warn!(job_id = %job.id, job_type = other, "unknown job type, skipping");
            let _ = llm_queries::mark_job_completed(&state.db, &job.id).await;
        }
    }
}

async fn process_semantic_job(state: AppState, job: llm_queries::SemanticIndexJob) {
    let Some(book_id) = job.book_id.clone() else {
        let _ = llm_queries::mark_job_failed(&state.db, &job.id, "missing_book_id").await;
        return;
    };

    let Some(semantic) = state.semantic_search.clone() else {
        let _ = llm_queries::mark_job_failed(&state.db, &job.id, "llm_unavailable").await;
        return;
    };

    if !semantic.is_configured() {
        let _ = llm_queries::mark_job_failed(&state.db, &job.id, "llm_unavailable").await;
        return;
    }

    let document = match semantic.load_book_document(&book_id).await {
        Ok(Some(document)) => document,
        Ok(None) => {
            let _ = llm_queries::mark_job_failed(&state.db, &job.id, "book_not_found").await;
            return;
        }
        Err(err) => {
            let _ = llm_queries::mark_job_failed(&state.db, &job.id, &format!("{err:#}")).await;
            return;
        }
    };

    match semantic
        .index_book(
            &book_id,
            &document.title,
            &document.authors,
            &document.description,
        )
        .await
    {
        Ok(()) => {
            if let Err(err) = llm_queries::mark_job_completed(&state.db, &job.id).await {
                tracing::error!(job_id = %job.id, error = %err, "failed to mark semantic job completed");
            } else {
                emit_llm_job_completed_event(&state, &job, Some(&book_id), Some(&document.title))
                    .await;
            }
        }
        Err(err) => {
            if let Err(update_err) =
                llm_queries::mark_job_failed(&state.db, &job.id, &format!("{err:#}")).await
            {
                tracing::error!(
                    job_id = %job.id,
                    error = %update_err,
                    "failed to mark semantic job failed"
                );
            }
        }
    }
}

async fn process_classify_job(state: AppState, job: llm_queries::SemanticIndexJob) {
    let Some(book_id) = job.book_id.clone() else {
        let _ = llm_queries::mark_job_failed(&state.db, &job.id, "missing_book_id").await;
        return;
    };

    let Some(chat_client) = state.chat_client.as_ref() else {
        let _ = llm_queries::mark_job_failed(&state.db, &job.id, "llm_unavailable").await;
        return;
    };

    let book = match book_queries::get_book_by_id(&state.db, &book_id, None, None).await {
        Ok(Some(book)) => book,
        Ok(None) => {
            let _ = llm_queries::mark_job_failed(&state.db, &job.id, "book_not_found").await;
            return;
        }
        Err(err) => {
            let _ = llm_queries::mark_job_failed(&state.db, &job.id, &format!("{err:#}")).await;
            return;
        }
    };

    let authors = book
        .authors
        .iter()
        .map(|author| author.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let description = book.description.as_deref().unwrap_or_default();

    let result = classify_book(chat_client, &book.title, &authors, description).await;
    if let Err(err) =
        llm_queries::insert_tag_suggestions(&state.db, &book_id, &result.suggestions).await
    {
        let _ = llm_queries::mark_job_failed(&state.db, &job.id, &format!("{err:#}")).await;
        return;
    }

    if let Err(err) = llm_queries::mark_job_completed(&state.db, &job.id).await {
        tracing::error!(job_id = %job.id, error = %err, "failed to mark classify job completed");
    } else {
        emit_llm_job_completed_event(&state, &job, Some(&book_id), Some(&book.title)).await;
    }
}

async fn process_organize_job(state: AppState, job: llm_queries::SemanticIndexJob) {
    let book_ids = match sqlx::query_scalar::<_, String>(
        r#"
        SELECT b.id
        FROM books b
        WHERE NOT EXISTS (
            SELECT 1
            FROM llm_jobs j
            WHERE j.job_type = 'classify'
              AND j.book_id = b.id
              AND j.status IN ('pending', 'running', 'completed')
        )
        ORDER BY b.created_at ASC
        LIMIT 50
        "#,
    )
    .fetch_all(&state.db)
    .await
    {
        Ok(ids) => ids,
        Err(err) => {
            let _ = llm_queries::mark_job_failed(&state.db, &job.id, &format!("{err:#}")).await;
            return;
        }
    };

    for book_id in book_ids {
        if let Err(err) = llm_queries::enqueue_classify_job(&state.db, &book_id).await {
            let _ = llm_queries::mark_job_failed(&state.db, &job.id, &format!("{err:#}")).await;
            return;
        }
    }

    if let Err(err) = llm_queries::mark_job_completed(&state.db, &job.id).await {
        tracing::error!(job_id = %job.id, error = %err, "failed to mark organize job completed");
    } else {
        emit_llm_job_completed_event(&state, &job, None, None).await;
    }
}

async fn process_backup_job(state: AppState, job: llm_queries::SemanticIndexJob) {
    if let Err(err) = llm_queries::mark_job_completed(&state.db, &job.id).await {
        tracing::error!(job_id = %job.id, error = %err, "failed to mark backup job completed");
    } else {
        emit_llm_job_completed_event(&state, &job, None, None).await;
    }
}

async fn emit_llm_job_completed_event(
    state: &AppState,
    job: &llm_queries::SemanticIndexJob,
    book_id: Option<&str>,
    title: Option<&str>,
) {
    let _ = webhook_engine::enqueue_event(
        &state.db,
        "llm_job.completed",
        serde_json::json!({
            "event": "llm_job.completed",
            "timestamp": Utc::now().to_rfc3339(),
            "library_name": state.config.app.library_name.clone(),
            "data": {
                "job_id": job.id.clone(),
                "type": job.job_type.clone(),
                "book_id": book_id,
                "title": title,
            }
        }),
    )
    .await;
}
