use crate::{
    db::queries::{books as book_queries, llm as llm_queries},
    llm::classify::classify_book,
    state::AppState,
};
use std::{sync::Arc, time::Duration};
use tokio::sync::Semaphore;

const LLM_JOB_POLL_INTERVAL: Duration = Duration::from_secs(30);
const MAX_CONCURRENT_LLM_JOBS: usize = 3;

pub fn spawn_semantic_job_runner(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run_semantic_job_runner(state).await;
    })
}

pub async fn reset_orphaned_semantic_jobs(state: &AppState) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        UPDATE llm_jobs
        SET status = 'pending', started_at = NULL, error_text = 'reset after server restart'
        WHERE status = 'running'
        "#,
    )
    .execute(&state.db)
    .await?;

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
    }
}
