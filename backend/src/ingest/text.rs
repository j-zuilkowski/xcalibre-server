//! Plain-text extraction from EPUB, PDF, MOBI/AZW3, and TXT formats.
//!
//! This module is the RAG content API foundation — it has **no LLM dependency**.
//! Extracted text is passed to the chunker ([`chunker`]) and optionally enriched
//! by the vision LLM pass (gated on `has_image + llm.enabled`).
//!
//! # Supported formats
//! | Format     | Strategy                                                              |
//! |------------|-----------------------------------------------------------------------|
//! | EPUB       | Unzip → parse `META-INF/container.xml` → find OPF → read spine items → strip HTML |
//! | PDF        | Regex-scan for `BT...ET` text blocks; page boundaries via `/Type /Page` markers |
//! | MOBI/AZW3  | [`mobi`] crate decode → split on `<mbp:pagebreak>` or heading tags    |
//! | TXT        | Raw `fs::read_to_string`                                              |
//!
//! # Chapter model
//! For PDFs, chapters are synthetic 5-page windows (PDF has no real chapter structure).
//! For EPUB/MOBI, each spine item or pagebreak segment becomes one chapter.
//!
//! # Vision pass
//! After chunking, any chunk with `is_image_heavy = true` (PDF page with < 80 words)
//! triggers a vision LLM call via [`vision_llm::describe_image_page`]. The description
//! is appended to the chunk text before storage. Vision calls are best-effort; failures
//! log a warning and fall through without the description.
//!
//! # External systems
//! - Reads from any [`StorageBackend`] (local FS or S3).
//! - Writes chunks to `book_chunks` table via `chunk_queries`.
//! - Optionally embeds chunks into `book_embeddings` via `SemanticSearch`.

use super::mobi_util;
use crate::{
    db::queries::{book_chunks as chunk_queries, books as book_queries},
    ingest::{
        chunker::{self, ChapterText, ChunkConfig, ChunkDomain},
        vision as vision_llm,
    },
    storage::StorageBackend,
    AppState,
};
use anyhow::Context;
use mobi::Mobi;
use regex::Regex;
use roxmltree::Document;
use std::{
    collections::HashMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::OnceLock,
};
use zip::ZipArchive;

/// Metadata for a single chapter/spine-item/page-window within a book file.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Chapter {
    pub index: u32,
    pub title: String,
    pub word_count: usize,
}

/// A local filesystem path that may be backed by a temporary file for remote backends.
///
/// The temporary file (if any) is deleted when this struct is dropped via
/// [`tempfile::TempPath`]'s `Drop` impl.
pub struct ExtractablePath {
    path: PathBuf,
    _temp_path: Option<tempfile::TempPath>,
}

