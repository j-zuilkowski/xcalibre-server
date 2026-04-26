//! Sub-chapter chunking for RAG indexing.
//!
//! Splits extracted chapter text into overlapping chunks suitable for embedding and
//! retrieval.  The algorithm is **domain-aware**: it detects document structure and
//! applies different splitting strategies depending on the content type.
//!
//! # Chunk strategies
//!
//! ## Narrative domain
//! Pure sliding-window over paragraphs with an overlap tail (no structural detection).
//!
//! ## Technical / Electronics domains
//! 1. Markdown and numbered headings (`## 2.1 Title`, `1.2.3 Section`) are detected
//!    and emitted as single-line heading chunks, maintaining a heading stack for
//!    hierarchical `heading_path` (`Chapter > Section > Subsection`).
//! 2. **Procedure blocks**: consecutive lines matching `Step N.` or `N)` patterns
//!    (≥ 3 lines) are kept together as a single `ChunkType::Procedure` chunk and are
//!    **never split across chunk boundaries**.  This preserves the semantic coherence
//!    of numbered instruction sequences.
//! 3. Remaining paragraphs are collected and split with target size + overlap window.
//!
//! ## Culinary domain
//! Title-case recipe titles followed by `Ingredients` or `Serves N` trigger a recipe
//! block collector that emits the entire recipe as `ChunkType::Example`.
//!
//! # Overlap window
//! After emitting a chunk, the last `overlap` words of that chunk are prepended to the
//! next chunk as context continuity.  Overlap is clamped to `target_size - 1`.
//!
//! # Image-heavy pages
//! If `ChapterText.is_image_heavy_page = true` (PDF page with < 80 words), chunks
//! are tagged `ChunkType::Image` and `is_image_heavy = true`, signalling the ingest
//! pipeline to attempt a vision LLM pass.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{fmt, str::FromStr, sync::OnceLock};
use utoipa::ToSchema;

/// Pre-processed chapter text ready for chunking.
///
/// `is_image_heavy_page` is `true` when the source is a PDF page with fewer than
/// 80 words — a heuristic indicating the page is primarily a diagram or schematic.
#[derive(Clone, Debug)]
pub struct ChapterText {
    pub chapter_index: usize,
    pub title: String,
    pub text: String,
    pub is_image_heavy_page: bool,
}

/// Configuration for the chunking algorithm.
///
/// - `target_size`: desired chunk size in words (default 600).
/// - `overlap`: number of words from the previous chunk to prepend for context (default 100).
/// - `domain`: controls which structural detection rules are applied.
#[derive(Clone, Debug)]
pub struct ChunkConfig {
    pub target_size: usize,
    pub overlap: usize,
    pub domain: ChunkDomain,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            target_size: 600,
            overlap: 100,
            domain: ChunkDomain::Technical,
        }
    }
}

/// Content domain hint that selects the structural detection rules during chunking.
///
/// `Technical` is the default. `Electronics` adds the same procedure detection
/// as `Technical` but with electronics-specific vision prompting. `Narrative`
/// disables all structural detection and uses pure sliding-window.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChunkDomain {
    #[default]
    Technical,
    Electronics,
    Culinary,
    Legal,
    Academic,
    Narrative,
}

impl FromStr for ChunkDomain {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let normalized = input.trim().to_ascii_lowercase();
        let domain = match normalized.as_str() {
            "technical" | "tech" => Self::Technical,
            "electronics" | "electrical" => Self::Electronics,
            "culinary" | "recipe" | "recipes" => Self::Culinary,
            "legal" => Self::Legal,
            "academic" => Self::Academic,
            "narrative" | "prose" => Self::Narrative,
            "" => Self::Technical,
            other => anyhow::bail!("unsupported chunk domain: {other}"),
        };
        Ok(domain)
    }
}

impl fmt::Display for ChunkDomain {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = match self {
            Self::Technical => "technical",
            Self::Electronics => "electronics",
            Self::Culinary => "culinary",
            Self::Legal => "legal",
            Self::Academic => "academic",
            Self::Narrative => "narrative",
        };
        f.write_str(value)
    }
}

/// Semantic type of a chunk, used by the search layer for filtering and display.
///
/// `Procedure` chunks are never split across boundaries (see module doc).
/// `Image` chunks signal that a vision LLM pass should be attempted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    Text,
    Procedure,
    Reference,
    Concept,
    Example,
    Image,
}

