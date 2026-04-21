use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::Context;
use rusqlite::{Connection, OpenFlags};

use super::schema::{
    CalibeSeries, CalibreAuthor, CalibreBook, CalibreComment, CalibreEntry, CalibreFormat,
    CalibreIdentifier, CalibreTag,
};

struct EntryBuilder {
    book: CalibreBook,
    authors: BTreeMap<i64, CalibreAuthor>,
    series: Option<CalibeSeries>,
    tags: BTreeMap<i64, CalibreTag>,
    formats: BTreeMap<i64, CalibreFormat>,
    identifiers: BTreeMap<i64, CalibreIdentifier>,
    comment: Option<CalibreComment>,
}

impl EntryBuilder {
    fn into_entry(self) -> CalibreEntry {
        CalibreEntry {
            book: self.book,
            authors: self.authors.into_values().collect(),
            series: self.series,
            tags: self.tags.into_values().collect(),
            formats: self.formats.into_values().collect(),
            identifiers: self.identifiers.into_values().collect(),
            comment: self.comment,
        }
    }
}

pub struct CalibreReader {
    library_path: PathBuf,
    conn: Connection,
}

impl CalibreReader {
    pub fn open(library_path: &Path) -> anyhow::Result<Self> {
        let metadata_path = library_path.join("metadata.db");
        let conn = Connection::open_with_flags(
            &metadata_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .with_context(|| format!("failed to open {:?}", metadata_path))?;

        Ok(Self {
            library_path: library_path.to_path_buf(),
            conn,
        })
    }

    pub fn read_all_entries(&self) -> anyhow::Result<Vec<CalibreEntry>> {
        let query = r#"
            SELECT
                b.id,
                b.title,
                b.sort,
                b.author_sort,
                b.pubdate,
                b.series_index,
                r.rating,
                b.has_cover,
                b.last_modified,
                a.id,
                a.name,
                a.sort,
                s.id,
                s.name,
                s.sort,
                t.id,
                t.name,
                d.id,
                d.book,
                d.format,
                d.name,
                d.uncompressed_size,
                i.id,
                i.book,
                i.type,
                i.val,
                c.id,
                c.book,
                c.text
            FROM books b
            LEFT JOIN books_authors_link bal ON bal.book = b.id
            LEFT JOIN authors a ON a.id = bal.author
            LEFT JOIN books_series_link bsl ON bsl.book = b.id
            LEFT JOIN series s ON s.id = bsl.series
            LEFT JOIN books_tags_link btl ON btl.book = b.id
            LEFT JOIN tags t ON t.id = btl.tag
            LEFT JOIN books_ratings_link brl ON brl.book = b.id
            LEFT JOIN ratings r ON r.id = brl.rating
            LEFT JOIN data d ON d.book = b.id
            LEFT JOIN identifiers i ON i.book = b.id
            LEFT JOIN comments c ON c.book = b.id
            ORDER BY b.id ASC
        "#;

        let mut statement = self
            .conn
            .prepare(query)
            .context("failed to prepare calibre entry query")?;

        let mut rows = statement
            .query([])
            .context("failed to query calibre metadata")?;

        let mut entries: BTreeMap<i64, EntryBuilder> = BTreeMap::new();

        while let Some(row) = rows.next().context("failed to advance sqlite rows")? {
            let book_id: i64 = row.get(0).context("missing books.id")?;
            let title: String = row.get(1).context("missing books.title")?;
            let sort: String = row.get(2).context("missing books.sort")?;
            let author_sort: String = row.get(3).context("missing books.author_sort")?;
            let pubdate: Option<String> = row.get(4).context("invalid books.pubdate")?;
            let series_index: Option<f64> = row.get(5).context("invalid books.series_index")?;
            let rating: Option<i64> = row.get(6).context("invalid ratings.rating")?;
            let has_cover_raw: i64 = row.get(7).context("missing books.has_cover")?;
            let last_modified: String = row.get(8).context("missing books.last_modified")?;

            let builder = entries.entry(book_id).or_insert_with(|| EntryBuilder {
                book: CalibreBook {
                    id: book_id,
                    title: title.clone(),
                    sort: sort.clone(),
                    author_sort: author_sort.clone(),
                    pubdate: pubdate.clone(),
                    series_index,
                    rating,
                    has_cover: has_cover_raw != 0,
                    last_modified: last_modified.clone(),
                },
                authors: BTreeMap::new(),
                series: None,
                tags: BTreeMap::new(),
                formats: BTreeMap::new(),
                identifiers: BTreeMap::new(),
                comment: None,
            });

            if builder.book.rating.is_none() {
                builder.book.rating = rating;
            }
            if builder.book.pubdate.is_none() {
                builder.book.pubdate = pubdate;
            }

            let author_id: Option<i64> = row.get(9).context("invalid authors.id")?;
            if let Some(author_id) = author_id {
                let author_name: String = row.get(10).context("missing authors.name")?;
                let author_sort_name: String = row.get(11).context("missing authors.sort")?;
                builder.authors.entry(author_id).or_insert(CalibreAuthor {
                    id: author_id,
                    name: author_name,
                    sort: author_sort_name,
                });
            }

            let series_id: Option<i64> = row.get(12).context("invalid series.id")?;
            if builder.series.is_none() {
                if let Some(series_id) = series_id {
                    let series_name: String = row.get(13).context("missing series.name")?;
                    let series_sort: String = row.get(14).context("missing series.sort")?;
                    builder.series = Some(CalibeSeries {
                        id: series_id,
                        name: series_name,
                        sort: series_sort,
                    });
                }
            }

            let tag_id: Option<i64> = row.get(15).context("invalid tags.id")?;
            if let Some(tag_id) = tag_id {
                let tag_name: String = row.get(16).context("missing tags.name")?;
                builder.tags.entry(tag_id).or_insert(CalibreTag {
                    id: tag_id,
                    name: tag_name,
                });
            }

            let format_id: Option<i64> = row.get(17).context("invalid data.id")?;
            if let Some(format_id) = format_id {
                let format_book_id: i64 = row.get(18).context("missing data.book")?;
                let format_name: String = row.get(19).context("missing data.format")?;
                let file_base_name: String = row.get(20).context("missing data.name")?;
                let uncompressed_size: Option<i64> =
                    row.get(21).context("invalid data.uncompressed_size")?;
                builder.formats.entry(format_id).or_insert(CalibreFormat {
                    id: format_id,
                    book_id: format_book_id,
                    format: format_name,
                    name: file_base_name,
                    uncompressed_size,
                });
            }

            let identifier_id: Option<i64> = row.get(22).context("invalid identifiers.id")?;
            if let Some(identifier_id) = identifier_id {
                let identifier_book_id: i64 = row.get(23).context("missing identifiers.book")?;
                let id_type: String = row.get(24).context("missing identifiers.type")?;
                let id_value: String = row.get(25).context("missing identifiers.val")?;
                builder
                    .identifiers
                    .entry(identifier_id)
                    .or_insert(CalibreIdentifier {
                        id: identifier_id,
                        book_id: identifier_book_id,
                        id_type,
                        value: id_value,
                    });
            }

            let comment_id: Option<i64> = row.get(26).context("invalid comments.id")?;
            if builder.comment.is_none() {
                if let Some(comment_id) = comment_id {
                    let comment_book_id: i64 = row.get(27).context("missing comments.book")?;
                    let comment_text: String = row.get(28).context("missing comments.text")?;
                    builder.comment = Some(CalibreComment {
                        id: comment_id,
                        book_id: comment_book_id,
                        text: comment_text,
                    });
                }
            }
        }

        Ok(entries
            .into_values()
            .map(EntryBuilder::into_entry)
            .collect())
    }

    pub fn file_path(&self, entry: &CalibreEntry, format: &CalibreFormat) -> PathBuf {
        let author_dir = sanitize_component(&entry.book.author_sort);
        let title_dir = sanitize_component(&format!("{} ({})", entry.book.title, entry.book.id));
        let extension = format.format.to_ascii_lowercase();
        let base_name = sanitize_component(&format.name);
        let suffix = format!(".{extension}");
        let file_name = if base_name.to_ascii_lowercase().ends_with(&suffix) {
            base_name
        } else {
            format!("{base_name}{suffix}")
        };

        self.library_path
            .join(author_dir)
            .join(title_dir)
            .join(file_name)
    }

    pub fn cover_path(&self, entry: &CalibreEntry) -> Option<PathBuf> {
        if !entry.book.has_cover {
            return None;
        }
        let author_dir = sanitize_component(&entry.book.author_sort);
        let title_dir = sanitize_component(&format!("{} ({})", entry.book.title, entry.book.id));

        Some(
            self.library_path
                .join(author_dir)
                .join(title_dir)
                .join("cover.jpg"),
        )
    }
}

fn sanitize_component(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ if ch.is_control() => '_',
            _ => ch,
        })
        .collect();

    let trimmed = sanitized.trim();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed.to_string()
    }
}