impl ExtractablePath {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Resolve a storage-relative path to a local filesystem path.
///
/// For local-FS backends, calls [`StorageBackend::resolve`] which returns a
/// `PathBuf` without any I/O. For S3 backends (which don't support `resolve`),
/// downloads the object to a [`tempfile::NamedTempFile`] and returns its path.
/// The temp file is cleaned up when the returned [`ExtractablePath`] is dropped.
pub async fn resolve_or_download_path(
    storage: &dyn StorageBackend,
    relative_path: &str,
) -> anyhow::Result<ExtractablePath> {
    match storage.resolve(relative_path) {
        Ok(path) => Ok(ExtractablePath {
            path,
            _temp_path: None,
        }),
        Err(_) => {
            let bytes = storage
                .get_bytes(relative_path)
                .await
                .with_context(|| format!("download object for extraction: {relative_path}"))?;
            let temp_file =
                tempfile::NamedTempFile::new().context("create temp extraction file")?;
            tokio::fs::write(temp_file.path(), &bytes)
                .await
                .context("write temp extraction file")?;
            let temp_path = temp_file.into_temp_path();
            let path = temp_path.to_path_buf();
            Ok(ExtractablePath {
                path,
                _temp_path: Some(temp_path),
            })
        }
    }
}

/// List the chapters (or synthetic page-windows for PDFs) in a book file.
///
/// `format` is case-insensitive. Returns an empty vec for unsupported formats rather
/// than an error, so callers can skip extraction gracefully.
pub fn list_chapters(path: &Path, format: &str) -> anyhow::Result<Vec<Chapter>> {
    let output = match normalize_format(format).as_str() {
        "EPUB" => list_epub_chapters(path).unwrap_or_default(),
        "PDF" => list_pdf_chapters(path).unwrap_or_default(),
        "TXT" => {
            let content = fs::read_to_string(path).unwrap_or_default();
            let word_count = content.split_whitespace().count();
            vec![Chapter {
                index: 0,
                title: "Full Text".to_string(),
                word_count,
            }]
        }
        "MOBI" | "AZW3" => list_mobi_chapters(path).unwrap_or_default(),
        _ => Vec::new(),
    };
    Ok(output)
}

/// Extract plain text from a book file.
///
/// `chapter` selects a specific chapter index. Pass `None` to extract all chapters
/// concatenated with `\n\n---\n\n` separators. Returns an empty string for
/// unsupported formats rather than an error.
pub fn extract_text(path: &Path, format: &str, chapter: Option<u32>) -> anyhow::Result<String> {
    let output = match normalize_format(format).as_str() {
        "EPUB" => extract_epub_text(path, chapter).unwrap_or_default(),
        "PDF" => extract_pdf_text(path, chapter).unwrap_or_default(),
        "TXT" => fs::read_to_string(path).unwrap_or_default(),
        "MOBI" | "AZW3" => extract_mobi_text(path, chapter).unwrap_or_default(),
        _ => String::new(),
    };
    Ok(output)
}

/// Extract, chunk, optionally vision-enrich, optionally embed, and store all chunks
/// for a single book.
///
/// Pipeline:
/// 1. Pick the best extractable format (EPUB > PDF > MOBI > AZW3 > TXT).
/// 2. Resolve or download the file via the storage backend.
/// 3. Extract per-chapter text and collect [`ChapterText`] structs.
/// 4. Run the domain-aware chunker to produce [`Chunk`]s.
/// 5. For image-heavy chunks (PDF pages with < 80 words), optionally run the vision
///    LLM pass to append a textual description — gated on `supports_vision()`.
/// 6. If semantic search is configured, embed each chunk's text via the embedding
///    endpoint and store as a little-endian `f32` blob.
/// 7. Atomically replace all existing chunks for the book in the DB.
///
/// Returns the number of chunks stored. Returns 0 (not an error) when the book has
/// no extractable content.
pub async fn generate_and_store_book_chunks(
    state: &AppState,
    book: &crate::db::models::Book,
    config: &ChunkConfig,
) -> anyhow::Result<usize> {
    let Some(format) = preferred_extractable_format(book) else {
        anyhow::bail!("book has no extractable format");
    };

    let format_file = book_queries::find_format_file(&state.db, &book.id, format)
        .await?
        .ok_or_else(|| anyhow::anyhow!("book has no extractable format"))?;
    let extractable_path = resolve_or_download_path(&*state.storage, &format_file.path).await?;
    let chapter_texts = collect_chapter_texts(extractable_path.path(), format).await?;
    let chunks = chunker::chunk_chapters(&chapter_texts, config);
    if chunks.is_empty() {
        chunk_queries::replace_book_chunks(&state.db, &book.id, &[]).await?;
        return Ok(0);
    }

    let vision_supported = match state.chat_client.as_ref() {
        Some(chat_client) => match chat_client.supports_vision().await {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(book_id = %book.id, error = %err, "vision capability check failed");
                false
            }
        },
        None => false,
    };