impl ChunkType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Procedure => "procedure",
            Self::Reference => "reference",
            Self::Concept => "concept",
            Self::Example => "example",
            Self::Image => "image",
        }
    }
}

impl FromStr for ChunkType {
    type Err = anyhow::Error;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        match input.trim().to_ascii_lowercase().as_str() {
            "text" => Ok(Self::Text),
            "procedure" => Ok(Self::Procedure),
            "reference" => Ok(Self::Reference),
            "concept" => Ok(Self::Concept),
            "example" => Ok(Self::Example),
            "image" => Ok(Self::Image),
            other => anyhow::bail!("unsupported chunk type: {other}"),
        }
    }
}

/// A single text chunk ready for storage and embedding.
///
/// `heading_path` is a `>` -separated breadcrumb built from the heading stack
/// (e.g. `"Chapter 1 > Installation > Prerequisites"`).
#[derive(Clone, Debug)]
pub struct Chunk {
    pub chunk_index: usize,
    pub chapter_index: usize,
    pub heading_path: Option<String>,
    pub chunk_type: ChunkType,
    pub text: String,
    pub word_count: usize,
    pub is_image_heavy: bool,
}

/// Chunk all chapters according to the provided config.
///
/// Dispatches to [`chunk_narrative`] or [`chunk_structured`] based on `config.domain`.
/// Chunk indices are assigned globally across all chapters.
pub fn chunk_chapters(chapters: &[ChapterText], config: &ChunkConfig) -> Vec<Chunk> {
    let target_size = config.target_size.max(1);
    let overlap = config.overlap.min(target_size.saturating_sub(1));
    let mut chunks = Vec::new();

    for chapter in chapters {
        let chapter_chunks = if config.domain == ChunkDomain::Narrative {
            chunk_narrative(chapter, target_size, overlap)
        } else {
            chunk_structured(chapter, config.domain, target_size, overlap)
        };
        chunks.extend(chapter_chunks);
    }

    chunks
}

fn chunk_narrative(chapter: &ChapterText, target_size: usize, overlap: usize) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let texts = split_text_into_chunks(&chapter.text, target_size, overlap);
    for text in texts {
        push_chunk(
            &mut chunks,
            chapter.chapter_index,
            Some(chapter.title.as_str()),
            None,
            ChunkType::Text,
            text,
            chapter.is_image_heavy_page,
        );
    }
    chunks
}

fn chunk_structured(
    chapter: &ChapterText,
    domain: ChunkDomain,
    target_size: usize,
    overlap: usize,
) -> Vec<Chunk> {
    let lines = chapter
        .text
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<_>>();
    let mut chunks = Vec::new();
    let mut cursor = 0usize;
    let mut heading_stack: Vec<String> = Vec::new();
    let mut current_heading: Option<String> = if chapter.title.trim().is_empty() {
        None
    } else {
        Some(chapter.title.trim().to_string())
    };

    while cursor < lines.len() {
        if let Some((heading_text, heading_level)) = parse_heading_line(&lines[cursor]) {
            apply_heading(&mut heading_stack, heading_level, heading_text.clone());
            current_heading = heading_path_for(&heading_stack, chapter.title.as_str());
            push_chunk(
                &mut chunks,
                chapter.chapter_index,
                Some(chapter.title.as_str()),
                current_heading.clone(),
                ChunkType::Text,
                heading_text,
                chapter.is_image_heavy_page,
            );
            cursor += 1;
            continue;
        }

        if matches!(domain, ChunkDomain::Technical | ChunkDomain::Electronics) {
            if let Some((block, consumed)) = parse_procedure_block(&lines[cursor..]) {
                push_chunk(
                    &mut chunks,
                    chapter.chapter_index,
                    current_heading.as_deref(),
                    current_heading.clone(),
                    ChunkType::Procedure,
                    block,
                    chapter.is_image_heavy_page,
                );
                cursor += consumed;
                continue;
            }
        }

        if matches!(domain, ChunkDomain::Culinary)
            && is_recipe_title_line(&lines[cursor])
            && matches!(
                lines.get(cursor + 1).map(|line| line.trim()),
                Some(line) if is_recipe_intro_line(line)
            )
        {
            let mut recipe_lines = Vec::new();
            let mut consumed = 0usize;
            while let Some(line) = lines.get(cursor + consumed) {
                if parse_heading_line(line).is_some() && consumed > 0 {
                    break;
                }
                recipe_lines.push(line.trim_end().to_string());
                consumed += 1;
            }
            let text = join_clean_lines(&recipe_lines);
            push_chunk(
                &mut chunks,
                chapter.chapter_index,
                current_heading.as_deref(),
                current_heading.clone(),
                ChunkType::Example,
                text,
                chapter.is_image_heavy_page,
            );
            cursor += consumed.max(1);
            continue;
        }

        let mut paragraph_lines = Vec::new();
        while cursor < lines.len() {
            if lines[cursor].trim().is_empty() {
                if !paragraph_lines.is_empty() {
                    break;
                }
                cursor += 1;
                continue;
            }
            if parse_heading_line(&lines[cursor]).is_some() {
                break;
            }
            if matches!(domain, ChunkDomain::Technical | ChunkDomain::Electronics)
                && parse_procedure_line(&lines[cursor]).is_some()
                && paragraph_lines.is_empty()
            {
                break;
            }
            if matches!(domain, ChunkDomain::Culinary)
                && is_recipe_title_line(&lines[cursor])
                && matches!(
                    lines.get(cursor + 1).map(|line| line.trim()),
                    Some(line) if is_recipe_intro_line(line)
                )
                && paragraph_lines.is_empty()
            {
                break;
            }

            paragraph_lines.push(lines[cursor].trim_end().to_string());
            cursor += 1;
        }

        let paragraph_text = join_clean_lines(&paragraph_lines);
        if paragraph_text.is_empty() {
            cursor = cursor.saturating_add(1);
            continue;
        }

        for text in split_text_into_chunks(&paragraph_text, target_size, overlap) {
            let chunk_type = if chapter.is_image_heavy_page && word_count(&text) < 80 {
                ChunkType::Image
            } else {
                ChunkType::Text
            };
            let is_image_heavy = matches!(chunk_type, ChunkType::Image);
            push_chunk(
                &mut chunks,
                chapter.chapter_index,
                current_heading.as_deref(),
                current_heading.clone(),
                chunk_type,
                text,
                is_image_heavy || chapter.is_image_heavy_page,
            );
        }
    }

    chunks
}

