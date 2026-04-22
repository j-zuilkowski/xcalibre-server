use super::mobi_util;
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

#[derive(Clone, Debug, serde::Serialize)]
pub struct Chapter {
    pub index: u32,
    pub title: String,
    pub word_count: usize,
}

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