    let mut inserts = Vec::with_capacity(chunks.len());
    for chunk in chunks {
        let mut text = chunk.text.clone();
        if vision_supported && chunk.is_image_heavy {
            match extract_page_image_bytes(extractable_path.path(), format, chunk.chapter_index)
                .await
            {
                Ok(Some(image_bytes)) => {
                    if let Some(chat_client) = state.chat_client.as_ref() {
                        match vision_llm::describe_image_page(
                            chat_client,
                            &image_bytes,
                            &config.domain,
                        )
                        .await
                        {
                            Ok(description) => {
                                text = format!(
                                    "{text}\n\n[Visual content description:]\n{description}"
                                );
                            }
                            Err(err) => {
                                tracing::warn!(
                                    book_id = %book.id,
                                    chapter_index = chunk.chapter_index,
                                    error = %err,
                                    "vision pass failed"
                                );
                            }
                        }
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    tracing::warn!(
                        book_id = %book.id,
                        chapter_index = chunk.chapter_index,
                        error = %err,
                        "failed to extract image bytes for vision pass"
                    );
                }
            }
        }

        let embedding = if let Some(semantic) = state.semantic_search.as_ref() {
            match semantic.embed_text(&text).await {
                Ok(vector) => Some(embedding_to_blob(&vector)),
                Err(err) => {
                    tracing::warn!(
                        book_id = %book.id,
                        chunk_index = chunk.chunk_index,
                        error = %err,
                        "failed to embed chunk"
                    );
                    None
                }
            }
        } else {
            None
        };

        let final_word_count = word_count(&text);
        inserts.push(chunk_queries::BookChunkInsert {
            chunk_index: chunk.chunk_index,
            chapter_index: chunk.chapter_index,
            heading_path: chunk.heading_path.clone(),
            chunk_type: chunk.chunk_type,
            text,
            word_count: final_word_count,
            has_image: chunk.is_image_heavy,
            embedding,
        });
    }

    chunk_queries::replace_book_chunks(&state.db, &book.id, &inserts).await?;
    Ok(inserts.len())
}

/// Re-chunk all books in the library that have no existing chunks.
///
/// Queries for books with no entries in `book_chunks`, then processes them
/// sequentially with a 5-second sleep between books to avoid overwhelming the
/// embedding endpoint. Returns the count of successfully processed books.
pub async fn rechunk_library(state: &AppState) -> anyhow::Result<usize> {
    let book_ids = sqlx::query_scalar::<_, String>(
        r#"
        SELECT b.id
        FROM books b
        LEFT JOIN book_chunks bc ON bc.book_id = b.id
        WHERE bc.book_id IS NULL
        GROUP BY b.id
        ORDER BY b.created_at ASC
        "#,
    )
    .fetch_all(&state.db)
    .await?;

    let mut processed = 0usize;
    for book_id in book_ids {
        let Some(book) = book_queries::get_book_by_id(&state.db, &book_id, None, None).await?
        else {
            continue;
        };

        if let Err(err) = generate_and_store_book_chunks(
            state,
            &book,
            &ChunkConfig {
                target_size: 600,
                overlap: 100,
                domain: ChunkDomain::Technical,
            },
        )
        .await
        {
            tracing::warn!(book_id = %book_id, error = %err, "failed to rechunk book");
        } else {
            processed += 1;
        }

        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
    }

    Ok(processed)
}

fn list_mobi_chapters(path: &Path) -> anyhow::Result<Vec<Chapter>> {
    let bytes = fs::read(path)?;
    let book = Mobi::new(&bytes)?;
    let chapters = mobi_chapter_fragments(&mobi_util::safe_mobi_content(&book));

    Ok(chapters
        .into_iter()
        .enumerate()
        .map(|(index, fragment)| {
            let text = mobi_util::strip_html_to_text(&fragment);
            let word_count = text.split_whitespace().count();
            let title = mobi_util::extract_heading_title(&fragment)
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| format!("Chapter {}", index + 1));
            Chapter {
                index: index as u32,
                title,
                word_count,
            }
        })
        .collect())
}

