use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use anyhow::Context;
use chrono::Utc;
use sqlx::{Row, Sqlite, SqlitePool, Transaction};
use uuid::Uuid;

use crate::calibre::reader::CalibreReader;
use crate::calibre::schema::{
    CalibeSeries, CalibreAuthor, CalibreEntry, CalibreFormat, CalibreTag,
};
use crate::import::covers;
use crate::report::{FailureRecord, MigrationReport};

#[derive(Clone, Debug)]
pub struct LocalFs {
    root: PathBuf,
}

impl LocalFs {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    pub fn resolve(&self, relative_path: &str) -> anyhow::Result<PathBuf> {
        let clean = sanitize_relative_path(relative_path)?;
        Ok(self.root.join(clean))
    }

    pub fn copy_from(&self, source: &Path, relative_path: &str) -> anyhow::Result<PathBuf> {
        let target = self.resolve(relative_path)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("create storage directory {}", parent.display()))?;
        }
        std::fs::copy(source, &target)
            .with_context(|| format!("copy {} -> {}", source.display(), target.display()))?;
        Ok(target)
    }
}

#[derive(Debug, Clone)]
struct AvailableFormat {
    source_path: PathBuf,
    source: CalibreFormat,
}

#[derive(Debug, Clone)]
struct CopiedFormat {
    id: String,
    format: String,
    path: String,
    size_bytes: i64,
}

#[derive(Clone)]
pub struct ImportPipeline {
    target_db: SqlitePool,
    storage: LocalFs,
    dry_run: bool,
    library_id: String,
}

impl ImportPipeline {
    pub fn new(
        target_db: SqlitePool,
        storage: LocalFs,
        dry_run: bool,
        library_id: impl Into<String>,
    ) -> Self {
        Self {
            target_db,
            storage,
            dry_run,
            library_id: library_id.into(),
        }
    }

    pub async fn run(
        &self,
        entries: Vec<CalibreEntry>,
        reader: &CalibreReader,
    ) -> anyhow::Result<MigrationReport> {
        let mut report = MigrationReport {
            total: entries.len(),
            ..MigrationReport::default()
        };

        for entry in entries {
            if self.is_duplicate_calibre_id(entry.book.id).await? {
                report.skipped += 1;
                continue;
            }

            let available_formats = collect_existing_formats(reader, &entry);
            if available_formats.is_empty() {
                eprintln!(
                    "warning: skipping calibre_id {} ({}) because no format files found on disk",
                    entry.book.id, entry.book.title
                );
                report.skipped += 1;
                continue;
            }

            if self.dry_run {
                println!("would import: {}", entry.book.title);
                report.imported += 1;
                continue;
            }

            match self
                .import_single_entry(&entry, &available_formats, reader)
                .await
            {
                Ok(()) => {
                    println!("imported: {}", entry.book.title);
                    report.imported += 1;
                }
                Err(err) => {
                    report.failed += 1;
                    report.failures.push(FailureRecord {
                        calibre_id: entry.book.id,
                        title: entry.book.title.clone(),
                        reason: err.to_string(),
                    });
                }
            }
        }

        Ok(report)
    }

    async fn is_duplicate_calibre_id(&self, calibre_id: i64) -> anyhow::Result<bool> {
        let exists = sqlx::query(
            "SELECT 1 FROM identifiers WHERE id_type = 'calibre_id' AND value = ? LIMIT 1",
        )
        .bind(calibre_id.to_string())
        .fetch_optional(&self.target_db)
        .await
        .context("query existing calibre_id identifier")?
        .is_some();

        Ok(exists)
    }

