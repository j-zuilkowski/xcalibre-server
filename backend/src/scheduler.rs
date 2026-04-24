use crate::{
    db::queries::{llm as llm_queries, scheduled_tasks as scheduled_task_queries},
    metrics, AppState,
};
use chrono::{DateTime, Utc};
use cron::Schedule;
use std::{str::FromStr, time::Duration};

const SCHEDULER_INTERVAL: Duration = Duration::from_secs(60);

pub fn spawn_scheduler(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        run_scheduler(state).await;
    })
}

pub async fn run_scheduler(state: AppState) {
    let mut interval = tokio::time::interval_at(
        tokio::time::Instant::now() + SCHEDULER_INTERVAL,
        SCHEDULER_INTERVAL,
    );
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    if let Err(err) = refresh_operational_metrics(&state).await {
        tracing::warn!(error = %err, "failed to refresh scheduler metrics");
    }

    loop {
        interval.tick().await;
        if let Err(err) = process_due_scheduled_tasks_once(&state).await {
            tracing::error!(error = %err, "scheduled task scheduler iteration failed");
        }
        if let Err(err) = refresh_operational_metrics(&state).await {
            tracing::warn!(error = %err, "failed to refresh scheduler metrics");
        }
    }
}

pub async fn process_due_scheduled_tasks_once(state: &AppState) -> anyhow::Result<usize> {
    let now = Utc::now();
    let due_tasks =
        scheduled_task_queries::list_due_scheduled_tasks(&state.db, &now.to_rfc3339()).await?;

    let mut completed = 0usize;
    for task in due_tasks {
        match run_scheduled_task(state, &task).await {
            Ok(()) => completed += 1,
            Err(err) => tracing::warn!(
                task_id = %task.id,
                task_name = %task.name,
                task_type = %task.task_type,
                error = %err,
                "failed to dispatch scheduled task"
            ),
        }
    }

    Ok(completed)
}

pub fn next_run_at_for_cron(cron_expr: &str, from: DateTime<Utc>) -> anyhow::Result<String> {
    let normalized = normalize_cron_expr(cron_expr)?;
    let schedule = Schedule::from_str(&normalized)?;
    let next_run = schedule
        .after(&from)
        .next()
        .ok_or_else(|| anyhow::anyhow!("cron expression has no future executions"))?;
    Ok(next_run.to_rfc3339())
}

fn normalize_cron_expr(cron_expr: &str) -> anyhow::Result<String> {
    let parts = cron_expr.split_whitespace().collect::<Vec<_>>();
    match parts.len() {
        5 => {
            let minute = parts[0];
            let hour = parts[1];
            let day_of_month = parts[2];
            let month = parts[3];
            let day_of_week = normalize_day_of_week_field(parts[4]);
            let (normalized_dom, normalized_dow) = match (day_of_month, day_of_week.as_str()) {
                ("*", "*") => ("*", "?"),
                ("*", dow) => ("?", dow),
                (dom, "*") => (dom, "?"),
                (dom, dow) => (dom, dow),
            };
            Ok(format!(
                "0 {minute} {hour} {normalized_dom} {month} {normalized_dow}"
            ))
        }
        6 | 7 => Ok(cron_expr.trim().to_string()),
        _ => anyhow::bail!("cron expression must have 5, 6, or 7 fields"),
    }
}

fn normalize_day_of_week_field(field: &str) -> String {
    field
        .split(',')
        .map(normalize_day_of_week_item)
        .collect::<Vec<_>>()
        .join(",")
}

fn normalize_day_of_week_item(item: &str) -> String {
    let item = item.trim();
    if item.is_empty() || item == "*" || item == "?" {
        return item.to_string();
    }

    if let Some((base, step)) = item.split_once('/') {
        return format!("{}/{}", normalize_day_of_week_item(base), step);
    }

    if let Some((start, end)) = item.split_once('-') {
        return format!(
            "{}-{}",
            normalize_day_of_week_item(start),
            normalize_day_of_week_item(end)
        );
    }

    match item {
        "0" | "7" => "1".to_string(),
        "1" => "2".to_string(),
        "2" => "3".to_string(),
        "3" => "4".to_string(),
        "4" => "5".to_string(),
        "5" => "6".to_string(),
        "6" => "7".to_string(),
        _ => item.to_string(),
    }
}

async fn run_scheduled_task(
    state: &AppState,
    task: &scheduled_task_queries::ScheduledTask,
) -> anyhow::Result<()> {
    match task.task_type.as_str() {
        "classify_all" => {
            let _ = llm_queries::enqueue_organize_job(&state.db).await?;
        }
        "semantic_index_all" => {
            let _ = llm_queries::enqueue_semantic_index_jobs_for_all_books(&state.db).await?;
        }
        "backup" => {
            let _ = llm_queries::enqueue_backup_job(&state.db).await?;
        }
        other => {
            return Err(anyhow::anyhow!("unsupported scheduled task type: {other}"));
        }
    }

    let now = Utc::now().to_rfc3339();
    let next_run_at = next_run_at_for_cron(&task.cron_expr, Utc::now())?;
    scheduled_task_queries::mark_scheduled_task_ran(&state.db, &task.id, &now, &next_run_at)
        .await?;
    Ok(())
}

async fn refresh_operational_metrics(state: &AppState) -> anyhow::Result<()> {
    metrics::refresh_database_size_metrics(&state.db).await?;

    let unindexed_books: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(1)
        FROM books
        WHERE indexed_at IS NULL
        "#,
    )
    .fetch_one(&state.db)
    .await?;
    metrics::set_search_index_lag(unindexed_books.max(0) as u64);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_standard_five_field_cron() {
        let normalized = normalize_cron_expr("0 2 * * 0").expect("normalize");
        assert_eq!(normalized, "0 0 2 ? * 1");
        let next_run = next_run_at_for_cron("0 2 * * 0", Utc::now());
        assert!(next_run.is_ok(), "{next_run:?}");
    }
}