fn extract_mobi_text(path: &Path, chapter: Option<u32>) -> anyhow::Result<String> {
    let bytes = fs::read(path)?;
    let book = Mobi::new(&bytes)?;
    let chapter_text = mobi_chapter_fragments(&mobi_util::safe_mobi_content(&book))
        .into_iter()
        .map(|fragment| mobi_util::strip_html_to_text(&fragment))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();

    if let Some(chapter_index) = chapter {
        return Ok(chapter_text
            .get(chapter_index as usize)
            .cloned()
            .unwrap_or_default());
    }

    Ok(chapter_text.join("\n\n---\n\n"))
}

fn mobi_chapter_fragments(raw_html: &str) -> Vec<String> {
    let mut fragments = mobi_util::split_on_mobi_pagebreak(raw_html);
    if fragments.len() <= 1 {
        fragments = mobi_util::split_on_heading_tags(raw_html);
    }
    if fragments.is_empty() && !raw_html.trim().is_empty() {
        fragments.push(raw_html.to_string());
    }
    fragments
}

fn list_epub_chapters(path: &Path) -> anyhow::Result<Vec<Chapter>> {
    let (mut archive, opf_path) = open_epub_archive(path)?;
    let spine_items = read_epub_spine_paths(&mut archive, &opf_path)?;

    let mut chapters = Vec::new();
    for (index, spine_path) in spine_items.into_iter().enumerate() {
        let html = read_zip_text(&mut archive, &spine_path)?;
        let text = strip_epub_html_to_text(&html);
        let word_count = text.split_whitespace().count();
        let title =
            extract_epub_chapter_title(&html).unwrap_or_else(|| format!("Chapter {}", index + 1));
        chapters.push(Chapter {
            index: index as u32,
            title,
            word_count,
        });
    }

    Ok(chapters)
}

fn extract_epub_text(path: &Path, chapter: Option<u32>) -> anyhow::Result<String> {
    let (mut archive, opf_path) = open_epub_archive(path)?;
    let spine_items = read_epub_spine_paths(&mut archive, &opf_path)?;

    if let Some(chapter_index) = chapter {
        let Some(spine_path) = spine_items.get(chapter_index as usize) else {
            return Ok(String::new());
        };
        let html = read_zip_text(&mut archive, spine_path)?;
        return Ok(strip_epub_html_to_text(&html));
    }

    let mut parts = Vec::new();
    for spine_path in spine_items {
        if let Ok(html) = read_zip_text(&mut archive, &spine_path) {
            let text = strip_epub_html_to_text(&html);
            if !text.is_empty() {
                parts.push(text);
            }
        }
    }

    Ok(parts.join("\n\n---\n\n"))
}

fn list_pdf_chapters(path: &Path) -> anyhow::Result<Vec<Chapter>> {
    let pages = read_pdf_pages_text(path)?;
    if pages.is_empty() {
        return Ok(Vec::new());
    }

    // PDFs have no built-in chapter structure, so we create synthetic 5-page windows.
    // Each window becomes one "chapter" for the purposes of the reader and chunker.
    let mut chapters = Vec::new();
    let total_pages = pages.len();
    for (chunk_index, chunk) in pages.chunks(5).enumerate() {
        let start = chunk_index * 5 + 1;
        let end = (start + chunk.len()).saturating_sub(1).min(total_pages);
        let word_count = chunk
            .iter()
            .flat_map(|page| page.split_whitespace())
            .count();
        chapters.push(Chapter {
            index: chunk_index as u32,
            title: format!("Pages {start}-{end}"),
            word_count,
        });
    }

    Ok(chapters)
}

