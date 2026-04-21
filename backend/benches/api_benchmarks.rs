use anyhow::Context;
use backend::{
    db::{
        self,
        queries::books::{self as book_queries, ListBooksParams},
    },
    search::{fts5::Fts5Backend, SearchBackend, SearchQuery},
};
use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sqlx::{migrate::Migrator, SqliteConnection, SqlitePool};
use std::{path::Path, sync::Arc};
use tokio::runtime::Runtime;
use uuid::Uuid;

#[derive(Clone)]
struct BenchFixture {
    db: SqlitePool,
    search: Arc<Fts5Backend>,
    book_id: String,
    author_ids: Vec<String>,
    tag_ids: Vec<String>,
}

fn bench_runtime() -> Runtime {
    Runtime::new().expect("create tokio runtime")
}

async fn build_fixture() -> anyhow::Result<BenchFixture> {
    let db = db::connect_sqlite_pool("sqlite::memory:", 1)
        .await
        .context("connect sqlite benchmark db")?;
    let migration_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations/sqlite");
    let migrator = Migrator::new(migration_path.as_path())
        .await
        .context("load sqlite migrations")?;
    migrator.run(&db).await.context("run sqlite migrations")?;

    let now = Utc::now().to_rfc3339();
    let author_ids = vec![
        insert_author(&db, "Alpha Author", &now).await?,
        insert_author(&db, "Beta Author", &now).await?,
        insert_author(&db, "Gamma Author", &now).await?,
    ];
    let tag_ids = vec![
        insert_tag(&db, "alpha-tag-1", &now).await?,
        insert_tag(&db, "alpha-tag-2", &now).await?,
        insert_tag(&db, "alpha-tag-3", &now).await?,
        insert_tag(&db, "alpha-tag-4", &now).await?,
        insert_tag(&db, "alpha-tag-5", &now).await?,
    ];

    let mut conn = db.acquire().await.context("acquire seed connection")?;
    sqlx::query("BEGIN")
        .execute(&mut *conn)
        .await
        .context("begin seed transaction")?;

    let mut first_book_id = String::new();
    for index in 0..1000 {
        let book_id = Uuid::new_v4().to_string();
        if index == 0 {
            first_book_id = book_id.clone();
        }

        let title = format!("Alpha Beta Benchmark Book {index:04}");
        let sort_title = title.clone();
        insert_book_row(&mut conn, &book_id, &title, &sort_title, &now).await?;

        if index == 0 {
            for (display_order, author_id) in author_ids.iter().enumerate() {
                insert_book_author(&mut conn, &book_id, author_id, display_order as i64).await?;
            }
            for tag_id in &tag_ids {
                insert_book_tag(&mut conn, &book_id, tag_id).await?;
            }
            insert_format(&mut conn, &book_id, "EPUB", &now).await?;
        } else {
            insert_book_author(&mut conn, &book_id, &author_ids[0], 0).await?;
        }
    }

    sqlx::query("COMMIT")
        .execute(&mut *conn)
        .await
        .context("commit seed transaction")?;

    Ok(BenchFixture {
        db: db.clone(),
        search: Arc::new(Fts5Backend::new(db)),
        book_id: first_book_id,
        author_ids,
        tag_ids,
    })
}

async fn insert_author(db: &SqlitePool, name: &str, now: &str) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(name)
        .bind(now)
        .execute(db)
        .await
        .context("insert author")?;
    Ok(id)
}

async fn insert_tag(db: &SqlitePool, name: &str, now: &str) -> anyhow::Result<String> {
    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO tags (id, name, last_modified) VALUES (?, ?, ?)")
        .bind(&id)
        .bind(name)
        .bind(now)
        .execute(db)
        .await
        .context("insert tag")?;
    Ok(id)
}

async fn insert_book_row(
    conn: &mut SqliteConnection,
    book_id: &str,
    title: &str,
    sort_title: &str,
    now: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO books (
            id, title, sort_title, description, pubdate, language, rating,
            series_id, series_index, document_type, has_cover, cover_path,
            flags, indexed_at, created_at, last_modified
        ) VALUES (?, ?, ?, NULL, NULL, NULL, NULL, NULL, NULL, 'unknown', 0, NULL, NULL, NULL, ?, ?)
        "#,
    )
    .bind(book_id)
    .bind(title)
    .bind(sort_title)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await
    .context("insert book")?;
    Ok(())
}

async fn insert_book_author(
    conn: &mut SqliteConnection,
    book_id: &str,
    author_id: &str,
    display_order: i64,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, ?)")
        .bind(book_id)
        .bind(author_id)
        .bind(display_order)
        .execute(&mut *conn)
        .await
        .context("insert book author")?;
    Ok(())
}

