use crate::search::{SearchBackend, SearchHit, SearchPage, SearchQuery};
use anyhow::Result;
use async_trait::async_trait;
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};

#[derive(Clone)]
pub struct Fts5Backend {
    db: SqlitePool,
}

impl Fts5Backend {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }
}

#[async_trait]
impl SearchBackend for Fts5Backend {
    async fn search(&self, query: &SearchQuery) -> Result<SearchPage> {
        let page = if query.page < 1 { 1 } else { query.page };
        let page_size = clamp_page_size(query.page_size);
        let offset = i64::from(page.saturating_sub(1)) * i64::from(page_size);

        let Some(match_query) = normalize_fts_query(&query.q) else {
            return Ok(SearchPage {
                hits: Vec::new(),
                total: 0,
                page,
                page_size,
            });
        };

        let mut total_query = QueryBuilder::<Sqlite>::new(
            "SELECT COUNT(DISTINCT b.id) AS total FROM books_fts INNER JOIN books b ON (books_fts.book_id = b.id OR books_fts.rowid = b.rowid)",
        );
        apply_filters(&mut total_query, query, &match_query);

        let total: i64 = total_query.build_query_scalar().fetch_one(&self.db).await?;

        let mut data_query = QueryBuilder::<Sqlite>::new(
            "SELECT b.id AS book_id, books_fts.rank AS rank FROM books_fts INNER JOIN books b ON (books_fts.book_id = b.id OR books_fts.rowid = b.rowid)",
        );
        apply_filters(&mut data_query, query, &match_query);
        data_query.push(" ORDER BY books_fts.rank ASC, b.id ASC LIMIT ");
        data_query.push_bind(i64::from(page_size));
        data_query.push(" OFFSET ");
        data_query.push_bind(offset);

        let rows = data_query.build().fetch_all(&self.db).await?;

        let min_rank = rows
            .iter()
            .map(|row| row.get::<f64, _>("rank"))
            .filter(|rank| rank.is_finite())
            .fold(f64::INFINITY, |acc, rank| acc.min(rank));

        let hits = rows
            .into_iter()
            .map(|row| {
                let rank = row.get::<f64, _>("rank");
                SearchHit {
                    book_id: row.get("book_id"),
                    score: score_from_rank(rank, min_rank),
                }
            })
            .collect::<Vec<_>>();

        Ok(SearchPage {
            hits,
            total: total.max(0) as u64,
            page,
            page_size,
        })
    }

    async fn suggest(&self, q: &str, limit: u8) -> Result<Vec<String>> {
        let Some(match_query) = normalize_fts_query(q) else {
            return Ok(Vec::new());
        };

        let rows = sqlx::query(
            r#"
            SELECT DISTINCT b.title AS title
            FROM books_fts
            INNER JOIN books b ON (books_fts.book_id = b.id OR books_fts.rowid = b.rowid)
            WHERE books_fts MATCH ?
            ORDER BY b.sort_title ASC
            LIMIT ?
            "#,
        )
        .bind(match_query)
        .bind(i64::from(limit.clamp(1, 10)))
        .fetch_all(&self.db)
        .await?;

        Ok(rows.into_iter().map(|row| row.get("title")).collect())
    }

    async fn is_available(&self) -> bool {
        true
    }

    fn backend_name(&self) -> &'static str {
        "fts5"
    }
}

fn apply_filters(qb: &mut QueryBuilder<'_, Sqlite>, query: &SearchQuery, match_query: &str) {
    let mut where_added = false;
    let mut and_where = |qb: &mut QueryBuilder<'_, Sqlite>| {
        if !where_added {
            qb.push(" WHERE ");
            where_added = true;
        } else {
            qb.push(" AND ");
        }
    };

    and_where(qb);
    qb.push("books_fts MATCH ");
    qb.push_bind(match_query.to_string());

    if let Some(author_id) = query
        .author_id
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push(
            "EXISTS (SELECT 1 FROM book_authors ba WHERE ba.book_id = b.id AND ba.author_id = ",
        );
        qb.push_bind(author_id);
        qb.push(")");
    }

    if let Some(tag) = query
        .tag
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push(
            "EXISTS (SELECT 1 FROM book_tags bt INNER JOIN tags t ON t.id = bt.tag_id WHERE bt.book_id = b.id AND lower(t.name) = ",
        );
        qb.push_bind(tag.to_lowercase());
        qb.push(")");
    }

    if let Some(language) = query
        .language
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push("lower(b.language) = ");
        qb.push_bind(language.to_lowercase());
    }

    if let Some(format) = query
        .format
        .as_ref()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
    {
        and_where(qb);
        qb.push("EXISTS (SELECT 1 FROM formats f WHERE f.book_id = b.id AND upper(f.format) = ");
        qb.push_bind(format.to_uppercase());
        qb.push(")");
    }

    if let Some(book_ids) = query.book_ids.as_ref() {
        if book_ids.is_empty() {
            and_where(qb);
            qb.push("1 = 0");
            return;
        }
        and_where(qb);
        qb.push("b.id IN (");
        for (index, book_id) in book_ids.iter().enumerate() {
            if index > 0 {
                qb.push(", ");
            }
            qb.push_bind(book_id.to_owned());
        }
        qb.push(")");
    }
}

fn normalize_fts_query(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    let mut sanitized = String::with_capacity(raw.len());
    for ch in raw.chars() {
        if ch.is_alphanumeric() || ch.is_whitespace() || ch == '*' {
            sanitized.push(ch);
        } else {
            sanitized.push(' ');
        }
    }

    let terms = sanitized
        .split_whitespace()
        .map(|term| term.trim_matches('*'))
        .filter(|term| !term.is_empty())
        .map(|term| format!("{term}*"))
        .collect::<Vec<_>>();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

fn score_from_rank(rank: f64, min_rank: f64) -> f32 {
    if !rank.is_finite() || !min_rank.is_finite() {
        return 0.0;
    }

    if min_rank.abs() < f64::EPSILON {
        return 1.0;
    }

    (rank / min_rank).clamp(0.0, 1.0) as f32
}

fn clamp_page_size(page_size: u32) -> u32 {
    match page_size {
        0 => 24,
        n if n > 100 => 100,
        n => n,
    }
}