fn extract_pdf_text(path: &Path, chapter: Option<u32>) -> anyhow::Result<String> {
    let pages = read_pdf_pages_text(path)?;
    if pages.is_empty() {
        return Ok(String::new());
    }

    if let Some(chapter_index) = chapter {
        let start = chapter_index as usize * 5;
        if start >= pages.len() {
            return Ok(String::new());
        }
        let end = (start + 5).min(pages.len());
        let chunk = pages[start..end].join("\n\n");
        return Ok(normalize_whitespace(&chunk));
    }

    Ok(normalize_whitespace(&pages.join("\n\n")))
}

fn open_epub_archive(
    path: &Path,
) -> anyhow::Result<(ZipArchive<std::io::Cursor<Vec<u8>>>, String)> {
    let bytes = fs::read(path).with_context(|| format!("read EPUB {}", path.display()))?;
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).context("open EPUB zip archive")?;
    let opf_path = read_epub_opf_path(&mut archive)?;
    Ok((archive, opf_path))
}

fn read_epub_opf_path(
    archive: &mut ZipArchive<std::io::Cursor<Vec<u8>>>,
) -> anyhow::Result<String> {
    let container = read_zip_text(archive, "META-INF/container.xml").context("read container")?;
    let doc = Document::parse(&container).context("parse container.xml")?;
    let rootfile = doc
        .descendants()
        .find(|node| node.is_element() && node.tag_name().name() == "rootfile")
        .and_then(|node| node.attribute("full-path"))
        .context("container.xml missing rootfile full-path")?;
    Ok(rootfile.to_string())
}

fn read_epub_spine_paths(
    archive: &mut ZipArchive<std::io::Cursor<Vec<u8>>>,
    opf_path: &str,
) -> anyhow::Result<Vec<String>> {
    let opf_xml =
        read_zip_text(archive, opf_path).with_context(|| format!("read OPF {opf_path}"))?;
    let doc = Document::parse(&opf_xml).context("parse OPF")?;
    let opf_dir = Path::new(opf_path)
        .parent()
        .map(path_to_zip_string)
        .unwrap_or_default();

    let manifest = doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "item")
        .filter_map(|node| {
            let id = node.attribute("id")?;
            let href = node.attribute("href")?;
            Some((id.to_string(), href.to_string()))
        })
        .collect::<HashMap<_, _>>();

    let mut spine = Vec::new();
    for node in doc
        .descendants()
        .filter(|node| node.is_element() && node.tag_name().name() == "itemref")
    {
        let Some(idref) = node.attribute("idref") else {
            continue;
        };
        let Some(href) = manifest.get(idref) else {
            continue;
        };
        spine.push(resolve_zip_relative_path(&opf_dir, href));
    }

    Ok(spine)
}

fn read_zip_text(
    archive: &mut ZipArchive<std::io::Cursor<Vec<u8>>>,
    path: &str,
) -> anyhow::Result<String> {
    let mut file = archive
        .by_name(path)
        .with_context(|| format!("open zip entry {path}"))?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
        .with_context(|| format!("read zip entry {path}"))?;
    Ok(String::from_utf8_lossy(&bytes).to_string())
}

fn resolve_zip_relative_path(base_dir: &str, relative: &str) -> String {
    let mut joined = if base_dir.is_empty() {
        PathBuf::from(relative)
    } else {
        Path::new(base_dir).join(relative)
    };

    let mut clean = PathBuf::new();
    for component in joined.components() {
        match component {
            std::path::Component::Normal(part) => clean.push(part),
            std::path::Component::ParentDir => {
                clean.pop();
            }
            std::path::Component::CurDir => {}
            std::path::Component::RootDir | std::path::Component::Prefix(_) => {}
        }
    }
    joined = clean;
    path_to_zip_string(joined.as_path())
}