async fn insert_book_tag(
    conn: &mut SqliteConnection,
    book_id: &str,
    tag_id: &str,
) -> anyhow::Result<()> {
    sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
        .bind(book_id)
        .bind(tag_id)
        .execute(&mut *conn)
        .await
        .context("insert book tag")?;
    Ok(())
}

async fn insert_format(
    conn: &mut SqliteConnection,
    book_id: &str,
    format: &str,
    now: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        r#"
        INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(Uuid::new_v4().to_string())
    .bind(book_id)
    .bind(format)
    .bind(format!("{book_id}.{}", format.to_lowercase()))
    .bind(0_i64)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await
    .context("insert format")?;
    Ok(())
}

fn bench_database(c: &mut Criterion) {
    let rt = Arc::new(bench_runtime());
    let fixture = rt
        .block_on(build_fixture())
        .expect("build benchmark fixture");

    let list_params = ListBooksParams {
        page: 1,
        page_size: 30,
        ..Default::default()
    };
    let search_query = SearchQuery {
        q: "alpha beta".to_string(),
        author_id: None,
        tag: None,
        language: None,
        format: None,
        page: 1,
        page_size: 30,
    };

    let mut group = c.benchmark_group("database");

    {
        let db = fixture.db.clone();
        let params = list_params.clone();
        let runtime = Arc::clone(&rt);
        group.bench_function("bench_list_books_1000", move |b| {
            b.iter(|| {
                let db = db.clone();
                let params = params.clone();
                runtime.block_on(async move {
                    let page = book_queries::list_books(&db, &params)
                        .await
                        .expect("list_books benchmark");
                    black_box(page.items.len());
                    black_box(page.total);
                });
            });
        });
    }

    {
        let search = fixture.search.clone();
        let query = search_query.clone();
        let runtime = Arc::clone(&rt);
        group.bench_function("bench_search_fts5", move |b| {
            b.iter(|| {
                let search = search.clone();
                let query = query.clone();
                runtime.block_on(async move {
                    let page = search.search(&query).await.expect("fts5 benchmark");
                    black_box(page.hits.len());
                    black_box(page.total);
                });
            });
        });
    }

    {
        let db = fixture.db.clone();
        let book_id = fixture.book_id.clone();
        let runtime = Arc::clone(&rt);
        group.bench_function("bench_get_book", move |b| {
            b.iter(|| {
                let db = db.clone();
                let book_id = book_id.clone();
                runtime.block_on(async move {
                    let book = book_queries::get_book_by_id(&db, &book_id)
                        .await
                        .expect("get_book benchmark")
                        .expect("book exists");
                    black_box(book.authors.len());
                    black_box(book.tags.len());
                    black_box(book.formats.len());
                });
            });
        });
    }

    group.finish();

    let mut group = c.benchmark_group("ingest");
    {
        let db = fixture.db.clone();
        let author_ids = fixture.author_ids.clone();
        let tag_ids = fixture.tag_ids.clone();
        let runtime = Arc::clone(&rt);
        group.bench_function("bench_insert_book", move |b| {
            b.iter(|| {
                let db = db.clone();
                let author_ids = author_ids.clone();
                let tag_ids = tag_ids.clone();
                runtime.block_on(async move {
                    insert_book_tx(&db, &author_ids, &tag_ids)
                        .await
                        .expect("insert benchmark");
                });
            });
        });
    }
    group.finish();
}

async fn insert_book_tx(
    db: &SqlitePool,
    author_ids: &[String],
    tag_ids: &[String],
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    let book_id = Uuid::new_v4().to_string();
    let mut conn = db.acquire().await.context("acquire insert connection")?;
    sqlx::query("BEGIN")
        .execute(&mut *conn)
        .await
        .context("begin insert transaction")?;

    insert_book_row(
        &mut conn,
        &book_id,
        "Benchmark Insert Book",
        "Benchmark Insert Book",
        &now,
    )
    .await?;

    for (display_order, author_id) in author_ids.iter().take(3).enumerate() {
        insert_book_author(&mut conn, &book_id, author_id, display_order as i64).await?;
    }
    for tag_id in tag_ids.iter().take(5) {
        insert_book_tag(&mut conn, &book_id, tag_id).await?;
    }
    insert_format(&mut conn, &book_id, "EPUB", &now).await?;

    sqlx::query("ROLLBACK")
        .execute(&mut *conn)
        .await
        .context("rollback insert transaction")?;
    Ok(())
}

criterion_group!(benches, bench_database);
criterion_main!(benches);
