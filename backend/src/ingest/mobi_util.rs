pub fn split_on_mobi_pagebreak(raw_html: &str) -> Vec<String> {
    let lower = raw_html.to_ascii_lowercase();
    let marker = "<mbp:pagebreak";
    let mut cursor = 0usize;
    let mut chunks = Vec::new();

    while cursor < raw_html.len() {
        let Some(relative_index) = lower[cursor..].find(marker) else {
            break;
        };
        let marker_start = cursor + relative_index;
        let candidate = raw_html[cursor..marker_start].trim();
        if !candidate.is_empty() {
            chunks.push(candidate.to_string());
        }

        let marker_rest = &lower[marker_start..];
        if let Some(relative_end) = marker_rest.find('>') {
            cursor = marker_start + relative_end + 1;
        } else {
            cursor = raw_html.len();
            break;
        }
    }

    let tail = raw_html[cursor..].trim();
    if !tail.is_empty() {
        chunks.push(tail.to_string());
    }

    chunks
}

pub fn split_on_heading_tags(raw_html: &str) -> Vec<String> {
    let lower = raw_html.to_ascii_lowercase();
    let mut indices = Vec::new();
    let mut cursor = 0usize;

    while let Some(index) = find_next_heading_index(&lower, cursor) {
        indices.push(index);
        cursor = index.saturating_add(1);
    }

    if indices.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    if indices[0] > 0 {
        let prefix = raw_html[..indices[0]].trim();
        if !prefix.is_empty() {
            chunks.push(prefix.to_string());
        }
    }

    for (pos, start) in indices.iter().enumerate() {
        let end = indices.get(pos + 1).copied().unwrap_or(raw_html.len());
        let segment = raw_html[*start..end].trim();
        if !segment.is_empty() {
            chunks.push(segment.to_string());
        }
    }

    chunks
}

pub fn find_next_heading_index(lower: &str, cursor: usize) -> Option<usize> {
    ["<h1", "<h2", "<h3", "<h4", "<h5", "<h6"]
        .iter()
        .filter_map(|marker| lower[cursor..].find(marker).map(|index| cursor + index))
        .min()
}

pub fn safe_mobi_content(book: &mobi::Mobi) -> String {
    let content = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        book.content_as_string_lossy()
    }))
    .unwrap_or_default();
    if !content.trim().is_empty() {
        return content;
    }
    book.description().unwrap_or_default()
}

pub fn extract_heading_title(segment: &str) -> Option<String> {
    let lower = segment.to_ascii_lowercase();
    let heading_index = find_next_heading_index(&lower, 0)?;
    let rest = &segment[heading_index..];
    let open_end = rest.find('>')?;
    let inner = &rest[open_end + 1..];
    let close_start = inner.to_ascii_lowercase().find("</h")?;
    let title = strip_html_to_text(&inner[..close_start]);
    if title.is_empty() {
        None
    } else {
        Some(title)
    }
}

pub fn strip_html_to_text(fragment: &str) -> String {
    let mut in_tag = false;
    let mut output = String::with_capacity(fragment.len());
    let mut tag = String::new();

    for ch in fragment.chars() {
        if ch == '<' {
            in_tag = true;
            tag.clear();
            continue;
        }
        if ch == '>' {
            if in_tag {
                let lower_tag = tag.trim().to_ascii_lowercase();
                if is_block_break_tag(&lower_tag) {
                    output.push('\n');
                    output.push('\n');
                } else {
                    output.push(' ');
                }
                in_tag = false;
            }
            continue;
        }
        if in_tag {
            tag.push(ch);
        } else {
            output.push(ch);
        }
    }

    let decoded = decode_basic_html_entities(&output);
    decoded
        .split("\n\n")
        .map(|part| part.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n\n")
}

pub fn decode_basic_html_entities(value: &str) -> String {
    value
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
}

pub fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn is_block_break_tag(tag: &str) -> bool {
    matches!(
        tag,
        "/p" | "/div" | "/h1" | "/h2" | "/h3" | "/h4" | "/h5" | "/h6" | "br" | "br/"
    ) || tag.starts_with("br ")
        || tag.starts_with("br/")
}