    async fn import_single_entry(
        &self,
        entry: &CalibreEntry,
        available_formats: &[AvailableFormat],
        reader: &CalibreReader,
    ) -> anyhow::Result<()> {
        let now = Utc::now().to_rfc3339();
        let book_id = Uuid::new_v4().to_string();

        let copied_formats = self.copy_formats(available_formats)?;
        let (has_cover, cover_path) = self.copy_cover_if_present(entry, &book_id, reader)?;

        let mut tx = self
            .target_db
            .begin()
            .await
            .context("begin import transaction")?;
        let series_id = self
            .get_or_create_series(&mut tx, entry.series.as_ref(), &now)
            .await?;

        sqlx::query(
            r#"
            INSERT INTO books (
                id, library_id, title, sort_title, description, pubdate, language, rating,
                series_id, series_index, has_cover, cover_path, flags, indexed_at,
                created_at, last_modified
            )
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&book_id)
        .bind(&self.library_id)
        .bind(&entry.book.title)
        .bind(&entry.book.sort)
        .bind(entry.comment.as_ref().map(|comment| comment.text.clone()))
        .bind(entry.book.pubdate.clone())
        .bind(Option::<String>::None)
        .bind(entry.book.rating)
        .bind(series_id)
        .bind(entry.book.series_index)
        .bind(if has_cover { 1_i64 } else { 0_i64 })
        .bind(cover_path)
        .bind(Option::<String>::None)
        .bind(Option::<String>::None)
        .bind(&now)
        .bind(&entry.book.last_modified)
        .execute(tx.as_mut())
        .await
        .context("insert target book")?;

        self.insert_authors(&mut tx, &book_id, &entry.authors, &now)
            .await?;
        self.insert_tags(&mut tx, &book_id, &entry.tags, &now)
            .await?;
        self.insert_formats(&mut tx, &book_id, &copied_formats, &now)
            .await?;
        self.insert_identifiers(&mut tx, &book_id, entry, &now)
            .await?;

        tx.commit().await.context("commit import transaction")?;
        Ok(())
    }

    fn copy_formats(
        &self,
        available_formats: &[AvailableFormat],
    ) -> anyhow::Result<Vec<CopiedFormat>> {
        let mut copied = Vec::with_capacity(available_formats.len());
        for format in available_formats {
            let format_id = Uuid::new_v4().to_string();
            let extension = format.source.format.to_ascii_lowercase();
            let relative_path = format!("books/{}/{format_id}.{extension}", &format_id[..2]);
            self.storage
                .copy_from(&format.source_path, &relative_path)?;
            let size_bytes = std::fs::metadata(&format.source_path)
                .map(|metadata| metadata.len() as i64)
                .unwrap_or_else(|_| format.source.uncompressed_size.unwrap_or(0));

            copied.push(CopiedFormat {
                id: format_id,
                format: format.source.format.clone(),
                path: relative_path,
                size_bytes,
            });
        }
        Ok(copied)
    }

    fn copy_cover_if_present(
        &self,
        entry: &CalibreEntry,
        book_id: &str,
        reader: &CalibreReader,
    ) -> anyhow::Result<(bool, Option<String>)> {
        let Some(source_cover) = reader.cover_path(entry) else {
            return Ok((false, None));
        };
        if !source_cover.exists() {
            return Ok((false, None));
        }

        let relative_cover = format!("covers/{}/{book_id}.jpg", &book_id[..2]);
        let relative_thumb = format!("covers/{}/{}.thumb.jpg", &book_id[..2], book_id);
        let target_cover = self.storage.resolve(&relative_cover)?;
        let target_thumb = self.storage.resolve(&relative_thumb)?;
        covers::copy_cover(&source_cover, &target_cover, &target_thumb)?;

        Ok((true, Some(relative_cover)))
    }

    async fn get_or_create_series(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        series: Option<&CalibeSeries>,
        now: &str,
    ) -> anyhow::Result<Option<String>> {
        let Some(series) = series else {
            return Ok(None);
        };

        if let Some(row) = sqlx::query("SELECT id FROM series WHERE name = ? LIMIT 1")
            .bind(&series.name)
            .fetch_optional(tx.as_mut())
            .await
            .context("query existing series")?
        {
            let id: String = row.try_get("id").context("read series id")?;
            return Ok(Some(id));
        }

        let series_id = Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO series (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)")
            .bind(&series_id)
            .bind(&series.name)
            .bind(&series.sort)
            .bind(now)
            .execute(tx.as_mut())
            .await
            .context("insert series")?;
        Ok(Some(series_id))
    }

