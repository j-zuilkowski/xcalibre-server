use crate::{db::queries::llm as llm_queries, state::AppState};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

const SEMANTIC_JOB_POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_SEMANTIC_JOBS: usize = 3;

pub fn spawn_semantic_job_runner(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run_semantic_job_runner(state).await;
    })
}

pub async fn run_semantic_job_runner(state: AppState) {
    let mut interval = tokio::time::interval(SEMANTIC_JOB_POLL_INTERVAL);
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_SEMANTIC_JOBS));

    loop {
        interval.tick().await;
        if let Err(err) = process_pending_jobs_once_with_semaphore(&state, semaphore.clone()).await
        {
            tracing::error!(error = %err, "semantic job runner iteration failed");
        }
    }
}

pub async fn process_pending_jobs_once(state: &AppState) -> anyhow::Result<usize> {
    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_SEMANTIC_JOBS));
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

        let Some(job) = llm_queries::claim_next_semantic_index_job(&state.db).await? else {
            drop(permit);
            break;
        };

        let state = state.clone();
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            process_semantic_job(state, job).await;
        }));
    }

    let processed = handles.len();
    for handle in handles {
        if let Err(err) = handle.await {
            tracing::error!(error = %err, "semantic job task join error");
        }
    }

    Ok(processed)
}

async fn process_semantic_job(state: AppState, job: llm_queries::SemanticIndexJob) {
    let Some(book_id) = job.book_id.clone() else {
        let _ = llm_queries::mark_semantic_job_failed(&state.db, &job.id, "missing_book_id").await;
        return;
    };

    let Some(semantic) = state.semantic_search.clone() else {
        let _ = llm_queries::mark_semantic_job_failed(&state.db, &job.id, "llm_unavailable").await;
        return;
    };

    if !semantic.is_configured() {
        let _ = llm_queries::mark_semantic_job_failed(&state.db, &job.id, "llm_unavailable").await;
        return;
    }

    let document = match semantic.load_book_document(&book_id).await {
        Ok(Some(document)) => document,
        Ok(None) => {
            let _ =
                llm_queries::mark_semantic_job_failed(&state.db, &job.id, "book_not_found").await;
            return;
        }
        Err(err) => {
            let _ = llm_queries::mark_semantic_job_failed(&state.db, &job.id, &format!("{err:#}"))
                .await;
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
            if let Err(err) = llm_queries::mark_semantic_job_completed(&state.db, &job.id).await {
                tracing::error!(job_id = %job.id, error = %err, "failed to mark semantic job completed");
            }
        }
        Err(err) => {
            if let Err(update_err) =
                llm_queries::mark_semantic_job_failed(&state.db, &job.id, &format!("{err:#}")).await
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