fn push_chunk(
    chunks: &mut Vec<Chunk>,
    chapter_index: usize,
    chapter_title: Option<&str>,
    heading_path: Option<String>,
    chunk_type: ChunkType,
    text: String,
    force_image: bool,
) {
    let text = text.trim().to_string();
    if text.is_empty() {
        return;
    }

    let word_count = word_count(&text);
    let is_image_heavy = force_image || matches!(chunk_type, ChunkType::Image);
    let heading_path = heading_path.or_else(|| {
        chapter_title
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    });

    chunks.push(Chunk {
        chunk_index: chunks.len(),
        chapter_index,
        heading_path,
        chunk_type: if is_image_heavy {
            ChunkType::Image
        } else {
            chunk_type
        },
        text,
        word_count,
        is_image_heavy,
    });
}

fn split_text_into_chunks(text: &str, target_size: usize, overlap: usize) -> Vec<String> {
    let paragraphs = text
        .split("\n\n")
        .map(str::trim)
        .filter(|paragraph| !paragraph.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();

    if paragraphs.is_empty() {
        return Vec::new();
    }

    let mut result = Vec::new();
    let mut current = Vec::<String>::new();
    let mut current_words = 0usize;

    for paragraph in paragraphs {
        let paragraph_words = word_count(&paragraph);
        if paragraph_words > target_size {
            if !current.is_empty() {
                emit_current_chunk(&mut result, &mut current, &mut current_words);
            }
            split_long_paragraph(&paragraph, target_size, overlap, &mut result);
            continue;
        }

        if !current.is_empty() && current_words + paragraph_words > target_size {
            emit_current_chunk(&mut result, &mut current, &mut current_words);
            let overlap_text = tail_words(
                result.last().map(String::as_str).unwrap_or_default(),
                overlap,
            );
            if !overlap_text.is_empty() {
                current.push(overlap_text);
            }
        }

        current.push(paragraph);
        current_words = word_count(&current.join("\n\n"));
    }

    if !current.is_empty() {
        emit_current_chunk(&mut result, &mut current, &mut current_words);
    }

    result
}

fn emit_current_chunk(
    result: &mut Vec<String>,
    current: &mut Vec<String>,
    current_words: &mut usize,
) {
    let text = join_clean_paragraphs(current);
    if !text.is_empty() {
        result.push(text);
    }
    current.clear();
    *current_words = 0;
}

/// Split a single paragraph that exceeds `target_size` using a sliding word window.
///
/// `step = target_size - overlap` advances the window on each iteration so consecutive
/// chunks share `overlap` words of context at their boundaries.
fn split_long_paragraph(
    paragraph: &str,
    target_size: usize,
    overlap: usize,
    result: &mut Vec<String>,
) {
    let words = split_words(paragraph);
    if words.is_empty() {
        return;
    }

    let step = target_size.saturating_sub(overlap).max(1);
    let mut start = 0usize;

    while start < words.len() {
        let end = (start + target_size).min(words.len());
        let chunk = words[start..end].join(" ");
        if !chunk.trim().is_empty() {
            result.push(chunk);
        }

        if end == words.len() {
            break;
        }
        start = start.saturating_add(step);
    }
}

fn heading_path_for(stack: &[String], chapter_title: &str) -> Option<String> {
    if stack.is_empty() {
        let trimmed = chapter_title.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    } else {
        Some(stack.join(" > "))
    }
}

fn apply_heading(stack: &mut Vec<String>, level: usize, heading_text: String) {
    let level = level.max(1);
    let next_len = level.saturating_sub(1);
    if stack.len() > next_len {
        stack.truncate(next_len);
    }
    stack.push(heading_text.trim().to_string());
}

fn parse_heading_line(line: &str) -> Option<(String, usize)> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(captures) = markdown_heading_regex().captures(trimmed) {
        let hashes = captures.get(1)?.as_str();
        let text = captures.get(2)?.as_str().trim().to_string();
        return Some((text, hashes.len()));
    }

    let captures = numbered_heading_regex().captures(trimmed)?;
    let number = captures.get(1)?.as_str();
    let depth = number.split('.').count().max(1);
    Some((trimmed.to_string(), depth))
}