    async fn insert_authors(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        book_id: &str,
        authors: &[CalibreAuthor],
        now: &str,
    ) -> anyhow::Result<()> {
        for (display_order, author) in authors.iter().enumerate() {
            let author_id = if let Some(row) =
                sqlx::query("SELECT id FROM authors WHERE name = ? LIMIT 1")
                    .bind(&author.name)
                    .fetch_optional(tx.as_mut())
                    .await
                    .context("query existing author")?
            {
                row.try_get::<String, _>("id").context("read author id")?
            } else {
                let author_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO authors (id, name, sort_name, last_modified) VALUES (?, ?, ?, ?)",
                )
                .bind(&author_id)
                .bind(&author.name)
                .bind(&author.sort)
                .bind(now)
                .execute(tx.as_mut())
                .await
                .context("insert author")?;
                author_id
            };

            sqlx::query(
                "INSERT INTO book_authors (book_id, author_id, display_order) VALUES (?, ?, ?)",
            )
            .bind(book_id)
            .bind(author_id)
            .bind(display_order as i64)
            .execute(tx.as_mut())
            .await
            .context("insert book author relation")?;
        }
        Ok(())
    }

    async fn insert_tags(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        book_id: &str,
        tags: &[CalibreTag],
        now: &str,
    ) -> anyhow::Result<()> {
        for tag in tags {
            let tag_id = if let Some(row) =
                sqlx::query("SELECT id FROM tags WHERE name = ? LIMIT 1")
                    .bind(&tag.name)
                    .fetch_optional(tx.as_mut())
                    .await
                    .context("query existing tag")?
            {
                row.try_get::<String, _>("id").context("read tag id")?
            } else {
                let tag_id = Uuid::new_v4().to_string();
                sqlx::query(
                    "INSERT INTO tags (id, name, source, last_modified) VALUES (?, ?, 'calibre_import', ?)",
                )
                .bind(&tag_id)
                .bind(&tag.name)
                .bind(now)
                .execute(tx.as_mut())
                .await
                .context("insert tag")?;
                tag_id
            };

            sqlx::query("INSERT INTO book_tags (book_id, tag_id, confirmed) VALUES (?, ?, 1)")
                .bind(book_id)
                .bind(tag_id)
                .execute(tx.as_mut())
                .await
                .context("insert book tag relation")?;
        }
        Ok(())
    }

    async fn insert_formats(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        book_id: &str,
        formats: &[CopiedFormat],
        now: &str,
    ) -> anyhow::Result<()> {
        for format in formats {
            sqlx::query(
                r#"
                INSERT INTO formats (id, book_id, format, path, size_bytes, created_at, last_modified)
                VALUES (?, ?, ?, ?, ?, ?, ?)
                "#,
            )
            .bind(&format.id)
            .bind(book_id)
            .bind(&format.format)
            .bind(&format.path)
            .bind(format.size_bytes)
            .bind(now)
            .bind(now)
            .execute(tx.as_mut())
            .await
            .context("insert book format")?;
        }
        Ok(())
    }

    async fn insert_identifiers(
        &self,
        tx: &mut Transaction<'_, Sqlite>,
        book_id: &str,
        entry: &CalibreEntry,
        now: &str,
    ) -> anyhow::Result<()> {
        let mut seen_types = HashSet::new();
        for identifier in &entry.identifiers {
            if !seen_types.insert(identifier.id_type.clone()) {
                continue;
            }
            sqlx::query(
                "INSERT INTO identifiers (id, book_id, id_type, value, last_modified) VALUES (?, ?, ?, ?, ?)",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(book_id)
            .bind(&identifier.id_type)
            .bind(&identifier.value)
            .bind(now)
            .execute(tx.as_mut())
            .await
            .context("insert source identifier")?;
        }

        sqlx::query(
            "INSERT INTO identifiers (id, book_id, id_type, value, last_modified) VALUES (?, ?, 'calibre_id', ?, ?)",
        )
        .bind(Uuid::new_v4().to_string())
        .bind(book_id)
        .bind(entry.book.id.to_string())
        .bind(now)
        .execute(tx.as_mut())
        .await
        .context("insert calibre_id identifier")?;
        Ok(())
    }
}

fn collect_existing_formats(reader: &CalibreReader, entry: &CalibreEntry) -> Vec<AvailableFormat> {
    let mut available = Vec::new();
    for format in &entry.formats {
        let path = reader.file_path(entry, format);
        if path.exists() {
            available.push(AvailableFormat {
                source_path: path,
                source: format.clone(),
            });
        }
    }
    available
}

fn sanitize_relative_path(relative_path: &str) -> anyhow::Result<PathBuf> {
    let path = Path::new(relative_path);
    if path.is_absolute() {
        anyhow::bail!("absolute paths are not allowed");
    }

    let mut clean = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(part) => clean.push(part),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                anyhow::bail!("path traversal is not allowed");
            }
        }
    }

    if clean.as_os_str().is_empty() {
        anyhow::bail!("empty storage path");
    }

    Ok(clean)
}
