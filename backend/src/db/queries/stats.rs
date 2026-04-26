//! Reading statistics queries for the user dashboard.
//! Touches: `book_user_state`, `reading_progress`, `formats`, `book_tags`,
//! `tags`, `book_authors`, `authors`.
//!
//! `get_user_stats` issues multiple targeted scalar/aggregate queries rather
//! than a single large join to keep each query fast on its individual indexes.
//!
//! Streak calculation is done in Rust: `reading_progress.updated_at` dates are
//! fetched as ordered `YYYY-MM-DD` strings, parsed to `NaiveDate`, and passed
//! to `compute_streaks` which walks consecutive-day runs.  The current streak
//! is the run that includes the most recent date; the longest streak is the
//! global maximum run.
//!
//! Monthly chart data: the last 12 calendar months are generated in Rust so
//! months with zero reads still appear as 0 in the result — the SQL query only
//! returns months that have at least one read record.

use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use serde::Serialize;
use sqlx::{Row, SqlitePool};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct NamedCount {
    pub name: String,
    pub count: i64,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
pub struct MonthlyCount {
    pub month: String,
    pub count: i64,
}

#[derive(Clone, Debug, Default, Serialize, ToSchema)]
#[schema(title = "UserStats")]
pub struct UserStats {
    pub total_books_read: i64,
    pub books_read_this_year: i64,
    pub books_read_this_month: i64,
    pub books_in_progress: i64,
    pub total_reading_sessions: i64,
    pub reading_streak_days: i64,
    pub longest_streak_days: i64,
    pub average_progress_per_session: f64,
    pub formats_read: BTreeMap<String, i64>,
    pub top_tags: Vec<NamedCount>,
    pub top_authors: Vec<NamedCount>,
    pub monthly_books: Vec<MonthlyCount>,
}

/// Builds the full `UserStats` for `user_id` by issuing ~8 targeted queries.
/// Top tags include only confirmed (`bt.confirmed = 1`) book–tag associations.
/// `reading_streak_days` counts consecutive days with any reading progress;
/// `longest_streak_days` is the historical maximum.
pub async fn get_user_stats(db: &SqlitePool, user_id: &str) -> anyhow::Result<UserStats> {
    let total_books_read: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM book_user_state
        WHERE user_id = ? AND is_read = 1
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let (year_start, year_end) = year_boundaries();
    // book_user_state stores the effective read timestamp in updated_at.
    let books_read_this_year: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM book_user_state
        WHERE user_id = ? AND is_read = 1
          AND updated_at >= ?
          AND updated_at < ?
        "#,
    )
    .bind(user_id)
    .bind(&year_start)
    .bind(&year_end)
    .fetch_one(db)
    .await?;

    let (month_start, month_end) = month_boundaries();
    let books_read_this_month: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM book_user_state
        WHERE user_id = ? AND is_read = 1
          AND updated_at >= ?
          AND updated_at < ?
        "#,
    )
    .bind(user_id)
    .bind(&month_start)
    .bind(&month_end)
    .fetch_one(db)
    .await?;

    let books_in_progress: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(DISTINCT book_id)
        FROM reading_progress
        WHERE user_id = ? AND percentage > 0 AND percentage < 100
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let total_reading_sessions: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM reading_progress
        WHERE user_id = ? AND percentage > 0
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let average_progress_per_session: f64 = sqlx::query_scalar(
        r#"
        SELECT COALESCE(AVG(percentage), 0.0)
        FROM reading_progress
        WHERE user_id = ? AND percentage > 0
        "#,
    )
    .bind(user_id)
    .fetch_one(db)
    .await?;

    let date_rows: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT DISTINCT DATE(updated_at)
        FROM reading_progress
        WHERE user_id = ?
        ORDER BY DATE(updated_at) DESC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;

    let mut dates = Vec::with_capacity(date_rows.len());
    for date in date_rows {
        if let Ok(parsed) = NaiveDate::parse_from_str(&date, "%Y-%m-%d") {
            dates.push(parsed);
        }
    }
    let (reading_streak_days, longest_streak_days) = compute_streaks(&dates);

    let format_rows = sqlx::query(
        r#"
        SELECT LOWER(f.format) AS format, COUNT(DISTINCT bus.book_id) AS count
        FROM book_user_state bus
        INNER JOIN formats f ON f.book_id = bus.book_id
        WHERE bus.user_id = ? AND bus.is_read = 1
        GROUP BY LOWER(f.format)
        ORDER BY count DESC, format ASC
        "#,
    )
    .bind(user_id)
    .fetch_all(db)
    .await?;
    let mut formats_read = BTreeMap::new();
    for row in format_rows {
        let format: String = row.get("format");
        let count: i64 = row.get("count");
        formats_read.insert(format, count);
    }

    let top_tags = fetch_named_counts(
        db,
        user_id,
        r#"
        SELECT t.name AS name, COUNT(DISTINCT bus.book_id) AS count
        FROM book_user_state bus
        INNER JOIN book_tags bt ON bt.book_id = bus.book_id AND bt.confirmed = 1
        INNER JOIN tags t ON t.id = bt.tag_id
        WHERE bus.user_id = ? AND bus.is_read = 1
        GROUP BY t.id, t.name
        ORDER BY count DESC, t.name ASC
        LIMIT 5
        "#,
    )
    .await?;

    let top_authors = fetch_named_counts(
        db,
        user_id,
        r#"
        SELECT a.name AS name, COUNT(DISTINCT bus.book_id) AS count
        FROM book_user_state bus
        INNER JOIN book_authors ba ON ba.book_id = bus.book_id
        INNER JOIN authors a ON a.id = ba.author_id
        WHERE bus.user_id = ? AND bus.is_read = 1
        GROUP BY a.id, a.name
        ORDER BY count DESC, a.name ASC
        LIMIT 5
        "#,
    )
    .await?;

    let months = last_twelve_months();
    let oldest_start = months
        .first()
        .map(|month| month.start.clone())
        .unwrap_or_else(|| month_start.clone());
    // The chart fills missing months in Rust so the UI always receives 12 points.
    let month_rows = sqlx::query(
        r#"
        SELECT strftime('%Y-%m', updated_at) AS month, COUNT(*) AS count
        FROM book_user_state
        WHERE user_id = ? AND is_read = 1
          AND updated_at >= ?
        GROUP BY month
        ORDER BY month
        "#,
    )
    .bind(user_id)
    .bind(&oldest_start)
    .fetch_all(db)
    .await?;
    let mut month_counts = BTreeMap::new();
    for row in month_rows {
        let month: String = row.get("month");
        let count: i64 = row.get("count");
        month_counts.insert(month, count);
    }
    let monthly_books = months
        .into_iter()
        .map(|month| {
            let label = month.label;
            let count = month_counts.remove(&label).unwrap_or(0);
            MonthlyCount {
                month: label,
                count,
            }
        })
        .collect();

    Ok(UserStats {
        total_books_read,
        books_read_this_year,
        books_read_this_month,
        books_in_progress,
        total_reading_sessions,
        reading_streak_days,
        longest_streak_days,
        average_progress_per_session,
        formats_read,
        top_tags,
        top_authors,
        monthly_books,
    })
}