fn path_to_zip_string(path: &Path) -> String {
    let parts = path
        .components()
        .filter_map(|component| match component {
            std::path::Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>();
    parts.join("/")
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn strip_epub_html_to_text(html: &str) -> String {
    let cleaned = script_style_regex().replace_all(html, " ");
    let without_tags = html_tag_regex().replace_all(&cleaned, " ");
    let decoded = decode_epub_html_entities(&without_tags);
    normalize_whitespace(&decoded)
}

fn extract_epub_chapter_title(html: &str) -> Option<String> {
    if let Ok(doc) = Document::parse(html) {
        for tag in ["title", "h1", "h2"] {
            if let Some(text) = doc
                .descendants()
                .find(|node| node.is_element() && node.tag_name().name().eq_ignore_ascii_case(tag))
                .and_then(epub_node_text)
                .filter(|text| !text.is_empty())
            {
                return Some(text);
            }
        }
    }

    for regex in epub_title_regexes() {
        if let Some(capture) = regex.captures(html) {
            let raw = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
            let title = strip_epub_html_to_text(raw);
            if !title.is_empty() {
                return Some(title);
            }
        }
    }

    None
}

fn epub_node_text(node: roxmltree::Node<'_, '_>) -> Option<String> {
    let text = node
        .descendants()
        .filter_map(|child| child.text())
        .collect::<Vec<_>>()
        .join(" ");
    let text = normalize_whitespace(&text);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

fn decode_epub_html_entities(input: &str) -> String {
    let mut output = input
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'");

    output = numeric_entity_regex()
        .replace_all(&output, |caps: &regex::Captures<'_>| {
            let whole = caps.get(0).map(|m| m.as_str()).unwrap_or_default();
            let Some(value) = caps.get(1).map(|m| m.as_str()) else {
                return whole.to_string();
            };

            let parsed =
                if let Some(hex) = value.strip_prefix('x').or_else(|| value.strip_prefix('X')) {
                    u32::from_str_radix(hex, 16).ok()
                } else {
                    value.parse::<u32>().ok()
                };

            parsed
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| whole.to_string())
        })
        .to_string();

    output
}

fn read_pdf_pages_text(path: &Path) -> anyhow::Result<Vec<String>> {
    let bytes = fs::read(path).with_context(|| format!("read PDF {}", path.display()))?;
    let raw = String::from_utf8_lossy(&bytes);
    let page_markers = pdf_page_regex()
        .find_iter(&raw)
        .map(|m| m.start())
        .collect::<Vec<_>>();

    if page_markers.is_empty() {
        let text = extract_pdf_segment_text(&raw);
        return if text.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![text])
        };
    }

    let mut markers = page_markers;
    markers.push(raw.len());

    let mut pages = Vec::new();
    for window in markers.windows(2) {
        let start = window[0];
        let end = window[1];
        let segment = &raw[start..end];
        let text = extract_pdf_segment_text(segment);
        pages.push(text);
    }

    Ok(pages)
}

fn extract_pdf_segment_text(segment: &str) -> String {
    let mut parts = Vec::new();

    for block in pdf_bt_et_regex()
        .captures_iter(segment)
        .filter_map(|caps| caps.get(1).map(|m| m.as_str()))
    {
        for capture in pdf_paren_text_regex().captures_iter(block) {
            let text = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
            let text = unescape_pdf_text(text);
            if !text.trim().is_empty() {
                parts.push(text);
            }
        }
    }

    if parts.is_empty() {
        for capture in pdf_paren_text_regex().captures_iter(segment) {
            let text = capture.get(1).map(|m| m.as_str()).unwrap_or_default();
            let text = unescape_pdf_text(text);
            if !text.trim().is_empty() {
                parts.push(text);
            }
        }
    }

    normalize_whitespace(&parts.join(" "))
}