/// Collect a contiguous numbered-list block from the start of `lines`.
///
/// Requires at least 3 consecutive procedure lines to be recognized as a block.
/// Fewer than 3 lines are treated as regular paragraph text, avoiding false positives
/// on lone numbered items embedded in prose.
fn parse_procedure_block(lines: &[String]) -> Option<(String, usize)> {
    let mut collected = Vec::new();
    let mut consumed = 0usize;

    while let Some(line) = lines.get(consumed) {
        if parse_procedure_line(line).is_some() {
            collected.push(line.trim_end().to_string());
            consumed += 1;
        } else {
            break;
        }
    }

    if collected.len() >= 3 {
        Some((join_clean_lines(&collected), consumed))
    } else {
        None
    }
}

fn parse_procedure_line(line: &str) -> Option<&str> {
    procedure_line_regex()
        .captures(line)
        .and_then(|captures| captures.get(0).map(|m| m.as_str()))
}

fn is_recipe_title_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.split_whitespace().count() >= 8 {
        return false;
    }
    if trimmed.chars().any(|ch| ch.is_ascii_punctuation()) {
        return false;
    }
    trimmed.split_whitespace().all(is_title_case_word)
}

fn is_title_case_word(word: &str) -> bool {
    let mut chars = word.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_uppercase() && chars.all(|ch| ch.is_lowercase() || ch.is_ascii_digit())
}

fn is_recipe_intro_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.eq_ignore_ascii_case("ingredients") || recipe_serves_regex().is_match(trimmed)
}

fn join_clean_lines(lines: &[String]) -> String {
    lines
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn join_clean_paragraphs(paragraphs: &[String]) -> String {
    paragraphs
        .iter()
        .map(|paragraph| paragraph.trim())
        .filter(|paragraph| !paragraph.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn word_count(text: &str) -> usize {
    text.split_whitespace().count()
}

fn split_words(text: &str) -> Vec<String> {
    text.split_whitespace().map(ToOwned::to_owned).collect()
}

fn tail_words(text: &str, count: usize) -> String {
    if count == 0 {
        return String::new();
    }

    let words = split_words(text);
    if words.is_empty() {
        return String::new();
    }

    let start = words.len().saturating_sub(count);
    words[start..].join(" ")
}

fn markdown_heading_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"^(#{1,4})\s+(.+)$").expect("valid markdown heading regex"))
}

fn numbered_heading_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^(\d+(?:\.\d+)*)\s+[A-Z]").expect("valid numbered heading regex")
    })
}

fn procedure_line_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| {
        Regex::new(r"^\s*(?:Step\s+)?\d+[\.\)]\s+\S").expect("valid procedure line regex")
    })
}

fn recipe_serves_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"(?i)^serves\s+\d+\b").expect("valid serves regex"))
}