#[derive(Clone, Debug)]
struct MonthWindow {
    label: String,
    start: String,
}

fn last_twelve_months() -> Vec<MonthWindow> {
    let today = Utc::now().date_naive();
    let mut year = today.year();
    let mut month = today.month();
    let mut months = Vec::with_capacity(12);

    for _ in 0..12 {
        months.push(MonthWindow {
            label: format!("{year:04}-{month:02}"),
            start: Utc
                .with_ymd_and_hms(year, month, 1, 0, 0, 0)
                .single()
                .expect("valid month boundary")
                .to_rfc3339(),
        });

        if month == 1 {
            year -= 1;
            month = 12;
        } else {
            month -= 1;
        }
    }

    months.reverse();
    months
}

fn year_boundaries() -> (String, String) {
    let today = Utc::now().date_naive();
    let year = today.year();

    let start = Utc
        .with_ymd_and_hms(year, 1, 1, 0, 0, 0)
        .single()
        .expect("valid year boundary")
        .to_rfc3339();
    let end = Utc
        .with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
        .single()
        .expect("valid year boundary")
        .to_rfc3339();

    (start, end)
}

fn month_boundaries() -> (String, String) {
    let today = Utc::now().date_naive();
    let year = today.year();
    let month = today.month();
    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };

    let start = Utc
        .with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .expect("valid month boundary")
        .to_rfc3339();
    let end = Utc
        .with_ymd_and_hms(next_year, next_month, 1, 0, 0, 0)
        .single()
        .expect("valid month boundary")
        .to_rfc3339();

    (start, end)
}

fn compute_streaks(dates: &[NaiveDate]) -> (i64, i64) {
    let Some(&first) = dates.first() else {
        return (0, 0);
    };

    let mut current_streak = 1_i64;
    let mut longest_streak = 1_i64;
    let mut run_length = 1_i64;
    let mut previous = first;
    let mut current_set = false;

    for &date in dates.iter().skip(1) {
        if previous.signed_duration_since(date).num_days() == 1 {
            run_length += 1;
        } else {
            if !current_set {
                current_streak = run_length;
                current_set = true;
            }
            longest_streak = longest_streak.max(run_length);
            run_length = 1;
        }
        previous = date;
    }

    if !current_set {
        current_streak = run_length;
    }
    longest_streak = longest_streak.max(run_length);

    (current_streak, longest_streak)
}

async fn fetch_named_counts(
    db: &SqlitePool,
    user_id: &str,
    sql: &str,
) -> anyhow::Result<Vec<NamedCount>> {
    let rows = sqlx::query(sql).bind(user_id).fetch_all(db).await?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        items.push(NamedCount {
            name: row.get("name"),
            count: row.get("count"),
        });
    }
    Ok(items)
}