fn unescape_pdf_text(input: &str) -> String {
    input
        .replace(r"\(", "(")
        .replace(r"\)", ")")
        .replace(r"\n", " ")
        .replace(r"\r", " ")
        .replace(r"\t", " ")
        .replace(r"\\", r"\")
}

fn preferred_extractable_format(book: &crate::db::models::Book) -> Option<&str> {
    ["EPUB", "PDF", "MOBI", "AZW3", "TXT"]
        .into_iter()
        .find(|candidate| {
            book.formats
                .iter()
                .any(|format| format.format.eq_ignore_ascii_case(candidate))
        })
}

async fn collect_chapter_texts(path: &Path, format: &str) -> anyhow::Result<Vec<ChapterText>> {
    let chapters = list_chapters(path, format)?;
    let mut outputs = Vec::with_capacity(chapters.len());
    for chapter in chapters {
        let text = extract_text(path, format, Some(chapter.index)).unwrap_or_default();
        let is_image_heavy_page = normalize_format(format) == "PDF" && chapter.word_count < 80;
        outputs.push(ChapterText {
            chapter_index: chapter.index as usize,
            title: chapter.title,
            text,
            is_image_heavy_page,
        });
    }
    Ok(outputs)
}

async fn extract_page_image_bytes(
    path: &Path,
    format: &str,
    chapter_index: usize,
) -> anyhow::Result<Option<Vec<u8>>> {
    match normalize_format(format).as_str() {
        "EPUB" => extract_epub_image_bytes(path, chapter_index).await,
        "PDF" => Ok(Some(fs::read(path)?)),
        _ => Ok(None),
    }
}

async fn extract_epub_image_bytes(
    path: &Path,
    chapter_index: usize,
) -> anyhow::Result<Option<Vec<u8>>> {
    let (mut archive, opf_path) = open_epub_archive(path)?;
    let spine_items = read_epub_spine_paths(&mut archive, &opf_path)?;
    let Some(spine_path) = spine_items.get(chapter_index) else {
        return Ok(None);
    };

    let html = read_zip_text(&mut archive, spine_path)?;
    let Some(image_href) = first_img_src(&html) else {
        return Ok(None);
    };

    let base_dir = Path::new(spine_path)
        .parent()
        .map(path_to_zip_string)
        .unwrap_or_default();
    let image_path = resolve_zip_relative_path(&base_dir, &image_href);

    let Ok(mut file) = archive.by_name(&image_path) else {
        return Ok(None);
    };
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    if bytes.is_empty() {
        Ok(None)
    } else {
        Ok(Some(bytes))
    }
}

fn first_img_src(html: &str) -> Option<String> {
    img_src_regex()
        .captures(html)
        .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
}

/// Serialize an embedding vector to a little-endian `f32` byte blob for sqlite-vec storage.
fn embedding_to_blob(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(std::mem::size_of_val(vector));
    for value in vector {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes
}

fn img_src_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r#"(?is)<img[^>]*src=["']([^"']+)["']"#).expect("valid img src regex")
    })
}

fn epub_title_regexes() -> &'static [Regex; 3] {
    static REGEXES: OnceLock<[Regex; 3]> = OnceLock::new();
    REGEXES.get_or_init(|| {
        [
            Regex::new(r"(?is)<title[^>]*>(.*?)</title>").expect("valid title regex"),
            Regex::new(r"(?is)<h1[^>]*>(.*?)</h1>").expect("valid h1 regex"),
            Regex::new(r"(?is)<h2[^>]*>(.*?)</h2>").expect("valid h2 regex"),
        ]
    })
}

fn script_style_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"(?is)<(script|style)[^>]*>.*?</(script|style)>")
            .expect("valid script/style regex")
    })
}

fn html_tag_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?is)<[^>]+>").expect("valid html tag regex"))
}

fn numeric_entity_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"&#([xX]?[0-9A-Fa-f]+);").expect("valid entity regex"))
}

fn pdf_page_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"/Type\s*/Page\b").expect("valid pdf page regex"))
}

fn pdf_bt_et_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?s)BT(.*?)ET").expect("valid BT/ET regex"))
}

fn pdf_paren_text_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\(([^()]*)\)").expect("valid paren text regex"))
}

fn normalize_format(format: &str) -> String {
    format.trim().to_uppercase()
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}
